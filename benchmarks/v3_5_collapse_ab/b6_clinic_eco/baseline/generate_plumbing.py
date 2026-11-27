#!/usr/bin/env python3
"""
Benchmark 6 — Baseline Arm (NO-EDL Python + CAD):
Revision B ECO — Full Building Plumbing System Generator & Verifier (17-Room Clinic).
Overlays IPC-compliant CW, HW, SAN, and VENT plumbing onto the Rev B Clinic.

Rev B ECO Plumbing Changes from Rev A:
  - ECO-1: Shifted MechRoom riser (MAIN_CW), WH1, MOP_MCH, VTR-2, and east corridor
           CW/HW/SAN/VENT headers east by +1.0 m (building width 27.0 -> 28.0 m), keeping
           strict 0-pipe clearance around shifted ElecITRoom (X=22800..25000).
  - ECO-2: Added Exam5Basin in North Wing Exam5 (1 DFU, CW/HW/SAN/VENT + isolation valves),
           re-synced relocated Exam1-4 & Procedure wet chases to new Rev B walls/columns
           (0 pipe-vs-column clashes across all 18 columns; total load 23 -> 24 DFU).

Outputs written to /home/lindah/projects/EDL/benchmarks/v3_5_collapse_ab/b6_clinic_eco/baseline/:
  - clinic_plumbing.dxf
  - clinic_plumbing.png
  - plumbing_schedule.csv
  - plumbing_manifest.json
"""

import csv
import json
import math
from collections import defaultdict
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as patches
import networkx as nx
from shapely.geometry import LineString, Point, box


BASE_DIR = Path("/home/lindah/projects/EDL/benchmarks/v3_5_collapse_ab/b6_clinic_eco/baseline")
ARCH_DIR = BASE_DIR
OUT_DIR = BASE_DIR
ARCH_MANIFEST_PATH = ARCH_DIR / "arch_manifest.json"
ARCH_DXF_PATH = ARCH_DIR / "clinic_arch.dxf"


# ---------------------------------------------------------------------------
# 1. Fixture & Plumbing Network Specification — REV B
# ---------------------------------------------------------------------------

# Corridor main header Y-coordinates (inside Central Corridor y = 5150..7350)
Y_SAN_MAIN = 7050.0   # Below slab, East -> West, north of W12 (y=5550..6950)
Y_VENT_MAIN = 6650.0  # Ceiling plenum, ties to VTR-1 & VTR-2
Y_CW_MAIN = 5850.0    # Ceiling plenum, fed from MAIN_CW in MechRoom
Y_HW_MAIN = 5500.0    # Ceiling plenum, fed from WH1 in MechRoom

# Special source/terminal coordinates (MechRoom shifted +1000 mm East per ECO-1)
MAIN_CW_PT = (27150.0, 8200.0)   # Main municipal cold water riser in MechRoom
SAN_OUT_PT = (150.0, Y_SAN_MAIN) # Building sanitary drain exit at West corridor wall
VTR1_PT = (1850.0, 10700.0)      # Main 100mm Vent Through Roof (PublicWC north wet wall)
VTR2_PT = (25360.0, 10680.0)     # Secondary 75mm Vent Through Roof (MechRoom north wall)


def build_fixtures():
    """All 14 wet fixtures & mechanical plumbing equipment (13 Rev A + Exam5Basin)."""
    return [
        # South clinical wing (re-synced to Rev B Exam1-4 & Procedure layouts)
        {
            "id": "LAV_EX1", "room": "Exam1", "kind": "basin",
            "x": 10500.0, "y": 4675.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 10360.0, "HW": 10440.0, "SAN": 10520.0, "VENT": 10600.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 4850.0,
        },
        {
            "id": "LAV_EX2", "room": "Exam2", "kind": "basin",
            "x": 14450.0, "y": 4675.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 14320.0, "HW": 14400.0, "SAN": 14480.0, "VENT": 14560.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 4850.0,
        },
        {
            "id": "LAV_EX3", "room": "Exam3", "kind": "basin",
            "x": 15850.0, "y": 4675.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 15820.0, "HW": 15900.0, "SAN": 15980.0, "VENT": 16060.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 4850.0,
        },
        {
            "id": "LAV_EX4", "room": "Exam4", "kind": "basin",
            "x": 21500.0, "y": 4675.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 21370.0, "HW": 21450.0, "SAN": 21530.0, "VENT": 21610.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 4850.0,
        },
        {
            "id": "SINK_PROC", "room": "Procedure", "kind": "sink",
            "x": 26200.0, "y": 4625.0, "dfu": 2,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 26080.0, "HW": 26160.0, "SAN": 26240.0, "VENT": 26320.0},
            "dn": {"CW": 20, "HW": 20, "SAN": 50, "VENT": 40},
            "valve_y": 4850.0,
        },
        # North wing
        {
            "id": "LAV_PWC", "room": "PublicWC", "kind": "basin",
            "x": 475.0, "y": 9950.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 260.0, "HW": 320.0, "SAN": 380.0, "VENT": 430.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 9200.0,
        },
        {
            "id": "WC_PWC", "room": "PublicWC", "kind": "wc",
            "x": 1650.0, "y": 10425.0, "dfu": 4,
            "has_cw": True, "has_hw": False, "has_san": True, "has_vent": True,
            "chase": {"CW": 1550.0, "SAN": 1650.0, "VENT": 1850.0},
            "dn": {"CW": 20, "SAN": 100, "VENT": 50},
            "valve_y": 9400.0,
        },
        {
            "id": "LAV_SWC", "room": "StaffWC", "kind": "basin",
            "x": 2500.0, "y": 9925.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 2250.0, "HW": 2310.0, "SAN": 2370.0, "VENT": 2430.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 9200.0,
        },
        {
            "id": "WC_SWC", "room": "StaffWC", "kind": "wc",
            "x": 3250.0, "y": 10425.0, "dfu": 4,
            "has_cw": True, "has_hw": False, "has_san": True, "has_vent": True,
            "chase": {"CW": 3440.0, "SAN": 3520.0, "VENT": 3620.0},
            "dn": {"CW": 20, "SAN": 100, "VENT": 50},
            "valve_y": 9400.0,
        },
        {
            "id": "SINK_BRK", "room": "Breakroom", "kind": "sink",
            "x": 4225.0, "y": 10430.0, "dfu": 2,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 3910.0, "HW": 3970.0, "SAN": 4030.0, "VENT": 4090.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 9400.0,
        },
        # NEW ECO-2: Exam5 Clinical Wash Basin on North Wing wet wall shared with Lab
        {
            "id": "Exam5Basin", "room": "Exam5", "kind": "basin",
            "x": 15025.0, "y": 10100.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 14960.0, "HW": 15025.0, "SAN": 15105.0, "VENT": 15185.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 9200.0,
        },
        {
            "id": "SINK_LAB", "room": "Lab", "kind": "sink",
            "x": 15850.0, "y": 10225.0, "dfu": 2,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 15560.0, "HW": 15620.0, "SAN": 15685.0, "VENT": 15750.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 9200.0,
        },
        # Shifted +1.0 m East per ECO-1 (MechRoom X = 25150..27850)
        {
            "id": "MOP_MCH", "room": "MechRoom", "kind": "mop_sink",
            "x": 25650.0, "y": 10325.0, "dfu": 3,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"SAN": 25260.0, "VENT": 25360.0},
            "dn": {"CW": 20, "HW": 20, "SAN": 75, "VENT": 50},
            "valve_y": 9200.0,
        },
        {
            "id": "WH1", "room": "MechRoom", "kind": "water_heater",
            "x": 27150.0, "y": 10150.0, "dfu": 0,
            "has_cw": True, "has_hw": True, "has_san": False, "has_vent": False,
            "chase": {},
            "dn": {"CW": 25, "HW": 25},
            "valve_y": 9550.0,
        },
    ]


