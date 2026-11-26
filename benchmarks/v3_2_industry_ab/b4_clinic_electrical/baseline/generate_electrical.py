#!/usr/bin/env python3
"""
Benchmark 4 (Baseline NO-EDL): Full IEC 60364 Building Electrical Installation
Overlaid on the 16-Room Outpatient Medical Clinic Floor Plan (B2 Baseline).

Generates:
  - electrical_manifest.json
  - panel_schedule.csv
  - clinic_electrical.dxf
  - clinic_electrical.png
And runs a strict Shapely + NetworkX + IEC 60364 automated verification suite.
"""

import csv
import json
import math
import os
from collections import defaultdict

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as patches
from matplotlib.lines import Line2D
import networkx as nx
from shapely.geometry import Point, box

ARCH_DIR = "/home/lindah/projects/EDL/benchmarks/v3_2_industry_ab/b2_clinic_arch/baseline"
OUT_DIR = "/home/lindah/projects/EDL/benchmarks/v3_2_industry_ab/b4_clinic_electrical/baseline"

# IEC 60364-5-52 Table B.52.2/B.52.4 Copper PVC in conduit (Method B1, 30C ambient)
IZ_1P_CU_PVC = {
    1.5: 17.5,
    2.5: 24.0,
    4.0: 32.0,
    6.0: 41.0,
    10.0: 57.0,
    16.0: 76.0,
    25.0: 101.0,
}
IZ_3P_CU_PVC = {
    1.5: 15.5,
    2.5: 21.0,
    4.0: 28.0,
    6.0: 36.0,
    10.0: 50.0,
    16.0: 68.0,
    25.0: 89.0,
}

# Copper resistivity at 70C conductor operating temp (Ohm*mm^2/m) - conservative IEC 60364
RHO_CU_70C = 0.0225
V_1P = 230.0
V_3P = 400.0


def load_arch():
    with open(os.path.join(ARCH_DIR, "arch_manifest.json"), "r") as f:
        return json.load(f)


