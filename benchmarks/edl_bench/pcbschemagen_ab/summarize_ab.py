#!/usr/bin/env python3
"""Merge Baseline & EDL PCBBench manifests, run UT Austin's unmodified verification_v2
(both strict and calibrated-roles, via authoritative KiCad 9 C++ netlist extraction)
+ KiCad 9 ERC, and print a full side-by-side scoreboard."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path("/home/lindah/projects/EDL/benchmarks/edl_bench")
AB_DIR = ROOT / "pcbschemagen_ab"
VERIFIER = ROOT / "verify_pcbschemagen.py"


def load_arm_manifest(path: Path, arm_name: str) -> list[dict]:
    data = json.loads(path.read_text(encoding="utf-8"))
    entries = data if isinstance(data, list) else data.get("tasks", data.get("entries", []))
    out = []
    for item in entries:
        tid = item.get("task_id") or item.get("id")
        sch = item.get("schematic") or item.get("kicad_sch") or item.get("path")
        if tid is not None and sch:
            p = Path(sch)
            if not p.is_absolute():
                p = (path.parent / p).resolve()
            out.append({
                "task_id": int(tid),
                "arm": arm_name,
                "name": item.get("name", f"T{tid}"),
                "schematic": str(p),
            })
    return out


def main() -> None:
    base_m = AB_DIR / "baseline" / "manifest.json"
    edl_m = AB_DIR / "edl" / "manifest.json"
    if not base_m.exists() or not edl_m.exists():
        print(f"Missing manifests: baseline={base_m.exists()} edl={edl_m.exists()}", file=sys.stderr)
        sys.exit(1)

    batch = load_arm_manifest(base_m, "baseline") + load_arm_manifest(edl_m, "edl")
    batch_path = AB_DIR / "batch_manifest.json"
    batch_path.write_text(json.dumps(batch, indent=2), encoding="utf-8")

    strict_out = AB_DIR / "results_strict.json"
    calib_out = AB_DIR / "results_calibrated.json"

    for label, extra, out_path in [
        ("STRICT", [], strict_out),
        ("CALIBRATED", ["--calibrated-roles"], calib_out),
    ]:
        cmd = [
            sys.executable, str(VERIFIER),
            "--suite", "pcbbench",
            "--parser", "kicad_cli",
            *extra,
            "--batch", str(batch_path),
            "--out", str(out_path),
        ]
        print(f"\n=== Running UT Austin verification_v2 ({label}) ===")
        res = subprocess.run(cmd, capture_output=True, text=True)
        if res.returncode != 0:
            print(f"Verifier ({label}) returned {res.returncode}: {res.stderr}", file=sys.stderr)
            sys.exit(res.returncode)

    strict = json.loads(strict_out.read_text(encoding="utf-8"))["results"]
    calib = json.loads(calib_out.read_text(encoding="utf-8"))["results"]

    by_key_s = {(r["task_id"], r["arm"]): r for r in strict}
    by_key_c = {(r["task_id"], r["arm"]): r for r in calib}
    task_ids = sorted({r["task_id"] for r in strict})

    print("\n================ UT AUSTIN PCBBENCH EXTERNAL A/B SCOREBOARD ================")
    hdr = f"{'Task':<5} | {'Base Strict':<24} | {'EDL Strict':<24} | {'Base Calib':<24} | {'EDL Calib':<24} | {'Base ERC':<9} | {'EDL ERC':<9}"
    print(hdr)
    print("-" * len(hdr))
    for tid in task_ids:
        bs = by_key_s.get((tid, "baseline"), {})
        es = by_key_s.get((tid, "edl"), {})
        bc = by_key_c.get((tid, "baseline"), {})
        ec = by_key_c.get((tid, "edl"), {})

        def fmt_v(r: dict) -> str:
            if not r:
                return "MISSING"
            rew = r.get("pcbsg_reward", 0.0)
            ok = "PASS" if r.get("pcbsg_passed") else (r.get("pcbsg_failing_layer") or "FAIL")
            return f"{rew:.2f} ({ok})"

        def fmt_erc(r: dict) -> str:
            if not r:
                return "-"
            e = r.get("kicad_erc", {})
            return f"{e.get('errors', '?')}E/{e.get('warnings', '?')}W"

        print(
            f"T{tid:<4} | {fmt_v(bs):<24} | {fmt_v(es):<24} | "
            f"{fmt_v(bc):<24} | {fmt_v(ec):<24} | {fmt_erc(bs):<9} | {fmt_erc(es):<9}"
        )

    print("-" * len(hdr))
    for arm in ("baseline", "edl"):
        s_rows = [by_key_s[(t, arm)] for t in task_ids if (t, arm) in by_key_s]
        c_rows = [by_key_c[(t, arm)] for t in task_ids if (t, arm) in by_key_c]
        s_mean = sum(r["pcbsg_reward"] for r in s_rows) / max(len(s_rows), 1)
        s_pass = sum(1 for r in s_rows if r["pcbsg_passed"])
        c_mean = sum(r["pcbsg_reward"] for r in c_rows) / max(len(c_rows), 1)
        c_pass = sum(1 for r in c_rows if r["pcbsg_passed"])
        erc_e = sum(r["kicad_erc"].get("errors", 0) or 0 for r in s_rows)
        erc_w = sum(r["kicad_erc"].get("warnings", 0) or 0 for r in s_rows)
        print(
            f"SUMMARY [{arm.upper():<8}]: "
            f"Strict={s_mean:.3f} ({s_pass}/{len(s_rows)} pass) | "
            f"Calib={c_mean:.3f} ({c_pass}/{len(c_rows)} pass) | "
            f"KiCad9 ERC={erc_e}E/{erc_w}W"
        )

    print("\n================ DETAILED FAILURE DIAGNOSTICS (STRICT) ================")
    for tid in task_ids:
        for arm in ("baseline", "edl"):
            r = by_key_s.get((tid, arm), {})
            if not r.get("pcbsg_passed"):
                print(f"\n[T{tid} | {arm.upper()}] reward={r.get('pcbsg_reward')} layer={r.get('pcbsg_failing_layer')} comps={r.get('parsed_components')} nets={r.get('parsed_nets')}")
                for err in r.get("pcbsg_errors", []):
                    print(f"  - {err}")


if __name__ == "__main__":
    main()