class PipeBuilder:
    """Builds exact node-to-node orthogonal pipe networks with automatic IDs."""

    def __init__(self):
        self.pipes = []
        self.counters = defaultdict(int)

    def add_seg(self, system, dn_mm, slope_pct, p1, p2, dfu_load=0, tag=""):
        x1, y1 = float(p1[0]), float(p1[1])
        x2, y2 = float(p2[0]), float(p2[1])
        if abs(x1 - x2) < 1e-6 and abs(y1 - y2) < 1e-6:
            return
        self.counters[system] += 1
        prefix = {"CW": "P-CW", "HW": "P-HW", "SAN": "P-SAN", "VENT": "P-VNT"}[system]
        pid = f"{prefix}-{self.counters[system]:02d}"
        self.pipes.append({
            "id": pid,
            "system": system,
            "dn_mm": int(dn_mm),
            "slope_pct": round(float(slope_pct), 2),
            "from_x": round(x1, 1),
            "from_y": round(y1, 1),
            "to_x": round(x2, 1),
            "to_y": round(y2, 1),
            "dfu_load": int(dfu_load),
            "_tag": tag,
        })

    def add_polyline(self, system, dn_mm, slope_pct, pts, dfu_load=0, tag=""):
        for a, b in zip(pts[:-1], pts[1:]):
            self.add_seg(system, dn_mm, slope_pct, a, b, dfu_load=dfu_load, tag=tag)


def generate_network(fixtures):
    pb = PipeBuilder()
    valves = []
    cleanouts = []

    def add_valve(vid, system, pt, desc="", dn_mm=20, room="Corridor"):
        valves.append({
            "id": vid,
            "system": system,
            "x": round(float(pt[0]), 1),
            "y": round(float(pt[1]), 1),
            "_desc": desc,
            "_dn_mm": int(dn_mm),
            "_room": room,
        })

    def add_cleanout(cid, pt, desc=""):
        cleanouts.append({
            "id": cid,
            "x": round(float(pt[0]), 1),
            "y": round(float(pt[1]), 1),
            "_desc": desc,
        })

    # ------------------------------------------------------------------
    # A. SANITARY DRAINAGE NETWORK (SAN)
    # ------------------------------------------------------------------
    san_junctions = {}
    for fx in fixtures:
        if not fx["has_san"]:
            continue
        x_s = fx["chase"]["SAN"]
        pt_fix = (fx["x"], fx["y"])
        pt_corner = (x_s, fx["y"])
        pt_main = (x_s, Y_SAN_MAIN)
        pb.add_polyline(
            "SAN", fx["dn"]["SAN"], 2.0,
            [pt_fix, pt_corner, pt_main],
            dfu_load=fx["dfu"],
            tag=f"branch:{fx['id']}",
        )
        san_junctions[x_s] = san_junctions.get(x_s, 0) + fx["dfu"]

    san_nodes_x = sorted(set(list(san_junctions.keys()) + [250.0, SAN_OUT_PT[0]]), reverse=True)

    cum_dfu = 0
    for x_hi, x_lo in zip(san_nodes_x[:-1], san_nodes_x[1:]):
        cum_dfu += san_junctions.get(x_hi, 0)
        if x_hi <= 3520.0:
            dn = 150
            slope = 1.5
        else:
            dn = 100
            slope = 2.0
        pb.add_seg(
            "SAN", dn, slope,
            (x_hi, Y_SAN_MAIN), (x_lo, Y_SAN_MAIN),
            dfu_load=cum_dfu,
            tag="trunk:SAN_MAIN",
        )

    # Cleanouts along SAN trunk (re-synced to Rev B junctions)
    add_cleanout("CO-PROC", (26240.0, Y_SAN_MAIN), "Upper terminal & Procedure scrub sink cleanout")
    add_cleanout("CO-MCH",  (25260.0, Y_SAN_MAIN), "MechRoom mop sink stack base cleanout")
    add_cleanout("CO-EX4",  (21530.0, Y_SAN_MAIN), "Exam4 South Wing junction cleanout")
    add_cleanout("CO-LAB",  (15685.0, Y_SAN_MAIN), "Lab / Exam5 wet-wall junction cleanout")
    add_cleanout("CO-EX1",  (10520.0, Y_SAN_MAIN), "Mid-corridor / Exam1 cleanout")
    add_cleanout("CO-BRK",  (4030.0,  Y_SAN_MAIN), "Breakroom / StaffWC transition cleanout")
    add_cleanout("CO-WC",   (1650.0,  Y_SAN_MAIN), "PublicWC 150mm collector cleanout")
    add_cleanout("CO-OUT",  (250.0,   Y_SAN_MAIN), "Main building drain exit cleanout")

    # ------------------------------------------------------------------
    # B. VENTING NETWORK (VENT)
    # ------------------------------------------------------------------
    vent_junctions_x = set()
    for fx in fixtures:
        if not fx["has_vent"]:
            continue
        x_v = fx["chase"]["VENT"]
        pt_fix = (fx["x"], fx["y"])
        pt_corner = (x_v, fx["y"])
        pt_main = (x_v, Y_VENT_MAIN)
        pb.add_polyline(
            "VENT", fx["dn"]["VENT"], 1.0,
            [pt_fix, pt_corner, pt_main],
            dfu_load=0,
            tag=f"vent:{fx['id']}",
        )
        vent_junctions_x.add(x_v)

    # Tie VTR-1 (PublicWC) and VTR-2 (MechRoom, shifted +1.0 m East)
    pb.add_seg("VENT", 100, 0.0, (1850.0, 10425.0), VTR1_PT, dfu_load=0, tag="vtr:VTR-1")
    pb.add_seg("VENT", 75, 0.0, (25360.0, 10325.0), VTR2_PT, dfu_load=0, tag="vtr:VTR-2")

    vent_nodes_x = sorted(vent_junctions_x, reverse=True)
    for x_hi, x_lo in zip(vent_nodes_x[:-1], vent_nodes_x[1:]):
        dn = 75 if x_hi <= 15750.0 else 50
        pb.add_seg(
            "VENT", dn, 0.5,
            (x_hi, Y_VENT_MAIN), (x_lo, Y_VENT_MAIN),
            dfu_load=0,
            tag="header:VENT_MAIN",
        )

    # ------------------------------------------------------------------
    # C. DOMESTIC COLD WATER NETWORK (CW) — shifted +1.0 m in MechRoom
    # ------------------------------------------------------------------
    cw_v_main = (27150.0, 8500.0)
    cw_dist_tee = (27150.0, 8850.0)
    cw_mop_tee = (27150.0, 9200.0)
    cw_wh_valve = (27150.0, 9550.0)
    wh_pt = (27150.0, 10150.0)

    pb.add_seg("CW", 32, 0.0, MAIN_CW_PT, cw_v_main, tag="main:CW_RISER")
    add_valve("V-MAIN-CW", "CW", cw_v_main, "Main building CW shutoff valve (DN32)", dn_mm=32, room="MechRoom")
    pb.add_seg("CW", 32, 0.0, cw_v_main, cw_dist_tee, tag="main:CW_RISER")

    pb.add_seg("CW", 25, 0.0, cw_dist_tee, cw_mop_tee, tag="mech:CW_WH_FEED")
    pb.add_seg("CW", 25, 0.0, cw_mop_tee, cw_wh_valve, tag="mech:CW_WH_FEED")
    add_valve("V-WH1-CW", "CW", cw_wh_valve, "Water Heater WH1 cold inlet isolation valve (DN25)", dn_mm=25, room="MechRoom")
    pb.add_seg("CW", 25, 0.0, cw_wh_valve, wh_pt, tag="mech:CW_WH_FEED")

    # MechRoom MOP_MCH CW branch (north of D16 swing y=8450)
    cw_mop_valve = (26200.0, 9200.0)
    pb.add_seg("CW", 20, 0.0, cw_mop_tee, cw_mop_valve, tag="branch:MOP_MCH")
    add_valve("V-MOP-CW", "CW", cw_mop_valve, "Mop sink CW isolation valve (DN20)", dn_mm=20, room="MechRoom")
    pb.add_polyline("CW", 20, 0.0, [cw_mop_valve, (25650.0, 9200.0), (25650.0, 10325.0)], tag="branch:MOP_MCH")

    # MechRoom south drop to Corridor CW Main Header (east of D16, west of C-F2 at 27850)
    cw_corr_start_x = 27450.0
    pb.add_polyline(
        "CW", 32, 0.0,
        [cw_dist_tee, (cw_corr_start_x, 8850.0), (cw_corr_start_x, Y_CW_MAIN)],
        tag="header:CW_DROP",
    )

    cw_junctions_x = {cw_corr_start_x, 26500.0, 7800.0}
    add_valve("V-Z1-CW", "CW", (26500.0, Y_CW_MAIN), "Clinical Wing CW zone isolation valve (DN32)", dn_mm=32, room="Corridor")
    add_valve("V-Z2-CW", "CW", (7800.0, Y_CW_MAIN), "West Restrooms & Breakroom CW zone isolation valve (DN25)", dn_mm=25, room="Corridor")

    for fx in fixtures:
        if not fx["has_cw"] or fx["room"] == "MechRoom":
            continue
        x_c = fx["chase"]["CW"]
        v_pt = (x_c, fx["valve_y"])
        pb.add_seg("CW", fx["dn"]["CW"], 0.0, (x_c, Y_CW_MAIN), v_pt, tag=f"branch:{fx['id']}")
        add_valve(f"V-{fx['id']}-CW", "CW", v_pt, f"{fx['id']} CW shutoff valve (DN{fx['dn']['CW']})", dn_mm=fx["dn"]["CW"], room=fx["room"])
        pb.add_polyline(
            "CW", fx["dn"]["CW"], 0.0,
            [v_pt, (x_c, fx["y"]), (fx["x"], fx["y"])],
            tag=f"branch:{fx['id']}",
        )
        cw_junctions_x.add(x_c)

    cw_nodes_x = sorted(cw_junctions_x, reverse=True)
    for x_hi, x_lo in zip(cw_nodes_x[:-1], cw_nodes_x[1:]):
        dn = 32 if x_lo >= 10360.0 else 25
        pb.add_seg(
            "CW", dn, 0.0,
            (x_hi, Y_CW_MAIN), (x_lo, Y_CW_MAIN),
            tag="header:CW_MAIN",
        )

    # ------------------------------------------------------------------
    # D. DOMESTIC HOT WATER NETWORK (HW) — shifted +1.0 m in MechRoom
    # ------------------------------------------------------------------
    hw_drop_x = 26750.0
    hw_wh_corner = (hw_drop_x, 10150.0)
    hw_wh_valve = (hw_drop_x, 9850.0)
    hw_mop_tee = (hw_drop_x, 9550.0)

    pb.add_seg("HW", 25, 0.0, wh_pt, hw_wh_corner, tag="mech:HW_WH_OUT")
    pb.add_seg("HW", 25, 0.0, hw_wh_corner, hw_wh_valve, tag="mech:HW_WH_OUT")
    add_valve("V-WH1-HW", "HW", hw_wh_valve, "Water Heater WH1 hot outlet isolation valve (DN25)", dn_mm=25, room="MechRoom")
    pb.add_seg("HW", 25, 0.0, hw_wh_valve, hw_mop_tee, tag="mech:HW_WH_OUT")

    hw_mop_valve = (26200.0, 9550.0)
    pb.add_seg("HW", 20, 0.0, hw_mop_tee, hw_mop_valve, tag="branch:MOP_MCH")
    add_valve("V-MOP-HW", "HW", hw_mop_valve, "Mop sink HW isolation valve (DN20)", dn_mm=20, room="MechRoom")
    pb.add_polyline(
        "HW", 20, 0.0,
        [hw_mop_valve, (25780.0, 9550.0), (25780.0, 10325.0), (25650.0, 10325.0)],
        tag="branch:MOP_MCH",
    )

    pb.add_seg("HW", 25, 0.0, hw_mop_tee, (hw_drop_x, Y_HW_MAIN), tag="header:HW_DROP")

    hw_junctions_x = {hw_drop_x, 26500.0, 7800.0}
    add_valve("V-Z1-HW", "HW", (26500.0, Y_HW_MAIN), "Clinical Wing HW zone isolation valve (DN25)", dn_mm=25, room="Corridor")
    add_valve("V-Z2-HW", "HW", (7800.0, Y_HW_MAIN), "West Restrooms & Breakroom HW zone isolation valve (DN25)", dn_mm=25, room="Corridor")

    for fx in fixtures:
        if not fx["has_hw"] or fx["room"] == "MechRoom":
            continue
        x_h = fx["chase"]["HW"]
        v_pt = (x_h, fx["valve_y"])
        pb.add_seg("HW", fx["dn"]["HW"], 0.0, (x_h, Y_HW_MAIN), v_pt, tag=f"branch:{fx['id']}")
        add_valve(f"V-{fx['id']}-HW", "HW", v_pt, f"{fx['id']} HW shutoff valve (DN{fx['dn']['HW']})", dn_mm=fx["dn"]["HW"], room=fx["room"])
        pb.add_polyline(
            "HW", fx["dn"]["HW"], 0.0,
            [v_pt, (x_h, fx["y"]), (fx["x"], fx["y"])],
            tag=f"branch:{fx['id']}",
        )
        hw_junctions_x.add(x_h)

    hw_nodes_x = sorted(hw_junctions_x, reverse=True)
    for x_hi, x_lo in zip(hw_nodes_x[:-1], hw_nodes_x[1:]):
        pb.add_seg(
            "HW", 25, 0.0,
            (x_hi, Y_HW_MAIN), (x_lo, Y_HW_MAIN),
            tag="header:HW_MAIN",
        )

    return pb.pipes, valves, cleanouts


