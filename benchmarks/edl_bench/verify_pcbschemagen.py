#!/usr/bin/env python3
"""External, unmodified UT Austin PCBSchemaGen_v2 (NeurIPS 2026) verifier harness for EDL-Bench.

Grades ANY KiCad 9 `.kicad_sch` (produced by EDL OR by No-EDL Baseline Python/KiCad tools)
using:
  1. UT Austin's unmodified `PCBSchemaGen_v2` deterministic structural verifier:
     kicad_parser.snapshot_from_kicad_sch
       -> canon_rules.apply_passive_canonicalization
       -> canon_rules.apply_kg_fuzzy_resolution
       -> topo.build_topology.augment_snapshot
       -> device_pin_corrections.apply_device_pin_corrections
       -> canon_rules.apply_net_canonicalization
       -> verification_v2.verify_circuit(snap, task_id=...)
     Returning exact Reward (0.0..1.0), passed (bool), failing_layer, and errors!
  2. Real KiCad 9 CLI ERC (`kicad-cli sch erc --severity-all --format json`).

Usage:
  python3 benchmarks/edl_bench/verify_pcbschemagen.py --suite pcbbench --task-id 52 path/to/design.kicad_sch
  python3 benchmarks/edl_bench/verify_pcbschemagen.py --batch benchmarks/edl_bench/pcbschemagen_ab/manifest.json --out results.json
"""

from __future__ import annotations

import argparse
import copy
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

EDL_ROOT = Path(__file__).resolve().parent.parent.parent
PCBSG_ROOT = EDL_ROOT / "benchmarks" / "external" / "PCBSchemaGen_v2"


def load_pcbsg_verifier(suite: str = "pcbbench", calibrated_roles: bool = False):
    if not PCBSG_ROOT.is_dir():
        raise FileNotFoundError(
            f"PCBSchemaGen_v2 not found at {PCBSG_ROOT}. Clone https://github.com/HZou9/PCBSchemaGen_v2 into benchmarks/external/PCBSchemaGen_v2"
        )
    if calibrated_roles:
        os.environ["PCBSG_ROLE_CALIBRATION"] = "1"
    else:
        os.environ.pop("PCBSG_ROLE_CALIBRATION", None)

    ws = tempfile.mkdtemp(prefix="edl_pcbsg_ws_")
    shutil.copytree(
        PCBSG_ROOT / "framework",
        Path(ws) / "task",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__"),
    )
    if suite == "ose":
        shutil.copy2(PCBSG_ROOT / "kg" / "kg_open_schematics.json", Path(ws) / "component.json")
        shutil.copy2(PCBSG_ROOT / "kg" / "kg_open_schematics.json", Path(ws) / "kg_component.json")
    else:
        shutil.copy2(PCBSG_ROOT / "component.json", Path(ws) / "component.json")
        shutil.copy2(PCBSG_ROOT / "kg_component.json", Path(ws) / "kg_component.json")
        if (PCBSG_ROOT / "kg.json").is_file():
            shutil.copy2(PCBSG_ROOT / "kg.json", Path(ws) / "kg.json")

    task_dir = str(Path(ws) / "task")
    calib_dir = str(PCBSG_ROOT / "tools" / "board_calibration")
    for p in (task_dir, calib_dir):
        if p not in sys.path:
            sys.path.insert(0, p)

    import canon_rules  # type: ignore
    import device_pin_corrections as dpc  # type: ignore
    import kicad_parser  # type: ignore
    import verification_v2  # type: ignore
    from topo import build_topology, kg_loader  # type: ignore

    kg_store = kg_loader.KGStore(base_dir=ws)
    return ws, kicad_parser, canon_rules, dpc, build_topology, verification_v2, kg_store


def run_kicad_erc(sch_path: Path) -> dict:
    with tempfile.TemporaryDirectory() as td:
        out_json = Path(td) / "erc.json"
        cp = subprocess.run(
            [
                "kicad-cli",
                "sch",
                "erc",
                "--format",
                "json",
                "--severity-all",
                "--exit-code-violations",
                "-o",
                str(out_json),
                str(sch_path),
            ],
            capture_output=True,
            text=True,
        )
        errs, warns = [], []
        if out_json.is_file():
            ej = json.loads(out_json.read_text(encoding="utf-8", errors="ignore"))
            for sh in ej.get("sheets", []):
                for v in sh.get("violations", []):
                    sev = v.get("severity", "")
                    msg = f"{v.get('type')}: {v.get('description')}"
                    if sev == "error":
                        errs.append(msg)
                    elif sev == "warning":
                        warns.append(msg)
        return {
            "rc": cp.returncode,
            "errors": len(errs),
            "warnings": len(warns),
            "error_samples": errs[:6],
            "warning_samples": warns[:6],
        }