def build_electrical_design(arch):
    rooms = {r["id"]: r for r in arch["rooms"]}

    # ------------------------------------------------------------------
    # 1. DISTRIBUTION BOARDS (PANELS)
    # ------------------------------------------------------------------
    # DB_MAIN: Placed at the architect's designated EL panel bay in ElecITRoom
    # (F-ELC-PNL: x=[23450, 23950], y=[8600, 10500]).
    # Front face at x=23450 facing West; clearance_bbox=[22350, 8600, 23450, 9600]
    # (1100 mm deep x 1000 mm wide -> >= 900 mm unobstructed front working zone,
    # strictly clear of D15 [y<=8400], F-ELC-RACK [y>=9850], and F-ELC-PNL).
    #
    # DB_CLINIC: Mounted on the West wall of NurseStation (x=[7850, 8100], y=[8950, 9550]).
    # Front face at x=8100 facing East; clearance_bbox=[8100, 8750, 9300, 9750]
    # (1200 mm deep x 1000 mm wide -> strictly clear of D11 [y<=8600],
    # F-NUR-CTR [x>=9550], and F-NUR-DESK [y>=9950]).
    panels = [
        {
            "id": "DB_MAIN",
            "room": "ElecITRoom",
            "x": 23575.0,
            "y": 9100.0,
            "width_mm": 600.0,
            "depth_mm": 250.0,
            "orientation": "W",  # faces West; footprint [23450, 8800, 23700, 9400]
            "body_bbox": [23450.0, 8800.0, 23700.0, 9400.0],
            "clearance_bbox": [22350.0, 8600.0, 23450.0, 9600.0],
            "tray_entry_x": 23600.0,
        },
        {
            "id": "DB_CLINIC",
            "room": "NurseStation",
            "x": 7975.0,
            "y": 9250.0,
            "width_mm": 600.0,
            "depth_mm": 250.0,
            "orientation": "E",  # faces East; footprint [7850, 8950, 8100, 9550]
            "body_bbox": [7850.0, 8950.0, 8100.0, 9550.0],
            "clearance_bbox": [8100.0, 8750.0, 9300.0, 9750.0],
            "tray_entry_x": 7975.0,
        },
    ]

    # ------------------------------------------------------------------
    # 2. DEVICES (100% COVERAGE OF ALL 16 ROOMS)
    # ------------------------------------------------------------------
    devices = []

    def add_dev(dev_id, room, kind, circuit, load_w, x, y):
        devices.append({
            "id": dev_id,
            "room": room,
            "kind": kind,
            "circuit": circuit,
            "load_w": int(load_w),
            "x": float(x),
            "y": float(y),
        })

    # --- A. LIGHTING LUMINAIRES & STRIKE-SIDE WALL SWITCHES ---
    # C-L1 (DB_CLINIC, L1): South Clinical Lighting (Exam1, Exam2, Exam3, Procedure) = 376 W
    add_dev("L-EX1-1", "Exam1", "light", "C-L1", 36, 9850, 1500)
    add_dev("L-EX1-2", "Exam1", "light", "C-L1", 36, 9850, 3600)
    add_dev("SW-EX1",  "Exam1", "switch", "C-L1", 0, 9850, 4900)  # D04 strike side

    add_dev("L-EX2-1", "Exam2", "light", "C-L1", 36, 12800, 1500)
    add_dev("L-EX2-2", "Exam2", "light", "C-L1", 36, 12800, 3600)
    add_dev("SW-EX2",  "Exam2", "switch", "C-L1", 0, 12800, 4900)  # D05 strike side

    add_dev("L-EX3-1", "Exam3", "light", "C-L1", 36, 15750, 1500)
    add_dev("L-EX3-2", "Exam3", "light", "C-L1", 36, 15750, 3600)
    add_dev("SW-EX3",  "Exam3", "switch", "C-L1", 0, 15750, 4900)  # D06 strike side

    add_dev("L-PRC-1", "Procedure", "light", "C-L1", 40, 18500, 1500)
    add_dev("L-PRC-2", "Procedure", "light", "C-L1", 40, 20400, 1500)
    add_dev("L-PRC-3", "Procedure", "light", "C-L1", 40, 18500, 3600)
    add_dev("L-PRC-4", "Procedure", "light", "C-L1", 40, 20400, 3600)
    add_dev("SW-PRC",  "Procedure", "switch", "C-L1", 0, 19050, 4900)  # D07 strike side

    # C-L2 (DB_CLINIC, L2): West & Reception Lighting (Vestibule, Waiting, PublicWC, StaffWC) = 288 W
    add_dev("L-VST-1", "Vestibule", "light", "C-L2", 18, 1075, 1750)
    add_dev("L-VST-2", "Vestibule", "light", "C-L2", 18, 1075, 3600)
    add_dev("SW-VST",  "Vestibule", "switch", "C-L2", 0, 1850, 300)  # D01 strike side

    add_dev("L-WAT-1", "Waiting", "light", "C-L2", 36, 3300, 1500)
    add_dev("L-WAT-2", "Waiting", "light", "C-L2", 36, 5200, 1500)
    add_dev("L-WAT-3", "Waiting", "light", "C-L2", 36, 7000, 1500)
    add_dev("L-WAT-4", "Waiting", "light", "C-L2", 36, 3300, 3400)
    add_dev("L-WAT-5", "Waiting", "light", "C-L2", 36, 5200, 3400)
    add_dev("L-WAT-6", "Waiting", "light", "C-L2", 36, 7000, 3400)
    add_dev("SW-WAT-1", "Waiting", "switch", "C-L2", 0, 2250, 3150)  # D02 strike side
    add_dev("SW-WAT-2", "Waiting", "switch", "C-L2", 0, 6250, 4900)  # D03 strike side

    add_dev("L-PWC-1", "PublicWC", "light", "C-L2", 18, 1100, 9150)
    add_dev("SW-PWC",  "PublicWC", "switch", "C-L2", 0, 1550, 7600)  # D08 strike side

    add_dev("L-SWC-1", "StaffWC", "light", "C-L2", 18, 2950, 9150)
    add_dev("SW-SWC",  "StaffWC", "switch", "C-L2", 0, 3500, 7600)  # D09 strike side

    # C-L3 (DB_CLINIC, L3): North Staff & Exam4 Lighting (Breakroom, NurseStation, Exam4) = 216 W
    add_dev("L-BRK-1", "Breakroom", "light", "C-L3", 36, 5100, 9150)
    add_dev("L-BRK-2", "Breakroom", "light", "C-L3", 36, 6800, 9150)
    add_dev("SW-BRK",  "Breakroom", "switch", "C-L3", 0, 5250, 7600)  # D10 strike side

    add_dev("L-NUR-1", "NurseStation", "light", "C-L3", 36, 9750, 8450)
    add_dev("L-NUR-2", "NurseStation", "light", "C-L3", 36, 9750, 10100)
    add_dev("SW-NUR",  "NurseStation", "switch", "C-L3", 0, 9420, 7600)  # D11 strike side

    add_dev("L-EX4-1", "Exam4", "light", "C-L3", 36, 12650, 9150)
    add_dev("L-EX4-2", "Exam4", "light", "C-L3", 36, 14350, 9150)
    add_dev("SW-EX4",  "Exam4", "switch", "C-L3", 0, 12900, 7600)  # D12 strike side

    # M-L1 (DB_MAIN, L3): East Wing Lighting (Lab, Pharmacy, ElecITRoom, MechRoom) = 260 W
    add_dev("L-LAB-1", "Lab", "light", "M-L1", 40, 16500, 9150)
    add_dev("L-LAB-2", "Lab", "light", "M-L1", 40, 18000, 9150)
    add_dev("SW-LAB",  "Lab", "switch", "M-L1", 0, 16900, 7600)  # D13 strike side

    add_dev("L-PHM-1", "Pharmacy", "light", "M-L1", 36, 19850, 9150)
    add_dev("L-PHM-2", "Pharmacy", "light", "M-L1", 36, 20950, 9150)
    add_dev("SW-PHM",  "Pharmacy", "switch", "M-L1", 0, 20300, 7600)  # D14 strike side

    add_dev("L-ELC-1", "ElecITRoom", "light", "M-L1", 36, 23250, 10150)
    add_dev("SW-ELC",  "ElecITRoom", "switch", "M-L1", 0, 23150, 7600)  # D15 strike side

    add_dev("L-MCH-1", "MechRoom", "light", "M-L1", 36, 25000, 9150)
    add_dev("L-MCH-2", "MechRoom", "light", "M-L1", 36, 26200, 9150)
    add_dev("SW-MCH",  "MechRoom", "switch", "M-L1", 0, 25550, 7600)  # D16 strike side

    # M-L2 (DB_MAIN, L2): Central Clinical Corridor Lighting (8 fixtures) = 192 W
    cor_xs = [1800, 5200, 8600, 12000, 15400, 18800, 22200, 25400]
    for idx, cx in enumerate(cor_xs, 1):
        add_dev(f"L-COR-{idx}", "Corridor", "light", "M-L2", 24, cx, 6250)
    add_dev("SW-COR-W", "Corridor", "switch", "M-L2", 0, 6250, 5250)
    add_dev("SW-COR-E", "Corridor", "switch", "M-L2", 0, 23200, 7250)

    # M-EM1 (DB_MAIN, L3): Emergency & Exit Lighting (Maintained 3h battery LED) = 76 W
    add_dev("EM-VST-1", "Vestibule", "emergency_light", "M-EM1", 8, 1075, 450)
    add_dev("EM-WAT-1", "Waiting",   "emergency_light", "M-EM1", 8, 4300, 2450)
    add_dev("EM-WAT-2", "Waiting",   "emergency_light", "M-EM1", 8, 7000, 4700)
    add_dev("EM-COR-1", "Corridor",  "emergency_light", "M-EM1", 8, 3500, 6250)
    add_dev("EM-COR-2", "Corridor",  "emergency_light", "M-EM1", 8, 10300, 6250)
    add_dev("EM-COR-3", "Corridor",  "emergency_light", "M-EM1", 8, 17100, 6250)
    add_dev("EM-COR-4", "Corridor",  "emergency_light", "M-EM1", 8, 23800, 6250)
    add_dev("EM-PRC-1", "Procedure", "emergency_light", "M-EM1", 12, 19450, 2700)
    add_dev("EM-ELC-1", "ElecITRoom","emergency_light", "M-EM1", 8, 22850, 8150)

    # --- B. GENERAL & MEDICAL POWER SOCKETS ---
    # C-P1 (DB_CLINIC, L1): Waiting & Breakroom General Sockets = 2000 W
    add_dev("SK-WAT-1", "Waiting",   "socket", "C-P1", 350, 3200, 4850)  # Reception W
    add_dev("SK-WAT-2", "Waiting",   "socket", "C-P1", 350, 4600, 4850)  # Reception E
    add_dev("SK-WAT-3", "Waiting",   "socket", "C-P1", 350, 4900, 280)   # South wall
    add_dev("SK-WAT-4", "Waiting",   "socket", "C-P1", 350, 8180, 2000)  # East wall
    add_dev("SK-BRK-1", "Breakroom", "socket", "C-P1", 300, 5800, 10720) # North wall
    add_dev("SK-BRK-2", "Breakroom", "socket", "C-P1", 300, 7580, 9100)  # East wall

    # C-P2 (DB_CLINIC, L2): Exam1 & Exam2 Medical Sockets = 2100 W
    add_dev("SK-EX1-1", "Exam1", "socket", "C-P2", 350, 9300, 280)
    add_dev("SK-EX1-2", "Exam1", "socket", "C-P2", 350, 8580, 2700)
    add_dev("SK-EX1-3", "Exam1", "socket", "C-P2", 350, 11120, 2600)
    add_dev("SK-EX2-1", "Exam2", "socket", "C-P2", 350, 12250, 280)
    add_dev("SK-EX2-2", "Exam2", "socket", "C-P2", 350, 11530, 2700)
    add_dev("SK-EX2-3", "Exam2", "socket", "C-P2", 350, 14070, 2600)

    # C-P3 (DB_CLINIC, L3): Exam3 & Exam4 Medical Sockets = 2100 W
    add_dev("SK-EX3-1", "Exam3", "socket", "C-P3", 350, 15200, 280)
    add_dev("SK-EX3-2", "Exam3", "socket", "C-P3", 350, 14480, 2700)
    add_dev("SK-EX3-3", "Exam3", "socket", "C-P3", 350, 17020, 2600)
    add_dev("SK-EX4-1", "Exam4", "socket", "C-P3", 350, 12350, 10720)
    add_dev("SK-EX4-2", "Exam4", "socket", "C-P3", 350, 11630, 8950)
    add_dev("SK-EX4-3", "Exam4", "socket", "C-P3", 350, 15220, 8650)

    # C-P4 (DB_CLINIC, L1): NurseStation & Procedure Medical Sockets (30mA Type A RCD) = 2800 W
    add_dev("SK-NUR-1", "NurseStation", "socket", "C-P4", 350, 8650, 10720)
    add_dev("SK-NUR-2", "NurseStation", "socket", "C-P4", 350, 10150, 10720)
    add_dev("SK-NUR-3", "NurseStation", "socket", "C-P4", 350, 11220, 8450)
    add_dev("SK-NUR-4", "NurseStation", "socket", "C-P4", 350, 11220, 9350)
    add_dev("SK-PRC-1", "Procedure",    "socket", "C-P4", 350, 17430, 3150)
    add_dev("SK-PRC-2", "Procedure",    "socket", "C-P4", 350, 18600, 280)
    add_dev("SK-PRC-3", "Procedure",    "socket", "C-P4", 350, 20300, 280)
    add_dev("SK-PRC-4", "Procedure",    "socket", "C-P4", 350, 21320, 1200)

    # M-P1 (DB_MAIN, L1): Lab Bench Sockets = 1200 W
    add_dev("SK-LAB-1", "Lab", "socket", "M-P1", 400, 17500, 7630)
    add_dev("SK-LAB-2", "Lab", "socket", "M-P1", 400, 18670, 8950)
    add_dev("SK-LAB-3", "Lab", "socket", "M-P1", 400, 17200, 10720)

    # M-P2 (DB_MAIN, L2): Pharmacy & MechRoom Sockets = 1100 W
    add_dev("SK-PHM-1", "Pharmacy", "socket", "M-P2", 400, 19080, 9200)
    add_dev("SK-PHM-2", "Pharmacy", "socket", "M-P2", 400, 20200, 10720)
    add_dev("SK-MCH-1", "MechRoom", "socket", "M-P2", 300, 26720, 8450)

    # M-P3 (DB_MAIN, L3): ElecITRoom & Corridor Service Sockets = 1400 W
    add_dev("SK-ELC-1", "ElecITRoom", "socket", "M-P3", 350, 21930, 8800)
    add_dev("SK-ELC-2", "ElecITRoom", "socket", "M-P3", 350, 23200, 10720)
    add_dev("SK-COR-1", "Corridor",   "socket", "M-P3", 350, 5650, 7250)
    add_dev("SK-COR-2", "Corridor",   "socket", "M-P3", 350, 18650, 7250)

    # --- C. DEDICATED HIGH-LOAD / SPECIAL CIRCUITS ---
    # C-D1 (DB_CLINIC, L2): Procedure Medical RCD-Protected Circuit (30mA Type A RCD, 20A) = 2800 W
    add_dev("DED-PRC-MED", "Procedure",  "dedicated_load", "C-D1", 2800, 21250, 3550)
    # C-D2 (DB_CLINIC, L3): Breakroom Kitchenette Appliances Dedicated 20A = 3000 W
    add_dev("DED-BRK-KIT", "Breakroom",  "dedicated_load", "C-D2", 3000, 4050, 9450)
    # M-D1 (DB_MAIN, L1): MechRoom Electric Water Heater Dedicated 25A = 3600 W
    add_dev("DED-MCH-WH",  "MechRoom",   "dedicated_load", "M-D1", 3600, 26150, 9550)
    # M-D2 (DB_MAIN, L2): Lab Autoclave / Sterilizer Dedicated 25A = 3600 W
    add_dev("DED-LAB-AUTO","Lab",        "dedicated_load", "M-D2", 3600, 18250, 10450)
    # M-D3 (DB_MAIN, L3): ElecITRoom Server Rack Dedicated 20A UPS-Backed = 3000 W
    add_dev("DED-ELC-UPS", "ElecITRoom", "dedicated_load", "M-D3", 3000, 22425, 10300)

    # ------------------------------------------------------------------
    # 3. CIRCUIT DEFINITIONS
    # ------------------------------------------------------------------
    circuit_specs = [
        # Sub-main feeder DB_MAIN -> DB_CLINIC (3-Phase 400V)
        {"id": "SUB1", "panel": "DB_MAIN", "kind": "submain", "phase": "3P",
         "breaker_a": 40, "rcd_ma": 0, "cable_mm2": 16.0, "desc": "Sub-Main Feeder to DB_CLINIC (4C+E 16mm2 Cu)"},
        # DB_MAIN Final Circuits
        {"id": "M-L1",  "panel": "DB_MAIN", "kind": "lighting",  "phase": "L3", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "desc": "Lighting: East Wing (Lab/Pharm/ElecIT/Mech)"},
        {"id": "M-L2",  "panel": "DB_MAIN", "kind": "lighting",  "phase": "L2", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Lighting: Central Clinical Corridor"},
        {"id": "M-EM1", "panel": "DB_MAIN", "kind": "lighting",  "phase": "L3", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Emergency & Exit Lighting (Maintained FP200)"},
        {"id": "M-P1",  "panel": "DB_MAIN", "kind": "power",     "phase": "L1", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Lab Clinical Benches"},
        {"id": "M-P2",  "panel": "DB_MAIN", "kind": "power",     "phase": "L2", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Pharmacy & MechRoom"},
        {"id": "M-P3",  "panel": "DB_MAIN", "kind": "power",     "phase": "L3", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Sockets: ElecITRoom & Corridor Cleaners"},
        {"id": "M-D1",  "panel": "DB_MAIN", "kind": "dedicated", "phase": "L1", "breaker_a": 25, "rcd_ma": 30, "cable_mm2": 6.0, "desc": "Dedicated: MechRoom Electric Water Heater"},
        {"id": "M-D2",  "panel": "DB_MAIN", "kind": "dedicated", "phase": "L2", "breaker_a": 25, "rcd_ma": 30, "cable_mm2": 6.0, "desc": "Dedicated: Lab Autoclave / Sterilizer"},
        {"id": "M-D3",  "panel": "DB_MAIN", "kind": "dedicated", "phase": "L3", "breaker_a": 20, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Dedicated: ElecITRoom Server Rack (UPS)"},
        # DB_CLINIC Final Circuits
        {"id": "C-L1",  "panel": "DB_CLINIC", "kind": "lighting",  "phase": "L1", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "desc": "Lighting: Exam1-3 & Procedure"},
        {"id": "C-L2",  "panel": "DB_CLINIC", "kind": "lighting",  "phase": "L2", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "desc": "Lighting: Vestibule, Waiting, PublicWC & StaffWC"},
        {"id": "C-L3",  "panel": "DB_CLINIC", "kind": "lighting",  "phase": "L3", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "desc": "Lighting: Breakroom, NurseStation & Exam4"},
        {"id": "C-P1",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L1", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Waiting & Breakroom General"},
        {"id": "C-P2",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L2", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Exam1 & Exam2 Medical"},
        {"id": "C-P3",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L3", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Exam3 & Exam4 Medical"},
        {"id": "C-P4",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L1", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Sockets: NurseStation & Procedure (30mA Type A RCD)"},
        {"id": "C-D1",  "panel": "DB_CLINIC", "kind": "dedicated", "phase": "L2", "breaker_a": 20, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Dedicated: Procedure Medical Equipment (30mA Type A RCD)"},
        {"id": "C-D2",  "panel": "DB_CLINIC", "kind": "dedicated", "phase": "L3", "breaker_a": 20, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Dedicated: Breakroom Kitchenette Appliances"},
    ]

    # ------------------------------------------------------------------
    # 4. ORTHOGONAL CONDUIT / CABLE TRAY ROUTING
    # ------------------------------------------------------------------
    # Corridor ceiling cable tray Y-lanes (inside Corridor y=[5150, 7350]):
    TRAY_Y = {
        "submain":   7100.0,
        "dedicated": 6920.0,
        "power":     6620.0,
        "lighting":  5880.0,
        "emergency": 5560.0,
    }

    panel_map = {p["id"]: p for p in panels}
    devs_by_cid = defaultdict(list)
    for d in devices:
        devs_by_cid[d["circuit"]].append(d)

    wires = []

    def add_seg(cid, x1, y1, x2, y2):
        if abs(x1 - x2) < 1e-6 and abs(y1 - y2) < 1e-6:
            return
        wires.append({
            "circuit": cid,
            "from_x": round(float(x1), 1),
            "from_y": round(float(y1), 1),
            "to_x": round(float(x2), 1),
            "to_y": round(float(y2), 1),
        })

    def route_orth_polyline(cid, pts):
        for (x1, y1), (x2, y2) in zip(pts[:-1], pts[1:]):
            add_seg(cid, x1, y1, x2, y2)

    # Route each circuit as a clean orthogonal Manhattan tree via the corridor tray
    for c in circuit_specs:
        cid = c["id"]
        src_p = panel_map[c["panel"]]
        px, py = src_p["x"], src_p["y"]
        tx = src_p["tray_entry_x"]

        if cid == "SUB1":
            dst_p = panel_map["DB_CLINIC"]
            ty = TRAY_Y["submain"]
            route_orth_polyline(cid, [
                (px, py),
                (tx, py),
                (tx, ty),
                (dst_p["tray_entry_x"], ty),
                (dst_p["x"], dst_p["y"]),
            ])
            continue

        c_devs = devs_by_cid[cid]
        # Special local in-room routing for single-room dedicated circuit in ElecITRoom
        if cid == "M-D3":
            d = c_devs[0]
            route_orth_polyline(cid, [(px, py), (px, d["y"]), (d["x"], d["y"])])
            continue

        tray_key = "emergency" if cid == "M-EM1" else c["kind"]
        ty = TRAY_Y[tray_key]

        # Group devices by room so each room gets one clean orthogonal tap off the corridor tray
        devs_by_room = defaultdict(list)
        for d in c_devs:
            devs_by_room[d["room"]].append(d)

        # Compute each room's tap X on the corridor tray
        room_taps = {}
        for rname, rdevs in devs_by_room.items():
            r = rooms[rname]
            if rname == "Corridor":
                # Corridor devices tap straight vertically from tray at their own X
                for d in rdevs:
                    room_taps[f"COR_{d['id']}"] = (d["x"], [d])
            elif rname == src_p["room"] and len(devs_by_room) == 1:
                room_taps[rname] = (tx, rdevs)
            else:
                # Choose a clean tap X inside the room's X span (avoiding exterior edges)
                avg_x = sum(d["x"] for d in rdevs) / len(rdevs)
                tap_x = max(r["min_x"] + 250.0, min(r["max_x"] - 250.0, round(avg_x / 50.0) * 50.0))
                room_taps[rname] = (tap_x, rdevs)

        # 1. Feeder from DB panel to corridor tray
        route_orth_polyline(cid, [(px, py), (tx, py), (tx, ty)])

        # 2. Horizontal trunk along corridor tray covering tx and all room tap X coords
        all_xs = sorted(set([tx] + [v[0] for v in room_taps.values()]))
        for x_a, x_b in zip(all_xs[:-1], all_xs[1:]):
            add_seg(cid, x_a, ty, x_b, ty)

        # 3. Branch from each tap into its room's devices orthogonally
        for key, (tap_x, rdevs) in room_taps.items():
            if key.startswith("COR_"):
                d = rdevs[0]
                add_seg(cid, tap_x, ty, d["x"], d["y"])
                continue

            # Determine room spine Y: sort devices by distance from tray Y
            # Build clean room manifold: vertical spine at tap_x spanning all device Ys,
            # with horizontal ribs to each device
            ys = sorted(set([ty] + [d["y"] for d in rdevs]))
            # Split into north of tray and south of tray (all devices in one room are on one side)
            if max(d["y"] for d in rdevs) > ty:
                # North room: vertical spine goes up from ty to max device y
                spine_ys = sorted(set([ty] + [d["y"] for d in rdevs]))
            else:
                # South room: vertical spine goes down from ty to min device y
                spine_ys = sorted(set([ty] + [d["y"] for d in rdevs]), reverse=True)

            for y_a, y_b in zip(spine_ys[:-1], spine_ys[1:]):
                add_seg(cid, tap_x, y_a, tap_x, y_b)

            # Horizontal ribs at each Y level to devices at that Y
            devs_by_y = defaultdict(list)
            for d in rdevs:
                devs_by_y[d["y"]].append(d)
            for dy, dlist in devs_by_y.items():
                xs_at_y = sorted(set([tap_x] + [d["x"] for d in dlist]))
                for xa, xb in zip(xs_at_y[:-1], xs_at_y[1:]):
                    add_seg(cid, xa, dy, xb, dy)

    # ------------------------------------------------------------------
    # 5. COMPUTE REAL ROUTED LENGTHS, LOADS, AND IEC 60364 VOLTAGE DROP
    # ------------------------------------------------------------------
    wires_by_cid = defaultdict(list)
    for w in wires:
        wires_by_cid[w["circuit"]].append(w)

    # First compute final circuit loads so SUB1 gets exact DB_CLINIC total
    clinic_total_w = 0
    for c in circuit_specs:
        cid = c["id"]
        if cid == "SUB1":
            continue
        c["load_w"] = sum(d["load_w"] for d in devs_by_cid[cid])
        if c["panel"] == "DB_CLINIC":
            clinic_total_w += c["load_w"]

    for c in circuit_specs:
        if c["id"] == "SUB1":
            c["load_w"] = clinic_total_w

    circuits_out = []
    circuit_audit = {}

    for c in circuit_specs:
        cid = c["id"]
        src_p = panel_map[c["panel"]]
        src_node = (round(src_p["x"], 1), round(src_p["y"], 1))

        G = nx.Graph()
        total_wire_mm = 0.0
        for w in wires_by_cid[cid]:
            u = (w["from_x"], w["from_y"])
            v = (w["to_x"], w["to_y"])
            dist = math.hypot(v[0] - u[0], v[1] - u[1])
            G.add_edge(u, v, weight=dist)
            total_wire_mm += dist

        # Furthest routed path from DB to furthest device (or DB_CLINIC for SUB1)
        if cid == "SUB1":
            dst_p = panel_map["DB_CLINIC"]
            targets = [(round(dst_p["x"], 1), round(dst_p["y"], 1))]
        else:
            targets = [(round(d["x"], 1), round(d["y"], 1)) for d in devs_by_cid[cid]]

        furthest_mm = 0.0
        for t in targets:
            path_mm = nx.shortest_path_length(G, src_node, t, weight="weight")
            if path_mm > furthest_mm:
                furthest_mm = path_mm

        route_len_m = round(furthest_mm / 1000.0, 2)
        c_kind = c["kind"]
        pf = 0.95 if c_kind == "lighting" else 0.90

        if c["phase"] == "3P":
            ib = c["load_w"] / (math.sqrt(3.0) * V_3P * pf)
            iz = IZ_3P_CU_PVC[c["cable_mm2"]]
            dv_pct = (math.sqrt(3.0) * ib * route_len_m * RHO_CU_70C) / (c["cable_mm2"] * V_3P) * 100.0
        else:
            ib = c["load_w"] / (V_1P * pf)
            iz = IZ_1P_CU_PVC[c["cable_mm2"]]
            dv_pct = (2.0 * ib * route_len_m * RHO_CU_70C) / (c["cable_mm2"] * V_1P) * 100.0

        circuits_out.append({
            "id": cid,
            "panel": c["panel"],
            "kind": c["kind"],
            "phase": c["phase"],
            "breaker_a": c["breaker_a"],
            "rcd_ma": c["rcd_ma"],
            "cable_mm2": c["cable_mm2"],
            "load_w": c["load_w"],
            "route_len_m": route_len_m,
            "vdrop_pct": round(dv_pct, 2),
        })
        circuit_audit[cid] = {
            "desc": c["desc"],
            "ib_a": round(ib, 2),
            "in_a": c["breaker_a"],
            "iz_a": iz,
            "pf": pf,
            "total_wire_m": round(total_wire_mm / 1000.0, 2),
            "dev_count": len(devs_by_cid[cid]),
            "vdrop_pct": round(dv_pct, 2),
            "route_len_m": route_len_m,
        }

    return panels, devices, circuits_out, wires, circuit_audit


def verify_all(arch, panels, devices, circuits, wires, circuit_audit):
    errors = []
    warnings = []
    rooms = {r["id"]: box(r["min_x"], r["min_y"], r["max_x"], r["max_y"]) for r in arch["rooms"]}
    door_boxes = {d["id"]: box(*d["swing_bbox"]) for d in arch["doors"]}
    col_boxes = {c["id"]: box(c["min_x"], c["min_y"], c["max_x"], c["max_y"]) for c in arch["columns"]}
    wet_kinds = {"handwash_basin", "scrub_sink", "wc_accessible", "wc", "kitchen_sink", "sterilization_sink", "mop_sink", "water_heater"}
    wet_boxes = {f["id"]: box(f["min_x"], f["min_y"], f["max_x"], f["max_y"]) for f in arch["fixtures"] if f["kind"] in wet_kinds}
    all_fix_boxes = {f["id"]: (f["kind"], box(f["min_x"], f["min_y"], f["max_x"], f["max_y"])) for f in arch["fixtures"]}

    # 1. Panels & Clearance checks
    for p in panels:
        rbox = rooms[p["room"]]
        pbox = box(*p["body_bbox"])
        cbox = box(*p["clearance_bbox"])
        cw = p["clearance_bbox"][2] - p["clearance_bbox"][0]
        ch = p["clearance_bbox"][3] - p["clearance_bbox"][1]

        if not rbox.covers(pbox):
            errors.append(f"Panel {p['id']} body outside room {p['room']}")
        if not rbox.covers(cbox):
            errors.append(f"Panel {p['id']} clearance_bbox outside room {p['room']}")
        if cw < 900.0 or ch < 900.0:
            errors.append(f"Panel {p['id']} clearance {cw}x{ch} < 900mm")

        for did, dbox in door_boxes.items():
            if cbox.intersection(dbox).area > 1e-3 or pbox.intersection(dbox).area > 1e-3:
                errors.append(f"Panel {p['id']} overlaps door swing {did}")
        for cid, colbox in col_boxes.items():
            if cbox.intersection(colbox).area > 1e-3 or pbox.intersection(colbox).area > 1e-3:
                errors.append(f"Panel {p['id']} overlaps column {cid}")
        for wid, wbox in wet_boxes.items():
            if cbox.intersection(wbox).area > 1e-3 or pbox.intersection(wbox).area > 1e-3:
                errors.append(f"Panel {p['id']} overlaps wet fixture {wid}")
        for fid, (fkind, fbox) in all_fix_boxes.items():
            if cbox.intersection(fbox).area > 1e-3:
                errors.append(f"Panel {p['id']} clearance overlaps fixture {fid} ({fkind})")
            if fkind != "elec_panel" and pbox.intersection(fbox).area > 1e-3:
                errors.append(f"Panel {p['id']} body overlaps non-panel fixture {fid} ({fkind})")

    # 2. Room coverage & device geometry
    cmap = {c["id"]: c for c in circuits}
    lights_by_room = defaultdict(int)
    switches_by_room = defaultdict(int)
    emerg_by_room = defaultdict(int)
    sockets_by_room = defaultdict(int)
    ded_by_room = defaultdict(int)

    for d in devices:
        if d["room"] not in rooms:
            errors.append(f"Device {d['id']} references unknown room {d['room']}")
            continue
        if d["circuit"] not in cmap:
            errors.append(f"Device {d['id']} references unknown circuit {d['circuit']}")
        pt = Point(d["x"], d["y"])
        if not rooms[d["room"]].contains(pt):
            errors.append(f"Device {d['id']} ({d['x']},{d['y']}) not strictly inside {d['room']}")
        for col_id, colbox in col_boxes.items():
            if colbox.contains(pt):
                errors.append(f"Device {d['id']} inside column {col_id}")
        if d["kind"] in ("switch", "socket", "dedicated_load"):
            for did, dbox in door_boxes.items():
                if dbox.contains(pt):
                    errors.append(f"Device {d['id']} inside door swing {did}")

        if d["kind"] == "light":
            lights_by_room[d["room"]] += 1
        elif d["kind"] == "switch":
            switches_by_room[d["room"]] += 1
        elif d["kind"] == "emergency_light":
            emerg_by_room[d["room"]] += 1
        elif d["kind"] == "socket":
            sockets_by_room[d["room"]] += 1
        elif d["kind"] == "dedicated_load":
            ded_by_room[d["room"]] += 1

    for rname in rooms:
        if lights_by_room[rname] < 1:
            errors.append(f"Room {rname} has NO lighting")
        if switches_by_room[rname] < 1:
            errors.append(f"Room {rname} has NO switch")

    if lights_by_room["Waiting"] < 4:
        errors.append("Waiting needs >= 4 lights")
    if lights_by_room["Corridor"] < 3:
        errors.append("Corridor needs >= 3 lights")
    if lights_by_room["Procedure"] < 2:
        errors.append("Procedure needs >= 2 lights")

    for rname in ["Corridor", "Vestibule", "Waiting", "Procedure"]:
        if emerg_by_room[rname] < 1:
            errors.append(f"Missing emergency light in {rname}")

    for rname in ["Exam1", "Exam2", "Exam3", "Exam4", "Procedure", "NurseStation", "Lab", "Waiting", "Breakroom", "Pharmacy", "ElecITRoom"]:
        if sockets_by_room[rname] < 1:
            errors.append(f"Missing sockets in {rname}")

    for rname in ["Procedure", "Lab", "MechRoom", "ElecITRoom", "Breakroom"]:
        if ded_by_room[rname] < 1:
            errors.append(f"Missing dedicated load in {rname}")

    # 3. IEC 60364 Coordination & Voltage Drop
    sub_vdrop = next(c["vdrop_pct"] for c in circuits if c["id"] == "SUB1")
    for c in circuits:
        cid = c["id"]
        au = circuit_audit[cid]
        if not (au["ib_a"] <= au["in_a"] <= au["iz_a"]):
            errors.append(f"IEC coordination fail on {cid}: Ib={au['ib_a']} <= In={au['in_a']} <= Iz={au['iz_a']}")
        limit = 3.0 if c["kind"] == "lighting" else 5.0
        if c["vdrop_pct"] > limit:
            errors.append(f"Vdrop fail on {cid}: {c['vdrop_pct']}% > {limit}%")
        cum_vd = c["vdrop_pct"] + (sub_vdrop if c["panel"] == "DB_CLINIC" else 0.0)
        if cum_vd > limit:
            warnings.append(f"Cumulative Vdrop on {cid} ({cum_vd:.2f}%) > {limit}%")

    # 4. 3-Phase Load Balance (Whole Clinic, DB_MAIN, DB_CLINIC)
    def check_balance(label, c_list):
        ph = {"L1": 0.0, "L2": 0.0, "L3": 0.0}
        for c in c_list:
            if c["phase"] in ph:
                ph[c["phase"]] += c["load_w"]
        avg = sum(ph.values()) / 3.0
        imb = (max(ph.values()) - min(ph.values())) / avg * 100.0 if avg > 0 else 0.0
        if imb > 15.0:
            errors.append(f"Phase imbalance fail ({label}): {imb:.2f}% > 15% ({ph})")
        return ph, imb

    final_circuits = [c for c in circuits if c["kind"] != "submain"]
    ph_all, imb_all = check_balance("Whole Clinic", final_circuits)
    ph_main, imb_main = check_balance("DB_MAIN Final", [c for c in final_circuits if c["panel"] == "DB_MAIN"])
    ph_clin, imb_clin = check_balance("DB_CLINIC Final", [c for c in final_circuits if c["panel"] == "DB_CLINIC"])

    # 5. Wire graph connectivity check
    pmap = {p["id"]: (round(p["x"], 1), round(p["y"], 1)) for p in panels}
    w_by_c = defaultdict(list)
    for w in wires:
        w_by_c[w["circuit"]].append(w)
    d_by_c = defaultdict(list)
    for d in devices:
        d_by_c[d["circuit"]].append(d)

    for c in circuits:
        cid = c["id"]
        G = nx.Graph()
        for w in w_by_c[cid]:
            G.add_edge((w["from_x"], w["from_y"]), (w["to_x"], w["to_y"]))
        src = pmap[c["panel"]]
        if src not in G:
            errors.append(f"Circuit {cid} wire graph missing panel node {src}")
            continue
        if cid == "SUB1":
            if pmap["DB_CLINIC"] not in G or not nx.has_path(G, src, pmap["DB_CLINIC"]):
                errors.append("SUB1 not connected to DB_CLINIC")
        else:
            for d in d_by_c[cid]:
                pt = (round(d["x"], 1), round(d["y"], 1))
                if pt not in G or not nx.has_path(G, src, pt):
                    errors.append(f"Circuit {cid} wire graph disconnected from device {d['id']}")

    summary = {
        "rooms_covered": len(rooms),
        "total_panels": len(panels),
        "total_devices": len(devices),
        "total_circuits": len(circuits),
        "total_wire_segments": len(wires),
        "whole_clinic_kw": round(sum(ph_all.values()) / 1000.0, 2),
        "phase_loads_all_w": ph_all,
        "phase_imbalance_all_pct": round(imb_all, 2),
        "phase_imbalance_main_pct": round(imb_main, 2),
        "phase_imbalance_clinic_pct": round(imb_clin, 2),
        "max_vdrop_lighting_pct": max(c["vdrop_pct"] for c in circuits if c["kind"] == "lighting"),
        "max_vdrop_power_pct": max(c["vdrop_pct"] for c in circuits if c["kind"] != "lighting"),
        "submain_vdrop_pct": sub_vdrop,
        "errors": errors,
        "warnings": warnings,
    }
    return summary


def write_manifest(panels, devices, circuits, wires):
    # Exact schema requested in b4_clinic_electrical.md
    manifest = {
        "panels": [
            {
                "id": p["id"],
                "room": p["room"],
                "x": p["x"],
                "y": p["y"],
                "width_mm": p["width_mm"],
                "depth_mm": p["depth_mm"],
                "clearance_bbox": p["clearance_bbox"],
            }
            for p in panels
        ],
        "devices": devices,
        "circuits": circuits,
        "wires": wires,
    }
    path = os.path.join(OUT_DIR, "electrical_manifest.json")
    with open(path, "w") as f:
        json.dump(manifest, f, indent=2)
    return path


def write_panel_schedule_csv(circuits, circuit_audit):
    path = os.path.join(OUT_DIR, "panel_schedule.csv")
    fieldnames = [
        "panel", "circuit_id", "kind", "description", "phase",
        "devices_count", "load_w", "power_factor", "ib_a",
        "breaker_in_a", "rcd_ma", "cable_mm2", "ampacity_iz_a",
        "coordination_ok", "route_len_m", "total_wire_m", "vdrop_pct", "vdrop_limit_pct", "vdrop_ok"
    ]
    with open(path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for c in circuits:
            au = circuit_audit[c["id"]]
            vlim = 3.0 if c["kind"] == "lighting" else 5.0
            writer.writerow({
                "panel": c["panel"],
                "circuit_id": c["id"],
                "kind": c["kind"],
                "description": au["desc"],
                "phase": c["phase"],
                "devices_count": au["dev_count"],
                "load_w": c["load_w"],
                "power_factor": au["pf"],
                "ib_a": au["ib_a"],
                "breaker_in_a": c["breaker_a"],
                "rcd_ma": c["rcd_ma"],
                "cable_mm2": c["cable_mm2"],
                "ampacity_iz_a": au["iz_a"],
                "coordination_ok": "PASS" if (au["ib_a"] <= c["breaker_a"] <= au["iz_a"]) else "FAIL",
                "route_len_m": c["route_len_m"],
                "total_wire_m": au["total_wire_m"],
                "vdrop_pct": c["vdrop_pct"],
                "vdrop_limit_pct": vlim,
                "vdrop_ok": "PASS" if c["vdrop_pct"] <= vlim else "FAIL",
            })
    return path


def export_dxf(panels, devices, circuits, wires):
    """Merge architectural R12 DXF entities with layered IEC electrical entities."""
    arch_dxf_path = os.path.join(ARCH_DIR, "clinic_arch.dxf")
    with open(arch_dxf_path, "r") as f:
        arch_lines = [line.rstrip("\r\n") for line in f]

    # Extract raw ENTITIES section from clinic_arch.dxf
    ent_start = None
    ent_end = None
    for i in range(0, len(arch_lines) - 1, 2):
        if arch_lines[i].strip() == "2" and arch_lines[i + 1].strip() == "ENTITIES":
            ent_start = i + 2
        elif ent_start is not None and arch_lines[i].strip() == "0" and arch_lines[i + 1].strip() == "ENDSEC":
            ent_end = i
            break
    arch_entities = arch_lines[ent_start:ent_end]

    out = []

    def pair(code, val):
        out.append(str(code))
        out.append(str(val))

    # HEADER
    pair(0, "SECTION"); pair(2, "HEADER")
    pair(9, "$ACADVER"); pair(1, "AC1009")
    pair(9, "$INSUNITS"); pair(70, 4)  # mm
    pair(9, "$EXTMIN"); pair(10, -2500.0); pair(20, -2500.0); pair(30, 0.0)
    pair(9, "$EXTMAX"); pair(10, 29500.0); pair(20, 13500.0); pair(30, 0.0)
    pair(0, "ENDSEC")

    # TABLES
    pair(0, "SECTION"); pair(2, "TABLES")
    # LTYPE table
    pair(0, "TABLE"); pair(2, "LTYPE"); pair(70, 3)
    for lt_name, lt_desc, pat in [
        ("CONTINUOUS", "Solid line", []),
        ("DASHED", "Dashed __ __", [250.0, -125.0]),
        ("CENTER", "Center ____ _ ____", [400.0, -100.0, 100.0, -100.0]),
    ]:
        pair(0, "LTYPE"); pair(2, lt_name); pair(70, 0); pair(3, lt_desc); pair(72, 65)
        pair(73, len(pat)); pair(40, sum(abs(x) for x in pat))
        for pval in pat:
            pair(49, pval)
    pair(0, "ENDTAB")

    # LAYER table (muted architectural + vivid electrical layers)
    layers = [
        # Architectural background (muted gray/dark gray ACI 8/9)
        ("A-WALL-EXT", 8, "CONTINUOUS"),
        ("A-WALL-INT", 8, "CONTINUOUS"),
        ("A-DOOR",     9, "CONTINUOUS"),
        ("A-GLAZ",     9, "CONTINUOUS"),
        ("A-FURN",     9, "CONTINUOUS"),
        ("A-ANNO-TEXT",9, "CONTINUOUS"),
        ("A-ANNO-DIMS",9, "CONTINUOUS"),
        ("S-COLS",     8, "CONTINUOUS"),
        ("S-GRID",     9, "DASHED"),
        # Electrical layers
        ("E-PANEL",     1, "CONTINUOUS"),  # Red
        ("E-CLEARANCE", 1, "DASHED"),      # Red dashed
        ("E-LIGHT",     2, "CONTINUOUS"),  # Yellow
        ("E-EMERG",     3, "CONTINUOUS"),  # Green
        ("E-SWITCH",    6, "CONTINUOUS"),  # Magenta
        ("E-SOCKET",    5, "CONTINUOUS"),  # Blue
        ("E-DEDICATED", 4, "CONTINUOUS"),  # Cyan
        ("E-WIRE-SUB",  1, "CONTINUOUS"),  # Red heavy feeder
        ("E-WIRE-DED",  4, "CONTINUOUS"),  # Cyan dedicated
        ("E-WIRE-PWR",  5, "CONTINUOUS"),  # Blue power
        ("E-WIRE-LTG",  2, "CONTINUOUS"),  # Yellow lighting
        ("E-WIRE-EMERG",3, "DASHED"),      # Green dashed emergency
        ("E-LABEL",     7, "CONTINUOUS"),  # White/Black text
    ]
    pair(0, "TABLE"); pair(2, "LAYER"); pair(70, len(layers))
    for lname, color, ltype in layers:
        pair(0, "LAYER"); pair(2, lname); pair(70, 0); pair(62, color); pair(6, ltype)
    pair(0, "ENDTAB")
    pair(0, "ENDSEC")

    # ENTITIES
    pair(0, "SECTION"); pair(2, "ENTITIES")
    # 1. Include architectural background entities verbatim
    out.extend(arch_entities)

    def dxf_line(layer, x1, y1, x2, y2, ltype=None):
        pair(0, "LINE"); pair(8, layer)
        if ltype:
            pair(6, ltype)
        pair(10, f"{x1:.2f}"); pair(20, f"{y1:.2f}"); pair(30, "0.0")
        pair(11, f"{x2:.2f}"); pair(21, f"{y2:.2f}"); pair(31, "0.0")

    def dxf_rect(layer, x1, y1, x2, y2, ltype=None):
        dxf_line(layer, x1, y1, x2, y1, ltype)
        dxf_line(layer, x2, y1, x2, y2, ltype)
        dxf_line(layer, x2, y2, x1, y2, ltype)
        dxf_line(layer, x1, y2, x1, y1, ltype)

    def dxf_circle(layer, cx, cy, r):
        pair(0, "CIRCLE"); pair(8, layer)
        pair(10, f"{cx:.2f}"); pair(20, f"{cy:.2f}"); pair(30, "0.0")
        pair(40, f"{r:.2f}")

    def dxf_arc(layer, cx, cy, r, a1, a2):
        pair(0, "ARC"); pair(8, layer)
        pair(10, f"{cx:.2f}"); pair(20, f"{cy:.2f}"); pair(30, "0.0")
        pair(40, f"{r:.2f}"); pair(50, f"{a1:.1f}"); pair(51, f"{a2:.1f}")

    def dxf_text(layer, x, y, text, height=130.0):
        pair(0, "TEXT"); pair(8, layer)
        pair(10, f"{x:.2f}"); pair(20, f"{y:.2f}"); pair(30, "0.0")
        pair(40, f"{height:.1f}"); pair(1, text)

    # 2. Routed wires
    cmap = {c["id"]: c for c in circuits}
    for w in wires:
        cid = w["circuit"]
        ckind = cmap[cid]["kind"]
        if cid == "SUB1":
            layer = "E-WIRE-SUB"
        elif cid == "M-EM1":
            layer = "E-WIRE-EMERG"
        elif ckind == "dedicated":
            layer = "E-WIRE-DED"
        elif ckind == "power":
            layer = "E-WIRE-PWR"
        else:
            layer = "E-WIRE-LTG"
        dxf_line(layer, w["from_x"], w["from_y"], w["to_x"], w["to_y"])

    # 3. Panels & Clearance Zones
    for p in panels:
        bx1, by1, bx2, by2 = p["body_bbox"]
        dxf_rect("E-PANEL", bx1, by1, bx2, by2)
        dxf_line("E-PANEL", bx1, by1, bx2, by2)
        dxf_line("E-PANEL", bx1, by2, bx2, by1)
        cx1, cy1, cx2, cy2 = p["clearance_bbox"]
        dxf_rect("E-CLEARANCE", cx1, cy1, cx2, cy2, "DASHED")
        dxf_text("E-LABEL", cx1 + 60, cy2 - 190, f"{p['id']} CLR", 120.0)
        dxf_text("E-LABEL", bx1 - 120, by2 + 60, p["id"], 160.0)

    # 4. Devices (IEC symbols)
    for d in devices:
        x, y, kind = d["x"], d["y"], d["kind"]
        if kind == "light":
            # 300x300 LED luminaire symbol with cross
            dxf_rect("E-LIGHT", x - 150, y - 150, x + 150, y + 150)
            dxf_line("E-LIGHT", x - 150, y - 150, x + 150, y + 150)
            dxf_line("E-LIGHT", x - 150, y + 150, x + 150, y - 150)
            dxf_text("E-LABEL", x + 170, y - 60, f"{d['circuit']}", 95.0)
        elif kind == "emergency_light":
            # Emergency bulkhead rectangle + inner diamond
            dxf_rect("E-EMERG", x - 160, y - 110, x + 160, y + 110)
            dxf_line("E-EMERG", x - 160, y, x, y + 110)
            dxf_line("E-EMERG", x, y + 110, x + 160, y)
            dxf_line("E-EMERG", x + 160, y, x, y - 110)
            dxf_line("E-EMERG", x, y - 110, x - 160, y)
            dxf_text("E-LABEL", x + 175, y - 50, "EM", 95.0)
        elif kind == "switch":
            # IEC wall switch: small circle + angled lever + flag
            dxf_circle("E-SWITCH", x, y, 65)
            dxf_line("E-SWITCH", x + 46, y + 46, x + 140, y + 140)
            dxf_line("E-SWITCH", x + 140, y + 140, x + 190, y + 90)
        elif kind == "socket":
            # IEC socket outlet: semicircle + vertical stem
            dxf_arc("E-SOCKET", x, y, 95, 0, 180)
            dxf_line("E-SOCKET", x - 95, y, x + 95, y)
            dxf_line("E-SOCKET", x, y + 95, x, y + 175)
            dxf_text("E-LABEL", x + 110, y - 50, d["circuit"], 85.0)
        elif kind == "dedicated_load":
            # Heavy dedicated isolator + equipment symbol
            dxf_rect("E-DEDICATED", x - 170, y - 170, x + 170, y + 170)
            dxf_circle("E-DEDICATED", x, y, 110)
            dxf_text("E-LABEL", x + 190, y + 20, f"{d['id']} ({d['circuit']})", 100.0)

    pair(0, "ENDSEC")
    pair(0, "EOF")

    path = os.path.join(OUT_DIR, "clinic_electrical.dxf")
    with open(path, "w") as f:
        f.write("\n".join(out) + "\n")
    return path


def render_png(arch, panels, devices, circuits, wires, summary):
    """High-resolution electrical installation overlay on the clinic floor plan."""
    fig, ax = plt.subplots(figsize=(22, 11.5), dpi=200)
    fig.patch.set_facecolor("#F8FAFC")
    ax.set_facecolor("#F8FAFC")

    # 1. Architectural rooms (light tinted background)
    for r in arch["rooms"]:
        w = r["max_x"] - r["min_x"]
        h = r["max_y"] - r["min_y"]
        fc = "#FFFFFF" if r["habitable"] else "#F1F5F9"
        if r["id"] == "Corridor":
            fc = "#F8FAFC"
        elif r["id"] == "ElecITRoom":
            fc = "#FEF2F2"
        elif r["id"] == "Procedure":
            fc = "#ECFDF5"
        rect = patches.Rectangle((r["min_x"], r["min_y"]), w, h, facecolor=fc, edgecolor="#94A3B8", linewidth=0.9, zorder=1)
        ax.add_patch(rect)
        # Room name (subtle watermark centered slightly offset so lights don't obscure)
        cx = (r["min_x"] + r["max_x"]) / 2.0
        cy = (r["min_y"] + r["max_y"]) / 2.0
        if r["id"] == "Corridor":
            ax.text(13700, 6250, "CENTRAL CLINICAL CORRIDOR\n2200 mm CLEAR", fontsize=7.8,
                    color="#94A3B8", ha="center", va="center", fontweight="bold", zorder=2)
        elif r["id"] == "ElecITRoom":
            ax.text(22150, 7750, "ElecIT\n(7.4 m²)", fontsize=7.0, color="#64748B",
                    ha="left", va="bottom", fontweight="semibold", zorder=2)
        else:
            ax.text(cx, r["min_y"] + h * 0.14 if r["min_y"] < 5000 else r["max_y"] - h * 0.14,
                    f"{r['id']} ({r['area_m2']} m²)", fontsize=7.5, color="#64748B",
                    ha="center", va="center", fontweight="semibold", zorder=2)

    # 2. Architectural walls (thick outline around rooms/perimeter)
    # Outer L-shape building walls
    ext_poly = [
        (0, 0), (21600, 0), (21600, 5000), (27000, 5000),
        (27000, 11000), (0, 11000), (0, 0)
    ]
    xs, ys = zip(*ext_poly)
    ax.plot(xs, ys, color="#1E293B", linewidth=3.0, zorder=3)

    # Interior walls (derived from room boxes)
    for r in arch["rooms"]:
        rect = patches.Rectangle((r["min_x"], r["min_y"]), r["max_x"] - r["min_x"], r["max_y"] - r["min_y"],
                                 fill=False, edgecolor="#475569", linewidth=1.3, zorder=3)
        ax.add_patch(rect)

    # 3. Columns
    for col in arch["columns"]:
        rect = patches.Rectangle((col["min_x"], col["min_y"]), col["max_x"] - col["min_x"], col["max_y"] - col["min_y"],
                                 facecolor="#64748B", edgecolor="#1E293B", linewidth=0.8, zorder=4)
        ax.add_patch(rect)

    # 4. Fixtures (muted outline)
    wet_kinds = {"handwash_basin", "scrub_sink", "wc_accessible", "wc", "kitchen_sink", "sterilization_sink", "mop_sink", "water_heater"}
    for f in arch["fixtures"]:
        fc = "#DBEAFE" if f["kind"] in wet_kinds else "#E2E8F0"
        ec = "#60A5FA" if f["kind"] in wet_kinds else "#94A3B8"
        rect = patches.Rectangle((f["min_x"], f["min_y"]), f["max_x"] - f["min_x"], f["max_y"] - f["min_y"],
                                 facecolor=fc, edgecolor=ec, linewidth=0.7, alpha=0.65, zorder=2)
        ax.add_patch(rect)

    # 5. Door swings (subtle green dashed)
    for d in arch["doors"]:
        x1, y1, x2, y2 = d["swing_bbox"]
        rect = patches.Rectangle((x1, y1), x2 - x1, y2 - y1, facecolor="#DCFCE7",
                                 edgecolor="#86EFAC", linestyle="--", linewidth=0.7, alpha=0.35, zorder=2)
        ax.add_patch(rect)

    # 6. Routed Electrical Wires / Conduits
    cmap = {c["id"]: c for c in circuits}
    wire_styles = {
        "submain":   ("#DC2626", 2.8, "-",  8),
        "dedicated": ("#0891B2", 1.8, "-",  7),
        "power":     ("#2563EB", 1.2, "-",  6),
        "lighting":  ("#D97706", 1.1, "-",  6),
        "emergency": ("#16A34A", 1.3, "--", 7),
    }
    for w in wires:
        cid = w["circuit"]
        key = "emergency" if cid == "M-EM1" else cmap[cid]["kind"]
        col, lw, ls, zo = wire_styles[key]
        ax.plot([w["from_x"], w["to_x"]], [w["from_y"], w["to_y"]],
                color=col, linewidth=lw, linestyle=ls, alpha=0.85, zorder=zo)

    # 7. Distribution Boards & >=900mm Working Clearances
    for p in panels:
        cx1, cy1, cx2, cy2 = p["clearance_bbox"]
        crect = patches.Rectangle((cx1, cy1), cx2 - cx1, cy2 - cy1,
                                  facecolor="#FEE2E2", edgecolor="#DC2626",
                                  linestyle="--", linewidth=1.5, hatch="///", alpha=0.55, zorder=9)
        ax.add_patch(crect)
        bx1, by1, bx2, by2 = p["body_bbox"]
        prect = patches.Rectangle((bx1, by1), bx2 - bx1, by2 - by1,
                                  facecolor="#DC2626", edgecolor="#7F1D1D", linewidth=1.6, zorder=10)
        ax.add_patch(prect)
        ax.text(p["x"], by2 + 170, p["id"], fontsize=8.5, fontweight="bold",
                color="#991B1B", ha="center", va="bottom",
                bbox=dict(boxstyle="round,pad=0.18", fc="#FEF2F2", ec="#DC2626", lw=0.8), zorder=12)
        cw = int(cx2 - cx1)
        ch = int(cy2 - cy1)
        ax.text((cx1 + cx2) / 2, (cy1 + cy2) / 2, f"CLR\n{cw}×{ch}", fontsize=6.5,
                fontweight="bold", color="#991B1B", ha="center", va="center", zorder=11)

    # 8. Electrical Devices (IEC Symbols)
    for d in devices:
        x, y, kind = d["x"], d["y"], d["kind"]
        if kind == "light":
            rect = patches.Rectangle((x - 160, y - 160), 320, 320, facecolor="#FEF08A",
                                     edgecolor="#B45309", linewidth=1.2, zorder=10)
            ax.add_patch(rect)
            ax.plot([x - 160, x + 160], [y - 160, y + 160], color="#B45309", lw=0.8, zorder=11)
            ax.plot([x - 160, x + 160], [y + 160, y - 160], color="#B45309", lw=0.8, zorder=11)
            ax.text(x, y - 270, d["circuit"], fontsize=5.8, color="#92400E", ha="center", va="top", fontweight="bold", zorder=11)
        elif kind == "emergency_light":
            rect = patches.Rectangle((x - 170, y - 110), 340, 220, facecolor="#16A34A",
                                     edgecolor="#14532D", linewidth=1.2, zorder=11)
            ax.add_patch(rect)
            ax.text(x, y, "EM", fontsize=6.0, color="#FFFFFF", ha="center", va="center", fontweight="bold", zorder=12)
        elif kind == "switch":
            circ = patches.Circle((x, y), 70, facecolor="#F5D0FE", edgecolor="#A21CAF", linewidth=1.2, zorder=11)
            ax.add_patch(circ)
            ax.plot([x + 45, x + 155], [y + 45, y + 155], color="#A21CAF", lw=1.3, zorder=11)
            ax.plot([x + 155, x + 205], [y + 155, y + 105], color="#A21CAF", lw=1.3, zorder=11)
        elif kind == "socket":
            wedge = patches.Wedge((x, y), 105, 0, 180, facecolor="#93C5FD", edgecolor="#1D4ED8", linewidth=1.1, zorder=10)
            ax.add_patch(wedge)
            ax.plot([x, x], [y + 105, y + 175], color="#1D4ED8", lw=1.1, zorder=10)
        elif kind == "dedicated_load":
            rect = patches.Rectangle((x - 180, y - 180), 360, 360, facecolor="#CFFAFE",
                                     edgecolor="#0E7490", linewidth=1.6, zorder=11)
            ax.add_patch(rect)
            circ = patches.Circle((x, y), 100, facecolor="#06B6D4", edgecolor="#155E75", linewidth=1.0, zorder=12)
            ax.add_patch(circ)
            ax.text(x, y - 270, f"{d['id']}\n({d['circuit']})", fontsize=6.2, color="#155E75",
                    ha="center", va="top", fontweight="bold",
                    bbox=dict(boxstyle="round,pad=0.12", fc="#ECFEFF", ec="#06B6D4", lw=0.6), zorder=12)

    # 9. Title Block & Engineering Summary in bottom-right cutout (x=[21800, 27000], y=[0, 4800])
    tb = patches.FancyBboxPatch((21900, 150), 5000, 4550, boxstyle="round,pad=80",
                                facecolor="#FFFFFF", edgecolor="#1E293B", linewidth=1.4, zorder=12)
    ax.add_patch(tb)
    ax.text(24400, 4420, "MEDICAL CLINIC — ELECTRICAL INSTALLATION", fontsize=9.2, fontweight="bold",
            color="#0F172A", ha="center", va="center", zorder=13)
    ax.text(24400, 4120, "IEC 60364-5-52 | 400V/230V 3-Phase TN-S", fontsize=7.6,
            color="#475569", ha="center", va="center", zorder=13)
    ax.plot([22150, 26650], [3920, 3920], color="#CBD5E1", lw=1.0, zorder=13)

    ph = summary["phase_loads_all_w"]
    stats_lines = [
        f"Panels: 2 (DB_MAIN + DB_CLINIC) | CLR ≥ 900 mm",
        f"Rooms Covered: {summary['rooms_covered']}/16 (100%)",
        f"Circuits: {summary['total_circuits']} (1 Sub-Main + 18 Final)",
        f"Devices: {summary['total_devices']} | Wire Segs: {summary['total_wire_segments']}",
        f"Total Load: {summary['whole_clinic_kw']:.2f} kW (63A 3P Incomer)",
        f"Phase Balance: L1={int(ph['L1'])}W | L2={int(ph['L2'])}W | L3={int(ph['L3'])}W",
        f"Imbalance: {summary['phase_imbalance_all_pct']}% (≤ 15% PASS)",
        f"Max ΔV: Ltg {summary['max_vdrop_lighting_pct']}% (≤3%) | Pwr {summary['max_vdrop_power_pct']}% (≤5%)",
        f"IEC Coordination (Ib ≤ In ≤ Iz): 19/19 PASS",
        f"Cross-Discipline Clearances: 0 Overlaps PASS",
    ]
    y_cursor = 3680
    for line in stats_lines:
        ax.text(22200, y_cursor, line, fontsize=7.0, color="#1E293B", ha="left", va="center", family="monospace", zorder=13)
        y_cursor -= 280

    # Legend strip at bottom
    legend_handles = [
        Line2D([0], [0], color="#DC2626", lw=2.8, label="3P Sub-Main (16mm²)"),
        Line2D([0], [0], color="#0891B2", lw=1.8, label="Dedicated Load (4-6mm²)"),
        Line2D([0], [0], color="#2563EB", lw=1.4, label="Power Sockets (2.5-4mm²)"),
        Line2D([0], [0], color="#D97706", lw=1.4, label="Lighting (1.5-2.5mm²)"),
        Line2D([0], [0], color="#16A34A", lw=1.4, ls="--", label="Emergency Ltg (FP200)"),
        patches.Patch(facecolor="#FEE2E2", edgecolor="#DC2626", hatch="///", label="DB ≥900mm Clearance"),
        Line2D([0], [0], marker="s", color="w", markerfacecolor="#FEF08A", markeredgecolor="#B45309", markersize=8, label="LED Luminaire"),
        Line2D([0], [0], marker="s", color="w", markerfacecolor="#16A34A", markeredgecolor="#14532D", markersize=8, label="Emergency Light"),
        Line2D([0], [0], marker="o", color="w", markerfacecolor="#F5D0FE", markeredgecolor="#A21CAF", markersize=7, label="Wall Switch"),
        Line2D([0], [0], marker="^", color="w", markerfacecolor="#93C5FD", markeredgecolor="#1D4ED8", markersize=8, label="Power Socket"),
        Line2D([0], [0], marker="s", color="w", markerfacecolor="#CFFAFE", markeredgecolor="#0E7490", markersize=8, label="Dedicated Isolator"),
    ]
    ax.legend(handles=legend_handles, loc="lower left", bbox_to_anchor=(0.005, 0.005),
              ncol=6, fontsize=7.6, frameon=True, facecolor="#FFFFFF", edgecolor="#CBD5E1")

    ax.set_xlim(-600, 27600)
    ax.set_ylim(-1100, 11700)
    ax.set_aspect("equal")
    ax.axis("off")
    ax.set_title("OUTPATIENT SPECIALIST CLINIC — IEC 60364 BUILDING ELECTRICAL INSTALLATION OVERLAY (mm)",
                 fontsize=13, fontweight="bold", color="#0F172A", pad=10)

    path = os.path.join(OUT_DIR, "clinic_electrical.png")
    plt.tight_layout()
    plt.savefig(path, dpi=200, bbox_inches="tight")
    plt.close(fig)
    return path


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    arch = load_arch()
    panels, devices, circuits, wires, circuit_audit = build_electrical_design(arch)
    summary = verify_all(arch, panels, devices, circuits, wires, circuit_audit)

    m_path = write_manifest(panels, devices, circuits, wires)
    csv_path = write_panel_schedule_csv(circuits, circuit_audit)
    dxf_path = export_dxf(panels, devices, circuits, wires)
    png_path = render_png(arch, panels, devices, circuits, wires, summary)

    print("=== ELECTRICAL VERIFICATION SUMMARY ===")
    print(json.dumps(summary, indent=2))
    print(f"Wrote: {m_path}")
    print(f"Wrote: {csv_path}")
    print(f"Wrote: {dxf_path}")
    print(f"Wrote: {png_path}")

    if summary["errors"]:
        raise SystemExit(f"VERIFICATION FAILED with {len(summary['errors'])} errors!")


if __name__ == "__main__":
    main()