# ---------------------------------------------------------------------------
# 2. Rigorous Engineering & Geometric Verification — REV B
# ---------------------------------------------------------------------------

def verify_all(arch, fixtures, valves, cleanouts, pipes):
    errors = []
    warnings = []

    rooms_by_id = {r["id"]: r for r in arch["rooms"]}
    elec_room = rooms_by_id["ElecITRoom"]
    elec_poly = box(elec_room["min_x"], elec_room["min_y"], elec_room["max_x"], elec_room["max_y"])
    col_polys = [(c["id"], box(c["min_x"], c["min_y"], c["max_x"], c["max_y"])) for c in arch["columns"]]
    bldg_w = arch.get("building_width_mm", 28000.0)
    bldg_h = arch.get("building_height_mm", 11000.0)
    bldg_poly = box(0, 0, bldg_w, bldg_h)

    # 1. Clash check: ElecITRoom (zero tolerance + clearance reporting)
    min_elec_dist = float("inf")
    for p in pipes:
        line = LineString([(p["from_x"], p["from_y"]), (p["to_x"], p["to_y"])])
        d = line.distance(elec_poly)
        min_elec_dist = min(min_elec_dist, d)
        if line.intersects(elec_poly):
            errors.append(f"CLASH: Pipe {p['id']} ({p['system']}) intersects ElecITRoom!")
    for item_type, items in [("fixture", fixtures), ("valve", valves), ("cleanout", cleanouts)]:
        for it in items:
            pt = Point(it["x"], it["y"])
            d = pt.distance(elec_poly)
            min_elec_dist = min(min_elec_dist, d)
            if pt.intersects(elec_poly):
                errors.append(f"CLASH: {item_type} {it['id']} inside ElecITRoom!")

    # 2. Clash check: All 18 structural columns (exact + pipe outer radius buffer)
    min_col_clearance = float("inf")
    tightest_col_pair = None
    for p in pipes:
        line = LineString([(p["from_x"], p["from_y"]), (p["to_x"], p["to_y"])])
        r_mm = p["dn_mm"] / 2.0
        for cid, cpoly in col_polys:
            d = line.distance(cpoly) - r_mm
            if d < min_col_clearance:
                min_col_clearance = d
                tightest_col_pair = (p["id"], cid, round(d, 1))
            if line.intersects(cpoly) or d <= 0:
                errors.append(f"CLASH: Pipe {p['id']} (DN{p['dn_mm']}) clashes with column {cid} (clear={d:.1f}mm)!")

    # 3. Building envelope check
    for p in pipes:
        line = LineString([(p["from_x"], p["from_y"]), (p["to_x"], p["to_y"])])
        if not bldg_poly.buffer(1.0).contains(line):
            errors.append(f"BOUNDS: Pipe {p['id']} extends outside building envelope!")

    # 4. Graph connectivity per system
    graphs = {}
    for sys_name in ("CW", "HW", "SAN", "VENT"):
        G = nx.Graph()
        for p in pipes:
            if p["system"] == sys_name:
                u = (round(p["from_x"], 1), round(p["from_y"], 1))
                v = (round(p["to_x"], 1), round(p["to_y"], 1))
                length = math.hypot(v[0] - u[0], v[1] - u[1])
                G.add_edge(u, v, id=p["id"], dn_mm=p["dn_mm"], length=length, dfu=p["dfu_load"])
        graphs[sys_name] = G
        n_comp = nx.number_connected_components(G)
        if n_comp != 1:
            errors.append(f"CONNECTIVITY: {sys_name} has {n_comp} connected components (expected 1)!")

    # 5. Fixture service & room containment verification (including Exam5Basin!)
    fx_ids = {fx["id"] for fx in fixtures}
    if "Exam5Basin" not in fx_ids:
        errors.append("ECO-2 FAIL: Exam5Basin missing from plumbing fixtures!")

    for fx in fixtures:
        rm = rooms_by_id[fx["room"]]
        if not (rm["min_x"] <= fx["x"] <= rm["max_x"] and rm["min_y"] <= fx["y"] <= rm["max_y"]):
            errors.append(f"ROOM: Fixture {fx['id']} ({fx['x']},{fx['y']}) outside room {fx['room']}!")
        pt = (round(fx["x"], 1), round(fx["y"], 1))
        for flag, sys_name in [("has_cw", "CW"), ("has_hw", "HW"), ("has_san", "SAN"), ("has_vent", "VENT")]:
            if fx[flag]:
                if pt not in graphs[sys_name]:
                    errors.append(f"DISCONNECTED: Fixture {fx['id']} not connected to {sys_name} graph!")
                else:
                    inc_dns = [d["dn_mm"] for _, _, d in graphs[sys_name].edges(pt, data=True)]
                    max_dn = max(inc_dns)
                    if sys_name == "SAN":
                        if fx["kind"] == "wc" and max_dn < 100:
                            errors.append(f"SIZE: WC {fx['id']} SAN branch DN{max_dn} < 100mm!")
                        if fx["kind"] in ("basin", "sink", "mop_sink") and max_dn < 40:
                            errors.append(f"SIZE: {fx['id']} SAN branch DN{max_dn} < 40mm!")
                    if sys_name in ("CW", "HW") and max_dn < 15:
                        errors.append(f"SIZE: {fx['id']} {sys_name} branch DN{max_dn} < 15mm!")

    if MAIN_CW_PT not in graphs["CW"]:
        errors.append("SOURCE: MAIN_CW not connected to CW graph!")
    if SAN_OUT_PT not in graphs["SAN"]:
        errors.append("EXIT: SAN_OUT not connected to SAN graph!")
    if VTR1_PT not in graphs["VENT"] or VTR2_PT not in graphs["VENT"]:
        errors.append("VENT: VTR stack not connected to VENT graph!")

    # 6. Valves & cleanouts lie on exact nodes
    for v in valves:
        pt = (round(v["x"], 1), round(v["y"], 1))
        if pt not in graphs[v["system"]]:
            errors.append(f"VALVE: {v['id']} ({v['system']}) at {pt} not on a pipe node!")
    for co in cleanouts:
        pt = (round(co["x"], 1), round(co["y"], 1))
        if pt not in graphs["SAN"]:
            errors.append(f"CLEANOUT: {co['id']} at {pt} not on a SAN pipe node!")

    # 7. SAN slope & DFU balance verification
    total_fx_dfu = sum(f["dfu"] for f in fixtures)
    max_pipe_dfu = max(p["dfu_load"] for p in pipes if p["system"] == "SAN")
    if max_pipe_dfu != total_fx_dfu:
        errors.append(f"DFU: Max SAN trunk DFU ({max_pipe_dfu}) != total fixture DFU ({total_fx_dfu})!")
    for p in pipes:
        if p["system"] == "SAN":
            if not (1.0 <= p["slope_pct"] <= 2.0):
                errors.append(f"SLOPE: SAN pipe {p['id']} slope {p['slope_pct']}% outside 1.0–2.0%!")

    # 8. Door swing intersection audit
    door_hits = 0
    for p in pipes:
        if p["_tag"].startswith("branch:"):
            line = LineString([(p["from_x"], p["from_y"]), (p["to_x"], p["to_y"])])
            for d in arch["doors"]:
                sb = d["swing_bbox"]
                if line.intersects(box(sb[0], sb[1], sb[2], sb[3])):
                    door_hits += 1

    stats = {
        "total_fixtures": len(fixtures),
        "total_dfu": total_fx_dfu,
        "max_san_trunk_dfu": max_pipe_dfu,
        "total_pipes": len(pipes),
        "total_valves": len(valves),
        "total_cleanouts": len(cleanouts),
        "min_elec_clearance_mm": round(min_elec_dist, 1),
        "elec_room_dry_check": "PASS (0 pipes/equip)" if min_elec_dist > 0 else "FAIL",
        "min_col_clearance_mm": round(min_col_clearance, 1),
        "tightest_col_pair": tightest_col_pair,
        "door_swing_crossings": door_hits,
    }
    return errors, warnings, stats