def _parse_sexp(text: str):
    tokens = re.findall(r'"(?:[^"\\]|\\.)*"|\(|\)|[^\s()]+', text)
    stack: list = [[]]
    for tok in tokens:
        if tok == "(":
            node: list = []
            stack[-1].append(node)
            stack.append(node)
        elif tok == ")":
            if len(stack) > 1:
                stack.pop()
        else:
            if tok.startswith('"') and tok.endswith('"'):
                tok = tok[1:-1].replace('\\"', '"')
            stack[-1].append(tok)
    return stack[0][0] if stack[0] else []


def _find_child(node: list, tag: str):
    for c in node[1:]:
        if isinstance(c, list) and c and c[0] == tag:
            return c
    return None


def snapshot_from_kicad_cli(sch_path: Path) -> dict:
    """Extract the authoritative KiCad 9 C++ netlist directly via `kicad-cli sch export netlist`.

    Unlike PCBSchemaGen_v2/tools/board_calibration/kicad_parser.py (which omits KiCad's symbol-to-schematic
    Y-axis negation on lines 416 & 715 and flips vertical symbols upside-down), `kicad-cli` uses KiCad 9's
    exact native schematic connectivity engine identically for every `.kicad_sch` file.
    """
    with tempfile.TemporaryDirectory() as td:
        net_file = Path(td) / "out.net"
        cp = subprocess.run(
            ["kicad-cli", "sch", "export", "netlist", str(sch_path), "-o", str(net_file)],
            capture_output=True,
            text=True,
        )
        if cp.returncode != 0 or not net_file.is_file():
            raise RuntimeError(f"kicad-cli netlist export failed (rc={cp.returncode}): {cp.stderr.strip()}")
        root = _parse_sexp(net_file.read_text(encoding="utf-8", errors="ignore"))

    comp_map: dict[str, dict] = {}
    comps_node = _find_child(root, "components")
    if comps_node:
        for c in comps_node[1:]:
            if not (isinstance(c, list) and c and c[0] == "comp"):
                continue
            ref_n = _find_child(c, "ref")
            if not ref_n or len(ref_n) < 2:
                continue
            ref = ref_n[1]
            if ref.startswith("#"):
                continue
            val_n = _find_child(c, "value")
            val = val_n[1] if val_n and len(val_n) >= 2 else ""
            ls_n = _find_child(c, "libsource")
            part_id = val
            if ls_n:
                p_n = _find_child(ls_n, "part")
                if p_n and len(p_n) >= 2:
                    part_id = p_n[1]
            comp_map[ref] = {"ref": ref, "part_id": part_id, "value": val, "pins": []}

    nets_out: list[dict] = []
    nets_node = _find_child(root, "nets")
    if nets_node:
        for net in nets_node[1:]:
            if not (isinstance(net, list) and net and net[0] == "net"):
                continue
            name_n = _find_child(net, "name")
            if not name_n or len(name_n) < 2:
                continue
            name = name_n[1].lstrip("/")
            if name.startswith("unconnected-"):
                for ch in net[1:]:
                    if isinstance(ch, list) and ch and ch[0] == "node":
                        ref_n = _find_child(ch, "ref")
                        pin_n = _find_child(ch, "pin")
                        if not (ref_n and pin_n):
                            continue
                        ref, pin = ref_n[1], pin_n[1]
                        pf_n = _find_child(ch, "pinfunction")
                        pname = pf_n[1] if pf_n and len(pf_n) >= 2 else ""
                        if ref in comp_map:
                            comp_map[ref]["pins"].append({"pin_id": pin, "pin_name": pname, "net": None})
                continue
            endpoints = []
            for ch in net[1:]:
                if isinstance(ch, list) and ch and ch[0] == "node":
                    ref_n = _find_child(ch, "ref")
                    pin_n = _find_child(ch, "pin")
                    if not (ref_n and pin_n):
                        continue
                    ref, pin = ref_n[1], pin_n[1]
                    if ref not in comp_map:
                        continue
                    pf_n = _find_child(ch, "pinfunction")
                    pname = pf_n[1] if pf_n and len(pf_n) >= 2 else ""
                    endpoints.append({"ref": ref, "pin_id": pin, "pin_name": pname})
                    comp_map[ref]["pins"].append({"pin_id": pin, "pin_name": pname, "net": name})
            if endpoints:
                nets_out.append({"name": name, "endpoints": endpoints})

    return {"components": list(comp_map.values()), "nets": nets_out}


