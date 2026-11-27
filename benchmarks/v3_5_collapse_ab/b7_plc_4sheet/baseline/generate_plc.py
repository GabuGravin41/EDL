#!/usr/bin/env python3
"""
Benchmark 7 — Baseline Arm (Pure Python + KiCad 9, NO EDL)
4-Sheet Hierarchical Industrial PLC & Ethernet/Fieldbus Controller

Generates:
  kicad/plc_controller.kicad_pro
  kicad/sym-lib-table
  kicad/plc_power.kicad_sym
  kicad/plc_controller.kicad_sch         (Root hierarchy sheet)
  kicad/Sheet1_Power.kicad_sch           (Sheet 1: Multi-Rail Industrial Power)
  kicad/Sheet2_MCU_Ethernet.kicad_sch    (Sheet 2: STM32F407 MCU + LAN8720A PHY)
  kicad/Sheet3_Isolated_IO.kicad_sch     (Sheet 3: 4-Ch Iso DI + 4-Ch 24V Relay DO)
  kicad/Sheet4_Analog_Fieldbus.kicad_sch (Sheet 4: 2-Ch Analog FE + Iso RS-485 & CAN)
"""

import json
import os
from kicad_lib import (
    Schematic,
    SymbolLibraryCache,
    create_custom_power_lib,
    gen_uuid,
)

BASE_DIR = "/home/lindah/projects/EDL/benchmarks/v3_5_collapse_ab/b7_plc_4sheet/baseline"
KICAD_DIR = os.path.join(BASE_DIR, "kicad")
PROJECT_NAME = "plc_controller"


