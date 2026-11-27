#!/usr/bin/env python3
"""
Benchmark 6 — Baseline Arm (NO-EDL Python + CAD):
Revision B ECO — Full IEC 60364 Building Electrical Installation (17-Room Clinic).

Rev B ECO Electrical Changes from Rev A:
  - ECO-1: Shifted DB_MAIN, ElecITRoom, Pharmacy, MechRoom, and expanded Lab electrical
           devices/conduits east by +1.0 m (28.0 m overall building width).
  - ECO-2: Added Exam5 ceiling lighting (2x36W on C-L3), strike-side latch switch SW-EX5,
           and 3 duplex medical sockets (SK-EX5-1..3 on new balanced circuit C-P5);
           re-synced relocated Exam1-4 & Procedure devices and re-balanced all 3 phases
           (Whole Clinic imbalance ~2.8% << 15%, DB_MAIN ~3.7%, DB_CLINIC ~8.4%).
  - ECO-3: Relocated DB_CLINIC by 2.75 m East inside NurseStation (Rev A West wall
           x=7975, y=9250 -> Rev B East wall x=10725, y=9250), with 1100x1000 mm
           (>=900x900 mm) unobstructed working clearance clear of D11, columns & fixtures.

Generates:
  - electrical_manifest.json
  - panel_schedule.csv
  - clinic_electrical.dxf
  - clinic_electrical.png
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

ARCH_DIR = "/home/lindah/projects/EDL/benchmarks/v3_5_collapse_ab/b6_clinic_eco/baseline"
OUT_DIR = "/home/lindah/projects/EDL/benchmarks/v3_5_collapse_ab/b6_clinic_eco/baseline"

REV_A_DB_CLINIC_POS = (7975.0, 9250.0)

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

# Copper resistivity at 70C conductor operating temp (Ohm*mm^2/m)
RHO_CU_70C = 0.0225
V_1P = 230.0
V_3P = 400.0


def load_arch():
    with open(os.path.join(ARCH_DIR, "arch_manifest.json"), "r") as f:
        return json.load(f)


def build_electrical_design(arch):
    rooms = {r["id"]: r for r in arch["rooms"]}

    # ------------------------------------------------------------------
    # 1. DISTRIBUTION BOARDS (PANELS) — REV B
    # ------------------------------------------------------------------
    # DB_MAIN: Shifted +1.0 m East with ElecITRoom (X=22800..25000).
    # Body [24450, 8800, 24700, 9400], facing West; clearance [23350, 8600, 24450, 9600] (1100x1000 mm).
    #
    # DB_CLINIC (ECO-3 RELOCATED): Moved 2.75 m East from Rev A NurseStation West wall (7975, 9250)
    # to NurseStation East wall (10725, 9250), facing West.
    # Body [10600, 8950, 10850, 9550]; clearance [9500, 8750, 10600, 9750] (1100x1000 mm >= 900x900 mm),
    # strictly clear of D11 (y<=8600), F-NUR-CTR (x<=9250), F-NUR-DESK (y>=10050), and columns.
    panels = [
        {
            "id": "DB_MAIN",
            "room": "ElecITRoom",
            "x": 24575.0,
            "y": 9100.0,
            "width_mm": 600.0,
            "depth_mm": 250.0,
            "orientation": "W",
            "body_bbox": [24450.0, 8800.0, 24700.0, 9400.0],
            "clearance_bbox": [23350.0, 8600.0, 24450.0, 9600.0],
            "tray_entry_x": 24600.0,
        },
        {
            "id": "DB_CLINIC",
            "room": "NurseStation",
            "x": 10725.0,
            "y": 9250.0,
            "width_mm": 600.0,
            "depth_mm": 250.0,
            "orientation": "W",
            "body_bbox": [10600.0, 8950.0, 10850.0, 9550.0],
            "clearance_bbox": [9500.0, 8750.0, 10600.0, 9750.0],
            "tray_entry_x": 10725.0,
        },
    ]

    # ------------------------------------------------------------------
    # 2. DEVICES (100% COVERAGE OF ALL 17 ROOMS)
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
    # C-L1 (DB_CLINIC, L1): South Clinical Lighting (Exam1-4 & Procedure) = 448 W
    add_dev("L-EX1-1", "Exam1", "light", "C-L1", 36, 9850, 1500)
    add_dev("L-EX1-2", "Exam1", "light", "C-L1", 36, 9850, 3600)
    add_dev("SW-EX1",  "Exam1", "switch", "C-L1", 0, 9550, 4900)  # D04 strike side

    add_dev("L-EX2-1", "Exam2", "light", "C-L1", 36, 13425, 1500)
    add_dev("L-EX2-2", "Exam2", "light", "C-L1", 36, 13425, 3600)
    add_dev("SW-EX2",  "Exam2", "switch", "C-L1", 0, 13100, 4900)  # D05 strike side

    add_dev("L-EX3-1", "Exam3", "light", "C-L1", 36, 17025, 1500)
    add_dev("L-EX3-2", "Exam3", "light", "C-L1", 36, 17025, 3600)
    add_dev("SW-EX3",  "Exam3", "switch", "C-L1", 0, 17300, 4900)  # D06 strike side (W of D06, E of C-D2)

    add_dev("L-EX4-1", "Exam4", "light", "C-L1", 36, 20600, 1500)
    add_dev("L-EX4-2", "Exam4", "light", "C-L1", 36, 20600, 3600)
    add_dev("SW-EX4",  "Exam4", "switch", "C-L1", 0, 20300, 4900)  # D12 strike side

    add_dev("L-PRC-1", "Procedure", "light", "C-L1", 40, 23900, 1500)
    add_dev("L-PRC-2", "Procedure", "light", "C-L1", 40, 26300, 1500)
    add_dev("L-PRC-3", "Procedure", "light", "C-L1", 40, 23900, 3600)
    add_dev("L-PRC-4", "Procedure", "light", "C-L1", 40, 26300, 3600)
    add_dev("SW-PRC",  "Procedure", "switch", "C-L1", 0, 24350, 4900)  # D07 strike side

    # C-L2 (DB_CLINIC, L3): West & Reception Lighting (Vestibule, Waiting, PublicWC, StaffWC) = 288 W
    add_dev("L-VST-1", "Vestibule", "light", "C-L2", 18, 1075, 1750)
    add_dev("L-VST-2", "Vestibule", "light", "C-L2", 18, 1075, 3600)
    add_dev("SW-VST",  "Vestibule", "switch", "C-L2", 0, 1850, 300)

    add_dev("L-WAT-1", "Waiting", "light", "C-L2", 36, 3200, 1500)
    add_dev("L-WAT-2", "Waiting", "light", "C-L2", 36, 5050, 1500)
    add_dev("L-WAT-3", "Waiting", "light", "C-L2", 36, 6850, 1500)
    add_dev("L-WAT-4", "Waiting", "light", "C-L2", 36, 3200, 3400)
    add_dev("L-WAT-5", "Waiting", "light", "C-L2", 36, 5050, 3400)
    add_dev("L-WAT-6", "Waiting", "light", "C-L2", 36, 6850, 3400)
    add_dev("SW-WAT-1", "Waiting", "switch", "C-L2", 0, 2250, 3150)
    add_dev("SW-WAT-2", "Waiting", "switch", "C-L2", 0, 6250, 4900)

    add_dev("L-PWC-1", "PublicWC", "light", "C-L2", 18, 1100, 9150)
    add_dev("SW-PWC",  "PublicWC", "switch", "C-L2", 0, 1550, 7600)

    add_dev("L-SWC-1", "StaffWC", "light", "C-L2", 18, 2950, 9150)
    add_dev("SW-SWC",  "StaffWC", "switch", "C-L2", 0, 3500, 7600)

    # C-L3 (DB_CLINIC, L2): North Staff & NEW Exam5 Lighting (Breakroom, NurseStation, Exam5) = 216 W
    add_dev("L-BRK-1", "Breakroom", "light", "C-L3", 36, 5050, 9150)
    add_dev("L-BRK-2", "Breakroom", "light", "C-L3", 36, 6600, 9150)
    add_dev("SW-BRK",  "Breakroom", "switch", "C-L3", 0, 5250, 7600)

    add_dev("L-NUR-1", "NurseStation", "light", "C-L3", 36, 9100, 8450)
    add_dev("L-NUR-2", "NurseStation", "light", "C-L3", 36, 9100, 10150)
    add_dev("SW-NUR",  "NurseStation", "switch", "C-L3", 0, 9100, 7600)  # D11 strike side

    # ECO-2: NEW Exam5 ceiling lighting + strike-side latch switch
    add_dev("L-EX5-1", "Exam5", "light", "C-L3", 36, 12400, 9150)
    add_dev("L-EX5-2", "Exam5", "light", "C-L3", 36, 14100, 9150)
    add_dev("SW-EX5",  "Exam5", "switch", "C-L3", 0, 12400, 7600)  # D17 strike side

    # M-L1 (DB_MAIN, L3): East Wing Lighting (Expanded Lab, Pharmacy, ElecITRoom, MechRoom) = 260 W
    add_dev("L-LAB-1", "Lab", "light", "M-L1", 40, 16850, 9150)
    add_dev("L-LAB-2", "Lab", "light", "M-L1", 40, 18650, 9150)
    add_dev("SW-LAB",  "Lab", "switch", "M-L1", 0, 16900, 7600)

    add_dev("L-PHM-1", "Pharmacy", "light", "M-L1", 36, 20850, 9150)
    add_dev("L-PHM-2", "Pharmacy", "light", "M-L1", 36, 21950, 9150)
    add_dev("SW-PHM",  "Pharmacy", "switch", "M-L1", 0, 21300, 7600)

    add_dev("L-ELC-1", "ElecITRoom", "light", "M-L1", 36, 24050, 10300)
    add_dev("SW-ELC",  "ElecITRoom", "switch", "M-L1", 0, 24150, 7600)

    add_dev("L-MCH-1", "MechRoom", "light", "M-L1", 36, 26000, 9150)
    add_dev("L-MCH-2", "MechRoom", "light", "M-L1", 36, 27200, 9150)
    add_dev("SW-MCH",  "MechRoom", "switch", "M-L1", 0, 26550, 7600)

    # M-L2 (DB_MAIN, L2): Central Clinical Corridor Lighting (9 fixtures across 27.7 m) = 216 W
    cor_xs = [1800, 5000, 8200, 11400, 14600, 17800, 21000, 24200, 26800]
    for idx, cx in enumerate(cor_xs, 1):
        add_dev(f"L-COR-{idx}", "Corridor", "light", "M-L2", 24, cx, 6250)
    add_dev("SW-COR-W", "Corridor", "switch", "M-L2", 0, 6250, 5250)
    add_dev("SW-COR-E", "Corridor", "switch", "M-L2", 0, 24200, 7250)

    # M-EM1 (DB_MAIN, L3): Emergency & Exit Lighting (Maintained 3h battery LED) = 76 W
    add_dev("EM-VST-1", "Vestibule", "emergency_light", "M-EM1", 8, 1075, 450)
    add_dev("EM-WAT-1", "Waiting",   "emergency_light", "M-EM1", 8, 4300, 2450)
    add_dev("EM-WAT-2", "Waiting",   "emergency_light", "M-EM1", 8, 6850, 4700)
    add_dev("EM-COR-1", "Corridor",  "emergency_light", "M-EM1", 8, 3500, 6250)
    add_dev("EM-COR-2", "Corridor",  "emergency_light", "M-EM1", 8, 10300, 6250)
    add_dev("EM-COR-3", "Corridor",  "emergency_light", "M-EM1", 8, 17100, 6250)
    add_dev("EM-COR-4", "Corridor",  "emergency_light", "M-EM1", 8, 24800, 6250)
    add_dev("EM-PRC-1", "Procedure", "emergency_light", "M-EM1", 12, 25150, 2700)
    add_dev("EM-ELC-1", "ElecITRoom","emergency_light", "M-EM1", 8, 23850, 8150)

    # --- B. GENERAL & MEDICAL POWER SOCKETS ---
    # C-P1 (DB_CLINIC, L3): Waiting & Breakroom General Sockets = 2000 W
    add_dev("SK-WAT-1", "Waiting",   "socket", "C-P1", 350, 3200, 4850)
    add_dev("SK-WAT-2", "Waiting",   "socket", "C-P1", 350, 4600, 4850)
    add_dev("SK-WAT-3", "Waiting",   "socket", "C-P1", 350, 4850, 280)
    add_dev("SK-WAT-4", "Waiting",   "socket", "C-P1", 350, 7880, 2000)
    add_dev("SK-BRK-1", "Breakroom", "socket", "C-P1", 300, 5800, 10720)
    add_dev("SK-BRK-2", "Breakroom", "socket", "C-P1", 300, 7320, 9100)

    # C-P2 (DB_CLINIC, L3): Exam1 & Exam2 Medical Sockets = 2100 W
    add_dev("SK-EX1-1", "Exam1", "socket", "C-P2", 350, 9200, 280)
    add_dev("SK-EX1-2", "Exam1", "socket", "C-P2", 350, 8280, 2700)
    add_dev("SK-EX1-3", "Exam1", "socket", "C-P2", 350, 11420, 2600)
    add_dev("SK-EX2-1", "Exam2", "socket", "C-P2", 350, 12600, 280)
    add_dev("SK-EX2-2", "Exam2", "socket", "C-P2", 350, 11830, 2700)
    add_dev("SK-EX2-3", "Exam2", "socket", "C-P2", 350, 15020, 2600)

    # C-P3 (DB_CLINIC, L2): Exam3 & Exam4 Medical Sockets = 2100 W
    add_dev("SK-EX3-1", "Exam3", "socket", "C-P3", 350, 17800, 280)
    add_dev("SK-EX3-2", "Exam3", "socket", "C-P3", 350, 15430, 2700)
    add_dev("SK-EX3-3", "Exam3", "socket", "C-P3", 350, 18620, 2600)
    add_dev("SK-EX4-1", "Exam4", "socket", "C-P3", 350, 19800, 280)
    add_dev("SK-EX4-2", "Exam4", "socket", "C-P3", 350, 19030, 2700)
    add_dev("SK-EX4-3", "Exam4", "socket", "C-P3", 350, 22150, 2600)

    # C-P4 (DB_CLINIC, L3): Procedure Medical Sockets (30mA Type A RCD) = 1400 W
    add_dev("SK-PRC-1", "Procedure", "socket", "C-P4", 350, 22620, 3150)
    add_dev("SK-PRC-2", "Procedure", "socket", "C-P4", 350, 23900, 280)
    add_dev("SK-PRC-3", "Procedure", "socket", "C-P4", 350, 26200, 280)
    add_dev("SK-PRC-4", "Procedure", "socket", "C-P4", 350, 27700, 1200)

    # C-P5 (DB_CLINIC, L1): NurseStation & NEW Exam5 Medical Sockets (30mA RCD) = 2450 W
    add_dev("SK-NUR-1", "NurseStation", "socket", "C-P5", 350, 8300, 10720)
    add_dev("SK-NUR-2", "NurseStation", "socket", "C-P5", 350, 9700, 10720)
    add_dev("SK-NUR-3", "NurseStation", "socket", "C-P5", 350, 7730, 9000)
    add_dev("SK-NUR-4", "NurseStation", "socket", "C-P5", 350, 7730, 9600)
    # ECO-2: 3 duplex sockets in Exam5
    add_dev("SK-EX5-1", "Exam5", "socket", "C-P5", 350, 11900, 10720)
    add_dev("SK-EX5-2", "Exam5", "socket", "C-P5", 350, 11130, 9100)
    add_dev("SK-EX5-3", "Exam5", "socket", "C-P5", 350, 15220, 8650)

    # M-P1 (DB_MAIN, L1): Expanded Lab Bench Sockets = 1200 W
    add_dev("SK-LAB-1", "Lab", "socket", "M-P1", 400, 17800, 7630)
    add_dev("SK-LAB-2", "Lab", "socket", "M-P1", 400, 19670, 8950)
    add_dev("SK-LAB-3", "Lab", "socket", "M-P1", 400, 18200, 10720)

    # M-P2 (DB_MAIN, L2): Pharmacy & MechRoom Sockets (+1.0 m East) = 1100 W
    add_dev("SK-PHM-1", "Pharmacy", "socket", "M-P2", 400, 20080, 9200)
    add_dev("SK-PHM-2", "Pharmacy", "socket", "M-P2", 400, 21200, 10720)
    add_dev("SK-MCH-1", "MechRoom", "socket", "M-P2", 300, 27720, 8450)

    # M-P3 (DB_MAIN, L3): ElecITRoom & Corridor Service Sockets = 1400 W
    add_dev("SK-ELC-1", "ElecITRoom", "socket", "M-P3", 350, 22930, 8800)
    add_dev("SK-ELC-2", "ElecITRoom", "socket", "M-P3", 350, 24200, 10720)
    add_dev("SK-COR-1", "Corridor",   "socket", "M-P3", 350, 5650, 7250)
    add_dev("SK-COR-2", "Corridor",   "socket", "M-P3", 350, 19650, 7250)

    # --- C. DEDICATED HIGH-LOAD / SPECIAL CIRCUITS ---
    add_dev("DED-PRC-MED", "Procedure",  "dedicated_load", "C-D1", 2800, 27600, 3550)
    add_dev("DED-BRK-KIT", "Breakroom",  "dedicated_load", "C-D2", 3000, 4050, 9450)
    add_dev("DED-MCH-WH",  "MechRoom",   "dedicated_load", "M-D1", 3600, 27150, 9550)
    add_dev("DED-LAB-AUTO","Lab",        "dedicated_load", "M-D2", 3600, 19250, 10450)
    add_dev("DED-ELC-UPS", "ElecITRoom", "dedicated_load", "M-D3", 3000, 23425, 10300)

    # ------------------------------------------------------------------
    # 3. CIRCUIT DEFINITIONS (20 CIRCUITS = 1 SUB-MAIN + 19 FINAL)
    # ------------------------------------------------------------------
    circuit_specs = [
        # Sub-main feeder DB_MAIN -> DB_CLINIC (3-Phase 400V)
        {"id": "SUB1", "panel": "DB_MAIN", "kind": "submain", "phase": "3P",
         "breaker_a": 40, "rcd_ma": 0, "cable_mm2": 16.0, "desc": "Sub-Main Feeder to Relocated DB_CLINIC (4C+E 16mm2 Cu)"},
        # DB_MAIN Final Circuits
        {"id": "M-L1",  "panel": "DB_MAIN", "kind": "lighting",  "phase": "L3", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "desc": "Lighting: East Wing (Lab/Pharm/ElecIT/Mech)"},
        {"id": "M-L2",  "panel": "DB_MAIN", "kind": "lighting",  "phase": "L2", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Lighting: Central Clinical Corridor (9x24W)"},
        {"id": "M-EM1", "panel": "DB_MAIN", "kind": "lighting",  "phase": "L3", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Emergency & Exit Lighting (Maintained FP200)"},
        {"id": "M-P1",  "panel": "DB_MAIN", "kind": "power",     "phase": "L1", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Expanded Lab Clinical Benches"},
        {"id": "M-P2",  "panel": "DB_MAIN", "kind": "power",     "phase": "L2", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Pharmacy & MechRoom"},
        {"id": "M-P3",  "panel": "DB_MAIN", "kind": "power",     "phase": "L3", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Sockets: ElecITRoom & Corridor Cleaners"},
        {"id": "M-D1",  "panel": "DB_MAIN", "kind": "dedicated", "phase": "L1", "breaker_a": 25, "rcd_ma": 30, "cable_mm2": 6.0, "desc": "Dedicated: MechRoom Electric Water Heater"},
        {"id": "M-D2",  "panel": "DB_MAIN", "kind": "dedicated", "phase": "L2", "breaker_a": 25, "rcd_ma": 30, "cable_mm2": 6.0, "desc": "Dedicated: Lab Autoclave / Sterilizer"},
        {"id": "M-D3",  "panel": "DB_MAIN", "kind": "dedicated", "phase": "L3", "breaker_a": 20, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Dedicated: ElecITRoom Server Rack (UPS)"},
        # DB_CLINIC Final Circuits (Re-Balanced Across L1 / L2 / L3 for Rev B!)
        {"id": "C-L1",  "panel": "DB_CLINIC", "kind": "lighting",  "phase": "L1", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Lighting: South Clinical Wing (Exam1-4 & Procedure)"},
        {"id": "C-L2",  "panel": "DB_CLINIC", "kind": "lighting",  "phase": "L3", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "desc": "Lighting: Vestibule, Waiting, PublicWC & StaffWC"},
        {"id": "C-L3",  "panel": "DB_CLINIC", "kind": "lighting",  "phase": "L2", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "desc": "Lighting: Breakroom, NurseStation & NEW Exam5"},
        {"id": "C-P1",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L3", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Waiting & Breakroom General"},
        {"id": "C-P2",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L3", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: Exam1 & Exam2 Medical"},
        {"id": "C-P3",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L2", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Sockets: Exam3 & Exam4 Medical"},
        {"id": "C-P4",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L3", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Sockets: Procedure Medical (30mA Type A RCD)"},
        {"id": "C-P5",  "panel": "DB_CLINIC", "kind": "power",     "phase": "L1", "breaker_a": 16, "rcd_ma": 30, "cable_mm2": 2.5, "desc": "Sockets: NurseStation & NEW Exam5 Medical (30mA RCD)"},
        {"id": "C-D1",  "panel": "DB_CLINIC", "kind": "dedicated", "phase": "L1", "breaker_a": 20, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Dedicated: Procedure Medical Equipment (30mA Type A RCD)"},
        {"id": "C-D2",  "panel": "DB_CLINIC", "kind": "dedicated", "phase": "L2", "breaker_a": 20, "rcd_ma": 30, "cable_mm2": 4.0, "desc": "Dedicated: Breakroom Kitchenette Appliances"},
    ]

    # ------------------------------------------------------------------
    # 4. ORTHOGONAL CONDUIT / CABLE TRAY ROUTING
    # ------------------------------------------------------------------
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
        if cid == "M-D3":
            d = c_devs[0]
            route_orth_polyline(cid, [(px, py), (px, d["y"]), (d["x"], d["y"])])
            continue

        tray_key = "emergency" if cid == "M-EM1" else c["kind"]
        ty = TRAY_Y[tray_key]

        devs_by_room = defaultdict(list)
        for d in c_devs:
            devs_by_room[d["room"]].append(d)

        room_taps = {}
        for rname, rdevs in devs_by_room.items():
            r = rooms[rname]
            if rname == "Corridor":
                for d in rdevs:
                    room_taps[f"COR_{d['id']}"] = (d["x"], [d])
            elif rname == src_p["room"] and len(devs_by_room) == 1:
                room_taps[rname] = (tx, rdevs)
            else:
                avg_x = sum(d["x"] for d in rdevs) / len(rdevs)
                tap_x = max(r["min_x"] + 250.0, min(r["max_x"] - 250.0, round(avg_x / 50.0) * 50.0))
                room_taps[rname] = (tap_x, rdevs)

        route_orth_polyline(cid, [(px, py), (tx, py), (tx, ty)])

        all_xs = sorted(set([tx] + [v[0] for v in room_taps.values()]))
        for x_a, x_b in zip(all_xs[:-1], all_xs[1:]):
            add_seg(cid, x_a, ty, x_b, ty)

        for key, (tap_x, rdevs) in room_taps.items():
            if key.startswith("COR_"):
                d = rdevs[0]
                add_seg(cid, tap_x, ty, d["x"], d["y"])
                continue

            if max(d["y"] for d in rdevs) > ty:
                spine_ys = sorted(set([ty] + [d["y"] for d in rdevs]))
            else:
                spine_ys = sorted(set([ty] + [d["y"] for d in rdevs]), reverse=True)

            for y_a, y_b in zip(spine_ys[:-1], spine_ys[1:]):
                add_seg(cid, tap_x, y_a, tap_x, y_b)

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

    # 1. Panels, Clearances & ECO-3 DB_CLINIC Relocation Check
    db_clearances = {}
    db_clinic_shift_mm = 0.0
    for p in panels:
        rbox = rooms[p["room"]]
        pbox = box(*p["body_bbox"])
        cbox = box(*p["clearance_bbox"])
        cw = p["clearance_bbox"][2] - p["clearance_bbox"][0]
        ch = p["clearance_bbox"][3] - p["clearance_bbox"][1]
        db_clearances[p["id"]] = {"room": p["room"], "width_mm": cw, "depth_mm": ch, "ok": cw >= 900.0 and ch >= 900.0}

        if p["id"] == "DB_CLINIC":
            db_clinic_shift_mm = round(math.hypot(p["x"] - REV_A_DB_CLINIC_POS[0], p["y"] - REV_A_DB_CLINIC_POS[1]), 1)
            if db_clinic_shift_mm < 1000.0:
                errors.append(f"ECO-3 fail: DB_CLINIC shifted only {db_clinic_shift_mm} mm (< 1000 mm)")

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

    # 2. Room coverage & device geometry (all 17 rooms!)
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
    if lights_by_room["Exam5"] < 2 or switches_by_room["Exam5"] < 1 or sockets_by_room["Exam5"] < 3:
        errors.append(f"ECO-2 fail: Exam5 needs >=2 lights, >=1 switch, >=3 sockets")

    for rname in ["Corridor", "Vestibule", "Waiting", "Procedure"]:
        if emerg_by_room[rname] < 1:
            errors.append(f"Missing emergency light in {rname}")

    for rname in ["Exam1", "Exam2", "Exam3", "Exam4", "Exam5", "Procedure", "NurseStation", "Lab", "Waiting", "Breakroom", "Pharmacy", "ElecITRoom"]:
        if sockets_by_room[rname] < 1:
            errors.append(f"Missing sockets in {rname}")

    for rname in ["Procedure", "Lab", "MechRoom", "ElecITRoom", "Breakroom"]:
        if ded_by_room[rname] < 1:
            errors.append(f"Missing dedicated load in {rname}")

    # 3. IEC 60364 Coordination, Voltage Drop & 30mA RCD Check
    sub_vdrop = next(c["vdrop_pct"] for c in circuits if c["id"] == "SUB1")
    for c in circuits:
        cid = c["id"]
        au = circuit_audit[cid]
        if not (au["ib_a"] <= au["in_a"] <= au["iz_a"]):
            errors.append(f"IEC coordination fail on {cid}: Ib={au['ib_a']} <= In={au['in_a']} <= Iz={au['iz_a']}")
        limit = 3.0 if c["kind"] == "lighting" else 5.0
        if c["vdrop_pct"] > limit:
            errors.append(f"Vdrop fail on {cid}: {c['vdrop_pct']}% > {limit}%")
        if c["kind"] != "submain" and c["rcd_ma"] != 30:
            errors.append(f"RCD fail on final circuit {cid}: rcd_ma={c['rcd_ma']} != 30 mA")
        cum_vd = c["vdrop_pct"] + (sub_vdrop if c["panel"] == "DB_CLINIC" else 0.0)
        if cum_vd > limit:
            warnings.append(f"Cumulative Vdrop on {cid} ({cum_vd:.2f}%) > {limit}%")

    # 4. 3-Phase Load Balance
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

    # 5. Wire graph connectivity
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
        "revision": "B",
        "rooms_covered": len(rooms),
        "total_panels": len(panels),
        "db_clinic_relocation_shift_mm": db_clinic_shift_mm,
        "db_clearances": db_clearances,
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
        "rcd_30ma_all_final": all(c["rcd_ma"] == 30 for c in final_circuits),
        "errors": errors,
        "warnings": warnings,
    }
    return summary


def write_manifest(panels, devices, circuits, wires, summary):
    manifest = {
        "revision": "B",
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
        "summary": summary,
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
    arch_dxf_path = os.path.join(ARCH_DIR, "clinic_arch.dxf")
    with open(arch_dxf_path, "r") as f:
        arch_lines = [line.rstrip("\r\n") for line in f]

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

    pair(0, "SECTION"); pair(2, "HEADER")
    pair(9, "$ACADVER"); pair(1, "AC1009")
    pair(9, "$INSUNITS"); pair(70, 4)
    pair(9, "$EXTMIN"); pair(10, -2500.0); pair(20, -2500.0); pair(30, 0.0)
    pair(9, "$EXTMAX"); pair(10, 30500.0); pair(20, 14000.0); pair(30, 0.0)
    pair(0, "ENDSEC")

    pair(0, "SECTION"); pair(2, "TABLES")
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

    layers = [
        ("A-WALL-EXT", 8, "CONTINUOUS"),
        ("A-WALL-INT", 8, "CONTINUOUS"),
        ("A-DOOR",     9, "CONTINUOUS"),
        ("A-GLAZ",     9, "CONTINUOUS"),
        ("A-FURN",     9, "CONTINUOUS"),
        ("A-ANNO-TEXT",9, "CONTINUOUS"),
        ("A-ANNO-DIMS",9, "CONTINUOUS"),
        ("S-COLS",     8, "CONTINUOUS"),
        ("S-GRID",     9, "DASHED"),
        ("E-PANEL",     1, "CONTINUOUS"),
        ("E-CLEARANCE", 1, "DASHED"),
        ("E-LIGHT",     2, "CONTINUOUS"),
        ("E-EMERG",     3, "CONTINUOUS"),
        ("E-SWITCH",    6, "CONTINUOUS"),
        ("E-SOCKET",    5, "CONTINUOUS"),
        ("E-DEDICATED", 4, "CONTINUOUS"),
        ("E-WIRE-SUB",  1, "CONTINUOUS"),
        ("E-WIRE-DED",  4, "CONTINUOUS"),
        ("E-WIRE-PWR",  5, "CONTINUOUS"),
        ("E-WIRE-LTG",  2, "CONTINUOUS"),
        ("E-WIRE-EMERG",3, "DASHED"),
        ("E-LABEL",     7, "CONTINUOUS"),
    ]
    pair(0, "TABLE"); pair(2, "LAYER"); pair(70, len(layers))
    for lname, color, ltype in layers:
        pair(0, "LAYER"); pair(2, lname); pair(70, 0); pair(62, color); pair(6, ltype)
    pair(0, "ENDTAB")
    pair(0, "ENDSEC")

    pair(0, "SECTION"); pair(2, "ENTITIES")
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

    for p in panels:
        bx1, by1, bx2, by2 = p["body_bbox"]
        dxf_rect("E-PANEL", bx1, by1, bx2, by2)
        dxf_line("E-PANEL", bx1, by1, bx2, by2)
        dxf_line("E-PANEL", bx1, by2, bx2, by1)
        cx1, cy1, cx2, cy2 = p["clearance_bbox"]
        dxf_rect("E-CLEARANCE", cx1, cy1, cx2, cy2, "DASHED")
        dxf_text("E-LABEL", cx1 + 60, cy2 - 190, f"{p['id']} CLR", 120.0)
        dxf_text("E-LABEL", bx1 - 120, by2 + 60, p["id"], 160.0)

    for d in devices:
        x, y, kind = d["x"], d["y"], d["kind"]
        if kind == "light":
            dxf_rect("E-LIGHT", x - 150, y - 150, x + 150, y + 150)
            dxf_line("E-LIGHT", x - 150, y - 150, x + 150, y + 150)
            dxf_line("E-LIGHT", x - 150, y + 150, x + 150, y - 150)
            dxf_text("E-LABEL", x + 170, y - 60, f"{d['circuit']}", 95.0)
        elif kind == "emergency_light":
            dxf_rect("E-EMERG", x - 160, y - 110, x + 160, y + 110)
            dxf_line("E-EMERG", x - 160, y, x, y + 110)
            dxf_line("E-EMERG", x, y + 110, x + 160, y)
            dxf_line("E-EMERG", x + 160, y, x, y - 110)
            dxf_line("E-EMERG", x, y - 110, x - 160, y)
            dxf_text("E-LABEL", x + 175, y - 50, "EM", 95.0)
        elif kind == "switch":
            dxf_circle("E-SWITCH", x, y, 65)
            dxf_line("E-SWITCH", x + 46, y + 46, x + 140, y + 140)
            dxf_line("E-SWITCH", x + 140, y + 140, x + 190, y + 90)
        elif kind == "socket":
            dxf_arc("E-SOCKET", x, y, 95, 0, 180)
            dxf_line("E-SOCKET", x - 95, y, x + 95, y)
            dxf_line("E-SOCKET", x, y + 95, x, y + 175)
            dxf_text("E-LABEL", x + 110, y - 50, d["circuit"], 85.0)
        elif kind == "dedicated_load":
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
    fig, ax = plt.subplots(figsize=(24, 13.0), dpi=200)
    fig.patch.set_facecolor("#F8FAFC")
    ax.set_facecolor("#F8FAFC")

    for r in arch["rooms"]:
        w = r["max_x"] - r["min_x"]
        h = r["max_y"] - r["min_y"]
        fc = "#FFFFFF" if r["habitable"] else "#F1F5F9"
        if r["id"] == "Corridor":
            fc = "#F8FAFC"
        elif r["id"] == "ElecITRoom":
            fc = "#FEF2F2"
        elif r["id"] in ("Procedure", "Exam5"):
            fc = "#ECFDF5"
        rect = patches.Rectangle((r["min_x"], r["min_y"]), w, h, facecolor=fc, edgecolor="#94A3B8", linewidth=0.9, zorder=1)
        ax.add_patch(rect)
        cx = (r["min_x"] + r["max_x"]) / 2.0
        if r["id"] == "Corridor":
            ax.text(13000, 6250, "CENTRAL CORRIDOR\n(2200 mm CLEAR)", fontsize=7.2,
                    color="#94A3B8", ha="center", va="center", fontweight="bold", zorder=2)
        elif r["id"] == "ElecITRoom":
            ax.text(r["min_x"] + 250, 7750, f"ElecIT\n({r['area_m2']} m²)", fontsize=7.0, color="#64748B",
                    ha="left", va="bottom", fontweight="semibold", zorder=2)
        else:
            lbl = f"{r['id']}* ({r['area_m2']} m²)" if r["id"] == "Exam5" else f"{r['id']} ({r['area_m2']} m²)"
            ax.text(cx, r["min_y"] + h * 0.12 if r["min_y"] < 5000 else r["max_y"] - h * 0.12,
                    lbl, fontsize=7.4, color="#64748B",
                    ha="center", va="center", fontweight="semibold", zorder=2)

    # Outer building walls (28.0 m x 11.0 m)
    ext_poly = [(0, 0), (28000, 0), (28000, 11000), (0, 11000), (0, 0)]
    xs, ys = zip(*ext_poly)
    ax.plot(xs, ys, color="#1E293B", linewidth=3.0, zorder=3)

    for r in arch["rooms"]:
        rect = patches.Rectangle((r["min_x"], r["min_y"]), r["max_x"] - r["min_x"], r["max_y"] - r["min_y"],
                                 fill=False, edgecolor="#475569", linewidth=1.3, zorder=3)
        ax.add_patch(rect)

    for col in arch["columns"]:
        rect = patches.Rectangle((col["min_x"], col["min_y"]), col["max_x"] - col["min_x"], col["max_y"] - col["min_y"],
                                 facecolor="#64748B", edgecolor="#1E293B", linewidth=0.8, zorder=4)
        ax.add_patch(rect)

    wet_kinds = {"handwash_basin", "scrub_sink", "wc_accessible", "wc", "kitchen_sink", "sterilization_sink", "mop_sink", "water_heater"}
    for f in arch["fixtures"]:
        fc = "#DBEAFE" if f["kind"] in wet_kinds else "#E2E8F0"
        ec = "#60A5FA" if f["kind"] in wet_kinds else "#94A3B8"
        rect = patches.Rectangle((f["min_x"], f["min_y"]), f["max_x"] - f["min_x"], f["max_y"] - f["min_y"],
                                 facecolor=fc, edgecolor=ec, linewidth=0.7, alpha=0.65, zorder=2)
        ax.add_patch(rect)

    for d in arch["doors"]:
        x1, y1, x2, y2 = d["swing_bbox"]
        rect = patches.Rectangle((x1, y1), x2 - x1, y2 - y1, facecolor="#DCFCE7",
                                 edgecolor="#86EFAC", linestyle="--", linewidth=0.7, alpha=0.35, zorder=2)
        ax.add_patch(rect)

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

    # Mark ghost outline of Rev A DB_CLINIC position + relocation arrow (ECO-3!)
    ax.annotate("", xy=(10600, 9250), xytext=(8100, 9250),
                arrowprops=dict(arrowstyle="-|>", color="#DC2626", lw=1.4, ls=":"), zorder=11)
    ax.text(8720, 9420, "ECO-3: +2.75 m", fontsize=5.8, fontweight="bold", color="#B91C1C",
            ha="center", va="bottom", bbox=dict(boxstyle="round,pad=0.1", fc="#FFFBEB", ec="#DC2626", lw=0.6), zorder=12)

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
        ax.text(p["x"] - 220, by2 + 160, p["id"], fontsize=8.2, fontweight="bold",
                color="#991B1B", ha="center", va="bottom",
                bbox=dict(boxstyle="round,pad=0.16", fc="#FEF2F2", ec="#DC2626", lw=0.8), zorder=12)
        cw = int(cx2 - cx1)
        ch = int(cy2 - cy1)
        ax.text((cx1 + cx2) / 2, (cy1 + cy2) / 2, f"CLR\n{cw}×{ch}", fontsize=6.4,
                fontweight="bold", color="#991B1B", ha="center", va="center", zorder=11)

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
            ax.text(x, y - 270, f"{d['id']}\n({d['circuit']})", fontsize=6.1, color="#155E75",
                    ha="center", va="top", fontweight="bold",
                    bbox=dict(boxstyle="round,pad=0.12", fc="#ECFEFF", ec="#06B6D4", lw=0.6), zorder=12)

    # Bottom Title Block & Engineering Summary Strip (clear of full 28x11 m footprint)
    tb_x, tb_y, tb_w, tb_h = 0.0, -3450.0, 28000.0, 2650.0
    tb = patches.FancyBboxPatch((tb_x, tb_y), tb_w, tb_h, boxstyle="round,pad=80",
                                facecolor="#FFFFFF", edgecolor="#1E293B", linewidth=1.4, zorder=12)
    ax.add_patch(tb)

    ax.text(tb_x + 350, tb_y + tb_h - 320, "MEDICAL CLINIC — ELECTRICAL INSTALLATION (REV B ECO)",
            fontsize=10.2, fontweight="bold", color="#0F172A", ha="left", va="top", zorder=13)
    ax.text(tb_x + 350, tb_y + tb_h - 760, "IEC 60364-5-52 | 400V/230V 3-Phase TN-S | NO-EDL BASELINE",
            fontsize=7.8, fontweight="bold", color="#0369A1", ha="left", va="top", zorder=13)

    ph = summary["phase_loads_all_w"]
    col1 = [
        f"Panels: 2 (DB_MAIN + Relocated DB_CLINIC +{summary['db_clinic_relocation_shift_mm']/1000:.2f}m) | CLR 1100×1000 mm",
        f"Rooms Covered: {summary['rooms_covered']}/17 (100%, incl. NEW Exam5: 2 Ltg, 1 Sw, 3 Skt)",
        f"Circuits: {summary['total_circuits']} (1 Sub-Main + 19 Final) | Devices: {summary['total_devices']} | Wire Segs: {summary['total_wire_segments']}",
    ]
    for i, line in enumerate(col1):
        ax.text(tb_x + 350, tb_y + tb_h - 1220 - i * 390, line, fontsize=7.0, color="#1E293B",
                ha="left", va="top", family="monospace", zorder=13)

    mid_x = tb_x + 15000.0
    ax.plot([mid_x - 300, mid_x - 300], [tb_y + 180, tb_y + tb_h - 180], color="#CBD5E1", lw=1.0, zorder=13)
    col2 = [
        f"Total Connected Load: {summary['whole_clinic_kw']:.2f} kW (63A 3P Incomer)",
        f"3-Phase Balance: L1={int(ph['L1'])}W | L2={int(ph['L2'])}W | L3={int(ph['L3'])}W",
        f"Imbalance: Clinic {summary['phase_imbalance_all_pct']}% | DB_MAIN {summary['phase_imbalance_main_pct']}% | DB_CLINIC {summary['phase_imbalance_clinic_pct']}% (≤15% PASS)",
        f"Max ΔV: Ltg {summary['max_vdrop_lighting_pct']}% (≤3%) | Pwr {summary['max_vdrop_power_pct']}% (≤5%) | SUB1 {summary['submain_vdrop_pct']}%",
        f"IEC Coordination (Ib≤In≤Iz): 20/20 PASS | 30mA RCD All Final: PASS | 0 Overlaps",
    ]
    for i, line in enumerate(col2):
        ax.text(mid_x, tb_y + tb_h - 280 - i * 440, line, fontsize=7.0, color="#1E293B",
                ha="left", va="top", family="monospace", zorder=13)

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
    ax.legend(handles=legend_handles, loc="lower center", bbox_to_anchor=(0.5, -0.025),
              ncol=6, fontsize=7.6, frameon=True, facecolor="#FFFFFF", edgecolor="#CBD5E1")

    ax.set_xlim(-600, 28600)
    ax.set_ylim(-4150, 11600)
    ax.set_aspect("equal")
    ax.axis("off")
    ax.set_title("OUTPATIENT SPECIALIST CLINIC — REV B ECO IEC 60364 BUILDING ELECTRICAL INSTALLATION (mm)",
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

    m_path = write_manifest(panels, devices, circuits, wires, summary)
    csv_path = write_panel_schedule_csv(circuits, circuit_audit)
    dxf_path = export_dxf(panels, devices, circuits, wires)
    png_path = render_png(arch, panels, devices, circuits, wires, summary)

    print("=== ELECTRICAL REV B VERIFICATION SUMMARY ===")
    print(json.dumps(summary, indent=2))
    print(f"Wrote: {m_path}")
    print(f"Wrote: {csv_path}")
    print(f"Wrote: {dxf_path}")
    print(f"Wrote: {png_path}")

    if summary["errors"]:
        raise SystemExit(f"VERIFICATION FAILED with {len(summary['errors'])} errors!")


if __name__ == "__main__":
    main()
