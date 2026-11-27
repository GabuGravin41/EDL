#!/usr/bin/env python3
"""
Verification Gate Runner & High-Res Renderer for Benchmark 7 Baseline.
Runs:
  1. kicad-cli sch erc --exit-code-violations --severity-all -o erc.rpt kicad/plc_controller.kicad_sch
  2. kicad-cli sch export netlist -o plc.net kicad/plc_controller.kicad_sch + strict audit
  3. kicad-cli sch export svg + inkscape 3200px PNG renders for root + all 4 child sheets
"""

import glob
import os
import re
import subprocess
import sys

BASE_DIR = "/home/lindah/projects/EDL/benchmarks/v3_5_collapse_ab/b7_plc_4sheet/baseline"
KICAD_SCH = os.path.join(BASE_DIR, "kicad", "plc_controller.kicad_sch")
ERC_RPT = os.path.join(BASE_DIR, "erc.rpt")
NET_FILE = os.path.join(BASE_DIR, "plc.net")
SVG_DIR = os.path.join(BASE_DIR, "svg_tmp")


def run(cmd, check=True):
    print(">>>", " ".join(cmd))
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.stdout.strip():
        print(res.stdout.strip())
    if res.stderr.strip():
        print(res.stderr.strip(), file=sys.stderr)
    if check and res.returncode != 0:
        raise RuntimeError(f"Command failed with exit code {res.returncode}: {' '.join(cmd)}")
    return res


def audit_netlist(net_path: str):
    with open(net_path, "r", encoding="utf-8") as f:
        txt = f.read()

    # Components (real parts, excluding #PWR/#FLG)
    comps = re.findall(r'\(comp \(ref "([^"]+)"\)', txt)
    real_comps = [c for c in comps if not c.startswith("#")]

    # Nets
    nets = {}
    for m in re.finditer(r'\(net \(code "[^"]+"\) \(name "([^"]+)"\)[^\n]*\n((?:[ \t]+\(node[^\n]*\n)*)', txt):
        nname = m.group(1)
        body = m.group(2)
        nodes = re.findall(r'\(node \(ref "([^"]+)"\) \(pin "([^"]+)"\)', body)
        nets[nname] = nodes

    multi_pin_nets = {k: v for k, v in nets.items() if len(v) >= 2 and not k.startswith("unconnected-")}

    # Galvanic isolation check
    domains = ["GND", "ISO_IO_GND", "ISO_BUS_GND"]
    domain_refs = {}
    for d in domains:
        if d not in nets:
            raise AssertionError(f"Missing isolation ground net: {d}")
        domain_refs[d] = {ref for ref, _ in nets[d] if not ref.startswith("#")}

    pairs = [("GND", "ISO_IO_GND"), ("GND", "ISO_BUS_GND"), ("ISO_IO_GND", "ISO_BUS_GND")]
    print("\n================ NETLIST AUDIT ================")
    print(f"Total real components : {len(real_comps)} (gate: >= 150)")
    print(f"Total nets            : {len(nets)}")
    print(f"Multi-pin nets (>=2)  : {len(multi_pin_nets)} (gate: >= 90)")
    for d in domains:
        print(f"  Domain {d:12s} : {len(domain_refs[d]):3d} unique component refs")
    all_isolated = True
    for a, b in pairs:
        shared = sorted(domain_refs[a] & domain_refs[b])
        print(f"  Shared refs ({a} & {b}): {len(shared)} {shared}")
        if shared:
            all_isolated = False

    # Verify cross-sheet signals exist and have >=2 pins
    hier_sigs = [
        "SYS_RST_N", "PWR_GOOD", "V24_SENSE",
        "DI1_MCU", "DI2_MCU", "DI3_MCU", "DI4_MCU",
        "DO1_MCU", "DO2_MCU", "DO3_MCU", "DO4_MCU",
        "AIN1_ADC", "AIN2_ADC",
        "RS485_TX", "RS485_RX", "RS485_DE",
        "CAN_TX", "CAN_RX",
    ]
    print("\nCross-sheet hierarchical net connectivity:")
    missing_hier = []
    for sig in hier_sigs:
        matches = [(k, v) for k, v in nets.items() if k.endswith("/" + sig) or k == sig]
        if not matches:
            print(f"  MISSING: {sig}")
            missing_hier.append(sig)
        else:
            k, v = matches[0]
            refs = sorted({r for r, _ in v})
            print(f"  {sig:12s} -> {k:34s} ({len(v)} pins: {', '.join(refs)})")
            if len(v) < 2:
                missing_hier.append(sig)

    assert len(real_comps) >= 150, f"Component gate failed: {len(real_comps)} < 150"
    assert len(multi_pin_nets) >= 90, f"Multi-pin net gate failed: {len(multi_pin_nets)} < 90"
    assert all_isolated, "Galvanic isolation gate FAILED: shared refs across ground domains!"
    assert not missing_hier, f"Hierarchical signal connectivity FAILED: {missing_hier}"
    print("ALL NETLIST & ISOLATION GATES PASSED!")
    return len(real_comps), len(multi_pin_nets), {d: len(domain_refs[d]) for d in domains}


def render_pngs():
    os.makedirs(SVG_DIR, exist_ok=True)
    for old in glob.glob(os.path.join(SVG_DIR, "*.svg")):
        os.remove(old)

    run(["kicad-cli", "sch", "export", "svg", "-o", SVG_DIR, KICAD_SCH])
    svgs = sorted(glob.glob(os.path.join(SVG_DIR, "*.svg")))
    print("Exported SVGs:", [os.path.basename(s) for s in svgs])

    mapping = {
        "plc_controller.svg": "root.png",
        "plc_controller-Sheet1_Power.svg": "sheet1_power.png",
        "plc_controller-Sheet2_MCU_Ethernet.svg": "sheet2_mcu_eth.png",
        "plc_controller-Sheet3_Isolated_IO.svg": "sheet3_iso_io.png",
        "plc_controller-Sheet4_Analog_Fieldbus.svg": "sheet4_analog_bus.png",
    }
    for svg_path in svgs:
        bn = os.path.basename(svg_path)
        out_name = mapping.get(bn)
        if not out_name:
            # Fuzzy fallback by sheet name
            for key, val in [
                ("Sheet1_Power", "sheet1_power.png"),
                ("Sheet2_MCU_Ethernet", "sheet2_mcu_eth.png"),
                ("Sheet3_Isolated_IO", "sheet3_iso_io.png"),
                ("Sheet4_Analog_Fieldbus", "sheet4_analog_bus.png"),
            ]:
                if key in bn:
                    out_name = val
                    break
            if not out_name:
                out_name = "root.png"
        out_png = os.path.join(BASE_DIR, out_name)
        run([
            "inkscape", svg_path,
            "--export-type=png",
            "--export-width=3200",
            "--export-background=white",
            "--export-background-opacity=1.0",
            f"--export-filename={out_png}",
        ])
        print(f"Rendered {out_png} ({os.path.getsize(out_png):,} bytes)")


def main():
    # Gate 1: ERC
    run(["kicad-cli", "sch", "erc", "--exit-code-violations", "--severity-all", "-o", ERC_RPT, KICAD_SCH])
    # Gate 2: Netlist
    run(["kicad-cli", "sch", "export", "netlist", "-o", NET_FILE, KICAD_SCH])
    audit_netlist(NET_FILE)
    # Gate 3: Render PNGs
    render_pngs()


if __name__ == "__main__":
    main()