# ---------------------------------------------------------------------------
# 3. DXF Export (AC1009 R12 — Architectural Underlay + Plumbing Layers)
# ---------------------------------------------------------------------------

def write_dxf(arch_dxf_path, out_dxf_path, fixtures, valves, cleanouts, pipes, total_dfu):
    with open(arch_dxf_path, "r") as f:
        arch_lines = [l.rstrip("\r\n") for l in f]

    ent_start = None
    ent_end = None
    for i in range(0, len(arch_lines) - 1, 2):
        if arch_lines[i].strip() == "2" and arch_lines[i + 1].strip() == "ENTITIES":
            ent_start = i + 2
        elif ent_start is not None and arch_lines[i].strip() == "0" and arch_lines[i + 1].strip() == "ENDSEC":
            ent_end = i
            break
    arch_entity_lines = arch_lines[ent_start:ent_end]

    existing_layers = []
    in_layer_tab = False
    i = 0
    while i < len(arch_lines) - 1:
        code, val = arch_lines[i].strip(), arch_lines[i + 1].strip()
        if code == "2" and val == "LAYER":
            in_layer_tab = True
        elif in_layer_tab and code == "0" and val == "ENDTAB":
            break
        elif in_layer_tab and code == "0" and val == "LAYER":
            layer_name, color, ltype = "", "8", "CONTINUOUS"
            j = i + 2
            while j < len(arch_lines) - 1 and not (arch_lines[j].strip() == "0"):
                c2, v2 = arch_lines[j].strip(), arch_lines[j + 1].strip()
                if c2 == "2":
                    layer_name = v2
                elif c2 == "62":
                    color = v2
                elif c2 == "6":
                    ltype = v2
                j += 2
            dim_color = "9" if layer_name in ("S-GRID", "A-FURN", "A-ANNO-TEXT", "A-ANNO-DIMS", "A-DOOR") else "8"
            existing_layers.append((layer_name, dim_color, ltype))
            i = j
            continue
        i += 2

    plumbing_layers = [
        ("P-CW", "5", "CONTINUOUS"),
        ("P-HW", "1", "DASHED"),
        ("P-SAN", "3", "CONTINUOUS"),
        ("P-VENT", "6", "DASHDOT"),
        ("P-VALVES", "4", "CONTINUOUS"),
        ("P-CLEANOUTS", "2", "CONTINUOUS"),
        ("P-EQUIP", "5", "CONTINUOUS"),
        ("P-LABELS", "7", "CONTINUOUS"),
    ]
    all_layers = existing_layers + plumbing_layers

    out = []
    def emit(code, val):
        out.append(str(code))
        out.append(str(val))

    emit(0, "SECTION"); emit(2, "HEADER")
    emit(9, "$ACADVER"); emit(1, "AC1009")
    emit(9, "$INSUNITS"); emit(70, 4)
    emit(9, "$EXTMIN"); emit(10, -2500.0); emit(20, -2500.0); emit(30, 0.0)
    emit(9, "$EXTMAX"); emit(10, 30500.0); emit(20, 14000.0); emit(30, 0.0)
    emit(0, "ENDSEC")

    emit(0, "SECTION"); emit(2, "TABLES")
    emit(0, "TABLE"); emit(2, "LTYPE"); emit(70, 3)
    for lt_name, desc, pat_len, dashes in [
        ("CONTINUOUS", "Solid line", 0.0, []),
        ("DASHED", "Dashed", 400.0, [250.0, -150.0]),
        ("DASHDOT", "Dash dot", 450.0, [250.0, -75.0, 50.0, -75.0]),
    ]:
        emit(0, "LTYPE"); emit(2, lt_name); emit(70, 0); emit(3, desc)
        emit(72, 65); emit(73, len(dashes)); emit(40, pat_len)
        for d in dashes:
            emit(49, d)
    emit(0, "ENDTAB")

    emit(0, "TABLE"); emit(2, "LAYER"); emit(70, len(all_layers))
    for lname, lcol, lt in all_layers:
        emit(0, "LAYER"); emit(2, lname); emit(70, 0); emit(62, lcol); emit(6, lt)
    emit(0, "ENDTAB")
    emit(0, "ENDSEC")

    emit(0, "SECTION"); emit(2, "ENTITIES")
    out.extend(arch_entity_lines)

    def dxf_line(layer, x1, y1, x2, y2):
        emit(0, "LINE"); emit(8, layer)
        emit(10, f"{x1:.2f}"); emit(20, f"{y1:.2f}"); emit(30, "0.0")
        emit(11, f"{x2:.2f}"); emit(21, f"{y2:.2f}"); emit(31, "0.0")

    def dxf_circle(layer, cx, cy, r):
        emit(0, "CIRCLE"); emit(8, layer)
        emit(10, f"{cx:.2f}"); emit(20, f"{cy:.2f}"); emit(30, "0.0")
        emit(40, f"{r:.2f}")

    def dxf_text(layer, x, y, h, txt):
        emit(0, "TEXT"); emit(8, layer)
        emit(10, f"{x:.2f}"); emit(20, f"{y:.2f}"); emit(30, "0.0")
        emit(40, f"{h:.1f}"); emit(1, txt)

    sys_layer = {"CW": "P-CW", "HW": "P-HW", "SAN": "P-SAN", "VENT": "P-VENT"}

    for p in pipes:
        dxf_line(sys_layer[p["system"]], p["from_x"], p["from_y"], p["to_x"], p["to_y"])
        length = math.hypot(p["to_x"] - p["from_x"], p["to_y"] - p["from_y"])
        if length >= 1800 and p["_tag"].startswith(("trunk:", "header:")):
            mx = 0.5 * (p["from_x"] + p["to_x"])
            my = 0.5 * (p["from_y"] + p["to_y"])
            if p["system"] == "SAN":
                lbl = f"SAN DN{p['dn_mm']} @{p['slope_pct']}% ({p['dfu_load']}DFU)"
            else:
                lbl = f"{p['system']} DN{p['dn_mm']}"
            dxf_text("P-LABELS", mx - 350, my + 45, 95.0, lbl)

    for v in valves:
        vx, vy = v["x"], v["y"]
        s = 70.0
        dxf_line("P-VALVES", vx - s, vy - s, vx + s, vy + s)
        dxf_line("P-VALVES", vx - s, vy + s, vx + s, vy - s)
        dxf_line("P-VALVES", vx - s, vy - s, vx - s, vy + s)
        dxf_line("P-VALVES", vx + s, vy - s, vx + s, vy + s)
        if v["id"] in ("V-MAIN-CW", "V-WH1-CW", "V-WH1-HW", "V-Z1-CW", "V-Z1-HW", "V-Z2-CW", "V-Z2-HW"):
            dxf_text("P-LABELS", vx + 90, vy - 40, 85.0, v["id"])

    for co in cleanouts:
        cx, cy = co["x"], co["y"]
        r = 95.0
        dxf_circle("P-CLEANOUTS", cx, cy, r)
        dxf_line("P-CLEANOUTS", cx - r * 0.7, cy - r * 0.7, cx + r * 0.7, cy + r * 0.7)
        dxf_line("P-CLEANOUTS", cx - r * 0.7, cy + r * 0.7, cx + r * 0.7, cy - r * 0.7)
        dxf_text("P-LABELS", cx - 120, cy + 120, 85.0, co["id"])

    dxf_circle("P-EQUIP", MAIN_CW_PT[0], MAIN_CW_PT[1], 140.0)
    dxf_circle("P-EQUIP", MAIN_CW_PT[0], MAIN_CW_PT[1], 70.0)
    dxf_text("P-LABELS", MAIN_CW_PT[0] - 420, MAIN_CW_PT[1] - 260, 105.0, "MAIN_CW DN32")

    dxf_circle("P-CLEANOUTS", SAN_OUT_PT[0], SAN_OUT_PT[1], 110.0)
    dxf_text("P-LABELS", SAN_OUT_PT[0] + 50, SAN_OUT_PT[1] + 150, 100.0, f"SAN_OUT DN150 ({total_dfu} DFU)")

    for vtr_id, pt in [("VTR-1 DN100", VTR1_PT), ("VTR-2 DN75", VTR2_PT)]:
        dxf_circle("P-VENT", pt[0], pt[1], 90.0)
        dxf_text("P-LABELS", pt[0] - 180, pt[1] - 200, 80.0, vtr_id)

    for fx in fixtures:
        if fx["kind"] == "water_heater":
            dxf_circle("P-EQUIP", fx["x"], fx["y"], 380.0)
            dxf_circle("P-EQUIP", fx["x"], fx["y"], 320.0)
            dxf_text("P-LABELS", fx["x"] - 160, fx["y"] - 45, 130.0, "WH1")
        else:
            dxf_circle("P-EQUIP", fx["x"], fx["y"], 75.0)
            dxf_text("P-LABELS", fx["x"] - 220, fx["y"] - 190, 75.0, f"{fx['id']} ({fx['dfu']}DFU)")

    emit(0, "ENDSEC")
    emit(0, "EOF")

    with open(out_dxf_path, "w") as f:
        f.write("\n".join(out) + "\n")