def write_project_files():
    os.makedirs(KICAD_DIR, exist_ok=True)
    pwr_lib_path = os.path.join(KICAD_DIR, "plc_power.kicad_sym")
    create_custom_power_lib(pwr_lib_path)

    sym_lib_table = (
        "(sym_lib_table\n"
        "  (version 7)\n"
        '  (lib (name "plc_power")(type "KiCad")(uri "${KIPRJMOD}/plc_power.kicad_sym")(options "")(descr "Isolated Power & Ground Domains"))\n'
        ")\n"
    )
    with open(os.path.join(KICAD_DIR, "sym-lib-table"), "w", encoding="utf-8") as f:
        f.write(sym_lib_table)

    pro = {
        "board": {"3dviewports": [], "design_settings": {}, "layer_presets": []},
        "erc": {
            "erc_exclusions": [],
            "meta": {"version": 0},
            "pin_map": [
                [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 2],
                [0, 2, 0, 1, 0, 0, 1, 0, 2, 2, 2, 2],
                [0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 2],
                [0, 1, 0, 0, 0, 0, 1, 1, 2, 1, 1, 2],
                [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 2],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
                [1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 2],
                [0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 2],
                [0, 2, 1, 2, 0, 0, 1, 0, 2, 2, 2, 2],
                [0, 2, 0, 1, 0, 0, 1, 0, 2, 0, 0, 2],
                [0, 2, 1, 1, 0, 0, 1, 0, 2, 0, 0, 2],
                [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
            ],
            "rule_severities": {},
        },
        "meta": {"filename": f"{PROJECT_NAME}.kicad_pro", "version": 3},
        "net_settings": {"classes": [{"name": "Default"}]},
        "schematic": {"drawing": {}, "meta": {"version": 1}},
        "sheets": [],
    }
    with open(os.path.join(KICAD_DIR, f"{PROJECT_NAME}.kicad_pro"), "w", encoding="utf-8") as f:
        json.dump(pro, f, indent=2)
    return pwr_lib_path


# ==============================================================================
# SHEET 1: MULTI-RAIL INDUSTRIAL POWER ARCHITECTURE
# ==============================================================================
def build_sheet1_power(lib: SymbolLibraryCache, root_uuid: str, sheet_uuid: str) -> Schematic:
    s = Schematic(
        title="Sheet 1 — Multi-Rail Power Architecture",
        lib_cache=lib,
        project_name=PROJECT_NAME,
        sheet_uuid=sheet_uuid,
        instance_path=f"/{root_uuid}/{sheet_uuid}",
        page_num="2",
        paper="A2",
        comment1="24V EMI + LM2596S-ADJ 5V Buck + Digital/Analog 3.3V LDOs",
        comment2="TMR-0511 Isolated 5V Bus Bias + Dual MCP100 Supervisors",
    )

    # ------------------------------------------------------------------
    # ZONE 1: 24V INDUSTRIAL INPUT PROTECTION, PI EMI FILTER & SENSE
    # ------------------------------------------------------------------
    s.add_box(22, 26, 252, 178, "1. 24V INDUSTRIAL INPUT PROTECTION, PI EMI FILTER & RAIL MONITOR",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    # J1: 24V Input Terminal (rotated 180 so pins face RIGHT)
    s.add_comp("Connector:Screw_Terminal_01x02", "J1", "PWR_24V_IN", 40.64, 78.74, rot=180,
               footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-02_1x02_P5.08mm_Horizontal",
               ref_offset=(0, -10), val_offset=(0, 10))
    # Pin 1 is at (45.72, 78.74), Pin 2 is at (45.72, 81.28)
    j1_1 = s.pin("J1", "1")
    j1_2 = s.pin("J1", "2")
    # Route J1 pin 2 to GND + PWR_FLAG
    gnd_in_pt = (53.34, 134.62)
    s.wire_L(j1_2, (53.34, j1_2[1]))
    s.wire((53.34, j1_2[1]), gnd_in_pt)
    s.pwr("GND", gnd_in_pt[0], gnd_in_pt[1])
    s.pwr_flag(53.34, 121.92)
    s.junction((53.34, 121.92))

    # F1: 2A Slow-Blow Fuse (horizontal, rot=90: pin1 left, pin2 right)
    s.add_comp("Device:Fuse", "F1", "2A_SB", 63.50, 78.74, rot=90,
               footprint="Fuse:Fuse_1206_3216Metric",
               ref_offset=(0, -4.5), val_offset=(0, 4.5))
    s.wire(j1_1, s.pin("F1", "1"))

    # D1: Reverse Polarity Schottky SS34 (rot=180: A left, K right)
    s.add_comp("Diode:SS34", "D1", "SS34", 86.36, 78.74, rot=180,
               footprint="Diode_SMD:D_SMA",
               ref_offset=(0, -5.0), val_offset=(0, 5.0))
    s.wire(s.pin("F1", "2"), s.pin("D1", "2"))

    # D2: 28V TVS SMBJ28A (vertical, rot=90: A2 top, A1 bottom)
    s.add_comp("Device:D_TVS", "D2", "SMBJ28A", 104.14, 104.14, rot=90,
               footprint="Diode_SMD:D_SMB",
               ref_offset=(5.0, -2.0), val_offset=(5.0, 2.0))
    s.wire(s.pin("D1", "1"), (104.14, 78.74))
    s.wire((104.14, 78.74), s.pin("D2", "2"))
    s.junction((104.14, 78.74))
    s.wire(s.pin("D2", "1"), (104.14, 134.62))
    s.pwr("GND", 104.14, 134.62)

    # C1: Pi Filter Input Cap 100uF 50V
    s.add_comp("Device:C_Polarized", "C1", "100uF/50V", 127.00, 104.14, rot=0,
               footprint="Capacitor_SMD:CP_Elec_8x10",
               ref_offset=(5.0, -2.0), val_offset=(5.0, 2.0))
    s.wire((104.14, 78.74), (127.00, 78.74))
    s.wire((127.00, 78.74), s.pin("C1", "1"))
    s.junction((127.00, 78.74))
    s.wire(s.pin("C1", "2"), (127.00, 134.62))
    s.pwr("GND", 127.00, 134.62)

    # L1: Pi EMI Inductor 10uH (horizontal, rot=90)
    s.add_comp("Device:L", "L1", "10uH_3A", 147.32, 78.74, rot=90,
               footprint="Inductor_SMD:L_Bourns_SRR1260",
               ref_offset=(0, -5.0), val_offset=(0, 5.0))
    s.wire((127.00, 78.74), s.pin("L1", "1"))

    # C2: Pi Filter Output Cap 100uF 50V & C3: 100nF ceramic -> +24V rail
    s.add_comp("Device:C_Polarized", "C2", "100uF/50V", 167.64, 104.14, rot=0,
               footprint="Capacitor_SMD:CP_Elec_8x10",
               ref_offset=(5.0, -2.0), val_offset=(5.0, 2.0))
    s.add_comp("Device:C", "C3", "100nF/50V", 190.50, 104.14, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.5, -2.0), val_offset=(4.5, 2.0))

    s.wire(s.pin("L1", "2"), (167.64, 78.74))
    s.wire((167.64, 78.74), s.pin("C2", "1"))
    s.junction((167.64, 78.74))
    s.wire(s.pin("C2", "2"), (167.64, 134.62))
    s.pwr("GND", 167.64, 134.62)

    s.wire((167.64, 78.74), (190.50, 78.74))
    s.wire((190.50, 78.74), s.pin("C3", "1"))
    s.junction((190.50, 78.74))
    s.wire(s.pin("C3", "2"), (190.50, 134.62))
    s.pwr("GND", 190.50, 134.62)

    # +24V rail symbol + PWR_FLAG
    s.wire((190.50, 78.74), (190.50, 60.96))
    s.pwr("+24V", 190.50, 60.96)
    s.wire((190.50, 78.74), (203.20, 78.74))
    s.pwr_flag(203.20, 71.12)
    s.wire((203.20, 78.74), (203.20, 71.12))
    s.junction((203.20, 78.74))

    # 24V Voltage Monitor Divider (R1=100k, R2=10k, C4=100nF -> V24_SENSE)
    s.add_comp("Device:R", "R1", "100k", 218.44, 96.52, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.0, -1.8), val_offset=(4.0, 1.8))
    s.add_comp("Device:R", "R2", "10k", 218.44, 119.38, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.0, -1.8), val_offset=(4.0, 1.8))
    s.add_comp("Device:C", "C4", "100nF", 233.68, 119.38, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.0, -1.8), val_offset=(4.0, 1.8))

    s.wire((203.20, 78.74), (218.44, 78.74))
    s.wire((218.44, 78.74), s.pin("R1", "1"))
    s.wire(s.pin("R1", "2"), s.pin("R2", "1"))
    s.wire(s.pin("R2", "2"), (218.44, 134.62))
    s.pwr("GND", 218.44, 134.62)

    mid_v24 = (218.44, 107.95)
    s.junction(mid_v24)
    s.wire(mid_v24, (233.68, 107.95))
    s.wire((233.68, 107.95), s.pin("C4", "1"))
    s.junction((233.68, 107.95))
    s.wire(s.pin("C4", "2"), (233.68, 134.62))
    s.pwr("GND", 233.68, 134.62)
    s.wire((233.68, 107.95), (243.84, 107.95))
    s.hlabel("V24_SENSE", (243.84, 107.95), shape="output", angle=0, justify="left")

    # ------------------------------------------------------------------
    # ZONE 2: 24V -> 5V BUCK CONVERTER (LM2596S-ADJ)
    # ------------------------------------------------------------------
    s.add_box(264, 26, 566, 178, "2. 24V -> 5V INDUSTRIAL BUCK CONVERTER (LM2596S-ADJ, 3A)",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    s.add_comp("Regulator_Switching:LM2596S-ADJ", "U1", "LM2596S-ADJ", 358.14, 93.98, rot=0,
               footprint="Package_TO_SOT_SMD:TO-263-5_TabPin3",
               ref_offset=(0, -13), val_offset=(0, -9.5))

    u1_vin = s.pin("U1", "1")   # (345.44, 91.44)
    u1_en = s.pin("U1", "5")    # (345.44, 96.52)
    u1_gnd = s.pin("U1", "3")   # (358.14, 101.60)
    u1_fb = s.pin("U1", "4")    # (370.84, 91.44)
    u1_out = s.pin("U1", "2")   # (370.84, 96.52)

    # Buck Input Caps C5 (470uF), C6 (100nF)
    s.add_comp("Device:C_Polarized", "C5", "470uF/50V", 294.64, 114.30, rot=0,
               footprint="Capacitor_SMD:CP_Elec_10x10.5",
               ref_offset=(5.0, -2.0), val_offset=(5.0, 2.0))
    s.add_comp("Device:C", "C6", "100nF/50V", 317.50, 114.30, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.5, -2.0), val_offset=(4.5, 2.0))

    s.pwr("+24V", 294.64, 71.12)
    s.wire((294.64, 71.12), (294.64, 91.44))
    s.wire((294.64, 91.44), u1_vin)
    s.wire((294.64, 91.44), s.pin("C5", "1"))
    s.junction((294.64, 91.44))
    s.wire(s.pin("C5", "2"), (294.64, 142.24))
    s.pwr("GND", 294.64, 142.24)

    s.wire((317.50, 91.44), s.pin("C6", "1"))
    s.junction((317.50, 91.44))
    s.wire(s.pin("C6", "2"), (317.50, 142.24))
    s.pwr("GND", 317.50, 142.24)

    # R5: 10k EN pulldown
    s.add_comp("Device:R", "R5", "10k", 335.28, 119.38, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(-5.5, -1.8), val_offset=(-5.5, 1.8))
    s.wire_L(u1_en, s.pin("R5", "1"))
    s.wire(s.pin("R5", "2"), (335.28, 142.24))
    s.pwr("GND", 335.28, 142.24)

    s.wire(u1_gnd, (358.14, 142.24))
    s.pwr("GND", 358.14, 142.24)

    # Catch diode D3 (SS34, rot=270: K top, A bottom)
    s.add_comp("Diode:SS34", "D3", "SS34", 391.16, 119.38, rot=270,
               footprint="Diode_SMD:D_SMA",
               ref_offset=(5.0, -2.0), val_offset=(5.0, 2.0))
    s.wire(u1_out, (391.16, 96.52))
    s.wire((391.16, 96.52), s.pin("D3", "1"))
    s.junction((391.16, 96.52))
    s.wire(s.pin("D3", "2"), (391.16, 142.24))
    s.pwr("GND", 391.16, 142.24)

    # L2: 33uH 3A Buck Inductor (horizontal rot=90)
    s.add_comp("Device:L", "L2", "33uH_3A", 419.10, 96.52, rot=90,
               footprint="Inductor_SMD:L_Bourns_SRR1260",
               ref_offset=(0, -5.0), val_offset=(0, 5.0))
    s.wire((391.16, 96.52), s.pin("L2", "1"))

    # Output caps C7 (470uF), C8 (100nF), feedback R3 (3.09k), R4 (1.0k), C9 (10nF)
    s.add_comp("Device:C_Polarized", "C7", "470uF/16V", 447.04, 119.38, rot=0,
               footprint="Capacitor_SMD:CP_Elec_10x10.5",
               ref_offset=(5.0, -2.0), val_offset=(5.0, 2.0))
    s.add_comp("Device:C", "C8", "100nF", 472.44, 119.38, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.5, -2.0), val_offset=(4.5, 2.0))
    s.add_comp("Device:R", "R3", "3.09k_1%", 502.92, 111.76, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    s.add_comp("Device:R", "R4", "1.0k_1%", 502.92, 132.08, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    s.add_comp("Device:C", "C9", "10nF", 525.78, 111.76, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))

    # +5V rail bus from L2 pin 2
    s.wire(s.pin("L2", "2"), (525.78, 96.52))
    for cx in [447.04, 472.44, 502.92]:
        s.junction((cx, 96.52))
    s.wire((447.04, 96.52), s.pin("C7", "1"))
    s.wire(s.pin("C7", "2"), (447.04, 142.24))
    s.pwr("GND", 447.04, 142.24)

    s.wire((472.44, 96.52), s.pin("C8", "1"))
    s.wire(s.pin("C8", "2"), (472.44, 142.24))
    s.pwr("GND", 472.44, 142.24)

    s.wire((502.92, 96.52), s.pin("R3", "1"))
    s.wire(s.pin("R3", "2"), s.pin("R4", "1"))
    s.wire(s.pin("R4", "2"), (502.92, 142.24))
    s.pwr("GND", 502.92, 142.24)

    s.wire((525.78, 96.52), s.pin("C9", "1"))
    fb_node = (502.92, 121.92)
    s.junction(fb_node)
    s.wire_L(s.pin("C9", "2"), fb_node, horiz_first=False)
    # Route FB back to U1 pin 4 neatly above inductor
    s.wire(u1_fb, (383.54, 91.44))
    s.wire((383.54, 91.44), (383.54, 68.58))
    s.wire((383.54, 68.58), (543.56, 68.58))
    s.wire((543.56, 68.58), (543.56, 121.92))
    s.wire((543.56, 121.92), (525.78, 121.92))
    s.junction((525.78, 121.92))
    s.label("BUCK_FB", (447.04, 68.58))

    # +5V rail symbol + PWR_FLAG
    s.wire((525.78, 96.52), (556.26, 96.52))
    s.junction((525.78, 96.52))
    s.wire((556.26, 96.52), (556.26, 76.20))
    s.pwr("+5V", 556.26, 76.20)
    s.wire((548.64, 96.52), (548.64, 86.36))
    s.junction((548.64, 96.52))
    s.pwr_flag(548.64, 86.36)

    # ------------------------------------------------------------------
    # ZONE 3: DIGITAL +3V3 LDO & LOW-NOISE ANALOG +3V3A LDO
    # ------------------------------------------------------------------
    s.add_box(22, 190, 212, 386, "3. DIGITAL +3V3 & LOW-NOISE ANALOG +3V3A LDOs",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    # 3A: Digital 3.3V LDO (U2 AMS1117-3.3)
    s.add_comp("Regulator_Linear:AMS1117-3.3", "U2", "AMS1117-3.3", 104.14, 236.22, rot=0,
               footprint="Package_TO_SOT_SMD:SOT-223-3_TabPin2",
               ref_offset=(0, -10.5), val_offset=(0, -7.2))
    s.add_comp("Device:C", "C10", "10uF", 66.04, 254.00, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    s.add_comp("Device:C_Polarized", "C11", "22uF", 142.24, 254.00, rot=0,
               footprint="Capacitor_SMD:CP_Elec_5x5.4",
               ref_offset=(5.0, -1.8), val_offset=(5.0, 1.8))
    s.add_comp("Device:C", "C12", "100nF", 167.64, 254.00, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))

    s.pwr("+5V", 48.26, 220.98)
    s.wire((48.26, 220.98), (48.26, 236.22))
    s.wire((48.26, 236.22), s.pin("U2", "3"))
    s.wire((66.04, 236.22), s.pin("C10", "1"))
    s.junction((66.04, 236.22))
    s.wire(s.pin("C10", "2"), (66.04, 269.24))
    s.pwr("GND", 66.04, 269.24)
    s.wire(s.pin("U2", "1"), (104.14, 269.24))
    s.pwr("GND", 104.14, 269.24)

    s.wire(s.pin("U2", "2"), (187.96, 236.22))
    s.wire((142.24, 236.22), s.pin("C11", "1"))
    s.junction((142.24, 236.22))
    s.wire(s.pin("C11", "2"), (142.24, 269.24))
    s.pwr("GND", 142.24, 269.24)
    s.wire((167.64, 236.22), s.pin("C12", "1"))
    s.junction((167.64, 236.22))
    s.wire(s.pin("C12", "2"), (167.64, 269.24))
    s.pwr("GND", 167.64, 269.24)
    s.wire((187.96, 236.22), (187.96, 220.98))
    s.pwr("+3V3", 187.96, 220.98)

    # 3B: Low-Noise Analog 3.3V LDO (FB1 -> U3 AMS1117-3.3 -> +3V3A)
    s.add_comp("Device:FerriteBead", "FB1", "600R@100MHz", 55.88, 325.12, rot=90,
               footprint="Inductor_SMD:L_0805_2012Metric",
               ref_offset=(0, -4.5), val_offset=(0, 4.5))
    s.add_comp("Device:C", "C13", "10uF", 78.74, 342.90, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    s.add_comp("Regulator_Linear:AMS1117-3.3", "U3", "AMS1117-3.3", 109.22, 325.12, rot=0,
               footprint="Package_TO_SOT_SMD:SOT-223-3_TabPin2",
               ref_offset=(0, -10.5), val_offset=(0, -7.2))
    s.add_comp("Device:C_Polarized", "C14", "22uF", 142.24, 342.90, rot=0,
               footprint="Capacitor_SMD:CP_Elec_5x5.4",
               ref_offset=(5.0, -1.8), val_offset=(5.0, 1.8))
    s.add_comp("Device:C", "C15", "100nF", 165.10, 342.90, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    s.add_comp("Device:C", "C16", "10nF", 185.42, 342.90, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))

    s.pwr("+5V", 38.10, 309.88)
    s.wire((38.10, 309.88), (38.10, 325.12))
    s.wire((38.10, 325.12), s.pin("FB1", "1"))
    s.wire(s.pin("FB1", "2"), s.pin("U3", "3"))
    s.wire((78.74, 325.12), s.pin("C13", "1"))
    s.junction((78.74, 325.12))
    s.wire(s.pin("C13", "2"), (78.74, 358.14))
    s.pwr("GND", 78.74, 358.14)
    # PWR_FLAG on 5V_ANA (after passive FB1, feeds U3 power_in)
    s.wire((88.90, 325.12), (88.90, 314.96))
    s.junction((88.90, 325.12))
    s.pwr_flag(88.90, 314.96)
    s.label("5V_ANA", (68.58, 325.12))

    s.wire(s.pin("U3", "1"), (109.22, 358.14))
    s.pwr("GND", 109.22, 358.14)

    s.wire(s.pin("U3", "2"), (198.12, 325.12))
    for cx, cref in [(142.24, "C14"), (165.10, "C15"), (185.42, "C16")]:
        s.wire((cx, 325.12), s.pin(cref, "1"))
        s.junction((cx, 325.12))
        s.wire(s.pin(cref, "2"), (cx, 358.14))
        s.pwr("GND", cx, 358.14)
    s.wire((198.12, 325.12), (198.12, 309.88))
    s.pwr("+3V3A", 198.12, 309.88)

    # ------------------------------------------------------------------
    # ZONE 4: ISOLATED 5V FIELDBUS BIAS DC/DC (TMR-0511)
    # ------------------------------------------------------------------
    s.add_box(224, 190, 398, 386, "4. ISOLATED 5V FIELDBUS BIAS (1.5kV DC/DC)",
              stroke_rgb=(180, 40, 20), fill_rgba=(255, 248, 245, 0.7))

    s.add_isolation_barrier(304.80, 208, 378, "MAIN (+5V/GND)", "ISO_BUS_5V/GND",
                            banner="ISO BARRIER (1.5kV)")

    s.add_comp("Converter_DCDC:TMR-0511", "PS1", "TMR-0511", 304.80, 274.32, rot=0,
               footprint="Converter_DCDC:Converter_DCDC_TRACO_TMR-1-xxxx_Single_THT",
               ref_offset=(0, -14), val_offset=(0, 12))
    # Primary side (left of barrier)
    ps_vinp = s.pin("PS1", "2")
    ps_vinm = s.pin("PS1", "1")
    ps_rem = s.pin("PS1", "3")
    ps_voutp = s.pin("PS1", "6")
    ps_voutm = s.pin("PS1", "7")

    s.add_comp("Device:C", "C17", "4.7uF", 264.16, 287.02, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(-5.0, -1.8), val_offset=(-5.0, 1.8))
    s.add_comp("Device:R", "R6", "10k", 281.94, 254.00, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(-4.5, -1.8), val_offset=(-4.5, 1.8))

    s.pwr("+5V", 248.92, 243.84)
    s.wire((248.92, 243.84), (248.92, ps_vinp[1]))
    s.wire((248.92, ps_vinp[1]), ps_vinp)
    s.wire((264.16, ps_vinp[1]), s.pin("C17", "1"))
    s.junction((264.16, ps_vinp[1]))
    s.wire((264.16, 304.80), s.pin("C17", "2"))
    s.wire((264.16, 304.80), (281.94, 304.80))
    s.wire_L(ps_vinm, (281.94, 304.80), horiz_first=True)
    s.junction((281.94, 304.80))
    s.wire((281.94, 304.80), (281.94, 314.96))
    s.pwr("GND", 281.94, 314.96)

    # Pull Remote (pin 3) via R6 to +5V (keeps TMR-0511 enabled & ERC-clean)
    s.pwr("+5V", 281.94, 243.84)
    s.wire((281.94, 243.84), s.pin("R6", "1"))
    s.wire_L(s.pin("R6", "2"), ps_rem, horiz_first=False)

    # Secondary side (right of barrier): C18 pre-filter -> FB2/FB3 -> C19/C20 -> ISO_BUS_5V / ISO_BUS_GND
    # NOTE: FB3 cleanly separates PS1 pin 7 (ISO_DCDC_RTN) from ISO_BUS_GND so PS1 has 0 shared refs across GND/ISO_BUS_GND!
    s.add_comp("Device:C", "C18", "4.7uF", 327.66, 287.02, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    s.add_comp("Device:FerriteBead", "FB2", "600R", 342.90, 271.78, rot=90,
               footprint="Inductor_SMD:L_0805_2012Metric",
               ref_offset=(0, -4.5), val_offset=(0, 4.5))
    s.add_comp("Device:FerriteBead", "FB3", "600R", 342.90, 304.80, rot=90,
               footprint="Inductor_SMD:L_0805_2012Metric",
               ref_offset=(0, -4.5), val_offset=(0, 4.5))
    s.add_comp("Device:C", "C19", "10uF", 363.22, 287.02, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    s.add_comp("Device:C", "C20", "100nF", 383.54, 287.02, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))

    s.wire(ps_voutp, s.pin("FB2", "1"))
    s.wire((327.66, ps_voutp[1]), s.pin("C18", "1"))
    s.junction((327.66, ps_voutp[1]))

    # Route ps_voutm down at x=320.04 so it NEVER crosses C18 pin 1 wire at x=327.66!
    s.wire(ps_voutm, (320.04, ps_voutm[1]))
    s.wire((320.04, ps_voutm[1]), (320.04, 304.80))
    s.wire((320.04, 304.80), (327.66, 304.80))
    s.wire((327.66, 304.80), s.pin("C18", "2"))
    s.junction((327.66, 304.80))
    s.wire((327.66, 304.80), s.pin("FB3", "1"))
    s.label("ISO_DCDC_RTN", (332.74, 304.80), justify="left")

    s.wire(s.pin("FB2", "2"), (383.54, 271.78))
    s.wire((363.22, 271.78), s.pin("C19", "1"))
    s.junction((363.22, 271.78))
    s.wire((383.54, 271.78), s.pin("C20", "1"))
    s.junction((383.54, 271.78))
    s.wire((383.54, 271.78), (383.54, 251.46))
    s.pwr("ISO_BUS_5V", 383.54, 251.46)
    s.wire((363.22, 271.78), (363.22, 259.08))
    s.pwr_flag(363.22, 259.08)

    s.wire(s.pin("FB3", "2"), (383.54, 304.80))
    s.wire((363.22, 304.80), s.pin("C19", "2"))
    s.junction((363.22, 304.80))
    s.wire((383.54, 304.80), s.pin("C20", "2"))
    s.junction((383.54, 304.80))
    s.wire((383.54, 304.80), (383.54, 322.58))
    s.pwr("ISO_BUS_GND", 383.54, 322.58)
    s.wire((363.22, 304.80), (363.22, 320.04))
    s.pwr_flag(363.22, 320.04, rot=180)

    # ------------------------------------------------------------------
    # ZONE 5: SUPERVISORS, RAIL STATUS LEDs & TEST POINTS
    # ------------------------------------------------------------------
    s.add_box(408, 190, 566, 352, "5. SUPERVISORS, RAIL LEDs & TEST POINTS",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    # U4: 3.3V Reset Supervisor (MCP100-300D -> SYS_RST_N)
    s.add_comp("Power_Supervisor:MCP100-300D", "U4", "MCP100-300D", 447.04, 231.14, rot=0,
               footprint="Package_TO_SOT_SMD:SOT-23",
               ref_offset=(8, -9), val_offset=(12, -5.5))
    s.add_comp("Device:C", "C21", "100nF", 424.18, 231.14, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.5, -1.8), val_offset=(-5.5, 1.8))
    u4_vdd = s.pin("U4", "2")
    u4_vss = s.pin("U4", "3")
    u4_rst = s.pin("U4", "1")
    s.wire(u4_vdd, (444.50, 213.36))
    s.wire((444.50, 213.36), (424.18, 213.36))
    s.wire((424.18, 213.36), s.pin("C21", "1"))
    s.junction((444.50, 213.36))
    s.pwr("+3V3", 444.50, 213.36)
    s.wire(u4_vss, (444.50, 248.92))
    s.wire((444.50, 248.92), (424.18, 248.92))
    s.wire((424.18, 248.92), s.pin("C21", "2"))
    s.junction((444.50, 248.92))
    s.pwr("GND", 444.50, 248.92)
    s.stub_label(u4_rst, "SYS_RST_N", dx=10.16, hier=True, shape="output")

    # U5: 5V Power-Good Supervisor (MCP100-450D -> PWR_GOOD)
    s.add_comp("Power_Supervisor:MCP100-450D", "U5", "MCP100-450D", 518.16, 231.14, rot=0,
               footprint="Package_TO_SOT_SMD:SOT-23",
               ref_offset=(8, -9), val_offset=(12, -5.5))
    s.add_comp("Device:C", "C58", "100nF", 495.30, 231.14, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.5, -1.8), val_offset=(-5.5, 1.8))
    u5_vdd = s.pin("U5", "2")
    u5_vss = s.pin("U5", "3")
    u5_rst = s.pin("U5", "1")
    s.wire(u5_vdd, (515.62, 213.36))
    s.wire((515.62, 213.36), (495.30, 213.36))
    s.wire((495.30, 213.36), s.pin("C58", "1"))
    s.junction((515.62, 213.36))
    s.pwr("+5V", 515.62, 213.36)
    s.wire(u5_vss, (515.62, 248.92))
    s.wire((515.62, 248.92), (495.30, 248.92))
    s.wire((495.30, 248.92), s.pin("C58", "2"))
    s.junction((515.62, 248.92))
    s.pwr("GND", 515.62, 248.92)
    s.stub_label(u5_rst, "PWR_GOOD", dx=10.16, hier=True, shape="output")

    # Rail Status LEDs (24V, 5V, 3.3V)
    led_specs = [
        ("R7", "4.7k", "D4", "LED_24V_GRN", "+24V", 426.72),
        ("R8", "1k", "D5", "LED_5V_GRN", "+5V", 452.12),
        ("R9", "470R", "D6", "LED_3V3_GRN", "+3V3", 477.52),
    ]
    for rref, rval, dref, dval, rail, lx in led_specs:
        s.add_comp("Device:R", rref, rval, lx, 281.94, rot=0,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
        s.add_comp("Device:LED", dref, dval, lx, 302.26, rot=90,
                   footprint="LED_SMD:LED_0603_1608Metric",
                   ref_offset=(4.5, -1.5), val_offset=(4.5, 1.5))
        s.pwr(rail, lx, 271.78)
        s.wire((lx, 271.78), s.pin(rref, "1"))
        s.wire(s.pin(rref, "2"), s.pin(dref, "2"))
        s.wire(s.pin(dref, "1"), (lx, 314.96))
        s.pwr("GND", lx, 314.96)

    # Test Points (TP1..TP5)
    tp_specs = [
        ("TP1", "TP_24V", "+24V", 508.00, 281.94),
        ("TP2", "TP_5V", "+5V", 528.32, 281.94),
        ("TP3", "TP_3V3", "+3V3", 548.64, 281.94),
        ("TP4", "TP_3V3A", "+3V3A", 518.16, 325.12),
        ("TP5", "TP_GND", "GND", 543.56, 325.12),
    ]
    for tref, tval, rail, tx, ty in tp_specs:
        s.add_comp("Connector:TestPoint", tref, tval, tx, ty, rot=0,
                   footprint="TestPoint:TestPoint_Pad_D1.5mm",
                   ref_offset=(0, -5.0), val_offset=(0, 3.0))
        tp_p = s.pin(tref, "1")
        if rail == "GND":
            s.wire(tp_p, (tx, ty + 10.16))
            s.pwr("GND", tx, ty + 10.16)
        else:
            s.wire(tp_p, (tx, ty + 10.16))
            s.pwr(rail, tx, ty + 10.16, rot=180)

    return s


# ==============================================================================
# SHEET 2: STM32F407 MCU CORE, ETHERNET PHY (RMII), FLASH, I2C & DEBUG
# ==============================================================================
def build_sheet2_mcu_eth(lib: SymbolLibraryCache, root_uuid: str, sheet_uuid: str) -> Schematic:
    s = Schematic(
        title="Sheet 2 — STM32F407 MCU & LAN8720A Ethernet",
        lib_cache=lib,
        project_name=PROJECT_NAME,
        sheet_uuid=sheet_uuid,
        instance_path=f"/{root_uuid}/{sheet_uuid}",
        page_num="3",
        paper="A2",
        comment1="STM32F407VETx + 8MHz HSE + W25Q32 Flash + LM75C/PCF8563T",
        comment2="LAN8720A RMII PHY + HR911105A RJ45 + SWD & UART1 Debug",
    )

    # ------------------------------------------------------------------
    # CENTER-LEFT: STM32F407VETx MCU CORE
    # ------------------------------------------------------------------
    s.add_box(168, 22, 344, 240, "1. STM32F407VETx INDUSTRIAL MCU CORE (100-PIN LQFP)",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.5))

    mcu_x, mcu_y = 256.54, 134.62
    s.add_comp("MCU_ST_STM32F4:STM32F407VETx", "U6", "STM32F407VETx", mcu_x, mcu_y, rot=0,
               footprint="Package_QFP:LQFP-100_14x14mm_P0.5mm",
               ref_offset=(-18, -76), val_offset=(10, -76))

    # Power pins on top of U6: VBAT(6), VDD(11,19,28,50,75,100) -> +3V3; VDDA(22) -> +3V3A
    vdd_top_y = 53.34
    vdd_pins = ["6", "11", "19", "28", "50", "75", "100"]
    vdd_pts = [s.pin("U6", p) for p in vdd_pins]
    s.wire((vdd_pts[0][0], vdd_top_y), (vdd_pts[-1][0], vdd_top_y))
    for pt in vdd_pts:
        s.wire(pt, (pt[0], vdd_top_y))
        if pt != vdd_pts[0] and pt != vdd_pts[-1]:
            s.junction((pt[0], vdd_top_y))
    s.pwr("+3V3", vdd_pts[3][0], vdd_top_y)
    s.junction((vdd_pts[3][0], vdd_top_y))

    vdda_pt = s.pin("U6", "22")
    s.wire(vdda_pt, (vdda_pt[0], 45.72))
    s.pwr("+3V3A", vdda_pt[0], 45.72)

    # VREF+ (21, left side) -> +3V3A (rot=90 pointing left so it stays strictly on y=81.28 without crossing BOOT0!)
    vref_pt = s.pin("U6", "21")
    s.wire(vref_pt, (vref_pt[0] - 5.08, vref_pt[1]))
    s.pwr("+3V3A", vref_pt[0] - 5.08, vref_pt[1], rot=90)

    # Ground pins on bottom of U6: VSS(10,27,74,99) stacked at x=mcu_x, VSSA(20) at x=mcu_x+2.54
    for p in ["10", "27", "74", "99"]:
        vss_pt = s.pin("U6", p)
    vssa_pt = s.pin("U6", "20")
    gnd_bot_y = 215.90
    s.wire(vss_pt, (vss_pt[0], gnd_bot_y))
    s.wire(vssa_pt, (vssa_pt[0], gnd_bot_y))
    s.wire((vss_pt[0], gnd_bot_y), (vssa_pt[0], gnd_bot_y))
    s.junction((vss_pt[0], gnd_bot_y))
    s.pwr("GND", vss_pt[0], gnd_bot_y)

    # VCAP_1 (49) & VCAP_2 (73) -> C31, C32 (2.2uF to GND)
    vcap1 = s.pin("U6", "49")
    vcap2 = s.pin("U6", "73")
    s.add_comp("Device:C", "C31", "2.2uF", 205.74, 193.04, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.5, -1.8), val_offset=(-5.5, 1.8))
    s.add_comp("Device:C", "C32", "2.2uF", 220.98, 193.04, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.5, -1.8), val_offset=(-5.5, 1.8))
    s.wire_L(vcap1, s.pin("C31", "1"), horiz_first=True)
    s.wire_L(vcap2, s.pin("C32", "1"), horiz_first=True)
    s.wire(s.pin("C31", "2"), (205.74, 208.28))
    s.wire(s.pin("C32", "2"), (220.98, 208.28))
    s.pwr("GND", 205.74, 208.28)
    s.pwr("GND", 220.98, 208.28)

    # Wire ALL active MCU signal pins via clean short stubs + labels (left side & right side)
    left_sigs = [
        ("14", "MCU_NRST", False, ""),
        ("94", "MCU_BOOT0", False, ""),
        ("12", "HSE_IN", False, ""),
        ("13", "HSE_OUT", False, ""),
        ("1", "ETH_RST_N", False, ""),
        ("2", "LED_HEARTBEAT", False, ""),
        ("3", "LED_FAULT", False, ""),
        ("4", "PWR_GOOD", True, "input"),
        ("38", "DI1_MCU", True, "input"),
        ("39", "DI2_MCU", True, "input"),
        ("40", "DI3_MCU", True, "input"),
        ("41", "DI4_MCU", True, "input"),
        ("42", "DO1_MCU", True, "output"),
        ("43", "DO2_MCU", True, "output"),
        ("44", "DO3_MCU", True, "output"),
        ("45", "DO4_MCU", True, "output"),
        ("81", "CAN_RX", True, "input"),
        ("82", "CAN_TX", True, "output"),
        ("86", "RS485_TX", True, "output"),
        ("87", "RS485_RX", True, "input"),
        ("88", "RS485_DE", True, "output"),
    ]
    for pnum, lname, is_hier, hshape in left_sigs:
        pt = s.pin("U6", pnum)
        s.stub_label(pt, lname, dx=-17.78 if is_hier else -12.70, hier=is_hier, shape=hshape or "bidirectional")

    right_sigs = [
        ("23", "AIN1_ADC", True, "input"),
        ("24", "RMII_REF_CLK", False, ""),
        ("25", "ETH_MDIO", False, ""),
        ("26", "AIN2_ADC", True, "input"),
        ("29", "V24_SENSE", True, "input"),
        ("32", "RMII_CRS_DV", False, ""),
        ("68", "DBG_UART_TX", False, ""),
        ("69", "DBG_UART_RX", False, ""),
        ("72", "SWDIO", False, ""),
        ("76", "SWCLK", False, ""),
        ("77", "FLASH_CS_N", False, ""),
        ("89", "SWO", False, ""),
        ("92", "I2C1_SCL", False, ""),
        ("93", "I2C1_SDA", False, ""),
        ("48", "RMII_TX_EN", False, ""),
        ("51", "RMII_TXD0", False, ""),
        ("52", "RMII_TXD1", False, ""),
        ("16", "ETH_MDC", False, ""),
        ("33", "RMII_RXD0", False, ""),
        ("34", "RMII_RXD1", False, ""),
        ("78", "SPI3_SCK", False, ""),
        ("79", "SPI3_MISO", False, ""),
        ("80", "SPI3_MOSI", False, ""),
    ]
    for pnum, lname, is_hier, hshape in right_sigs:
        pt = s.pin("U6", pnum)
        s.stub_label(pt, lname, dx=17.78 if is_hier else 12.70, hier=is_hier, shape=hshape or "bidirectional")

    s.auto_nc_unused("U6")

    # ------------------------------------------------------------------
    # TOP-LEFT: 8MHz HSE CRYSTAL, RESET/BOOT0, STATUS LEDs, SWD & UART
    # ------------------------------------------------------------------
    s.add_box(22, 22, 158, 198, "2. CLOCK (8MHz), RESET/BOOT0, SWD & DEBUG UART",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    # 8MHz Crystal Y1 + C33, C34
    s.add_comp("Device:Crystal_GND24", "Y1", "8MHz", 55.88, 53.34, rot=0,
               footprint="Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm",
               ref_offset=(0, -6.0), val_offset=(0, -3.5))
    s.add_comp("Device:C", "C33", "18pF", 40.64, 68.58, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.0, -1.8), val_offset=(-5.0, 1.8))
    s.add_comp("Device:C", "C34", "18pF", 71.12, 68.58, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))
    y1_1, y1_3 = s.pin("Y1", "1"), s.pin("Y1", "3")
    y1_g = s.pin("Y1", "2")
    s.pin("Y1", "4")
    s.wire(y1_1, (40.64, 53.34))
    s.wire((40.64, 53.34), s.pin("C33", "1"))
    s.junction((40.64, 53.34))
    s.stub_label((40.64, 53.34), "HSE_IN", dx=-7.62)
    s.wire(y1_3, (71.12, 53.34))
    s.wire((71.12, 53.34), s.pin("C34", "1"))
    s.junction((71.12, 53.34))
    s.stub_label((71.12, 53.34), "HSE_OUT", dx=7.62)
    s.wire(s.pin("C33", "2"), (40.64, 78.74))
    s.wire(y1_g, (55.88, 78.74))
    s.wire(s.pin("C34", "2"), (71.12, 78.74))
    s.wire((40.64, 78.74), (71.12, 78.74))
    s.junction((55.88, 78.74))
    s.pwr("GND", 55.88, 78.74)

    # Reset Circuit: R12(10k PU), R13(100R from SYS_RST_N), C35(100nF), SW1(Reset), R14(10k BOOT0)
    s.add_comp("Device:R", "R12", "10k", 116.84, 50.80, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:R", "R13", "100R", 104.14, 60.96, rot=90,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(0, -4.2), val_offset=(0, 4.2))
    s.add_comp("Device:C", "C35", "100nF", 116.84, 71.12, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Switch:SW_Push", "SW1", "SW_RESET", 137.16, 71.12, rot=90,
               footprint="Button_Switch_SMD:SW_SPST_TL3342",
               ref_offset=(5.0, -1.5), val_offset=(5.0, 1.5))

    s.pwr("+3V3", 116.84, 40.64)
    s.wire((116.84, 40.64), s.pin("R12", "1"))
    s.wire(s.pin("R12", "2"), (116.84, 60.96))
    s.wire((116.84, 60.96), s.pin("C35", "1"))
    s.junction((116.84, 60.96))
    s.wire(s.pin("R13", "2"), (116.84, 60.96))
    s.stub_label(s.pin("R13", "1"), "SYS_RST_N", dx=-7.62, hier=True, shape="input")
    s.wire((116.84, 60.96), (137.16, 60.96))
    s.wire((137.16, 60.96), s.pin("SW1", "2"))
    s.junction((137.16, 60.96))
    s.stub_label((137.16, 60.96), "MCU_NRST", dx=7.62)
    s.wire(s.pin("C35", "2"), (116.84, 83.82))
    s.wire(s.pin("SW1", "1"), (137.16, 83.82))
    s.wire((116.84, 83.82), (137.16, 83.82))
    s.junction((116.84, 83.82))
    s.pwr("GND", 116.84, 83.82)

    # BOOT0 Pulldown R14 (10k) + Status LEDs (R15/D7 Heartbeat, R16/D8 Fault)
    s.add_comp("Device:R", "R14", "10k", 38.10, 109.22, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.stub_label(s.pin("R14", "1"), "MCU_BOOT0", dx=-7.62)
    s.wire(s.pin("R14", "2"), (38.10, 119.38))
    s.pwr("GND", 38.10, 119.38)

    for idx, (rref, dref, dval, sig, lx) in enumerate([
        ("R15", "D7", "LED_HB_BLU", "LED_HEARTBEAT", 63.50),
        ("R16", "D8", "LED_FLT_RED", "LED_FAULT", 88.90),
    ]):
        s.add_comp("Device:R", rref, "470R", lx, 104.14, rot=0,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
        s.add_comp("Device:LED", dref, dval, lx, 119.38, rot=90,
                   footprint="LED_SMD:LED_0603_1608Metric",
                   ref_offset=(4.5, -1.5), val_offset=(4.5, 1.5))
        s.stub_label(s.pin(rref, "1"), sig, dx=5.08)
        s.wire(s.pin(rref, "2"), s.pin(dref, "2"))
        s.wire(s.pin(dref, "1"), (lx, 129.54))
        s.pwr("GND", lx, 129.54)

    # J2: 10-Pin Cortex-M SWD Header (Conn_02x05_Odd_Even)
    s.add_comp("Connector_Generic:Conn_02x05_Odd_Even", "J2", "SWD_10P", 50.80, 165.10, rot=0,
               footprint="Connector_PinHeader_1.27mm:PinHeader_2x05_P1.27mm_Vertical_SMD",
               ref_offset=(0, -10.5), val_offset=(0, 10.5))
    # Odd pins (left): 1=VCC, 3=GND, 5=GND, 7=NC, 9=GND
    j2_1 = s.pin("J2", "1")
    s.wire(j2_1, (j2_1[0] - 5.08, j2_1[1]))
    s.wire((j2_1[0] - 5.08, j2_1[1]), (j2_1[0] - 5.08, j2_1[1] - 5.08))
    s.pwr("+3V3", j2_1[0] - 5.08, j2_1[1] - 5.08)
    for gp in ["3", "5", "9"]:
        p = s.pin("J2", gp)
        s.wire(p, (38.10, p[1]))
        s.junction((38.10, p[1]))
    s.wire((38.10, s.pin("J2", "3")[1]), (38.10, 180.34))
    s.pwr("GND", 38.10, 180.34)
    # Even pins (right): 2=SWDIO, 4=SWCLK, 6=SWO, 8=NC, 10=NRST
    s.stub_label(s.pin("J2", "2"), "SWDIO", dx=7.62)
    s.stub_label(s.pin("J2", "4"), "SWCLK", dx=7.62)
    s.stub_label(s.pin("J2", "6"), "SWO", dx=7.62)
    s.stub_label(s.pin("J2", "10"), "MCU_NRST", dx=7.62)
    s.auto_nc_unused("J2")

    # J3: 4-Pin Debug UART1 Header
    s.add_comp("Connector:Conn_01x04_Pin", "J3", "DBG_UART1", 121.92, 165.10, rot=0,
               footprint="Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical",
               ref_offset=(0, -9.5), val_offset=(0, 10.5))
    j3_1 = s.pin("J3", "1")
    s.wire(j3_1, (j3_1[0] + 5.08, j3_1[1]))
    s.wire((j3_1[0] + 5.08, j3_1[1]), (j3_1[0] + 5.08, j3_1[1] - 5.08))
    s.pwr("+3V3", j3_1[0] + 5.08, j3_1[1] - 5.08)
    s.stub_label(s.pin("J3", "2"), "DBG_UART_TX", dx=7.62)
    s.stub_label(s.pin("J3", "3"), "DBG_UART_RX", dx=7.62)
    j3_4 = s.pin("J3", "4")
    s.wire(j3_4, (j3_4[0] + 5.08, j3_4[1]))
    s.wire((j3_4[0] + 5.08, j3_4[1]), (j3_4[0] + 5.08, j3_4[1] + 5.08))
    s.pwr("GND", j3_4[0] + 5.08, j3_4[1] + 5.08)

    # ------------------------------------------------------------------
    # BOTTOM-LEFT: MCU DECOUPLING BANK (C22..C30) + SPI FLASH + I2C
    # ------------------------------------------------------------------
    s.add_box(22, 208, 158, 386, "3. MCU DECOUPLING BANK (VDD + VDDA)",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    # Row 1: +3V3 Decoupling (C22..C28 = 6x100nF + 4.7uF)
    dec_vdd = [("C22", "100nF"), ("C23", "100nF"), ("C24", "100nF"),
               ("C25", "100nF"), ("C26", "100nF"), ("C27", "100nF"), ("C28", "4.7uF")]
    for idx, (cref, cval) in enumerate(dec_vdd):
        cx = 35.56 + idx * 17.78
        s.add_comp("Device:C", cref, cval, cx, 246.38, rot=0,
                   footprint="Capacitor_SMD:C_0603_1608Metric",
                   ref_offset=(3.5, -1.8), val_offset=(3.5, 1.8))
        s.wire((cx, 233.68), s.pin(cref, "1"))
        s.wire(s.pin(cref, "2"), (cx, 259.08))
        if idx > 0 and idx < len(dec_vdd) - 1:
            s.junction((cx, 233.68))
            s.junction((cx, 259.08))
    s.wire((35.56, 233.68), (35.56 + 6 * 17.78, 233.68))
    s.wire((35.56, 259.08), (35.56 + 6 * 17.78, 259.08))
    s.pwr("+3V3", 35.56, 233.68)
    s.junction((35.56, 233.68))
    s.pwr("GND", 35.56, 259.08)
    s.junction((35.56, 259.08))

    # Row 2: +3V3A Analog Decoupling (C29 100nF, C30 1uF)
    for idx, (cref, cval) in enumerate([("C29", "100nF"), ("C30", "1uF")]):
        cx = 53.34 + idx * 25.40
        s.add_comp("Device:C", cref, cval, cx, 314.96, rot=0,
                   footprint="Capacitor_SMD:C_0603_1608Metric",
                   ref_offset=(4.0, -1.8), val_offset=(4.0, 1.8))
        s.wire((cx, 302.26), s.pin(cref, "1"))
        s.wire(s.pin(cref, "2"), (cx, 327.66))
    s.wire((53.34, 302.26), (78.74, 302.26))
    s.wire((53.34, 327.66), (78.74, 327.66))
    s.pwr("+3V3A", 53.34, 302.26)
    s.junction((53.34, 302.26))
    s.pwr("GND", 53.34, 327.66)
    s.junction((53.34, 327.66))

    # ------------------------------------------------------------------
    # BOTTOM-CENTER: SPI NOR FLASH (W25Q32) + I2C TEMP (LM75C) + RTC (PCF8563T)
    # ------------------------------------------------------------------
    s.add_box(168, 250, 344, 386, "4. SPI NOR FLASH (W25Q32) + I2C TEMP SENSOR (LM75C) & RTC (PCF8563T)",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    # U7: W25Q32JVSS SPI NOR Flash
    s.add_comp("Memory_Flash:W25Q32JVSS", "U7", "W25Q32JVSS", 213.36, 309.88, rot=0,
               footprint="Package_SO:SOIC-8_5.3x5.3mm_P1.27mm",
               ref_offset=(4, -14), val_offset=(4, 14))
    s.add_comp("Device:C", "C36", "100nF", 233.68, 309.88, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.0, -1.5), val_offset=(4.0, 1.5))
    s.add_comp("Device:R", "R17", "10k", 195.58, 287.02, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(-5.0, -1.5), val_offset=(-5.0, 1.5))
    s.add_comp("Device:R", "R18", "10k", 195.58, 337.82, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(-5.0, -1.5), val_offset=(-5.0, 1.5))

    u7_vcc, u7_gnd = s.pin("U7", "8"), s.pin("U7", "4")
    s.wire(u7_vcc, (213.36, 289.56))
    s.wire((213.36, 289.56), (233.68, 289.56))
    s.wire((233.68, 289.56), s.pin("C36", "1"))
    s.pwr("+3V3", 213.36, 289.56)
    s.junction((213.36, 289.56))
    s.wire(u7_gnd, (213.36, 330.20))
    s.wire((213.36, 330.20), (233.68, 330.20))
    s.wire((233.68, 330.20), s.pin("C36", "2"))
    s.pwr("GND", 213.36, 330.20)
    s.junction((213.36, 330.20))

    u7_cs = s.pin("U7", "1")
    s.wire(u7_cs, (195.58, u7_cs[1]))
    s.wire(s.pin("R17", "2"), (195.58, u7_cs[1]))
    s.junction((195.58, u7_cs[1]))
    s.pwr("+3V3", 195.58, 276.86)
    s.wire((195.58, 276.86), s.pin("R17", "1"))
    s.stub_label((195.58, u7_cs[1]), "FLASH_CS_N", dx=-7.62)

    s.stub_label(s.pin("U7", "6"), "SPI3_SCK", dx=-10.16)
    s.stub_label(s.pin("U7", "5"), "SPI3_MOSI", dx=-10.16)
    s.stub_label(s.pin("U7", "2"), "SPI3_MISO", dx=-10.16)
    # Tie WP(3) and HOLD(7) high via R18
    u7_wp, u7_hold = s.pin("U7", "3"), s.pin("U7", "7")
    s.wire(u7_wp, (195.58, u7_wp[1]))
    s.wire(u7_hold, (195.58, u7_hold[1]))
    s.wire((195.58, u7_wp[1]), (195.58, u7_hold[1]))
    s.wire((195.58, u7_hold[1]), s.pin("R18", "1"))
    s.junction((195.58, u7_hold[1]))
    s.wire(s.pin("R18", "2"), (195.58, 347.98))
    s.pwr("+3V3", 195.58, 347.98, rot=180)

    # I2C Pull-Ups (R19, R20 4.7k) + U8 (LM75C) + U9 (PCF8563T)
    s.add_comp("Device:R", "R19", "4.7k", 259.08, 279.40, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.0, -1.5), val_offset=(4.0, 1.5))
    s.add_comp("Device:R", "R20", "4.7k", 274.32, 279.40, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.0, -1.5), val_offset=(4.0, 1.5))
    s.pwr("+3V3", 259.08, 269.24)
    s.wire((259.08, 269.24), s.pin("R19", "1"))
    s.stub_label(s.pin("R19", "2"), "I2C1_SCL", dx=-2.54)
    s.pwr("+3V3", 274.32, 269.24)
    s.wire((274.32, 269.24), s.pin("R20", "1"))
    s.stub_label(s.pin("R20", "2"), "I2C1_SDA", dx=2.54)

    # U8: LM75C I2C Temperature Sensor
    s.add_comp("Sensor_Temperature:LM75C", "U8", "LM75C", 312.42, 281.94, rot=0,
               footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
               ref_offset=(0, -10.5), val_offset=(0, 10.5))
    s.add_comp("Device:C", "C37", "100nF", 289.56, 281.94, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.5, -1.5), val_offset=(-5.5, 1.5))
    s.wire(s.pin("U8", "8"), (312.42, 269.24))
    s.wire((312.42, 269.24), (289.56, 269.24))
    s.wire((289.56, 269.24), s.pin("C37", "1"))
    s.pwr("+3V3", 312.42, 269.24)
    s.junction((312.42, 269.24))
    # Tie A0, A1, A2, GND -> GND
    for ap in ["7", "6", "5"]:
        p = s.pin("U8", ap)
        s.wire(p, (299.72, p[1]))
        s.junction((299.72, p[1]))
    s.wire((299.72, s.pin("U8", "7")[1]), (299.72, 294.64))
    s.wire(s.pin("U8", "4"), (312.42, 294.64))
    s.wire((289.56, 294.64), (312.42, 294.64))
    s.wire((289.56, 294.64), s.pin("C37", "2"))
    s.junction((299.72, 294.64))
    s.junction((312.42, 294.64))
    s.pwr("GND", 312.42, 294.64)
    s.stub_label(s.pin("U8", "1"), "I2C1_SDA", dx=7.62)
    s.stub_label(s.pin("U8", "2"), "I2C1_SCL", dx=7.62)
    s.auto_nc_unused("U8")

    # U9: PCF8563T I2C RTC + Y2 (32.768kHz) + C38, C39, C40
    s.add_comp("Timer_RTC:PCF8563T", "U9", "PCF8563T", 312.42, 342.90, rot=0,
               footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
               ref_offset=(0, -12.5), val_offset=(0, 12.5))
    s.add_comp("Device:Crystal", "Y2", "32.768kHz", 287.02, 342.90, rot=90,
               footprint="Crystal:Crystal_SMD_3215-2Pin_3.2x1.5mm",
               ref_offset=(-5.5, -1.5), val_offset=(-5.5, 1.5))
    s.add_comp("Device:C", "C38", "12pF", 271.78, 325.12, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.0, -1.5), val_offset=(-5.0, 1.5))
    s.add_comp("Device:C", "C39", "12pF", 271.78, 360.68, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.0, -1.5), val_offset=(-5.0, 1.5))
    s.add_comp("Device:C", "C40", "100nF", 332.74, 342.90, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.0, -1.5), val_offset=(4.0, 1.5))

    s.wire(s.pin("U9", "8"), (312.42, 327.66))
    s.wire((312.42, 327.66), (332.74, 327.66))
    s.wire((332.74, 327.66), s.pin("C40", "1"))
    s.pwr("+3V3", 312.42, 327.66)
    s.junction((312.42, 327.66))
    s.wire(s.pin("U9", "4"), (312.42, 358.14))
    s.wire((312.42, 358.14), (332.74, 358.14))
    s.wire((332.74, 358.14), s.pin("C40", "2"))
    s.pwr("GND", 312.42, 358.14)
    s.junction((312.42, 358.14))

    # Y2 (rot=90): pin 2 is TOP (339.09), pin 1 is BOTTOM (346.71)
    u9_o1, u9_o2 = s.pin("U9", "1"), s.pin("U9", "2")
    s.wire(u9_o1, (287.02, u9_o1[1]))
    s.wire((287.02, u9_o1[1]), s.pin("Y2", "2"))
    s.wire((287.02, u9_o1[1]), (271.78, u9_o1[1]))
    s.junction((287.02, u9_o1[1]))
    s.wire((271.78, u9_o1[1]), s.pin("C38", "2"))
    s.wire(s.pin("C38", "1"), (271.78, 314.96))
    s.pwr("GND", 271.78, 314.96, rot=180)

    s.wire(u9_o2, (287.02, u9_o2[1]))
    s.wire((287.02, u9_o2[1]), s.pin("Y2", "1"))
    s.wire((287.02, u9_o2[1]), (271.78, u9_o2[1]))
    s.junction((287.02, u9_o2[1]))
    s.wire((271.78, u9_o2[1]), s.pin("C39", "1"))
    s.wire(s.pin("C39", "2"), (271.78, 370.84))
    s.pwr("GND", 271.78, 370.84)

    s.stub_label(s.pin("U9", "6"), "I2C1_SCL", dx=7.62)
    s.stub_label(s.pin("U9", "5"), "I2C1_SDA", dx=7.62)
    s.auto_nc_unused("U9")

    # ------------------------------------------------------------------
    # RIGHT HALF: EXTERNAL ETHERNET PHY (LAN8720A) + RJ45 MAGJACK
    # ------------------------------------------------------------------
    s.add_box(356, 22, 574, 348, "5. 10/100 ETHERNET PHY (LAN8720A RMII) + RJ45 MAGJACK (HR911105A)",
              stroke_rgb=(20, 100, 90), fill_rgba=(244, 253, 250, 0.7))

    phy_x, phy_y = 439.42, 134.62
    s.add_comp("Interface_Ethernet:LAN8720A", "U10", "LAN8720A", phy_x, phy_y, rot=0,
               footprint="Package_DFN_QFN:QFN-24-1EP_4x4mm_P0.5mm_EP2.6x2.6mm",
               ref_offset=(0, -34), val_offset=(0, 32))

    # RMII 33R Series Damping Resistors (R22..R28) staggered cleanly left of LAN8720A
    rmii_series = [
        ("17", "R22", "RMII_TXD0",    388.62),
        ("18", "R23", "RMII_TXD1",    406.40),
        ("16", "R24", "RMII_TX_EN",   388.62),
        ("8",  "R25", "RMII_RXD0",    406.40),
        ("7",  "R26", "RMII_RXD1",    388.62),
        ("11", "R27", "RMII_CRS_DV",  406.40),
        ("14", "R28", "RMII_REF_CLK", 388.62),
    ]
    for pnum, rref, netname, rx in rmii_series:
        pp = s.pin("U10", pnum)
        ry = pp[1]
        s.add_comp("Device:R", rref, "33R", rx, ry, rot=90,
                   footprint="Resistor_SMD:R_0402_1005Metric",
                   ref_offset=(0, -1.8), val_offset=(0, 1.8))
        s.wire(s.pin(rref, "2"), pp)
        p1 = s.pin(rref, "1")
        s.wire(p1, (375.92, ry))
        s.label(netname, (375.92, ry), justify="right")

    # MDIO (+1.5k pull-up R29), MDC, ~RST (+10k pull-up R30)
    mdio_p = s.pin("U10", "12")
    s.add_comp("Device:R", "R29", "1.5k", 411.48, 223.52, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.0, -1.5), val_offset=(4.0, 1.5))
    s.stub_label(mdio_p, "ETH_MDIO", dx=-20.32)
    s.pwr("+3V3", 411.48, 213.36)
    s.wire((411.48, 213.36), s.pin("R29", "1"))
    s.stub_label(s.pin("R29", "2"), "ETH_MDIO", dx=-5.08)

    s.stub_label(s.pin("U10", "13"), "ETH_MDC", dx=-20.32)

    rst_p = s.pin("U10", "15")
    s.add_comp("Device:R", "R30", "10k", 434.34, 223.52, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.0, -1.5), val_offset=(4.0, 1.5))
    s.stub_label(rst_p, "ETH_RST_N", dx=-20.32)
    s.pwr("+3V3", 434.34, 213.36)
    s.wire((434.34, 213.36), s.pin("R30", "1"))
    s.stub_label(s.pin("R30", "2"), "ETH_RST_N", dx=5.08)

    # 25MHz PHY Crystal Y3 + C41, C42 (below LAN8720A left)
    s.add_comp("Device:Crystal_GND24", "Y3", "25MHz", 388.62, 182.88, rot=0,
               footprint="Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm",
               ref_offset=(0, -6.0), val_offset=(0, -3.5))
    s.add_comp("Device:C", "C41", "18pF", 373.38, 198.12, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.0, -1.5), val_offset=(-5.0, 1.5))
    s.add_comp("Device:C", "C42", "18pF", 403.86, 198.12, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))

    x1_p, x2_p = s.pin("U10", "5"), s.pin("U10", "4")
    s.stub_label(x1_p, "PHY_XTAL1", dx=-12.70)
    s.stub_label(x2_p, "PHY_XTAL2", dx=-12.70)

    s.wire(s.pin("Y3", "1"), (373.38, 182.88))
    s.wire((373.38, 182.88), s.pin("C41", "1"))
    s.junction((373.38, 182.88))
    s.stub_label((373.38, 182.88), "PHY_XTAL1", dx=-7.62)
    s.wire(s.pin("Y3", "3"), (403.86, 182.88))
    s.wire((403.86, 182.88), s.pin("C42", "1"))
    s.junction((403.86, 182.88))
    s.stub_label((403.86, 182.88), "PHY_XTAL2", dx=7.62)
    y3_g = s.pin("Y3", "2")
    s.pin("Y3", "4")
    s.wire(s.pin("C41", "2"), (373.38, 208.28))
    s.wire(y3_g, (388.62, 208.28))
    s.wire(s.pin("C42", "2"), (403.86, 208.28))
    s.wire((373.38, 208.28), (403.86, 208.28))
    s.junction((388.62, 208.28))
    s.pwr("GND", 388.62, 208.28)

    # PHY Power: VDDIO(9), VDD1A(19), VDD2A(1) -> +3V3; VDDCR(6) -> C43/C44 + PWR_FLAG; VSS(25) -> GND
    p_vddio, p_vddcr, p_vdd1a, p_vdd2a = s.pin("U10", "9"), s.pin("U10", "6"), s.pin("U10", "19"), s.pin("U10", "1")
    s.wire(p_vddio, (p_vddio[0], 81.28))
    s.wire(p_vdd1a, (p_vdd1a[0], 81.28))
    s.wire(p_vdd2a, (p_vdd2a[0], 81.28))
    s.wire((p_vddio[0], 81.28), (p_vdd2a[0], 81.28))
    s.junction((p_vdd1a[0], 81.28))
    s.pwr("+3V3", p_vdd1a[0], 81.28)

    # VDDCR routed out neatly around LEFT of C43 (at x=363.22) so it NEVER crosses C43 pin 2 (GND)!
    s.wire(p_vddcr, (p_vddcr[0], 91.44))
    s.wire((p_vddcr[0], 91.44), (363.22, 91.44))
    s.wire((363.22, 91.44), (363.22, 58.42))
    s.wire((363.22, 58.42), (388.62, 58.42))
    s.add_comp("Device:C", "C43", "1uF", 373.38, 71.12, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.0, -1.5), val_offset=(-5.0, 1.5))
    s.add_comp("Device:C", "C44", "100nF", 388.62, 71.12, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.wire((373.38, 58.42), s.pin("C43", "1"))
    s.wire((388.62, 58.42), s.pin("C44", "1"))
    s.junction((373.38, 58.42))
    s.wire((373.38, 58.42), (373.38, 50.80))
    s.pwr_flag(373.38, 50.80)
    s.label("PHY_VDDCR", (388.62, 58.42))
    s.wire(s.pin("C43", "2"), (373.38, 81.28))
    s.wire(s.pin("C44", "2"), (388.62, 81.28))
    s.wire((373.38, 81.28), (388.62, 81.28))
    s.junction((388.62, 81.28))
    s.pwr("GND", 388.62, 81.28)

    # PHY Decoupling C45, C46, C47 (3x100nF on +3V3)
    for idx, cref in enumerate(["C45", "C46", "C47"]):
        cx = 469.90 + idx * 17.78
        s.add_comp("Device:C", cref, "100nF", cx, 66.04, rot=0,
                   footprint="Capacitor_SMD:C_0603_1608Metric",
                   ref_offset=(4.0, -1.5), val_offset=(4.0, 1.5))
        s.wire((cx, 53.34), s.pin(cref, "1"))
        s.wire(s.pin(cref, "2"), (cx, 78.74))
        if idx == 1:
            s.junction((cx, 53.34))
            s.junction((cx, 78.74))
    s.wire((469.90, 53.34), (505.46, 53.34))
    s.wire((469.90, 78.74), (505.46, 78.74))
    s.pwr("+3V3", 469.90, 53.34)
    s.junction((469.90, 53.34))
    s.pwr("GND", 469.90, 78.74)
    s.junction((469.90, 78.74))

    s.wire(s.pin("U10", "25"), (phy_x, 172.72))
    s.pwr("GND", phy_x, 172.72)

    # RBIAS (12.1k 1% to GND)
    s.add_comp("Device:R", "R21", "12.1k_1%", 469.90, 154.94, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.wire_L(s.pin("U10", "24"), s.pin("R21", "1"), horiz_first=True)
    s.wire(s.pin("R21", "2"), (469.90, 172.72))
    s.pwr("GND", 469.90, 172.72)

    # RJ45 MagJack J4 (HR911105A)
    rj_x, rj_y = 535.94, 134.62
    s.add_comp("Connector:RJ45_Hanrun_HR911105A_Horizontal", "J4", "HR911105A_RJ45", rj_x, rj_y, rot=0,
               footprint="Connector_RJ:RJ45_Hanrun_HR911105A_Horizontal",
               ref_offset=(0, -16), val_offset=(0, 19))

    # Connect MDI pairs (TXP->TD+, TXN->TD-, RXP->RD+, RXN->RD-) + 4x 49.9R 1% termination to +3V3
    mdi_pairs = [
        ("21", "1", "R31", "ETH_TXP", 472.44),
        ("20", "2", "R32", "ETH_TXN", 490.22),
        ("23", "3", "R33", "ETH_RXP", 508.00),
        ("22", "6", "R34", "ETH_RXN", 525.78),
    ]
    for phy_p, rj_p, rref, netn, tx_x in mdi_pairs:
        p_a = s.pin("U10", phy_p)
        p_b = s.pin("J4", rj_p)
        s.stub_label(p_a, netn, dx=10.16)
        s.stub_label(p_b, netn, dx=-10.16)
        s.add_comp("Device:R", rref, "49.9R", tx_x, 220.98, rot=0,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(4.2, -1.8), val_offset=(4.2, 1.8))
        s.wire((tx_x, 208.28), s.pin(rref, "1"))
        p2 = s.pin(rref, "2")
        s.wire(p2, (tx_x, p2[1] + 7.62))
        s.label(netn, (tx_x, p2[1] + 7.62), justify="left")
    s.wire((472.44, 208.28), (525.78, 208.28))
    for jx in [490.22, 508.00]:
        s.junction((jx, 208.28))
    s.pwr("+3V3", 472.44, 208.28)
    s.junction((472.44, 208.28))

    # Center Taps TCT (pin 4) & RCT (pin 5) -> +3V3 directly + C48/C49 (100nF to GND)
    j4_tct, j4_rct = s.pin("J4", "4"), s.pin("J4", "5")
    s.wire(j4_tct, (j4_tct[0] - 5.08, j4_tct[1]))
    s.wire((j4_tct[0] - 5.08, j4_tct[1]), (j4_tct[0] - 5.08, j4_tct[1] - 5.08))
    s.pwr("+3V3", j4_tct[0] - 5.08, j4_tct[1] - 5.08)
    s.wire(j4_rct, (j4_rct[0] - 5.08, j4_rct[1]))
    s.wire((j4_rct[0] - 5.08, j4_rct[1]), (j4_rct[0] - 5.08, j4_rct[1] + 7.62))
    s.pwr("+3V3", j4_rct[0] - 5.08, j4_rct[1] + 7.62, rot=180)
    for idx, (cref, cx) in enumerate([("C48", 482.60), ("C49", 508.00)]):
        s.add_comp("Device:C", cref, "100nF", cx, 266.70, rot=0,
                   footprint="Capacitor_SMD:C_0603_1608Metric",
                   ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
        s.pwr("+3V3", cx, 251.46)
        s.wire((cx, 251.46), s.pin(cref, "1"))
        s.wire(s.pin(cref, "2"), (cx, 279.40))
        s.pwr("GND", cx, 279.40)

    # RJ45 Shield (SH) & Chassis GND (pin 8) -> GND
    rj_g8, rj_sh = s.pin("J4", "8"), s.pin("J4", "SH")
    s.wire(rj_g8, (rj_g8[0], 157.48))
    s.wire(rj_sh, (rj_sh[0], 157.48))
    s.wire((rj_g8[0], 157.48), (rj_sh[0], 157.48))
    s.junction((rj_g8[0], 157.48))
    s.pwr("GND", rj_g8[0], 157.48)

    # RJ45 Status LEDs (LAN8720A LED1/LED2 -> R35/R36 270R -> RJ45 pins 11/9; pins 12/10 -> +3V3)
    s.stub_label(s.pin("U10", "3"), "ETH_LED1", dx=10.16)
    s.stub_label(s.pin("U10", "2"), "ETH_LED2", dx=10.16)
    for p_an, p_ka, rref, netn, ry, go_up in [
        ("12", "11", "R35", "ETH_LED1", 266.70, True),
        ("10", "9",  "R36", "ETH_LED2", 287.02, False),
    ]:
        pa = s.pin("J4", p_an)
        s.wire(pa, (pa[0] + 5.08, pa[1]))
        if go_up:
            s.wire((pa[0] + 5.08, pa[1]), (pa[0] + 5.08, pa[1] - 5.08))
            s.pwr("+3V3", pa[0] + 5.08, pa[1] - 5.08)
        else:
            s.wire((pa[0] + 5.08, pa[1]), (pa[0] + 5.08, pa[1] + 5.08))
            s.pwr("+3V3", pa[0] + 5.08, pa[1] + 5.08, rot=180)
        s.stub_label(s.pin("J4", p_ka), f"{netn}_K", dx=7.62)

        s.add_comp("Device:R", rref, "270R", 543.56, ry, rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -4.0), val_offset=(0, 4.0))
        s.stub_label(s.pin(rref, "1"), netn, dx=-7.62)
        s.stub_label(s.pin(rref, "2"), f"{netn}_K", dx=7.62)

    s.auto_nc_unused("U10")
    s.auto_nc_unused("J4")

    return s


# ==============================================================================
# SHEET 3: GALVANICALLY ISOLATED 4-CH DIGITAL INPUTS + 4-CH 24V RELAY OUTPUTS
# ==============================================================================
def build_sheet3_iso_io(lib: SymbolLibraryCache, root_uuid: str, sheet_uuid: str) -> Schematic:
    s = Schematic(
        title="Sheet 3 — Isolated 4-Ch DI & 4-Ch 24V Relays",
        lib_cache=lib,
        project_name=PROJECT_NAME,
        sheet_uuid=sheet_uuid,
        instance_path=f"/{root_uuid}/{sheet_uuid}",
        page_num="4",
        paper="A2",
        comment1="Galvanic Isolation: 0 Shared Refs (GND vs ISO_IO_GND)",
        comment2="4x PC817 24V DI + 4x PC817/2N7002/G5LE-1 SPDT Relays",
    )

    BARRIER_X = 213.36
    s.add_isolation_barrier(
        BARRIER_X, 24, 386,
        left_label="MCU LOGIC DOMAIN (+3V3 / GND)",
        right_label="ISOLATED FIELD I/O DOMAIN (ISO_IO_24V / ISO_IO_GND)",
        banner="GALVANIC ISOLATION BARRIER — 2.5 kVrms (0 SHARED REFS: GND vs ISO_IO_GND)",
    )

    # ------------------------------------------------------------------
    # ZONE 1 (TOP-RIGHT): ISOLATED 24V FIELD POWER INPUT & CONDITIONING
    # ------------------------------------------------------------------
    s.add_box(420, 32, 570, 148, "1. ISOLATED 24V FIELD I/O POWER INPUT",
              stroke_rgb=(180, 40, 20), fill_rgba=(255, 248, 245, 0.7))

    s.add_comp("Connector:Screw_Terminal_01x02", "J5", "ISO_IO_24V_IN", 441.96, 81.28, rot=180,
               footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-02_1x02_P5.08mm_Horizontal",
               ref_offset=(0, -9.5), val_offset=(0, 10.5))
    s.add_comp("Device:D_TVS", "D9", "SMBJ28A", 477.52, 99.06, rot=90,
               footprint="Diode_SMD:D_SMB",
               ref_offset=(5.0, -1.8), val_offset=(5.0, 1.8))
    s.add_comp("Device:C_Polarized", "C50", "100uF/50V", 508.00, 99.06, rot=0,
               footprint="Capacitor_SMD:CP_Elec_8x10",
               ref_offset=(5.0, -1.8), val_offset=(5.0, 1.8))
    s.add_comp("Device:C", "C51", "100nF/50V", 538.48, 99.06, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.5, -1.8), val_offset=(4.5, 1.8))

    # Note: J5 (rot=180) puts Pin 2 at TOP (y=78.74) and Pin 1 at BOTTOM (y=81.28)!
    j5_24v, j5_gnd = s.pin("J5", "2"), s.pin("J5", "1")
    s.wire(j5_24v, (553.72, j5_24v[1]))
    for cx, cref, pnum in [(477.52, "D9", "2"), (508.00, "C50", "1"), (538.48, "C51", "1")]:
        s.wire((cx, j5_24v[1]), s.pin(cref, pnum))
        s.junction((cx, j5_24v[1]))
    s.wire((553.72, j5_24v[1]), (553.72, 66.04))
    s.pwr("ISO_IO_24V", 553.72, 66.04)
    s.wire((462.28, j5_24v[1]), (462.28, 68.58))
    s.junction((462.28, j5_24v[1]))
    s.pwr_flag(462.28, 68.58)

    s.wire_L(j5_gnd, (462.28, 121.92), horiz_first=True)
    s.wire((462.28, 121.92), (553.72, 121.92))
    for cx, cref, pnum in [(477.52, "D9", "1"), (508.00, "C50", "2"), (538.48, "C51", "2")]:
        s.wire(s.pin(cref, pnum), (cx, 121.92))
        s.junction((cx, 121.92))
    s.wire((553.72, 121.92), (553.72, 134.62))
    s.pwr("ISO_IO_GND", 553.72, 134.62)
    s.wire((462.28, 121.92), (462.28, 132.08))
    s.junction((462.28, 121.92))
    s.pwr_flag(462.28, 132.08, rot=180)

    # ------------------------------------------------------------------
    # ZONE 2 (TOP): 4-CH OPTICALLY ISOLATED 24V DIGITAL INPUTS (DI1..DI4)
    # ------------------------------------------------------------------
    s.add_box(22, 32, 412, 198, "2. 4-CH OPTICALLY ISOLATED 24V DIGITAL INPUTS (DI1 - DI4)",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.35))

    # J6: 6-pin Digital Input Terminal (DI1..DI4, ISO_IO_GND, ISO_IO_24V)
    s.add_comp("Connector:Screw_Terminal_01x06", "J6", "J_DI_1_4", 393.70, 109.22, rot=0,
               footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-06_1x06_P5.08mm_Horizontal",
               ref_offset=(0, -14), val_offset=(0, 16))
    for ch in range(1, 5):
        s.stub_label(s.pin("J6", str(ch)), f"DI{ch}_FIELD", dx=-12.70)
    j6_5 = s.pin("J6", "5")
    s.wire(j6_5, (j6_5[0] - 7.62, j6_5[1]))
    s.wire((j6_5[0] - 7.62, j6_5[1]), (j6_5[0] - 7.62, j6_5[1] + 7.62))
    s.pwr("ISO_IO_GND", j6_5[0] - 7.62, j6_5[1] + 7.62)
    j6_6 = s.pin("J6", "6")
    s.wire(j6_6, (j6_6[0] - 17.78, j6_6[1]))
    s.wire((j6_6[0] - 17.78, j6_6[1]), (j6_6[0] - 17.78, j6_6[1] + 10.16))
    s.pwr("ISO_IO_24V", j6_6[0] - 17.78, j6_6[1] + 10.16, rot=180)

    # 4 DI Channels stacked cleanly vertically, PC817 mirrored horizontally (mirror y) so LED faces RIGHT (Field) and Transistor faces LEFT (MCU)!
    for idx in range(4):
        ch = idx + 1
        cy = 58.42 + idx * 35.56
        u_ref = f"U{10 + ch}"  # U11..U14
        r_a = f"R{36 + idx * 4 + 1}"
        r_k = f"R{36 + idx * 4 + 2}"
        r_pu = f"R{36 + idx * 4 + 3}"
        r_rc = f"R{36 + idx * 4 + 4}"
        d_rev = f"D{9 + ch}"   # D10..D13
        c_rc = f"C{51 + ch}"   # C52..C55

        # PC817 mirrored Y: pin 1(A) & 2(K) on RIGHT (+7.62), pin 4(C) & 3(E) on LEFT (-7.62)
        s.add_comp("Isolator:PC817", u_ref, "PC817", BARRIER_X, cy, rot=0, mirror="y",
                   footprint="Package_DIP:DIP-4_W7.62mm",
                   ref_offset=(-9.0, -7.0), val_offset=(-9.0, 7.0))
        p_a, p_k = s.pin(u_ref, "1"), s.pin(u_ref, "2")
        p_c, p_e = s.pin(u_ref, "4"), s.pin(u_ref, "3")

        # FIELD SIDE (Right of barrier):
        # Anti-parallel reverse diode D_rev across pins 1 & 2 (rot=270: K top, A bottom)
        s.add_comp("Diode:1N4148W", d_rev, "1N4148W", 238.76, cy, rot=270,
                   footprint="Diode_SMD:D_SOD-123",
                   ref_offset=(4.8, -1.5), val_offset=(4.8, 1.5))
        # Balanced input current-limiting resistors R_a (2.2k) and R_k (2.2k -> ISO_IO_GND)
        # NOTE: R_k separates PC817 pin 2 from ISO_IO_GND => 0 shared refs between GND & ISO_IO_GND!
        s.add_comp("Device:R", r_a, "2.2k", 274.32, cy - 2.54, rot=90,
                   footprint="Resistor_SMD:R_0805_2012Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.add_comp("Device:R", r_k, "2.2k", 284.48, cy + 7.62, rot=90,
                   footprint="Resistor_SMD:R_0805_2012Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))

        s.wire(p_a, s.pin(r_a, "1"))
        s.wire((238.76, cy - 2.54), s.pin(d_rev, "1"))
        s.junction((238.76, cy - 2.54))
        s.stub_label(s.pin(r_a, "2"), f"DI{ch}_FIELD", dx=10.16)

        s.wire(p_k, (238.76, cy + 2.54))
        s.wire((238.76, cy + 2.54), s.pin(d_rev, "2"))
        s.junction((238.76, cy + 2.54))
        s.wire_L((238.76, cy + 2.54), s.pin(r_k, "1"), horiz_first=False)
        rk2 = s.pin(r_k, "2")
        s.wire(rk2, (rk2[0] + 7.62, rk2[1]))
        s.wire((rk2[0] + 7.62, rk2[1]), (rk2[0] + 7.62, rk2[1] + 5.08))
        s.pwr("ISO_IO_GND", rk2[0] + 7.62, rk2[1] + 5.08)

        # MCU SIDE (Left of barrier):
        # Emitter -> GND; Collector -> R_pu (4.7k to +3V3) -> R_rc (1k) -> C_rc (100nF) -> DIx_MCU
        s.wire(p_e, (195.58, p_e[1]))
        s.wire((195.58, p_e[1]), (195.58, p_e[1] + 7.62))
        s.pwr("GND", 195.58, p_e[1] + 7.62)

        s.add_comp("Device:R", r_pu, "4.7k", 172.72, cy - 10.16, rot=0,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(-4.5, -1.5), val_offset=(-4.5, 1.5))
        s.add_comp("Device:R", r_rc, "1k", 147.32, cy - 2.54, rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.add_comp("Device:C", c_rc, "100nF", 121.92, cy + 5.08, rot=0,
                   footprint="Capacitor_SMD:C_0603_1608Metric",
                   ref_offset=(-5.0, -1.5), val_offset=(-5.0, 1.5))

        s.wire(p_c, s.pin(r_rc, "2"))
        s.wire((172.72, cy - 2.54), s.pin(r_pu, "2"))
        s.junction((172.72, cy - 2.54))
        s.pwr("+3V3", 172.72, cy - 17.78)
        s.wire((172.72, cy - 17.78), s.pin(r_pu, "1"))

        s.wire(s.pin(r_rc, "1"), (121.92, cy - 2.54))
        s.wire((121.92, cy - 2.54), s.pin(c_rc, "1"))
        s.junction((121.92, cy - 2.54))
        s.wire(s.pin(c_rc, "2"), (121.92, cy + 12.70))
        s.pwr("GND", 121.92, cy + 12.70)
        s.stub_label((121.92, cy - 2.54), f"DI{ch}_MCU", dx=-25.40, hier=True, shape="output")

    # ------------------------------------------------------------------
    # ZONE 3 (BOTTOM): 4-CH OPTICALLY ISOLATED 24V RELAY OUTPUTS (DO1..DO4)
    # ------------------------------------------------------------------
    s.add_box(22, 206, 570, 386, "3. 4-CH OPTICALLY ISOLATED 24V RELAY OUTPUTS (DO1 - DO4, SPDT G5LE-1)",
              stroke_rgb=(180, 40, 20), fill_rgba=(255, 248, 245, 0.35))

    # Output Terminal Blocks J7 (RLY1 & RLY2), J8 (RLY3 & RLY4)
    s.add_comp("Connector:Screw_Terminal_01x06", "J7", "J_RLY_1_2", 548.64, 256.54, rot=0,
               footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-06_1x06_P5.08mm_Horizontal",
               ref_offset=(0, -14), val_offset=(0, 16))
    s.add_comp("Connector:Screw_Terminal_01x06", "J8", "J_RLY_3_4", 548.64, 337.82, rot=0,
               footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-06_1x06_P5.08mm_Horizontal",
               ref_offset=(0, -14), val_offset=(0, 16))

    j7_nets = ["RLY1_COM", "RLY1_NO", "RLY1_NC", "RLY2_COM", "RLY2_NO", "RLY2_NC"]
    j8_nets = ["RLY3_COM", "RLY3_NO", "RLY3_NC", "RLY4_COM", "RLY4_NO", "RLY4_NC"]
    for i, nm in enumerate(j7_nets):
        s.stub_label(s.pin("J7", str(i + 1)), nm, dx=-12.70)
    for i, nm in enumerate(j8_nets):
        s.stub_label(s.pin("J8", str(i + 1)), nm, dx=-12.70)

    for idx in range(4):
        ch = idx + 1
        cy = 231.14 + idx * 40.64
        u_ref = f"U{14 + ch}"   # U15..U18
        r_led = f"R{52 + idx * 3 + 1}"
        r_g1 = f"R{52 + idx * 3 + 2}"
        r_g2 = f"R{52 + idx * 3 + 3}"
        q_ref = f"Q{ch}"        # Q1..Q4
        k_ref = f"K{ch}"        # K1..K4
        d_fly = f"D{13 + ch}"   # D14..D17

        # PC817 standard orientation: LED on LEFT (MCU), phototransistor on RIGHT (Field)
        s.add_comp("Isolator:PC817", u_ref, "PC817", BARRIER_X, cy, rot=0,
                   footprint="Package_DIP:DIP-4_W7.62mm",
                   ref_offset=(-9.0, -7.0), val_offset=(-9.0, 7.0))
        p_a, p_k = s.pin(u_ref, "1"), s.pin(u_ref, "2")
        p_c, p_e = s.pin(u_ref, "4"), s.pin(u_ref, "3")

        # MCU SIDE (Left): DOx_MCU -> R_led (330R) -> pin 1; pin 2 -> GND
        s.add_comp("Device:R", r_led, "330R", 167.64, cy - 2.54, rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.wire(s.pin(r_led, "2"), p_a)
        s.stub_label(s.pin(r_led, "1"), f"DO{ch}_MCU", dx=-25.40, hier=True, shape="input")
        s.wire(p_k, (195.58, p_k[1]))
        s.wire((195.58, p_k[1]), (195.58, p_k[1] + 7.62))
        s.pwr("GND", 195.58, p_k[1] + 7.62)

        # FIELD SIDE (Right):
        # Collector -> R_g1 (10k) to ISO_IO_24V; Emitter -> Gate of 2N7002 + R_g2 (10k to ISO_IO_GND)
        # NOTE: PC817 pin 3 connects to RLY_GATE, NOT ISO_IO_GND => 0 shared refs between GND & ISO_IO_GND!
        s.add_comp("Device:R", r_g1, "10k", 248.92, cy - 2.54, rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.add_comp("Device:R", r_g2, "10k", 279.40, cy + 12.70, rot=0,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(-4.5, -1.5), val_offset=(-4.5, 1.5))
        s.add_comp("Transistor_FET:2N7002", q_ref, "2N7002", 312.42, cy + 2.54, rot=0,
                   footprint="Package_TO_SOT_SMD:SOT-23",
                   ref_offset=(7.5, -2.5), val_offset=(7.5, 2.5))

        s.wire(p_c, s.pin(r_g1, "1"))
        rg1_2 = s.pin(r_g1, "2")
        s.wire(rg1_2, (rg1_2[0] + 5.08, rg1_2[1]))
        s.wire((rg1_2[0] + 5.08, rg1_2[1]), (rg1_2[0] + 5.08, rg1_2[1] - 5.08))
        s.pwr("ISO_IO_24V", rg1_2[0] + 5.08, rg1_2[1] - 5.08)

        q_g, q_d, q_s = s.pin(q_ref, "1"), s.pin(q_ref, "3"), s.pin(q_ref, "2")
        s.wire(p_e, q_g)
        s.wire((279.40, p_e[1]), s.pin(r_g2, "1"))
        s.junction((279.40, p_e[1]))
        s.wire(s.pin(r_g2, "2"), (279.40, cy + 20.32))
        s.pwr("ISO_IO_GND", 279.40, cy + 20.32)

        s.wire(q_s, (q_s[0], cy + 20.32))
        s.pwr("ISO_IO_GND", q_s[0], cy + 20.32)

        # Flyback Diode D_fly (1N4148W, rot=270: K top, A bottom) + Relay K (G5LE-1)
        s.add_comp("Diode:1N4148W", d_fly, "1N4148W", 347.98, cy, rot=270,
                   footprint="Diode_SMD:D_SOD-123",
                   ref_offset=(-5.5, -1.5), val_offset=(-5.5, 1.5))
        s.add_comp("Relay:G5LE-1", k_ref, "G5LE-1-DC24", 398.78, cy, rot=0,
                   footprint="Relay_THT:Relay_SPDT_Omron-G5LE-1",
                   ref_offset=(-12, -10.5), val_offset=(-12, 11.5))

        k_c1, k_c2 = s.pin(k_ref, "5"), s.pin(k_ref, "2")  # Coil pins (top & bottom)
        k_com, k_no, k_nc = s.pin(k_ref, "1"), s.pin(k_ref, "3"), s.pin(k_ref, "4")

        # Coil top -> ISO_IO_24V + Flyback K
        s.wire(k_c1, (347.98, k_c1[1]))
        s.wire((347.98, k_c1[1]), s.pin(d_fly, "1"))
        s.junction((347.98, k_c1[1]))
        s.wire((347.98, k_c1[1]), (347.98, k_c1[1] - 5.08))
        s.pwr("ISO_IO_24V", 347.98, k_c1[1] - 5.08)

        # Coil bottom -> Drain of 2N7002 + Flyback A
        s.wire(k_c2, (347.98, k_c2[1]))
        s.wire((347.98, k_c2[1]), s.pin(d_fly, "2"))
        s.junction((347.98, k_c2[1]))
        s.wire_L(q_d, (347.98, k_c2[1]), horiz_first=True)

        # Contacts -> Net labels matching J7/J8
        s.stub_label(k_com, f"RLY{ch}_COM", dx=12.70)
        s.wire(k_no, (k_no[0], k_no[1] - 2.54))
        s.stub_label((k_no[0], k_no[1] - 2.54), f"RLY{ch}_NO", dx=15.24)
        s.wire(k_nc, (k_nc[0], k_nc[1] - 5.08))
        s.stub_label((k_nc[0], k_nc[1] - 5.08), f"RLY{ch}_NC", dx=22.86)

    return s


# ==============================================================================
# SHEET 4: 2-CH ANALOG FRONT-END + ISOLATED RS-485 & CAN BUS
# ==============================================================================
def build_sheet4_analog_bus(lib: SymbolLibraryCache, root_uuid: str, sheet_uuid: str) -> Schematic:
    s = Schematic(
        title="Sheet 4 — Analog Front-End & Isolated Fieldbus",
        lib_cache=lib,
        project_name=PROJECT_NAME,
        sheet_uuid=sheet_uuid,
        instance_path=f"/{root_uuid}/{sheet_uuid}",
        page_num="5",
        paper="A2",
        comment1="2-Ch 0-10V/4-20mA FE (BAT54S + LM358, +3V3A)",
        comment2="Isolated RS-485 (LTC2850IS8) & CAN (MCP2551, ISO_BUS_GND)",
    )

    BARRIER_X = 314.96
    s.add_isolation_barrier(
        BARRIER_X, 24, 386,
        left_label="ANALOG & MCU DOMAIN (+3V3A / +3V3 / GND)",
        right_label="ISOLATED FIELDBUS DOMAIN (ISO_BUS_5V / ISO_BUS_GND)",
        banner="GALVANIC ISOLATION BARRIER — 2.5 kVrms (0 SHARED REFS: GND vs ISO_BUS_GND)",
    )

    # ------------------------------------------------------------------
    # ZONE 1 (TOP-LEFT): 2-CH INDUSTRIAL ANALOG FRONT-END (AIN1, AIN2)
    # ------------------------------------------------------------------
    s.add_box(22, 32, 296, 178, "1. 2-CH 0-10V / 4-20mA ANALOG FRONT-END (+3V3A, BAT54S CLAMP, LM358 BUFFER)",
              stroke_rgb=(30, 80, 140), fill_rgba=(245, 250, 255, 0.7))

    # LM358 Unit 3 (Power) + Decoupling C56, C57
    s.add_comp("Amplifier_Operational:LM358", "U19", "LM358", 259.08, 63.50, rot=0, unit=3,
               footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
               ref_offset=(4.5, -2.0), val_offset=(4.5, 2.0))
    s.add_comp("Device:C", "C56", "100nF", 279.40, 63.50, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:C", "C57", "1uF", 279.40, 106.68, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    u19_vp, u19_vm = s.pin("U19", "8", unit=3), s.pin("U19", "4", unit=3)
    s.wire(u19_vp, (u19_vp[0], 50.80))
    s.wire((u19_vp[0], 50.80), (279.40, 50.80))
    s.wire((279.40, 50.80), s.pin("C56", "1"))
    s.pwr("+3V3A", u19_vp[0], 50.80)
    s.junction((u19_vp[0], 50.80))
    s.wire(u19_vm, (u19_vm[0], 76.20))
    s.wire((u19_vm[0], 76.20), (279.40, 76.20))
    s.wire((279.40, 76.20), s.pin("C56", "2"))
    s.pwr("GND", u19_vm[0], 76.20)
    s.junction((u19_vm[0], 76.20))

    s.pwr("+3V3A", 279.40, 96.52)
    s.wire((279.40, 96.52), s.pin("C57", "1"))
    s.wire(s.pin("C57", "2"), (279.40, 116.84))
    s.pwr("GND", 279.40, 116.84)

    for idx in range(2):
        ch = idx + 1
        cy = 73.66 + idx * 66.04
        j_ref = f"J{8 + ch}"    # J9, J10
        r_s1 = f"R{64 + idx * 4 + 1}"
        r_b = f"R{64 + idx * 4 + 2}"
        r_f1 = f"R{64 + idx * 4 + 3}"
        r_f2 = f"R{64 + idx * 4 + 4}"
        c_1 = f"C{58 + idx * 2 + 1}"  # C59, C61
        c_2 = f"C{58 + idx * 2 + 2}"  # C60, C62
        d_cl = f"D{17 + ch}"          # D18, D19

        # Terminal Block J_AINx
        s.add_comp("Connector:Screw_Terminal_01x02", j_ref, f"J_AIN{ch}", 38.10, cy, rot=180,
                   footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-02_1x02_P5.08mm_Horizontal",
                   ref_offset=(0, -8.5), val_offset=(0, 9.5))
        j_sig, j_gnd = s.pin(j_ref, "1"), s.pin(j_ref, "2")
        s.wire(j_gnd, (48.26, j_gnd[1]))
        s.wire((48.26, j_gnd[1]), (48.26, cy + 25.40))
        s.pwr("GND", 48.26, cy + 25.40)

        # Precision Divider R_s1 (10k 0.1%) + Shunt/Burden R_b (4.3k 0.1%)
        s.add_comp("Device:R", r_s1, "10k_0.1%", 68.58, cy, rot=90,
                   footprint="Resistor_SMD:R_0805_2012Metric",
                   ref_offset=(0, -3.8), val_offset=(0, 3.8))
        s.add_comp("Device:R", r_b, "4.3k_0.1%", 86.36, cy + 15.24, rot=0,
                   footprint="Resistor_SMD:R_0805_2012Metric",
                   ref_offset=(6.0, -1.8), val_offset=(6.0, 1.8))
        s.wire(j_sig, s.pin(r_s1, "1"))
        s.wire(s.pin(r_s1, "2"), (114.30, cy))
        s.wire((86.36, cy), s.pin(r_b, "1"))
        s.junction((86.36, cy))
        s.wire(s.pin(r_b, "2"), (86.36, cy + 25.40))
        s.pwr("GND", 86.36, cy + 25.40)

        # BAT54S Dual Schottky Clamp (rot=90: pin 1(A) bottom -> GND, pin 2(K) top -> +3V3A, pin 3(COM) right -> signal!)
        # Check BAT54S rot=90: pin1 (-7.62,0) -> (0, -7.62) in sym -> (cx, cy+7.62) BOTTOM; pin2 (+7.62,0) -> (cx, cy-7.62) TOP; pin3 (0,-5.08) -> (+5.08,0) -> RIGHT!
        s.add_comp("Diode:BAT54S", d_cl, "BAT54S", 109.22, cy, rot=90,
                   footprint="Package_TO_SOT_SMD:SOT-23",
                   ref_offset=(-7.5, -2.0), val_offset=(-7.5, 2.0))
        d_a, d_k, d_com = s.pin(d_cl, "1"), s.pin(d_cl, "2"), s.pin(d_cl, "3")
        s.wire(d_k, (d_k[0], cy - 12.70))
        s.pwr("+3V3A", d_k[0], cy - 12.70)
        s.wire(d_a, (d_a[0], cy + 25.40))
        s.pwr("GND", d_a[0], cy + 25.40)
        s.junction((114.30, cy))

        # 1st-stage RC anti-aliasing filter: R_f1 (1k) + C_1 (10nF)
        s.add_comp("Device:R", r_f1, "1k", 132.08, cy, rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.add_comp("Device:C", c_1, "10nF", 149.86, cy + 15.24, rot=0,
                   footprint="Capacitor_SMD:C_0603_1608Metric",
                   ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
        s.wire(d_com, s.pin(r_f1, "1"))
        s.wire(s.pin(r_f1, "2"), (149.86, cy))
        s.wire((149.86, cy), s.pin(c_1, "1"))
        s.junction((149.86, cy))
        s.wire(s.pin(c_1, "2"), (149.86, cy + 25.40))
        s.pwr("GND", 149.86, cy + 25.40)

        # Op-Amp Buffer (LM358 unit ch)
        oa_x, oa_y = 177.80, cy + 2.54
        s.add_comp("Amplifier_Operational:LM358", "U19", "LM358", oa_x, oa_y, rot=0, unit=ch,
                   footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
                   ref_offset=(0, -9.5), val_offset=(0, 9.5))
        p_plus = s.pin("U19", "3" if ch == 1 else "5", unit=ch)
        p_minus = s.pin("U19", "2" if ch == 1 else "6", unit=ch)
        p_out = s.pin("U19", "1" if ch == 1 else "7", unit=ch)

        s.wire((149.86, cy), p_plus)
        # Unity-gain feedback loop below op-amp
        fb_y = cy + 15.24
        s.wire(p_minus, (p_minus[0] - 5.08, p_minus[1]))
        s.wire((p_minus[0] - 5.08, p_minus[1]), (p_minus[0] - 5.08, fb_y))
        s.wire((p_minus[0] - 5.08, fb_y), (p_out[0] + 5.08, fb_y))
        s.wire((p_out[0] + 5.08, fb_y), (p_out[0] + 5.08, p_out[1]))
        s.wire(p_out, (p_out[0] + 5.08, p_out[1]))
        s.junction((p_out[0] + 5.08, p_out[1]))

        # 2nd-stage RC output filter: R_f2 (100R) + C_2 (10nF) -> AINx_ADC
        s.add_comp("Device:R", r_f2, "100R", 208.28, p_out[1], rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.add_comp("Device:C", c_2, "10nF", 226.06, p_out[1] + 12.70, rot=0,
                   footprint="Capacitor_SMD:C_0603_1608Metric",
                   ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
        s.wire((p_out[0] + 5.08, p_out[1]), s.pin(r_f2, "1"))
        s.wire(s.pin(r_f2, "2"), (226.06, p_out[1]))
        s.wire((226.06, p_out[1]), s.pin(c_2, "1"))
        s.junction((226.06, p_out[1]))
        s.wire(s.pin(c_2, "2"), (226.06, cy + 25.40))
        s.pwr("GND", 226.06, cy + 25.40)
        s.stub_label((226.06, p_out[1]), f"AIN{ch}_ADC", dx=12.70, hier=True, shape="output")

    # ------------------------------------------------------------------
    # ZONE 2 (TOP-RIGHT): ISOLATED BUS 5V DECOUPLING & INDICATOR
    # ------------------------------------------------------------------
    s.add_box(336, 32, 570, 96, "2. ISOLATED BUS POWER CONDITIONING (ISO_BUS_5V / ISO_BUS_GND)",
              stroke_rgb=(180, 40, 20), fill_rgba=(255, 248, 245, 0.7))

    s.add_comp("Device:C", "C66", "10uF", 373.38, 66.04, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:C", "C67", "100nF", 403.86, 66.04, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:R", "R73", "1k", 447.04, 60.96, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:LED", "D20", "LED_ISO_BUS", 447.04, 76.20, rot=90,
               footprint="LED_SMD:LED_0603_1608Metric",
               ref_offset=(4.5, -1.5), val_offset=(4.5, 1.5))

    s.wire((373.38, 50.80), (447.04, 50.80))
    s.wire((373.38, 50.80), s.pin("C66", "1"))
    s.wire((403.86, 50.80), s.pin("C67", "1"))
    s.wire((447.04, 50.80), s.pin("R73", "1"))
    s.junction((403.86, 50.80))
    s.pwr("ISO_BUS_5V", 373.38, 50.80)
    s.junction((373.38, 50.80))

    s.wire(s.pin("R73", "2"), s.pin("D20", "2"))
    s.wire((373.38, 86.36), (447.04, 86.36))
    s.wire(s.pin("C66", "2"), (373.38, 86.36))
    s.wire(s.pin("C67", "2"), (403.86, 86.36))
    s.wire(s.pin("D20", "1"), (447.04, 86.36))
    s.junction((403.86, 86.36))
    s.pwr("ISO_BUS_GND", 373.38, 86.36)
    s.junction((373.38, 86.36))
    # Local 3.3V LDO on isolated bus side (U27: ISO_BUS_5V -> ISO_BUS_3V3 for 3.3V LTC2850xS8 RS-485!)
    s.add_comp("Regulator_Linear:AMS1117-3.3", "U27", "AMS1117-3.3", 500.38, 63.50, rot=0,
               footprint="Package_TO_SOT_SMD:SOT-223-3_TabPin2",
               ref_offset=(0, -9.5), val_offset=(0, -6.5))
    s.add_comp("Device:C", "C68", "10uF", 533.40, 73.66, rot=0,
               footprint="Capacitor_SMD:C_0805_2012Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:C", "C69", "100nF", 553.72, 73.66, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.wire_L((447.04, 50.80), s.pin("U27", "3"), horiz_first=True)
    s.junction((447.04, 50.80))
    s.wire(s.pin("U27", "1"), (500.38, 86.36))
    s.wire((447.04, 86.36), (553.72, 86.36))
    s.junction((447.04, 86.36))
    s.junction((500.38, 86.36))
    s.wire(s.pin("U27", "2"), (553.72, 63.50))
    s.wire((533.40, 63.50), s.pin("C68", "1"))
    s.junction((533.40, 63.50))
    s.wire(s.pin("C68", "2"), (533.40, 86.36))
    s.junction((533.40, 86.36))
    s.wire((553.72, 63.50), s.pin("C69", "1"))
    s.wire(s.pin("C69", "2"), (553.72, 86.36))
    s.label("ISO_BUS_3V3", (520.70, 63.50))

    # ------------------------------------------------------------------
    # ZONE 3 (MIDDLE): ISOLATED RS-485 PORT (3x PC817 + LTC2850IS8 + 120R + BIAS + TVS)
    # ------------------------------------------------------------------
    s.add_box(22, 188, 570, 290, "3. ISOLATED RS-485 TRANSCEIVER (OPTO-ISOLATED LTC2850IS8 + 120R TERM + FAIL-SAFE BIAS + TVS)",
              stroke_rgb=(180, 40, 20), fill_rgba=(255, 248, 245, 0.35))

    # Helper for active-low optocouplers across barrier (GUARANTEES 0 shared refs between GND & ISO_BUS_GND!)
    def add_tx_opto(uref, r_led_ref, r_pu_ref, cy, mcu_sig, bus_sig):
        # MCU -> Bus: LED on LEFT (pin1 <- 330R from +3V3, pin2 <- mcu_sig); Transistor on RIGHT (pin4 <- 4.7k from ISO_BUS_5V -> bus_sig, pin3 -> ISO_BUS_GND)
        # Touches ISO_BUS_GND only, NEVER GND!
        s.add_comp("Isolator:PC817", uref, "PC817", BARRIER_X, cy, rot=0,
                   footprint="Package_DIP:DIP-4_W7.62mm",
                   ref_offset=(-9.0, -6.5), val_offset=(-9.0, 6.5))
        s.add_comp("Device:R", r_led_ref, "330R", 279.40, cy - 2.54, rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.add_comp("Device:R", r_pu_ref, "4.7k", 347.98, cy - 10.16, rot=0,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
        p_a, p_k = s.pin(uref, "1"), s.pin(uref, "2")
        p_c, p_e = s.pin(uref, "4"), s.pin(uref, "3")

        s.wire(s.pin(r_led_ref, "2"), p_a)
        rl1 = s.pin(r_led_ref, "1")
        s.wire(rl1, (rl1[0] - 5.08, rl1[1]))
        s.wire((rl1[0] - 5.08, rl1[1]), (rl1[0] - 5.08, rl1[1] - 5.08))
        s.pwr("+3V3", rl1[0] - 5.08, rl1[1] - 5.08)
        s.stub_label(p_k, mcu_sig, dx=-50.80, hier=True, shape="input")

        s.wire(p_c, (347.98, p_c[1]))
        s.wire((347.98, p_c[1]), s.pin(r_pu_ref, "2"))
        s.junction((347.98, p_c[1]))
        s.pwr("ISO_BUS_5V", 347.98, cy - 17.78)
        s.wire((347.98, cy - 17.78), s.pin(r_pu_ref, "1"))
        s.stub_label((347.98, p_c[1]), bus_sig, dx=12.70)

        s.wire(p_e, (332.74, p_e[1]))
        s.wire((332.74, p_e[1]), (332.74, p_e[1] + 5.08))
        s.pwr("ISO_BUS_GND", 332.74, p_e[1] + 5.08)

    def add_rx_opto(uref, r_led_ref, r_pu_ref, cy, mcu_sig, bus_sig):
        # Bus -> MCU: mirror="y" so LED on RIGHT (pin1 <- 470R from ISO_BUS_5V, pin2 <- bus_sig); Transistor on LEFT (pin4 <- 4.7k from +3V3 -> mcu_sig, pin3 -> GND)
        # Touches GND only, NEVER ISO_BUS_GND!
        s.add_comp("Isolator:PC817", uref, "PC817", BARRIER_X, cy, rot=0, mirror="y",
                   footprint="Package_DIP:DIP-4_W7.62mm",
                   ref_offset=(-9.0, -6.5), val_offset=(-9.0, 6.5))
        s.add_comp("Device:R", r_led_ref, "470R", 347.98, cy - 2.54, rot=90,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(0, -3.5), val_offset=(0, 3.5))
        s.add_comp("Device:R", r_pu_ref, "4.7k", 279.40, cy - 10.16, rot=0,
                   footprint="Resistor_SMD:R_0603_1608Metric",
                   ref_offset=(-4.5, -1.5), val_offset=(-4.5, 1.5))
        p_a, p_k = s.pin(uref, "1"), s.pin(uref, "2")
        p_c, p_e = s.pin(uref, "4"), s.pin(uref, "3")

        s.wire(p_a, s.pin(r_led_ref, "1"))
        rl2 = s.pin(r_led_ref, "2")
        s.wire(rl2, (rl2[0] + 5.08, rl2[1]))
        s.wire((rl2[0] + 5.08, rl2[1]), (rl2[0] + 5.08, rl2[1] - 5.08))
        s.pwr("ISO_BUS_5V", rl2[0] + 5.08, rl2[1] - 5.08)
        s.stub_label(p_k, bus_sig, dx=25.40)

        s.wire(p_c, (279.40, p_c[1]))
        s.wire((279.40, p_c[1]), s.pin(r_pu_ref, "2"))
        s.junction((279.40, p_c[1]))
        s.pwr("+3V3", 279.40, cy - 17.78)
        s.wire((279.40, cy - 17.78), s.pin(r_pu_ref, "1"))
        s.stub_label((279.40, p_c[1]), mcu_sig, dx=-22.86, hier=True, shape="output")

        s.wire(p_e, (297.18, p_e[1]))
        s.wire((297.18, p_e[1]), (297.18, p_e[1] + 5.08))
        s.pwr("GND", 297.18, p_e[1] + 5.08)

    # RS-485 Optocouplers (U22=TX, U23=DE, U24=RX)
    add_tx_opto("U22", "R74", "R75", 213.36, "RS485_TX", "ISO_485_DI")
    add_tx_opto("U23", "R76", "R77", 243.84, "RS485_DE", "ISO_485_DE")
    add_rx_opto("U24", "R78", "R79", 274.32, "RS485_RX", "ISO_485_RO")

    # U20: LTC2850xS8 RS-485 Transceiver (3.3V ISO_BUS_3V3) + C63 (100nF) + Bus Termination/Bias/TVS + J11
    s.add_comp("Interface_UART:LTC2850xS8", "U20", "LTC2850IS8", 424.18, 241.30, rot=0,
               footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
               ref_offset=(4, -17), val_offset=(4, 17))
    s.add_comp("Device:C", "C63", "100nF", 381.00, 241.30, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.5, -1.5), val_offset=(-5.5, 1.5))

    s.wire(s.pin("U20", "8"), (424.18, 220.98))
    s.wire((424.18, 220.98), (381.00, 220.98))
    s.wire((381.00, 220.98), s.pin("C63", "1"))
    s.label("ISO_BUS_3V3", (424.18, 220.98))
    s.junction((424.18, 220.98))

    s.wire(s.pin("U20", "5"), (424.18, 261.62))
    s.wire((424.18, 261.62), (381.00, 261.62))
    s.wire((381.00, 261.62), s.pin("C63", "2"))
    s.pwr("ISO_BUS_GND", 424.18, 261.62)
    s.junction((424.18, 261.62))

    s.stub_label(s.pin("U20", "1"), "ISO_485_RO", dx=-12.70)
    u20_re, u20_de = s.pin("U20", "2"), s.pin("U20", "3")
    s.wire(u20_re, (u20_re[0] - 5.08, u20_re[1]))
    s.wire(u20_de, (u20_de[0] - 5.08, u20_de[1]))
    s.wire((u20_re[0] - 5.08, u20_re[1]), (u20_de[0] - 5.08, u20_de[1]))
    s.junction((u20_de[0] - 5.08, u20_de[1]))
    s.stub_label((u20_de[0] - 5.08, u20_de[1]), "ISO_485_DE", dx=-10.16)
    s.stub_label(s.pin("U20", "4"), "ISO_485_DI", dx=-12.70)

    u20_a, u20_b = s.pin("U20", "6"), s.pin("U20", "7")  # y=233.68, y=238.76
    # Bus termination R80(120R), fail-safe bias R81(680R to +5V), R82(680R to GND), TVS D21/D22, J11
    s.add_comp("Device:R", "R80", "120R", 472.44, 241.30, rot=0,
               footprint="Resistor_SMD:R_0805_2012Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:R", "R81", "680R", 454.66, 218.44, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:R", "R82", "680R", 454.66, 261.62, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:D_TVS", "D21", "SMBJ12A", 495.30, 264.16, rot=90,
               footprint="Diode_SMD:D_SMB",
               ref_offset=(4.5, -1.5), val_offset=(4.5, 1.5))
    s.add_comp("Device:D_TVS", "D22", "SMBJ12A", 518.16, 264.16, rot=90,
               footprint="Diode_SMD:D_SMB",
               ref_offset=(4.5, -1.5), val_offset=(4.5, 1.5))
    s.add_comp("Connector:Screw_Terminal_01x03", "J11", "J_RS485", 548.64, 246.38, rot=0,
               footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-03_1x03_P5.08mm_Horizontal",
               ref_offset=(0, -10.5), val_offset=(0, 11.5))

    j11_a, j11_b, j11_g = s.pin("J11", "1"), s.pin("J11", "2"), s.pin("J11", "3")
    # Route A line (y=233.68 -> J11_1 y=243.84 via clean horizontal bus at 233.68 then drop at 535.94)
    s.wire(u20_a, (535.94, u20_a[1]))
    s.wire_L((535.94, u20_a[1]), j11_a, horiz_first=False)
    s.wire((454.66, u20_a[1]), s.pin("R81", "2"))
    s.junction((454.66, u20_a[1]))
    s.pwr("ISO_BUS_5V", 454.66, 208.28)
    s.wire((454.66, 208.28), s.pin("R81", "1"))
    s.wire((472.44, u20_a[1]), s.pin("R80", "1"))
    s.junction((472.44, u20_a[1]))
    s.wire((495.30, u20_a[1]), s.pin("D21", "2"))
    s.junction((495.30, u20_a[1]))
    s.label("RS485_A", (508.00, u20_a[1]))

    # Route B line (y=238.76 -> down to 248.92 -> J11_2 y=246.38)
    s.wire(u20_b, (444.50, u20_b[1]))
    s.wire((444.50, u20_b[1]), (444.50, 248.92))
    s.wire((444.50, 248.92), (530.86, 248.92))
    s.wire_L((530.86, 248.92), j11_b, horiz_first=False)
    s.wire((454.66, 248.92), s.pin("R82", "1"))
    s.junction((454.66, 248.92))
    s.wire(s.pin("R82", "2"), (454.66, 276.86))
    s.pwr("ISO_BUS_GND", 454.66, 276.86)
    s.wire((472.44, 248.92), s.pin("R80", "2"))
    s.junction((472.44, 248.92))
    s.wire((518.16, 248.92), s.pin("D22", "2"))
    s.junction((518.16, 248.92))
    s.label("RS485_B", (508.00, 248.92))

    # TVS + J11 pin 3 -> ISO_BUS_GND
    s.wire(s.pin("D21", "1"), (495.30, 276.86))
    s.wire(s.pin("D22", "1"), (518.16, 276.86))
    s.wire((495.30, 276.86), (535.94, 276.86))
    s.junction((518.16, 276.86))
    s.wire_L(j11_g, (535.94, 276.86), horiz_first=True)
    s.pwr("ISO_BUS_GND", 518.16, 276.86)

    # ------------------------------------------------------------------
    # ZONE 4 (BOTTOM): ISOLATED CAN BUS (2x PC817 + MCP2551 + SPLIT 120R + TVS)
    # ------------------------------------------------------------------
    s.add_box(22, 298, 570, 386, "4. ISOLATED CAN 2.0B TRANSCEIVER (OPTO-ISOLATED MCP2551 + 120R SPLIT TERM + TVS)",
              stroke_rgb=(180, 40, 20), fill_rgba=(255, 248, 245, 0.35))

    # CAN Optocouplers (U25=TX, U26=RX)
    add_tx_opto("U25", "R83", "R84", 322.58, "CAN_TX", "ISO_CAN_TXD")
    add_rx_opto("U26", "R85", "R86", 358.14, "CAN_RX", "ISO_CAN_RXD")

    # U21: MCP2551-I-SN + C64(100nF) + R87(10k Rs) + Split Termination (R88/R89 60.4R + C65 4.7nF) + TVS D23/D24 + J12
    s.add_comp("Interface_CAN_LIN:MCP2551-I-SN", "U21", "MCP2551-I-SN", 424.18, 340.36, rot=0,
               footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
               ref_offset=(4, -14), val_offset=(4, 14))
    s.add_comp("Device:C", "C64", "100nF", 378.46, 340.36, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(-5.5, -1.5), val_offset=(-5.5, 1.5))
    s.add_comp("Device:R", "R87", "10k", 398.78, 360.68, rot=0,
               footprint="Resistor_SMD:R_0603_1608Metric",
               ref_offset=(-4.5, -1.5), val_offset=(-4.5, 1.5))

    s.wire(s.pin("U21", "3"), (424.18, 322.58))
    s.wire((424.18, 322.58), (378.46, 322.58))
    s.wire((378.46, 322.58), s.pin("C64", "1"))
    s.pwr("ISO_BUS_5V", 424.18, 322.58)
    s.junction((424.18, 322.58))

    s.wire(s.pin("U21", "2"), (424.18, 355.60))
    s.wire((424.18, 355.60), (378.46, 355.60))
    s.wire((378.46, 355.60), s.pin("C64", "2"))
    s.pwr("ISO_BUS_GND", 424.18, 355.60)
    s.junction((424.18, 355.60))

    s.stub_label(s.pin("U21", "1"), "ISO_CAN_TXD", dx=-12.70)
    s.stub_label(s.pin("U21", "4"), "ISO_CAN_RXD", dx=-12.70)
    s.wire_L(s.pin("U21", "8"), s.pin("R87", "1"), horiz_first=True)
    s.wire(s.pin("R87", "2"), (398.78, 373.38))
    s.pwr("ISO_BUS_GND", 398.78, 373.38)

    # CAN Split Termination (R88=60.4R, R89=60.4R, C65=4.7nF) + TVS D23/D24 + J12
    s.add_comp("Device:R", "R88", "60.4R_1%", 459.74, 330.20, rot=0,
               footprint="Resistor_SMD:R_0805_2012Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:R", "R89", "60.4R_1%", 459.74, 350.52, rot=0,
               footprint="Resistor_SMD:R_0805_2012Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:C", "C65", "4.7nF", 477.52, 355.60, rot=0,
               footprint="Capacitor_SMD:C_0603_1608Metric",
               ref_offset=(4.2, -1.5), val_offset=(4.2, 1.5))
    s.add_comp("Device:D_TVS", "D23", "SMBJ12A", 497.84, 360.68, rot=90,
               footprint="Diode_SMD:D_SMB",
               ref_offset=(4.5, -1.5), val_offset=(4.5, 1.5))
    s.add_comp("Device:D_TVS", "D24", "SMBJ12A", 520.70, 360.68, rot=90,
               footprint="Diode_SMD:D_SMB",
               ref_offset=(4.5, -1.5), val_offset=(4.5, 1.5))
    s.add_comp("Connector:Screw_Terminal_01x03", "J12", "J_CAN_BUS", 548.64, 337.82, rot=0,
               footprint="TerminalBlock_CUI:TerminalBlock_CUI_TB007-508-03_1x03_P5.08mm_Horizontal",
               ref_offset=(0, -10.5), val_offset=(0, 11.5))

    u21_h, u21_l = s.pin("U21", "7"), s.pin("U21", "6")
    s.auto_nc_unused("U21")
    j12_h, j12_l, j12_g = s.pin("J12", "1"), s.pin("J12", "2"), s.pin("J12", "3")

    # CANH bus at y=322.58
    s.wire_L(u21_h, (449.58, 322.58), horiz_first=True)
    s.wire((449.58, 322.58), (535.94, 322.58))
    s.wire_L((535.94, 322.58), j12_h, horiz_first=False)
    s.wire((459.74, 322.58), s.pin("R88", "1"))
    s.junction((459.74, 322.58))
    s.wire((497.84, 322.58), s.pin("D23", "2"))
    s.junction((497.84, 322.58))
    s.label("CANH", (508.00, 322.58))

    # CANL bus at y=358.14 -> drops up to J12_2
    s.wire_L(u21_l, (449.58, 358.14), horiz_first=True)
    s.wire((449.58, 358.14), (459.74, 358.14))
    s.wire((459.74, 358.14), s.pin("R89", "2"))
    # Continue CANL along y=342.90? Wait: route CANL cleanly at y=342.90 from U21_6!
    # Wait: let's route CANL cleanly along y=345.44 -> J12_2 (y=337.82) without crossing R88/R89!
    # Already wired u21_l to R89 pin 2 at y=358.14; continue to x=530.86 at y=358.14? Wait: C65 is at (477.52, 355.60)!
    # Let's avoid crossing C65 by placing R88/R89 center tap at y=340.36 and CANL bus at y=347.98? No, R89 pin2 is at 350.52+3.81=354.33!
    # Wait: C65 pin1 is at 355.60-3.81 = 351.79. Let's move CANL bus to y=363.22 -> No, D23/D24 are at y=360.68!
    # EASIEST & CLEANEST: move C65 to x=477.52, y=345.44 (pin1=341.63, pin2=349.25) -> Wait, let's route CANL at y=368.30? No, let's put R88/R89+C65 cleanly above/below!
    # Actually: R88 pin2 = 334.01, R89 pin1 = 346.71. Midpoint y = 340.36!
    s.wire(s.pin("R88", "2"), s.pin("R89", "1"))
    s.junction((459.74, 340.36))
    s.wire((459.74, 340.36), (477.52, 340.36))
    s.wire((477.52, 340.36), s.pin("C65", "1"))
    s.wire(s.pin("C65", "2"), (477.52, 373.38))
    s.pwr("ISO_BUS_GND", 477.52, 373.38)

    # Now wire CANL from R89 pin 2 (459.74, 354.33) -> (459.74, 358.14) -> (472.44, 358.14)? No, jump over C65 by going to (459.74, 358.14) and using label CANL!
    s.label("CANL", (459.74, 358.14))
    s.junction((459.74, 358.14))
    # Connect CANL at D24 and J12_2 cleanly via net label CANL (zero wire crossings over C65/D23!)
    s.wire((520.70, 337.82), s.pin("D24", "2"))
    s.wire((520.70, 337.82), j12_l)
    s.junction((520.70, 337.82))
    s.label("CANL", (520.70, 337.82))

    # TVS + J12 pin 3 -> ISO_BUS_GND
    s.wire(s.pin("D23", "1"), (497.84, 373.38))
    s.wire(s.pin("D24", "1"), (520.70, 373.38))
    s.wire((497.84, 373.38), (535.94, 373.38))
    s.junction((520.70, 373.38))
    s.wire_L(j12_g, (535.94, 373.38), horiz_first=True)
    s.pwr("ISO_BUS_GND", 520.70, 373.38)

    return s


# ==============================================================================
# ROOT HIERARCHICAL SHEET
# ==============================================================================
def build_root_sheet(
    lib: SymbolLibraryCache,
    root_uuid: str,
    sh1_uuid: str,
    sh2_uuid: str,
    sh3_uuid: str,
    sh4_uuid: str,
) -> Schematic:
    s = Schematic(
        title="Industrial PLC & Fieldbus Controller — Root",
        lib_cache=lib,
        project_name=PROJECT_NAME,
        sheet_uuid=root_uuid,
        instance_path=f"/{root_uuid}",
        page_num="1",
        paper="A2",
        comment1="4-Sheet A2 Hierarchy | 3 Galvanic Isolation Domains",
        comment2="STM32F407 + LAN8720A | Iso DI/Relays | Analog + RS485/CAN",
    )

    # Top title banner
    s.add_text("INDUSTRIAL PLC & ETHERNET / FIELDBUS CONTROLLER — SYSTEM HIERARCHY",
               297.0, 26.0, size=4.2, bold=True, rgb=(20, 50, 110), justify="center")
    s.add_text("4-Sheet Hierarchical KiCad 9 Architecture  |  3 Galvanic Isolation Domains (Main GND, ISO_IO_GND, ISO_BUS_GND)",
               297.0, 33.5, size=2.2, bold=False, rgb=(80, 90, 100), justify="center")

    # Sheet 1 Block (Top-Left)
    sh1_pins = s.add_sheet_block(
        name="Sheet1_Power",
        filename="Sheet1_Power.kicad_sch",
        x=45.72, y=55.88, w=147.32, h=111.76,
        child_uuid=sh1_uuid, page_num="2",
        pins=[
            ("SYS_RST_N", "output", "right", 30.48),
            ("PWR_GOOD",  "output", "right", 55.88),
            ("V24_SENSE", "output", "right", 81.28),
        ],
        fill_rgba=(240, 248, 255, 1.0),
    )
    s.add_text("Multi-Rail Industrial Power Architecture\n"
               "• 24V Input: Fuse, SS34, SMBJ28A TVS, Pi EMI\n"
               "• 24V -> 5V Buck (LM2596S-ADJ, 3A)\n"
               "• 5V -> +3V3 Digital & +3V3A Analog LDOs\n"
               "• 5V -> ISO_BUS_5V / ISO_BUS_GND (TMR-0511)\n"
               "• Dual Supervisors (MCP100) + LEDs + TPs",
               53.34, 83.82, size=1.9, rgb=(40, 55, 80))

    # Sheet 2 Block (Top-Right)
    sh2_pins = s.add_sheet_block(
        name="Sheet2_MCU_Ethernet",
        filename="Sheet2_MCU_Ethernet.kicad_sch",
        x=320.04, y=55.88, w=223.52, h=132.08,
        child_uuid=sh2_uuid, page_num="3",
        pins=[
            ("SYS_RST_N", "input", "left", 30.48),
            ("PWR_GOOD",  "input", "left", 55.88),
            ("V24_SENSE", "input", "left", 81.28),
            ("DI1_MCU",   "input",  "bottom", 17.78),
            ("DI2_MCU",   "input",  "bottom", 30.48),
            ("DI3_MCU",   "input",  "bottom", 43.18),
            ("DI4_MCU",   "input",  "bottom", 55.88),
            ("DO1_MCU",   "output", "bottom", 68.58),
            ("DO2_MCU",   "output", "bottom", 81.28),
            ("DO3_MCU",   "output", "bottom", 93.98),
            ("DO4_MCU",   "output", "bottom", 106.68),
            ("AIN1_ADC",  "input",  "bottom", 132.08),
            ("AIN2_ADC",  "input",  "bottom", 144.78),
            ("RS485_TX",  "output", "bottom", 157.48),
            ("RS485_RX",  "input",  "bottom", 170.18),
            ("RS485_DE",  "output", "bottom", 182.88),
            ("CAN_TX",    "output", "bottom", 195.58),
            ("CAN_RX",    "input",  "bottom", 208.28),
        ],
        fill_rgba=(242, 252, 248, 1.0),
    )
    s.add_text("STM32F407VETx MCU & Ethernet Core\n"
               "• Cortex-M4F 168MHz (LQFP-100) + 8MHz HSE\n"
               "• Full Decoupling Bank (VDD + VDDA + VCAP)\n"
               "• LAN8720A RMII PHY + 25MHz + HR911105A RJ45\n"
               "• W25Q32 SPI Flash | LM75C Temp | PCF8563T RTC\n"
               "• 10-Pin SWD Header + Debug UART1 + Status LEDs",
               358.14, 86.36, size=1.9, rgb=(30, 75, 65))

    # Sheet 3 Block (Bottom-Left)
    sh3_pins = s.add_sheet_block(
        name="Sheet3_Isolated_IO",
        filename="Sheet3_Isolated_IO.kicad_sch",
        x=45.72, y=236.22, w=203.20, h=127.00,
        child_uuid=sh3_uuid, page_num="4",
        pins=[
            ("DI1_MCU", "output", "top", 25.40),
            ("DI2_MCU", "output", "top", 45.72),
            ("DI3_MCU", "output", "top", 66.04),
            ("DI4_MCU", "output", "top", 86.36),
            ("DO1_MCU", "input",  "top", 106.68),
            ("DO2_MCU", "input",  "top", 127.00),
            ("DO3_MCU", "input",  "top", 147.32),
            ("DO4_MCU", "input",  "top", 167.64),
        ],
        fill_rgba=(255, 246, 242, 1.0),
    )
    s.add_text("Galvanically Isolated I/O (ISO_IO_GND Domain)\n"
               "• 2.5 kVrms Barrier — 0 Shared Refs w/ Main GND\n"
               "• 4-Ch 24V Digital Inputs (PC817, Balanced R, RC)\n"
               "• 4-Ch 24V SPDT Relays (PC817, 2N7002, G5LE-1)\n"
               "• Flyback Diodes (1N4148W) + Iso 24V Power Port",
               55.88, 287.02, size=1.9, rgb=(140, 45, 25))

    # Sheet 4 Block (Bottom-Right)
    sh4_pins = s.add_sheet_block(
        name="Sheet4_Analog_Fieldbus",
        filename="Sheet4_Analog_Fieldbus.kicad_sch",
        x=320.04, y=236.22, w=223.52, h=116.84,
        child_uuid=sh4_uuid, page_num="5",
        pins=[
            ("AIN1_ADC", "output", "top", 132.08),
            ("AIN2_ADC", "output", "top", 144.78),
            ("RS485_TX", "input",  "top", 157.48),
            ("RS485_RX", "output", "top", 170.18),
            ("RS485_DE", "input",  "top", 182.88),
            ("CAN_TX",   "input",  "top", 195.58),
            ("CAN_RX",   "output", "top", 208.28),
        ],
        fill_rgba=(252, 246, 255, 1.0),
    )
    s.add_text("Analog Front-End & Isolated Fieldbus (ISO_BUS_GND)\n"
               "• 2-Ch 0-10V / 4-20mA Analog FE (BAT54S + LM358)\n"
               "• 2.5 kVrms Barrier — 0 Shared Refs w/ GND or IO_GND\n"
               "• Opto-Isolated RS-485 (LTC2850IS8, 120R, Bias, TVS)\n"
               "• Opto-Isolated CAN (MCP2551, 120R Split, TVS)",
               330.20, 287.02, size=1.9, rgb=(90, 40, 120))

    # Wire Sheet 1 -> Sheet 2 (straight horizontal runs!)
    for sig in ["SYS_RST_N", "PWR_GOOD", "V24_SENSE"]:
        s.wire(sh1_pins[sig], sh2_pins[sig])

    # Wire Sheet 2 <-> Sheet 4 (straight vertical runs — x coordinates match identically!)
    for sig in ["AIN1_ADC", "AIN2_ADC", "RS485_TX", "RS485_RX", "RS485_DE", "CAN_TX", "CAN_RX"]:
        s.wire(sh2_pins[sig], sh4_pins[sig])

    # Wire Sheet 2 <-> Sheet 3 via clean staircase Manhattan channels in the middle corridor (y = 195..228)
    io_sigs = ["DI1_MCU", "DI2_MCU", "DI3_MCU", "DI4_MCU", "DO1_MCU", "DO2_MCU", "DO3_MCU", "DO4_MCU"]
    for idx, sig in enumerate(io_sigs):
        p_top = sh2_pins[sig]
        p_bot = sh3_pins[sig]
        ch_y = 228.60 - idx * 3.81
        s.wire_pts([p_top, (p_top[0], ch_y), (p_bot[0], ch_y), p_bot])

    # Power & Isolation Architecture Legend Box (positioned cleanly BELOW V24_SENSE and ABOVE IO bus corridor!)
    s.add_box(200.66, 147.32, 312.42, 195.58, "GLOBAL POWER & ISOLATION DOMAINS",
              stroke_rgb=(120, 40, 40), fill_rgba=(255, 252, 240, 0.95), stroke_type="solid")
    s.add_text("Domain 1 (Main Logic / Analog):\n"
               "  +24V, +5V, +3V3, +3V3A, GND\n"
               "Domain 2 (Isolated Field I/O — Sheet 3):\n"
               "  ISO_IO_24V, ISO_IO_GND (External 24V)\n"
               "Domain 3 (Isolated Fieldbus — Sheet 1 & 4):\n"
               "  ISO_BUS_5V, ISO_BUS_GND (TMR-0511 + FB2/3)\n"
               "Galvanic Isolation: 0 shared component refs!",
               204.47, 159.00, size=1.6, rgb=(60, 30, 30), justify="left top")

    sheet_inst = (
        f'\t(sheet_instances\n'
        f'\t\t(path "/"\n\t\t\t(page "1")\n\t\t)\n'
        f'\t\t(path "/{sh1_uuid}"\n\t\t\t(page "2")\n\t\t)\n'
        f'\t\t(path "/{sh2_uuid}"\n\t\t\t(page "3")\n\t\t)\n'
        f'\t\t(path "/{sh3_uuid}"\n\t\t\t(page "4")\n\t\t)\n'
        f'\t\t(path "/{sh4_uuid}"\n\t\t\t(page "5")\n\t\t)\n'
        f'\t)'
    )
    return s, sheet_inst


def main():
    pwr_lib_path = write_project_files()
    lib = SymbolLibraryCache(extra_libs={"plc_power": pwr_lib_path})

    root_uuid = gen_uuid("root-plc-controller")
    sh1_uuid = gen_uuid("sheet1-power")
    sh2_uuid = gen_uuid("sheet2-mcu-eth")
    sh3_uuid = gen_uuid("sheet3-iso-io")
    sh4_uuid = gen_uuid("sheet4-analog-bus")

    sh1 = build_sheet1_power(lib, root_uuid, sh1_uuid)
    sh2 = build_sheet2_mcu_eth(lib, root_uuid, sh2_uuid)
    sh3 = build_sheet3_iso_io(lib, root_uuid, sh3_uuid)
    sh4 = build_sheet4_analog_bus(lib, root_uuid, sh4_uuid)
    root, root_inst = build_root_sheet(lib, root_uuid, sh1_uuid, sh2_uuid, sh3_uuid, sh4_uuid)

    outputs = [
        ("plc_controller.kicad_sch", root.render(root_inst)),
        ("Sheet1_Power.kicad_sch", sh1.render()),
        ("Sheet2_MCU_Ethernet.kicad_sch", sh2.render()),
        ("Sheet3_Isolated_IO.kicad_sch", sh3.render()),
        ("Sheet4_Analog_Fieldbus.kicad_sch", sh4.render()),
    ]
    for fname, content in outputs:
        out_path = os.path.join(KICAD_DIR, fname)
        with open(out_path, "w", encoding="utf-8") as f:
            f.write(content)
        print(f"Wrote {out_path} ({len(content):,} bytes)")

    # Print per-sheet component summary
    total_real = 0
    for name, sch in [("Sheet1_Power", sh1), ("Sheet2_MCU_Ethernet", sh2),
                      ("Sheet3_Isolated_IO", sh3), ("Sheet4_Analog_Fieldbus", sh4)]:
        refs = sorted({sym["ref"] for sym in sch.symbols if not sym["is_power"]})
        total_real += len(refs)
        print(f"  {name}: {len(refs)} unique real components")
    print(f"TOTAL UNIQUE REAL COMPONENTS: {total_real}")


if __name__ == "__main__":
    main()