def verify_one(
    sch_path: Path,
    task_id: int,
    ctx,
    suite: str = "pcbbench",
    parser: str = "kicad_cli",
) -> dict:
    _, kicad_parser, canon_rules, dpc, build_topology, verification_v2, kg_store = ctx
    rec = {
        "task_id": task_id,
        "schematic": str(sch_path),
        "exists": sch_path.is_file(),
    }
    if not sch_path.is_file():
        rec.update(pcbsg_passed=False, pcbsg_reward=0.0, pcbsg_failing_layer="MISSING", pcbsg_errors=["file not found"])
        return rec

    rec["kicad_erc"] = run_kicad_erc(sch_path)
    try:
        if parser == "kicad_cli":
            snap = snapshot_from_kicad_cli(sch_path)
        else:
            raw_txt = sch_path.read_text(encoding="utf-8", errors="ignore")
            snap = kicad_parser.snapshot_from_kicad_sch(raw_txt)
    except Exception as e:
        rec.update(
            pcbsg_passed=False,
            pcbsg_reward=0.1,
            pcbsg_failing_layer="PARSE",
            pcbsg_errors=[f"{type(e).__name__}: {e}"],
        )
        return rec

    rec["parsed_components"] = len(snap.get("components", []))
    rec["parsed_nets"] = len(snap.get("nets", []))
    if not snap.get("components"):
        rec.update(
            pcbsg_passed=False,
            pcbsg_reward=0.1,
            pcbsg_failing_layer="EMPTY",
            pcbsg_errors=["parsed 0 components"],
        )
        return rec

    snap = copy.deepcopy(snap)
    canon_rules.apply_passive_canonicalization(snap)
    canon_rules.apply_kg_fuzzy_resolution(snap, kg_store)
    build_topology.augment_snapshot(snap, kg_store)
    if suite == "ose":
        # Open-Schematics-Eval board calibration rules (do NOT run on isolated PCBBench tasks:
        # canon_rules.apply_net_canonicalization collapses GND_ISO -> GND!)
        dpc.apply_device_pin_corrections(snap)
        canon_rules.apply_net_canonicalization(snap)

    try:
        v = verification_v2.verify_circuit(snap, task_id=task_id)
        rec.update(
            pcbsg_passed=bool(v.passed),
            pcbsg_reward=float(v.reward),
            pcbsg_failing_layer=v.failing_layer or "PASS",
            pcbsg_errors=list(v.errors or [])[:10],
            detected_topologies=list(getattr(v, "detected_topologies", []) or []),
        )
    except Exception as e:
        rec.update(
            pcbsg_passed=False,
            pcbsg_reward=0.0,
            pcbsg_failing_layer="CRASH",
            pcbsg_errors=[f"{type(e).__name__}: {e}"],
        )
    return rec


def main():
    ap = argparse.ArgumentParser(description="Grade KiCad schematics with UT Austin PCBSchemaGen_v2 verification_v2 + KiCad 9 ERC")
    ap.add_argument("--suite", choices=["pcbbench", "ose"], default="pcbbench")
    ap.add_argument("--parser", choices=["kicad_cli", "kicad_parser"], default="kicad_cli")
    ap.add_argument("--task-id", type=int, help="Single PCBBench/OSE task ID")
    ap.add_argument("schematic", nargs="?", help="Path to .kicad_sch file (single mode)")
    ap.add_argument("--batch", help="Path to JSON batch manifest [{task_id, arm, schematic}, ...]")
    ap.add_argument("--out", help="Optional output JSON path")
    ap.add_argument("--calibrated-roles", action="store_true")
    args = ap.parse_args()

    ctx = load_pcbsg_verifier(suite=args.suite, calibrated_roles=args.calibrated_roles)
    ws = ctx[0]
    try:
        if args.batch:
            items = json.loads(Path(args.batch).read_text(encoding="utf-8"))
            results = []
            for item in items:
                r = verify_one(
                    Path(item["schematic"]),
                    int(item["task_id"]),
                    ctx,
                    suite=args.suite,
                    parser=args.parser,
                )
                r["arm"] = item.get("arm", "unknown")
                r["name"] = item.get("name", f"T{item['task_id']}")
                results.append(r)
            payload = {
                "suite": args.suite,
                "parser": args.parser,
                "calibrated_roles": args.calibrated_roles,
                "results": results,
            }
        else:
            if not args.schematic or args.task_id is None:
                ap.error("Provide --task-id and schematic path, or --batch")
            payload = verify_one(
                Path(args.schematic),
                args.task_id,
                ctx,
                suite=args.suite,
                parser=args.parser,
            )

        txt = json.dumps(payload, indent=2)
        if args.out:
            Path(args.out).write_text(txt, encoding="utf-8")
        print(txt)
    finally:
        shutil.rmtree(ws, ignore_errors=True)


if __name__ == "__main__":
    main()
