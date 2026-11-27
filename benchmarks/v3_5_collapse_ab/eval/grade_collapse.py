#!/usr/bin/env python3
"""Independent external grader for EDL v3.5.0 'Collapse Point' A/B Benchmarks:
  - B6: Multi-Discipline Medical Clinic Revision B ECO (Arch + Plumbing + Electrical)
  - B7: 4-Sheet ~180-Component Hierarchical Industrial PLC Controller (KiCad 9)
"""

import csv
import json
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def count_lines(paths):
    total = 0
    detail = []
    for p in sorted(paths):
        n = len(p.read_text(encoding="utf-8", errors="ignore").splitlines())
        total += n
        detail.append(f"{p.name}:{n}L")
    return total, detail


def diff_changed_lines(rev_a_dir: Path, rev_b_dir: Path, exts):
    changed = 0
    per_file = {}
    for p in sorted(rev_b_dir.iterdir()):
        if not p.is_file() or p.suffix not in exts:
            continue
        a = rev_a_dir / p.name
        if not a.is_file():
            n = len(p.read_text(encoding="utf-8", errors="ignore").splitlines())
            changed += n
            per_file[p.name] = n
            continue
        cp = subprocess.run(
            ["diff", "-u", str(a), str(p)],
            capture_output=True,
            text=True,
        )
        c = sum(
            1
            for ln in cp.stdout.splitlines()
            if (ln.startswith("+") and not ln.startswith("+++"))
            or (ln.startswith("-") and not ln.startswith("---"))
        )
        changed += c
        per_file[p.name] = c
    return changed, per_file


def parse_notes_timing(notes_path: Path):
    if not notes_path.is_file():
        return {"notes_exists": False, "elapsed_sec": None, "iterations": None}
    txt = notes_path.read_text(encoding="utf-8", errors="ignore")
    m_el = re.search(r"(?:elapsed_sec|elapsed time)[^0-9]*([0-9]+)", txt, re.I)
    m_it = re.search(r"(?:build / verification iterations|iterations)[^0-9]*([0-9]+)", txt, re.I)
    return {
        "notes_exists": True,
        "elapsed_sec": int(m_el.group(1)) if m_el else None,
        "iterations": int(m_it.group(1)) if m_it else None,
    }


def grade_b6_arm(arm_dir: Path, is_edl: bool):
    exts = {".edl"} if is_edl else {".py"}
    src_files = [p for p in arm_dir.iterdir() if p.is_file() and p.suffix in exts]
    total_lines, src_detail = count_lines(src_files)
    changed_lines, changed_detail = diff_changed_lines(arm_dir / "rev_a", arm_dir, exts)

    deliverables = {
        "clinic_arch.dxf": (arm_dir / "clinic_arch.dxf").is_file(),
        "clinic_arch.png": (arm_dir / "clinic_arch.png").is_file(),
        "clinic_plumbing.dxf": (arm_dir / "clinic_plumbing.dxf").is_file(),
        "clinic_plumbing.png": (arm_dir / "clinic_plumbing.png").is_file(),
        "plumbing_schedule.csv": (arm_dir / "plumbing_schedule.csv").is_file(),
        "clinic_electrical.dxf": (arm_dir / "clinic_electrical.dxf").is_file(),
        "clinic_electrical.png": (arm_dir / "clinic_electrical.png").is_file(),
        "panel_schedule.csv": (arm_dir / "panel_schedule.csv").is_file(),
    }

    # Verify Exam5 presence across DXF / schedules / sources
    combined_src = "\n".join(p.read_text(encoding="utf-8", errors="ignore") for p in src_files)
    arch_dxf = (arm_dir / "clinic_arch.dxf").read_text(encoding="utf-8", errors="ignore") if deliverables["clinic_arch.dxf"] else ""
    plumb_dxf = (arm_dir / "clinic_plumbing.dxf").read_text(encoding="utf-8", errors="ignore") if deliverables["clinic_plumbing.dxf"] else ""
    elec_dxf = (arm_dir / "clinic_electrical.dxf").read_text(encoding="utf-8", errors="ignore") if deliverables["clinic_electrical.dxf"] else ""

    eco_checks = {
        "has_exam5_in_arch": bool(re.search(r"Exam\s*5", arch_dxf, re.I)),
        "has_exam5_in_plumbing": bool(re.search(r"Exam\s*5", plumb_dxf + combined_src, re.I)),
        "has_exam5_in_electrical": bool(re.search(r"Exam\s*5", elec_dxf + combined_src, re.I)),
        "all_deliverables_present": all(deliverables.values()),
    }

    edl_build = {}
    if is_edl:
        edl_bin = ROOT.parent.parent / "target" / "release" / "edl"
        for fn in ["clinic_arch.edl", "clinic_plumbing.edl", "clinic_electrical.edl"]:
            fp = arm_dir / fn
            if fp.is_file():
                cp = subprocess.run([str(edl_bin), "build", str(fp)], capture_output=True, text=True)
                edl_build[fn] = {"rc": cp.returncode, "ok": cp.returncode == 0}

    return {
        "arm": arm_dir.name,
        "timing": parse_notes_timing(arm_dir / "notes.md"),
        "source_lines": total_lines,
        "source_files": src_detail,
        "eco_diff_lines": changed_lines,
        "eco_diff_per_file": changed_detail,
        "deliverables": deliverables,
        "eco_checks": eco_checks,
        "edl_build": edl_build,
    }