# ---------------------------------------------------------------------------
# 4. CSV Takeoff Schedule Export
# ---------------------------------------------------------------------------

def write_schedule_csv(out_csv_path, fixtures, valves, cleanouts, pipes):
    rows = []
    pipe_agg = defaultdict(lambda: {"count": 0, "length_m": 0.0, "max_dfu": 0})
    for p in pipes:
        L = math.hypot(p["to_x"] - p["from_x"], p["to_y"] - p["from_y"]) / 1000.0
        key = (p["system"], p["dn_mm"], p["slope_pct"])
        pipe_agg[key]["count"] += 1
        pipe_agg[key]["length_m"] += L
        pipe_agg[key]["max_dfu"] = max(pipe_agg[key]["max_dfu"], p["dfu_load"])

    spec_map = {
        "CW": "Copper Type L (Insulated)",
        "HW": "Copper Type L (25mm Fiberglass Insul.)",
        "SAN": "PVC Schedule 40 DWV (Under-Slab)",
        "VENT": "PVC Schedule 40 DWV (Plenum / VTR)",
    }
    for (sys_name, dn, slope), data in sorted(pipe_agg.items(), key=lambda kv: (kv[0][0], -kv[0][1])):
        rows.append({
            "category": "PIPE",
            "id_or_code": f"PIPE-{sys_name}-DN{dn}",
            "system": sys_name,
            "room_or_zone": "Building-Wide",
            "dn_mm": dn,
            "slope_pct": slope,
            "quantity": round(data["length_m"], 2),
            "unit": "m",
            "dfu": data["max_dfu"],
            "description": f"{sys_name} Piping DN{dn} ({data['count']} segs) - {spec_map[sys_name]}",
        })

    rows.append({
        "category": "EQUIPMENT",
        "id_or_code": "MAIN_CW",
        "system": "CW",
        "room_or_zone": "MechRoom",
        "dn_mm": 32,
        "slope_pct": 0.0,
        "quantity": 1,
        "unit": "ea",
        "dfu": 0,
        "description": "Main Building Cold Water Supply Riser w/ PRV & Check Valve",
    })
    for fx in fixtures:
        cat = "EQUIPMENT" if fx["kind"] == "water_heater" else "FIXTURE"
        svcs = "+".join([s for s, k in [("CW", "has_cw"), ("HW", "has_hw"), ("SAN", "has_san"), ("VENT", "has_vent")] if fx[k]])
        dn_main = fx["dn"].get("SAN", fx["dn"].get("CW", 25))
        rows.append({
            "category": cat,
            "id_or_code": fx["id"],
            "system": svcs,
            "room_or_zone": fx["room"],
            "dn_mm": dn_main,
            "slope_pct": 2.0 if fx["has_san"] else 0.0,
            "quantity": 1,
            "unit": "ea",
            "dfu": fx["dfu"],
            "description": f"{fx['kind']} in {fx['room']} ({svcs})",
        })

    for v in valves:
        rows.append({
            "category": "VALVE",
            "id_or_code": v["id"],
            "system": v["system"],
            "room_or_zone": v["_room"],
            "dn_mm": v["_dn_mm"],
            "slope_pct": 0.0,
            "quantity": 1,
            "unit": "ea",
            "dfu": 0,
            "description": v["_desc"],
        })

    for co in cleanouts:
        dn_co = 150 if co["x"] <= 3520 else 100
        rows.append({
            "category": "CLEANOUT",
            "id_or_code": co["id"],
            "system": "SAN",
            "room_or_zone": "Corridor",
            "dn_mm": dn_co,
            "slope_pct": 0.0,
            "quantity": 1,
            "unit": "ea",
            "dfu": 0,
            "description": co["_desc"],
        })
    for vtr_id, rm, dn in [("VTR-1", "PublicWC", 100), ("VTR-2", "MechRoom", 75)]:
        rows.append({
            "category": "VENT_STACK",
            "id_or_code": vtr_id,
            "system": "VENT",
            "room_or_zone": rm,
            "dn_mm": dn,
            "slope_pct": 0.0,
            "quantity": 1,
            "unit": "ea",
            "dfu": 0,
            "description": f"Vent Stack Through Roof DN{dn} w/ Flashing",
        })

    fieldnames = ["category", "id_or_code", "system", "room_or_zone", "dn_mm", "slope_pct", "quantity", "unit", "dfu", "description"]
    with open(out_csv_path, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        w.writerows(rows)

    return pipe_agg


# ---------------------------------------------------------------------------
# 5. High-Resolution Engineering PNG Renderer — REV B
# ---------------------------------------------------------------------------

def render_png(out_png_path, arch, fixtures, valves, cleanouts, pipes, pipe_agg, stats):
    fig, ax = plt.subplots(figsize=(24, 13.2), dpi=200)
    fig.patch.set_facecolor("#F8FAFC")
    ax.set_facecolor("#F8FAFC")

    room_colors = {
        "ElecITRoom": "#FEE2E2",
        "MechRoom": "#E0F2FE",
        "PublicWC": "#EFF6FF",
        "StaffWC": "#EFF6FF",
        "Exam5": "#ECFDF5",
        "Corridor": "#FFFFFF",
    }
    for r in arch["rooms"]:
        w = r["max_x"] - r["min_x"]
        h = r["max_y"] - r["min_y"]
        fc = room_colors.get(r["id"], "#F8FAFC")
        ec = "#DC2626" if r["id"] == "ElecITRoom" else "#475569"
        lw = 2.0 if r["id"] == "ElecITRoom" else 1.2
        hatch = "///" if r["id"] == "ElecITRoom" else None
        rect = patches.Rectangle(
            (r["min_x"], r["min_y"]), w, h,
            facecolor=fc, edgecolor=ec, linewidth=lw, hatch=hatch,
            alpha=0.85 if r["id"] == "ElecITRoom" else 1.0, zorder=1
        )
        ax.add_patch(rect)

        cx = 0.5 * (r["min_x"] + r["max_x"])
        cy = 0.5 * (r["min_y"] + r["max_y"])
        if r["id"] == "Corridor":
            ax.text(7800, 6250, "CENTRAL CLINICAL CORRIDOR (PLUMBING UTILITY RACK)",
                    ha="center", va="center", fontsize=8.0, fontweight="bold", color="#94A3B8", zorder=2)
        elif r["id"] == "ElecITRoom":
            ax.text(cx, cy - 350, "Elec / IT Room\nNO PLUMBING ZONE",
                    ha="center", va="center", fontsize=8.5, fontweight="bold", color="#B91C1C",
                    bbox=dict(boxstyle="round,pad=0.25", facecolor="#FEF2F2", edgecolor="#DC2626", lw=1.0), zorder=4)
        else:
            ty = r["min_y"] + 900 if r["min_y"] >= 7500 else r["min_y"] + 1350
            tx = cx
            if r["id"] in ("PublicWC", "StaffWC"):
                ty = 8650
            elif r["id"] == "MechRoom":
                tx, ty = 25850, 7950
            lbl = f"{r['id']} (REV B)" if r["id"] == "Exam5" else r["id"]
            ax.text(tx, ty, lbl, ha="center", va="center", fontsize=8.5, fontweight="bold", color="#334155", zorder=3)

    # Outer building shell border (28.0 m x 11.0 m)
    shell_pts = [(0, 0), (28000, 0), (28000, 11000), (0, 11000), (0, 0)]
    xs, ys = zip(*shell_pts)
    ax.plot(xs, ys, color="#1E293B", linewidth=3.0, zorder=3)

    for af in arch["fixtures"]:
        w = af["max_x"] - af["min_x"]
        h = af["max_y"] - af["min_y"]
        is_wet = any(k in af["kind"] for k in ("sink", "basin", "wc", "water_heater"))
        fc = "#DBEAFE" if is_wet else "#E2E8F0"
        ec = "#60A5FA" if is_wet else "#CBD5E1"
        ax.add_patch(patches.Rectangle((af["min_x"], af["min_y"]), w, h, facecolor=fc, edgecolor=ec, lw=0.9, zorder=2))

    for d in arch["doors"]:
        sb = d["swing_bbox"]
        ax.add_patch(patches.Rectangle(
            (sb[0], sb[1]), sb[2] - sb[0], sb[3] - sb[1],
            facecolor="none", edgecolor="#CBD5E1", lw=0.6, linestyle=":", zorder=2
        ))

    for c in arch["columns"]:
        ax.add_patch(patches.Rectangle(
            (c["min_x"], c["min_y"]), c["max_x"] - c["min_x"], c["max_y"] - c["min_y"],
            facecolor="#991B1B", edgecolor="#450A0A", lw=1.0, zorder=5
        ))

    grid_x = [(0, "A"), (5600, "B"), (11200, "C"), (16800, "D"), (22400, "E"), (28000, "F")]
    grid_y = [(0, "1"), (5000, "2"), (11000, "3")]
    for gx, glbl in grid_x:
        ax.plot([gx, gx], [-700, 11850], color="#CBD5E1", lw=0.7, linestyle="--", zorder=0)
        ax.add_patch(patches.Circle((gx, 12150), 320, facecolor="white", edgecolor="#334155", lw=1.2, zorder=6))
        ax.text(gx, 12150, glbl, ha="center", va="center", fontsize=9, fontweight="bold", color="#1E293B", zorder=7)
    for gy, glbl in grid_y:
        ax.plot([-800, 28600], [gy, gy], color="#CBD5E1", lw=0.7, linestyle="--", zorder=0)
        ax.add_patch(patches.Circle((-1150, gy), 320, facecolor="white", edgecolor="#334155", lw=1.2, zorder=6))
        ax.text(-1150, gy, glbl, ha="center", va="center", fontsize=9, fontweight="bold", color="#1E293B", zorder=7)

    style = {
        "CW":   {"color": "#0284C7", "ls": "-",  "z": 10},
        "HW":   {"color": "#DC2626", "ls": "--", "z": 11},
        "SAN":  {"color": "#15803D", "ls": "-",  "z": 12},
        "VENT": {"color": "#9333EA", "ls": "-.", "z": 9},
    }

    for p in pipes:
        st = style[p["system"]]
        if p["system"] == "SAN":
            lw = 3.8 if p["dn_mm"] >= 150 else (3.0 if p["dn_mm"] >= 100 else 2.1)
        elif p["system"] in ("CW", "HW"):
            lw = 2.6 if p["dn_mm"] >= 25 else 1.6
        else:
            lw = 2.1 if p["dn_mm"] >= 75 else 1.4
        ax.plot(
            [p["from_x"], p["to_x"]], [p["from_y"], p["to_y"]],
            color=st["color"], linestyle=st["ls"], linewidth=lw,
            solid_capstyle="round", zorder=st["z"]
        )

    for ax_x in (23500, 18800, 12800, 7500, 2600):
        ax.annotate(
            "", xy=(ax_x - 600, Y_SAN_MAIN), xytext=(ax_x + 200, Y_SAN_MAIN),
            arrowprops=dict(arrowstyle="-|>", color="#15803D", lw=1.8, mutation_scale=13),
            zorder=15
        )

    callouts = [
        (23500, Y_SAN_MAIN + 175, "SAN DN100 @ 2.0% (5 DFU)", "#15803D"),
        (18800, Y_SAN_MAIN + 175, "SAN DN100 @ 2.0% (6 DFU)", "#15803D"),
        (12600, Y_SAN_MAIN + 175, "SAN DN100 @ 2.0% (11 DFU)", "#15803D"),
        (7400,  Y_SAN_MAIN + 175, "SAN DN100 @ 2.0% (12 DFU)", "#15803D"),
        (2350,  Y_SAN_MAIN + 175, "SAN DN150 @ 1.5% (18–24 DFU)", "#15803D"),
        (18800, Y_VENT_MAIN - 175, "VENT HEADER DN50", "#9333EA"),
        (6000,  Y_VENT_MAIN - 175, "VENT HEADER DN75", "#9333EA"),
        (18800, Y_CW_MAIN + 145, "CW MAIN DN32", "#0284C7"),
        (6000,  Y_CW_MAIN + 145, "CW MAIN DN25", "#0284C7"),
        (18800, Y_HW_MAIN - 175, "HW MAIN DN25", "#DC2626"),
        (6000,  Y_HW_MAIN - 175, "HW MAIN DN25", "#DC2626"),
    ]
    for lx, ly, txt, col in callouts:
        ax.text(
            lx, ly, txt, ha="center", va="center", fontsize=6.8, fontweight="bold", color=col,
            bbox=dict(boxstyle="round,pad=0.14", facecolor="white", edgecolor="none", alpha=0.88),
            zorder=16
        )

    for v in valves:
        vx, vy = v["x"], v["y"]
        col = "#0369A1" if v["system"] == "CW" else "#B91C1C"
        is_major = v["id"] in ("V-MAIN-CW", "V-WH1-CW", "V-WH1-HW", "V-Z1-CW", "V-Z1-HW", "V-Z2-CW", "V-Z2-HW")
        ms = 8.5 if is_major else 5.0
        ax.plot(vx, vy, marker="D", markersize=ms, color="white", markeredgecolor=col, markeredgewidth=1.6, zorder=17)
        if is_major:
            offsets = {
                "V-MAIN-CW": (-620, 0),
                "V-WH1-CW":  (320, -230),
                "V-WH1-HW":  (-350, 210),
                "V-Z1-CW":   (520, 220),
                "V-Z1-HW":   (520, -220),
                "V-Z2-CW":   (0, 240),
                "V-Z2-HW":   (0, -240),
            }
            dx, dy = offsets[v["id"]]
            ax.text(vx + dx, vy + dy, v["id"], ha="center", va="center", fontsize=6.2, fontweight="bold",
                    color=col, bbox=dict(boxstyle="round,pad=0.12", facecolor="#FFFBEB", edgecolor=col, lw=0.7), zorder=18)

    for co in cleanouts:
        cx, cy = co["x"], co["y"]
        ax.add_patch(patches.Circle((cx, cy), 120, facecolor="#FEF08A", edgecolor="#854D0E", lw=1.4, zorder=18))
        ax.text(cx, cy, "CO", ha="center", va="center", fontsize=5.2, fontweight="bold", color="#713F12", zorder=19)

    ax.add_patch(patches.Circle(MAIN_CW_PT, 210, facecolor="#0284C7", edgecolor="#0C4A6E", lw=1.8, zorder=19))
    ax.text(MAIN_CW_PT[0], MAIN_CW_PT[1], "CW", ha="center", va="center", fontsize=6.5, fontweight="bold", color="white", zorder=20)
    ax.text(MAIN_CW_PT[0], MAIN_CW_PT[1] - 350, "MAIN_CW\n(DN32 Riser)", ha="center", va="top",
            fontsize=6.2, fontweight="bold", color="#0C4A6E",
            bbox=dict(boxstyle="round,pad=0.14", facecolor="#E0F2FE", edgecolor="#0284C7", lw=0.8), zorder=20)

    ax.add_patch(patches.Circle(SAN_OUT_PT, 175, facecolor="#15803D", edgecolor="#052E16", lw=1.6, zorder=19))
    ax.text(SAN_OUT_PT[0] - 850, SAN_OUT_PT[1] + 50, f"SAN_OUT\nDN150 ({stats['total_dfu']} DFU)", ha="center", va="center",
            fontsize=6.8, fontweight="bold", color="#14532D",
            bbox=dict(boxstyle="round,pad=0.18", facecolor="#DCFCE7", edgecolor="#15803D", lw=0.9), zorder=20)

    for vtr_id, pt in [("VTR-1 DN100", VTR1_PT), ("VTR-2 DN75", VTR2_PT)]:
        ax.add_patch(patches.Circle(pt, 120, facecolor="#F3E8FF", edgecolor="#7E22CE", lw=1.4, zorder=19))
        ax.text(pt[0], 11220, vtr_id, ha="center", va="bottom", fontsize=6.2, fontweight="bold", color="#6B21A8",
                bbox=dict(boxstyle="round,pad=0.12", facecolor="#FAF5FF", edgecolor="#9333EA", lw=0.6), zorder=20)

    for fx in fixtures:
        if fx["kind"] == "water_heater":
            ax.add_patch(patches.Circle((fx["x"], fx["y"]), 400, facecolor="#FFF7ED", edgecolor="#EA580C", lw=2.2, zorder=18))
            ax.text(fx["x"], fx["y"], "WH1\nElec.\nWater\nHeater", ha="center", va="center",
                    fontsize=6.8, fontweight="bold", color="#9A3412", zorder=19)
        else:
            ax.add_patch(patches.Circle((fx["x"], fx["y"]), 110, facecolor="#1E3A8A", edgecolor="white", lw=1.2, zorder=18))
            lbl_dx, lbl_dy = 0, -330
            if fx["id"] in ("WC_PWC", "WC_SWC"):
                lbl_dy = -420
            elif fx["id"] == "SINK_PROC":
                lbl_dy = -380
            elif fx["id"] == "MOP_MCH":
                lbl_dx, lbl_dy = 120, -470
            elif fx["id"] == "Exam5Basin":
                lbl_dx, lbl_dy = -620, -220
            elif fx["id"] == "SINK_LAB":
                lbl_dx, lbl_dy = 360, -340
            ax.text(
                fx["x"] + lbl_dx, fx["y"] + lbl_dy, f"{fx['id']}\n({fx['dfu']} DFU)",
                ha="center", va="top", fontsize=6.2, fontweight="bold", color="#0F172A",
                bbox=dict(boxstyle="round,pad=0.12", facecolor="white", edgecolor="#94A3B8", lw=0.6, alpha=0.92),
                zorder=19
            )

    # Bottom Title Block & Legend Strip (clear of full 28x11 m footprint)
    tb_x, tb_y, tb_w, tb_h = 0.0, -4150.0, 28000.0, 2800.0
    tb = patches.FancyBboxPatch(
        (tb_x, tb_y), tb_w, tb_h,
        boxstyle="round,pad=100", facecolor="white", edgecolor="#1E293B", linewidth=1.5, zorder=15
    )
    ax.add_patch(tb)

    ax.text(tb_x + 350, tb_y + tb_h - 380, "OUTPATIENT SPECIALIST CLINIC — REV B ECO", ha="left", va="top",
            fontsize=11.0, fontweight="bold", color="#0F172A", zorder=16)
    ax.text(tb_x + 350, tb_y + tb_h - 860, "PLUMBING PLAN — CW / HW / SAN / VENT (IPC | NO-EDL BASELINE)", ha="left", va="top",
            fontsize=8.0, fontweight="bold", color="#0369A1", zorder=16)

    sys_len = defaultdict(float)
    for (s, dn, sl), d in pipe_agg.items():
        sys_len[s] += d["length_m"]

    left_lines = [
        f"Wet Fixtures Served: {stats['total_fixtures']} / {stats['total_fixtures']} (100%, incl. NEW Exam5Basin)",
        f"Total Drainage Load: {stats['total_dfu']} DFU (Rev A 23 -> Rev B 24 DFU)",
        f"SAN Main Trunk: DN100 @ 2.0% -> DN150 @ 1.5%  |  CW: DN32/25  |  HW: DN25",
    ]
    for i, ln in enumerate(left_lines):
        ax.text(tb_x + 350, tb_y + tb_h - 1340 - i * 380, ln, ha="left", va="top", fontsize=7.2, color="#1E293B", zorder=16)

    mid_x = tb_x + 11400.0
    ax.plot([mid_x - 300, mid_x - 300], [tb_y + 200, tb_y + tb_h - 200], color="#CBD5E1", lw=1.0, zorder=16)
    mid_lines = [
        f"Valves: {stats['total_valves']} (Main + Zone + Fixture)  |  Cleanouts: {stats['total_cleanouts']}",
        f"Pipe Lengths: CW {sys_len['CW']:.1f}m | HW {sys_len['HW']:.1f}m | SAN {sys_len['SAN']:.1f}m | VENT {sys_len['VENT']:.1f}m",
        f"ElecITRoom Exclusion Zone: {stats['elec_room_dry_check']} (min {stats['min_elec_clearance_mm']:.0f} mm)",
        f"Column Clash Audit (18 Cols): 0 Clashes (min {stats['min_col_clearance_mm']:.0f} mm clear)",
        f"ECO-1 MechRoom +1.0 m East  |  ECO-2 Exam5Basin + Re-Synced South Clinical Chases",
    ]
    for i, ln in enumerate(mid_lines):
        ax.text(mid_x, tb_y + tb_h - 320 - i * 440, ln, ha="left", va="top", fontsize=7.2, color="#1E293B", zorder=16)

    rig_x = tb_x + 21600.0
    ax.plot([rig_x - 300, rig_x - 300], [tb_y + 200, tb_y + tb_h - 200], color="#CBD5E1", lw=1.0, zorder=16)
    ax.text(rig_x, tb_y + tb_h - 300, "LEGEND", ha="left", va="top", fontsize=8.0, fontweight="bold", color="#0F172A", zorder=16)
    legend_items = [
        (rig_x, tb_y + tb_h - 760,  "#0284C7", "-",  2.6, "Cold Water (CW) DN15-32"),
        (rig_x, tb_y + tb_h - 1160, "#DC2626", "--", 2.6, "Hot Water (HW) DN15-25"),
        (rig_x, tb_y + tb_h - 1560, "#15803D", "-",  3.4, "Sanitary (SAN) DN50-150"),
        (rig_x, tb_y + tb_h - 1960, "#9333EA", "-.", 2.2, "Vent (VENT) DN40-100 + VTR"),
    ]
    for lx, ly, col, ls, lw, desc in legend_items:
        ax.plot([lx, lx + 800], [ly, ly], color=col, linestyle=ls, linewidth=lw, zorder=16)
        ax.text(lx + 960, ly, desc, ha="left", va="center", fontsize=7.0, color="#1E293B", zorder=16)

    # Symbol row right below footer
    ax.plot(400, -4700, marker="D", markersize=7, color="white", markeredgecolor="#0369A1", markeredgewidth=1.5, zorder=16)
    ax.text(780, -4700, "Isolation Valve (CW/HW)", ha="left", va="center", fontsize=7.5, color="#1E293B")
    ax.add_patch(patches.Circle((6000, -4700), 140, facecolor="#FEF08A", edgecolor="#854D0E", lw=1.3, zorder=16))
    ax.text(6000, -4700, "CO", ha="center", va="center", fontsize=5.2, fontweight="bold", color="#713F12", zorder=17)
    ax.text(6320, -4700, "Sanitary Cleanout (IPC 708)", ha="left", va="center", fontsize=7.5, color="#1E293B")
    ax.add_patch(patches.Rectangle((12000, -4840), 280, 280, facecolor="#991B1B", edgecolor="#450A0A", lw=1.0, zorder=16))
    ax.text(12450, -4700, "RC Column (300×300 — 0 Clashes)", ha="left", va="center", fontsize=7.5, color="#1E293B")
    ax.add_patch(patches.Rectangle((19200, -4840), 380, 280, facecolor="#FEE2E2", edgecolor="#DC2626", hatch="///", lw=1.0, zorder=16))
    ax.text(19750, -4700, "Elec/IT Exclusion Zone (0 Pipes)", ha="left", va="center", fontsize=7.5, color="#B91C1C", fontweight="bold")

    ax.set_xlim(-1800, 29000)
    ax.set_ylim(-5200, 12800)
    ax.set_aspect("equal")
    ax.axis("off")

    plt.tight_layout(pad=0.4)
    plt.savefig(out_png_path, dpi=200, facecolor=fig.get_facecolor())
    plt.close(fig)


# ---------------------------------------------------------------------------
# 6. Main Entrypoint
# ---------------------------------------------------------------------------

def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(ARCH_MANIFEST_PATH, "r") as f:
        arch = json.load(f)

    fixtures = build_fixtures()
    pipes, valves, cleanouts = generate_network(fixtures)
    errors, warnings, stats = verify_all(arch, fixtures, valves, cleanouts, pipes)

    print("=== PLUMBING REV B VERIFICATION REPORT ===")
    for k, v in stats.items():
        print(f"  {k}: {v}")
    if warnings:
        print("WARNINGS:")
        for w in warnings:
            print("  -", w)
    if errors:
        print("ERRORS:")
        for e in errors:
            print("  -", e)
        raise SystemExit(1)
    print("STATUS: ALL CHECKS PASSED (0 ERRORS)")

    manifest = {
        "revision": "B",
        "fixtures": [
            {
                "id": fx["id"],
                "room": fx["room"],
                "kind": fx["kind"],
                "x": fx["x"],
                "y": fx["y"],
                "dfu": fx["dfu"],
                "has_cw": fx["has_cw"],
                "has_hw": fx["has_hw"],
                "has_san": fx["has_san"],
                "has_vent": fx["has_vent"],
            }
            for fx in fixtures
        ],
        "valves": [
            {"id": v["id"], "system": v["system"], "x": v["x"], "y": v["y"]}
            for v in valves
        ],
        "cleanouts": [
            {"id": c["id"], "x": c["x"], "y": c["y"]}
            for c in cleanouts
        ],
        "pipes": [
            {
                "id": p["id"],
                "system": p["system"],
                "dn_mm": p["dn_mm"],
                "slope_pct": p["slope_pct"],
                "from_x": p["from_x"],
                "from_y": p["from_y"],
                "to_x": p["to_x"],
                "to_y": p["to_y"],
                "dfu_load": p["dfu_load"],
            }
            for p in pipes
        ],
        "stats": stats,
    }
    manifest_path = OUT_DIR / "plumbing_manifest.json"
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    pipe_agg = write_schedule_csv(OUT_DIR / "plumbing_schedule.csv", fixtures, valves, cleanouts, pipes)
    write_dxf(ARCH_DXF_PATH, OUT_DIR / "clinic_plumbing.dxf", fixtures, valves, cleanouts, pipes, stats["total_dfu"])
    render_png(OUT_DIR / "clinic_plumbing.png", arch, fixtures, valves, cleanouts, pipes, pipe_agg, stats)
    print("Generated all plumbing Rev B deliverables successfully in:", OUT_DIR)


if __name__ == "__main__":
    main()