def parse_kicad_netlist(net_text: str):
    comps = re.findall(r'\(comp\s+\(ref\s+"([^"]+)"\)', net_text)
    nets = {}
    for m in re.finditer(r'\(net\s+\(code\s+"[^"]+"\)\s+\(name\s+"([^"]+)"\)', net_text):
        nname = m.group(1)
        start = m.end()
        depth = 1
        pos = start
        while pos < len(net_text) and depth > 0:
            if net_text[pos] == "(":
                depth += 1
            elif net_text[pos] == ")":
                depth -= 1
            pos += 1
        body = net_text[start:pos]
        nodes = re.findall(r'\(node\s+\(ref\s+"([^"]+)"\)\s+\(pin\s+"([^"]+)"\)', body)
        nets[nname] = nodes
    return comps, nets


def grade_b7_arm(arm_dir: Path, is_edl: bool):
    exts = {".edl"} if is_edl else {".py"}
    src_files = [p for p in arm_dir.iterdir() if p.is_file() and p.suffix in exts]
    total_lines, src_detail = count_lines(src_files)

    kdir = arm_dir / "kicad"
    sch_files = sorted(kdir.glob("*.kicad_sch")) if kdir.is_dir() else []
    # Find root sheet (one containing `(sheet ` child blocks)
    root_sch = None
    for sf in sch_files:
        if "(sheet\n" in sf.read_text(encoding="utf-8", errors="ignore") or "(sheet " in sf.read_text(encoding="utf-8", errors="ignore"):
            root_sch = sf
            break
    if root_sch is None and sch_files:
        root_sch = sch_files[0]

    pngs = sorted(p.name for p in arm_dir.glob("*.png"))
    res = {
        "arm": arm_dir.name,
        "timing": parse_notes_timing(arm_dir / "notes.md"),
        "source_lines": total_lines,
        "source_files": src_detail,
        "kicad_sheet_files": [p.name for p in sch_files],
        "root_sheet": root_sch.name if root_sch else None,
        "png_files": pngs,
        "erc_cli_rc": None,
        "erc_errors": None,
        "erc_warnings": None,
        "component_count": 0,
        "multi_pin_nets": 0,
        "single_pin_nets": 0,
        "isolation_check": {},
    }

    if root_sch is None:
        return res

    with tempfile.TemporaryDirectory() as td:
        erc_json = Path(td) / "erc.json"
        net_file = Path(td) / "plc.net"
        cp_erc = subprocess.run(
            ["kicad-cli", "sch", "erc", "--format", "json", "--severity-all", "--exit-code-violations", "-o", str(erc_json), str(root_sch)],
            capture_output=True,
            text=True,
        )
        res["erc_cli_rc"] = cp_erc.returncode
        if erc_json.is_file():
            ej = json.loads(erc_json.read_text())
            errs = []
            warns = []
            for sh in ej.get("sheets", []):
                for v in sh.get("violations", []):
                    sev = v.get("severity", "")
                    if sev == "error":
                        errs.append(f"{v.get('type')}: {v.get('description')}")
                    elif sev == "warning":
                        warns.append(f"{v.get('type')}: {v.get('description')}")
            res["erc_errors"] = len(errs)
            res["erc_warnings"] = len(warns)
            res["erc_error_samples"] = errs[:8]
            res["erc_warning_samples"] = warns[:8]

        cp_net = subprocess.run(
            ["kicad-cli", "sch", "export", "netlist", "-o", str(net_file), str(root_sch)],
            capture_output=True,
            text=True,
        )
        res["netlist_cli_rc"] = cp_net.returncode
        if net_file.is_file():
            net_txt = net_file.read_text(encoding="utf-8", errors="ignore")
            comps, nets = parse_kicad_netlist(net_txt)
            comp_vals = dict(
                re.findall(r'\(comp\s+\(ref\s+"([^"]+)"\)\s*\(value\s+"([^"]+)"\)', net_txt)
            )
            real_comps = [c for c in comps if not c.startswith("#")]
            multi_nets = {k: v for k, v in nets.items() if len(v) >= 2 and not k.startswith("unconnected-")}
            single_nets = {k: v for k, v in nets.items() if len(v) == 1 and not k.startswith("unconnected-")}
            res["component_count"] = len(real_comps)
            res["multi_pin_nets"] = len(multi_nets)
            res["single_pin_nets"] = len(single_nets)

            # Check galvanic isolation across GND, ISO_IO_GND, ISO_BUS_GND
            # Legitimate galvanic barrier components (optocouplers, digital isolators, isolated DC/DCs)
            # are identified by their real KiCad value/part rather than RefDes guessing.
            iso_val_re = re.compile(r"(PC817|EL817|TLP|ADuM|ISO[0-9]|Si86|TMR-|B0505)", re.I)

            def refs_for(pattern, exclude_isolators=True):
                s = set()
                for n, nodes in nets.items():
                    if re.fullmatch(pattern, n, re.I) or n.endswith("/" + pattern.strip("^$")):
                        for ref, _ in nodes:
                            if ref.startswith("#"):
                                continue
                            val = comp_vals.get(ref, "")
                            if exclude_isolators and iso_val_re.search(val):
                                continue
                            s.add(ref)
                return s

            g_main = refs_for(r"/?GND")
            g_io = refs_for(r"/?ISO_IO_GND")
            g_bus = refs_for(r"/?ISO_BUS_GND")
            res["isolation_check"] = {
                "GND_refs": len(g_main),
                "ISO_IO_GND_refs": len(g_io),
                "ISO_BUS_GND_refs": len(g_bus),
                "non_isolator_shared_GND_vs_ISO_IO": sorted(g_main & g_io),
                "non_isolator_shared_GND_vs_ISO_BUS": sorted(g_main & g_bus),
                "non_isolator_shared_ISO_IO_vs_ISO_BUS": sorted(g_io & g_bus),
            }

    if is_edl:
        edl_bin = ROOT.parent.parent / "target" / "release" / "edl"
        edl_file = arm_dir / "plc_controller.edl"
        if edl_file.is_file():
            cp = subprocess.run([str(edl_bin), "build", str(edl_file)], capture_output=True, text=True)
            res["edl_build_rc"] = cp.returncode

    return res


def main():
    out = {
        "b6_clinic_eco": {
            "baseline": grade_b6_arm(ROOT / "b6_clinic_eco" / "baseline", False),
            "edl": grade_b6_arm(ROOT / "b6_clinic_eco" / "edl", True),
        },
        "b7_plc_4sheet": {
            "baseline": grade_b7_arm(ROOT / "b7_plc_4sheet" / "baseline", False),
            "edl": grade_b7_arm(ROOT / "b7_plc_4sheet" / "edl", True),
        },
    }
    out_path = ROOT / "eval" / "results.json"
    out_path.write_text(json.dumps(out, indent=2))
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
