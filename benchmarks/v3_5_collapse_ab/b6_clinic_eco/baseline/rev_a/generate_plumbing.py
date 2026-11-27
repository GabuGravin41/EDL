#!/usr/bin/env python3
"""
Benchmark 3 (Baseline / NO-EDL): Full Building Plumbing System Generator & Verifier
Overlays IPC-compliant CW, HW, SAN, and VENT plumbing onto the Baseline B2 15-Room Medical Clinic.

Outputs written to /home/lindah/projects/EDL/benchmarks/v3_2_industry_ab/b3_clinic_plumbing/baseline/:
  - clinic_plumbing.dxf
  - clinic_plumbing.png
  - plumbing_schedule.csv
  - plumbing_manifest.json
"""

import csv
import json
import math
import os
from collections import defaultdict
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as patches
from matplotlib.lines import Line2D
import networkx as nx
from shapely.geometry import LineString, Point, box


BASE_DIR = Path("/home/lindah/projects/EDL/benchmarks/v3_2_industry_ab")
ARCH_DIR = BASE_DIR / "b2_clinic_arch" / "baseline"
OUT_DIR = BASE_DIR / "b3_clinic_plumbing" / "baseline"
ARCH_MANIFEST_PATH = ARCH_DIR / "arch_manifest.json"
ARCH_DXF_PATH = ARCH_DIR / "clinic_arch.dxf"


# ---------------------------------------------------------------------------
# 1. Fixture & Plumbing Network Specification
# ---------------------------------------------------------------------------

# Corridor main header Y-coordinates (inside Central Corridor y = 5150..7350)
Y_SAN_MAIN = 7050.0   # Below slab, East -> West, north of W12 (y=5550..6950)
Y_VENT_MAIN = 6650.0  # Ceiling plenum, ties to VTR-1 & VTR-2
Y_CW_MAIN = 5850.0    # Ceiling plenum, fed from MAIN_CW in MechRoom
Y_HW_MAIN = 5500.0    # Ceiling plenum, fed from WH1 in MechRoom

# Special source/terminal coordinates
MAIN_CW_PT = (26150.0, 8200.0)   # Main municipal cold water riser in MechRoom
SAN_OUT_PT = (150.0, Y_SAN_MAIN) # Building sanitary drain exit at West corridor wall
VTR1_PT = (1850.0, 10700.0)      # Main 100mm Vent Through Roof (PublicWC north wet wall)
VTR2_PT = (24360.0, 10680.0)     # Secondary 75mm Vent Through Roof (MechRoom north wall)


def build_fixtures():
    """All 13 wet fixtures & mechanical plumbing equipment (11 mandatory + 2 StaffWC)."""
    return [
        # South clinical wing
        {
            "id": "LAV_EX1", "room": "Exam1", "kind": "basin",
            "x": 11100.0, "y": 4200.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 11040.0, "HW": 11095.0, "SAN": 11150.0, "VENT": 11205.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 4700.0,
        },
        {
            "id": "LAV_EX2", "room": "Exam2", "kind": "basin",
            "x": 13875.0, "y": 4300.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 13800.0, "HW": 13875.0, "SAN": 13955.0, "VENT": 14035.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 4750.0,
        },
        {
            "id": "LAV_EX3", "room": "Exam3", "kind": "basin",
            "x": 16825.0, "y": 4200.0, "dfu": 1,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 16750.0, "HW": 16825.0, "SAN": 16905.0, "VENT": 16985.0},
            "dn": {"CW": 15, "HW": 15, "SAN": 50, "VENT": 40},
            "valve_y": 4700.0,
        },
        {
            "id": "SINK_PROC", "room": "Procedure", "kind": "sink",
            "x": 20400.0, "y": 4625.0, "dfu": 2,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"CW": 20320.0, "HW": 20400.0, "SAN": 20480.0, "VENT": 20560.0},
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
        {
            "id": "LAV_EX4", "room": "Exam4", "kind": "basin",
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
        {
            "id": "MOP_MCH", "room": "MechRoom", "kind": "mop_sink",
            "x": 24650.0, "y": 10325.0, "dfu": 3,
            "has_cw": True, "has_hw": True, "has_san": True, "has_vent": True,
            "chase": {"SAN": 24260.0, "VENT": 24360.0},
            "dn": {"CW": 20, "HW": 20, "SAN": 75, "VENT": 50},
            "valve_y": 9200.0,
        },
        {
            "id": "WH1", "room": "MechRoom", "kind": "water_heater",
            "x": 26150.0, "y": 10150.0, "dfu": 0,
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
    # Collect corridor junction X coordinates and DFU contributions
    san_junctions = {}  # x -> dfu added at that junction
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

    # Add explicit building exit cleanout node on SAN trunk at x = 250
    san_nodes_x = sorted(set(list(san_junctions.keys()) + [250.0, SAN_OUT_PT[0]]), reverse=True)

    # Build SAN main collector trunk East -> West with exact cumulative DFU
    cum_dfu = 0
    for x_hi, x_lo in zip(san_nodes_x[:-1], san_nodes_x[1:]):
        cum_dfu += san_junctions.get(x_hi, 0)
        # Upsize trunk to DN150 at 1.5% slope once StaffWC (x=3520) joins (17+ DFU + WCs)
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

    # Cleanouts along SAN trunk (upper terminal, stack bases, junctions, building exit)
    add_cleanout("CO-MCH", (24260.0, Y_SAN_MAIN), "Upper terminal & MechRoom mop sink stack base")
    add_cleanout("CO-PROC", (20480.0, Y_SAN_MAIN), "Procedure scrub sink junction cleanout")
    add_cleanout("CO-LAB", (15685.0, Y_SAN_MAIN), "Lab / Exam4 wet-wall junction cleanout")
    add_cleanout("CO-EX1", (11150.0, Y_SAN_MAIN), "Mid-corridor / Exam1 cleanout")
    add_cleanout("CO-BRK", (4030.0, Y_SAN_MAIN), "Breakroom / StaffWC transition cleanout")
    add_cleanout("CO-WC", (1650.0, Y_SAN_MAIN), "PublicWC 150mm collector cleanout")
    add_cleanout("CO-OUT", (250.0, Y_SAN_MAIN), "Main building drain exit cleanout")

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

    # Tie VTR-1 (Main 100mm Vent Through Roof at PublicWC north wall) and VTR-2 (MechRoom)
    pb.add_seg("VENT", 100, 0.0, (1850.0, 10425.0), VTR1_PT, dfu_load=0, tag="vtr:VTR-1")
    pb.add_seg("VENT", 75, 0.0, (24360.0, 10325.0), VTR2_PT, dfu_load=0, tag="vtr:VTR-2")

    vent_nodes_x = sorted(vent_junctions_x, reverse=True)
    for x_hi, x_lo in zip(vent_nodes_x[:-1], vent_nodes_x[1:]):
        # DN75 main vent header across WC/central zone, DN50 east wing header
        dn = 75 if x_hi <= 15750.0 else 50
        pb.add_seg(
            "VENT", dn, 0.5,
            (x_hi, Y_VENT_MAIN), (x_lo, Y_VENT_MAIN),
            dfu_load=0,
            tag="header:VENT_MAIN",
        )

    # ------------------------------------------------------------------
    # C. DOMESTIC COLD WATER NETWORK (CW)
    # ------------------------------------------------------------------
    # MechRoom CW manifold from MAIN_CW riser -> V-MAIN-CW -> main distribution tee
    cw_v_main = (26150.0, 8500.0)
    cw_dist_tee = (26150.0, 8850.0)
    cw_mop_tee = (26150.0, 9200.0)
    cw_wh_valve = (26150.0, 9550.0)
    wh_pt = (26150.0, 10150.0)

    pb.add_seg("CW", 32, 0.0, MAIN_CW_PT, cw_v_main, tag="main:CW_RISER")
    add_valve("V-MAIN-CW", "CW", cw_v_main, "Main building CW shutoff valve (DN32)", dn_mm=32, room="MechRoom")
    pb.add_seg("CW", 32, 0.0, cw_v_main, cw_dist_tee, tag="main:CW_RISER")

    # MechRoom north branch: tee -> mop sink tee -> V-WH1-CW -> WH1 inlet
    pb.add_seg("CW", 25, 0.0, cw_dist_tee, cw_mop_tee, tag="mech:CW_WH_FEED")
    pb.add_seg("CW", 25, 0.0, cw_mop_tee, cw_wh_valve, tag="mech:CW_WH_FEED")
    add_valve("V-WH1-CW", "CW", cw_wh_valve, "Water Heater WH1 cold inlet isolation valve (DN25)", dn_mm=25, room="MechRoom")
    pb.add_seg("CW", 25, 0.0, cw_wh_valve, wh_pt, tag="mech:CW_WH_FEED")

    # MechRoom MOP_MCH CW branch (north of D16 swing y=8450)
    cw_mop_valve = (25200.0, 9200.0)
    pb.add_seg("CW", 20, 0.0, cw_mop_tee, cw_mop_valve, tag="branch:MOP_MCH")
    add_valve("V-MOP-CW", "CW", cw_mop_valve, "Mop sink CW isolation valve (DN20)", dn_mm=20, room="MechRoom")
    pb.add_polyline("CW", 20, 0.0, [cw_mop_valve, (24650.0, 9200.0), (24650.0, 10325.0)], tag="branch:MOP_MCH")

    # MechRoom south drop to Corridor CW Main Header (east of D16, west of C-F2)
    cw_corr_start_x = 26450.0
    pb.add_polyline(
        "CW", 32, 0.0,
        [cw_dist_tee, (cw_corr_start_x, 8850.0), (cw_corr_start_x, Y_CW_MAIN)],
        tag="header:CW_DROP",
    )

    # Corridor CW branches for all non-MechRoom fixtures
    cw_junctions_x = {cw_corr_start_x, 21200.0, 7800.0}  # include Z1 & Z2 zone valve nodes
    add_valve("V-Z1-CW", "CW", (21200.0, Y_CW_MAIN), "Clinical Wing CW zone isolation valve (DN32)", dn_mm=32, room="Corridor")
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
        # DN32 main CW header east of Exam1 (x >= 11040), DN25 west of Exam1
        dn = 32 if x_lo >= 11040.0 else 25
        pb.add_seg(
            "CW", dn, 0.0,
            (x_hi, Y_CW_MAIN), (x_lo, Y_CW_MAIN),
            tag="header:CW_MAIN",
        )

    # ------------------------------------------------------------------
    # D. DOMESTIC HOT WATER NETWORK (HW)
    # ------------------------------------------------------------------
    # WH1 outlet -> V-WH1-HW -> MOP_MCH tee -> drop to Corridor HW Main Header
    hw_drop_x = 25750.0
    hw_wh_corner = (hw_drop_x, 10150.0)
    hw_wh_valve = (hw_drop_x, 9850.0)
    hw_mop_tee = (hw_drop_x, 9550.0)

    pb.add_seg("HW", 25, 0.0, wh_pt, hw_wh_corner, tag="mech:HW_WH_OUT")
    pb.add_seg("HW", 25, 0.0, hw_wh_corner, hw_wh_valve, tag="mech:HW_WH_OUT")
    add_valve("V-WH1-HW", "HW", hw_wh_valve, "Water Heater WH1 hot outlet isolation valve (DN25)", dn_mm=25, room="MechRoom")
    pb.add_seg("HW", 25, 0.0, hw_wh_valve, hw_mop_tee, tag="mech:HW_WH_OUT")

    # MechRoom MOP_MCH HW branch
    hw_mop_valve = (25200.0, 9550.0)
    pb.add_seg("HW", 20, 0.0, hw_mop_tee, hw_mop_valve, tag="branch:MOP_MCH")
    add_valve("V-MOP-HW", "HW", hw_mop_valve, "Mop sink HW isolation valve (DN20)", dn_mm=20, room="MechRoom")
    pb.add_polyline(
        "HW", 20, 0.0,
        [hw_mop_valve, (24780.0, 9550.0), (24780.0, 10325.0), (24650.0, 10325.0)],
        tag="branch:MOP_MCH",
    )

    # Drop to Corridor HW Main Header (DN25)
    pb.add_seg("HW", 25, 0.0, hw_mop_tee, (hw_drop_x, Y_HW_MAIN), tag="header:HW_DROP")

    hw_junctions_x = {hw_drop_x, 21200.0, 7800.0}
    add_valve("V-Z1-HW", "HW", (21200.0, Y_HW_MAIN), "Clinical Wing HW zone isolation valve (DN25)", dn_mm=25, room="Corridor")
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
# 2. Rigorous Engineering & Geometric Verification
# ---------------------------------------------------------------------------

def verify_all(arch, fixtures, valves, cleanouts, pipes):
    errors = []
    warnings = []

    rooms_by_id = {r["id"]: r for r in arch["rooms"]}
    elec_room = rooms_by_id["ElecITRoom"]
    elec_poly = box(elec_room["min_x"], elec_room["min_y"], elec_room["max_x"], elec_room["max_y"])
    col_polys = [(c["id"], box(c["min_x"], c["min_y"], c["max_x"], c["max_y"])) for c in arch["columns"]]
    bldg_poly = box(0, 0, 21600, 5000).union(box(0, 5000, 27000, 11000))

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

    # 2. Clash check: All 17 structural columns (exact + pipe outer radius buffer)
    min_col_clearance = float("inf")
    tightest_col_pair = None
    for p in pipes:
        line = LineString([(p["from_x"], p["from_y"]), (p["to_x"], p["to_y"])])
        r_mm = p["dn_mm"] / 2.0
        for cid, cpoly in col_polys:
            d = line.distance(cpoly) - r_mm
            if d < min_col_clearance:
                min_col_clearance = d
                tightest_col_pair = (p["id"], cid, d)
            if line.intersects(cpoly) or d <= 0:
                errors.append(f"CLASH: Pipe {p['id']} (DN{p['dn_mm']}) clashes with column {cid} (clear={d:.1f}mm)!")

    # 3. Building envelope check
    for p in pipes:
        line = LineString([(p["from_x"], p["from_y"]), (p["to_x"], p["to_y"])])
        if not bldg_poly.buffer(1.0).contains(line):
            errors.append(f"BOUNDS: Pipe {p['id']} extends outside building envelope!")

    # 4. Graph connectivity per system (exact node-to-node)
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

    # 5. Fixture service & room containment verification
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
                    # Check pipe diameter rules at fixture branch
                    inc_dns = [d["dn_mm"] for _, _, d in graphs[sys_name].edges(pt, data=True)]
                    max_dn = max(inc_dns)
                    if sys_name == "SAN":
                        if fx["kind"] == "wc" and max_dn < 100:
                            errors.append(f"SIZE: WC {fx['id']} SAN branch DN{max_dn} < 100mm!")
                        if fx["kind"] in ("basin", "sink", "mop_sink") and max_dn < 40:
                            errors.append(f"SIZE: {fx['id']} SAN branch DN{max_dn} < 40mm!")
                    if sys_name in ("CW", "HW") and max_dn < 15:
                        errors.append(f"SIZE: {fx['id']} {sys_name} branch DN{max_dn} < 15mm!")

    # Verify MAIN_CW, SAN_OUT, VTR-1, VTR-2 are in their system graphs
    if MAIN_CW_PT not in graphs["CW"]:
        errors.append("SOURCE: MAIN_CW not connected to CW graph!")
    if SAN_OUT_PT not in graphs["SAN"]:
        errors.append("EXIT: SAN_OUT not connected to SAN graph!")
    if VTR1_PT not in graphs["VENT"] or VTR2_PT not in graphs["VENT"]:
        errors.append("VENT: VTR stack not connected to VENT graph!")

    # 6. Valves & cleanouts lie on exact nodes of their system graphs
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

    # 8. Door swing intersection audit (informational check for vertical room chases)
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
        "min_col_clearance_mm": round(min_col_clearance, 1),
        "tightest_col_pair": tightest_col_pair,
        "door_swing_crossings": door_hits,
    }
    return errors, warnings, stats


# ---------------------------------------------------------------------------
# 3. DXF Export (AC1009 R12 — Architectural Underlay + Plumbing Layers)
# ---------------------------------------------------------------------------

def write_dxf(arch_dxf_path, out_dxf_path, fixtures, valves, cleanouts, pipes):
    with open(arch_dxf_path, "r") as f:
        arch_lines = [l.rstrip("\r\n") for l in f]

    # Extract existing ENTITIES body from clinic_arch.dxf
    ent_start = None
    ent_end = None
    for i in range(0, len(arch_lines) - 1, 2):
        if arch_lines[i].strip() == "2" and arch_lines[i + 1].strip() == "ENTITIES":
            ent_start = i + 2
        elif ent_start is not None and arch_lines[i].strip() == "0" and arch_lines[i + 1].strip() == "ENDSEC":
            ent_end = i
            break
    arch_entity_lines = arch_lines[ent_start:ent_end]

    # Collect existing layers from clinic_arch.dxf
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
            # Dim architectural layers to subtle gray (ACI 8/9) except S-COLS (red/muted)
            dim_color = "9" if layer_name in ("S-GRID", "A-FURN", "A-ANNO", "A-DIMS", "A-DOOR") else "8"
            existing_layers.append((layer_name, dim_color, ltype))
            i = j
            continue
        i += 2

    plumbing_layers = [
        ("P-CW", "5", "CONTINUOUS"),        # Blue
        ("P-HW", "1", "DASHED"),            # Red
        ("P-SAN", "3", "CONTINUOUS"),       # Green
        ("P-VENT", "6", "DASHDOT"),         # Magenta
        ("P-VALVES", "4", "CONTINUOUS"),    # Cyan
        ("P-CLEANOUTS", "2", "CONTINUOUS"), # Yellow
        ("P-EQUIP", "5", "CONTINUOUS"),     # Blue
        ("P-LABELS", "7", "CONTINUOUS"),    # White/Black
    ]
    all_layers = existing_layers + plumbing_layers

    out = []
    def emit(code, val):
        out.append(str(code))
        out.append(str(val))

    # HEADER
    emit(0, "SECTION")
    emit(2, "HEADER")
    emit(9, "$ACADVER"); emit(1, "AC1009")
    emit(9, "$INSUNITS"); emit(70, 4)  # millimeters
    emit(9, "$EXTMIN"); emit(10, -2500.0); emit(20, -2500.0); emit(30, 0.0)
    emit(9, "$EXTMAX"); emit(10, 29500.0); emit(20, 13500.0); emit(30, 0.0)
    emit(0, "ENDSEC")

    # TABLES
    emit(0, "SECTION")
    emit(2, "TABLES")

    # LTYPE table
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

    # LAYER table
    emit(0, "TABLE"); emit(2, "LAYER"); emit(70, len(all_layers))
    for lname, lcol, lt in all_layers:
        emit(0, "LAYER"); emit(2, lname); emit(70, 0); emit(62, lcol); emit(6, lt)
    emit(0, "ENDTAB")
    emit(0, "ENDSEC")

    # ENTITIES
    emit(0, "SECTION")
    emit(2, "ENTITIES")

    # 1. Architectural background entities
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

    # 2. Plumbing Pipes
    for p in pipes:
        dxf_line(sys_layer[p["system"]], p["from_x"], p["from_y"], p["to_x"], p["to_y"])
        # Label major header/trunk segments
        length = math.hypot(p["to_x"] - p["from_x"], p["to_y"] - p["from_y"])
        if length >= 1800 and p["_tag"].startswith(("trunk:", "header:")):
            mx = 0.5 * (p["from_x"] + p["to_x"])
            my = 0.5 * (p["from_y"] + p["to_y"])
            if p["system"] == "SAN":
                lbl = f"SAN DN{p['dn_mm']} @{p['slope_pct']}% ({p['dfu_load']}DFU)"
            else:
                lbl = f"{p['system']} DN{p['dn_mm']}"
            dxf_text("P-LABELS", mx - 350, my + 45, 95.0, lbl)

    # 3. Valves (bowtie symbol + label)
    for v in valves:
        vx, vy = v["x"], v["y"]
        s = 70.0
        dxf_line("P-VALVES", vx - s, vy - s, vx + s, vy + s)
        dxf_line("P-VALVES", vx - s, vy + s, vx + s, vy - s)
        dxf_line("P-VALVES", vx - s, vy - s, vx - s, vy + s)
        dxf_line("P-VALVES", vx + s, vy - s, vx + s, vy + s)
        if v["id"] in ("V-MAIN-CW", "V-WH1-CW", "V-WH1-HW", "V-Z1-CW", "V-Z1-HW", "V-Z2-CW", "V-Z2-HW"):
            dxf_text("P-LABELS", vx + 90, vy - 40, 85.0, v["id"])

    # 4. Cleanouts (circle + X + CO label)
    for co in cleanouts:
        cx, cy = co["x"], co["y"]
        r = 95.0
        dxf_circle("P-CLEANOUTS", cx, cy, r)
        dxf_line("P-CLEANOUTS", cx - r * 0.7, cy - r * 0.7, cx + r * 0.7, cy + r * 0.7)
        dxf_line("P-CLEANOUTS", cx - r * 0.7, cy + r * 0.7, cx + r * 0.7, cy - r * 0.7)
        dxf_text("P-LABELS", cx - 120, cy + 120, 85.0, co["id"])

    # 5. Equipment & Fixture Nodes (WH1, MAIN_CW, SAN_OUT, VTR-1, VTR-2)
    # MAIN_CW riser
    dxf_circle("P-EQUIP", MAIN_CW_PT[0], MAIN_CW_PT[1], 140.0)
    dxf_circle("P-EQUIP", MAIN_CW_PT[0], MAIN_CW_PT[1], 70.0)
    dxf_text("P-LABELS", MAIN_CW_PT[0] - 420, MAIN_CW_PT[1] - 260, 105.0, "MAIN_CW DN32")

    # SAN_OUT exit
    dxf_circle("P-CLEANOUTS", SAN_OUT_PT[0], SAN_OUT_PT[1], 110.0)
    dxf_text("P-LABELS", SAN_OUT_PT[0] + 50, SAN_OUT_PT[1] + 150, 100.0, "SAN_OUT DN150 (23 DFU)")

    # VTR stacks
    for vtr_id, pt, dn in [("VTR-1 DN100", VTR1_PT, 100), ("VTR-2 DN75", VTR2_PT, 75)]:
        dxf_circle("P-VENT", pt[0], pt[1], 90.0)
        dxf_text("P-LABELS", pt[0] - 180, pt[1] - 200, 80.0, vtr_id)

    # Fixtures markers & tags
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

    # 1. Pipe summary by (system, dn_mm, slope_pct)
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

    # 2. Fixtures & Major Equipment
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

    # 3. Valves
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

    # 4. Cleanouts & VTR Stacks
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
# 5. High-Resolution Engineering PNG Renderer
# ---------------------------------------------------------------------------

def render_png(out_png_path, arch, fixtures, valves, cleanouts, pipes, pipe_agg, stats):
    fig, ax = plt.subplots(figsize=(24, 12.5), dpi=200)
    fig.patch.set_facecolor("#F8FAFC")
    ax.set_facecolor("#F8FAFC")

    # 1. Architectural rooms underlay
    room_colors = {
        "ElecITRoom": "#FEE2E2",   # Red-tinted NO-PLUMBING exclusion zone!
        "MechRoom": "#E0F2FE",
        "PublicWC": "#EFF6FF",
        "StaffWC": "#EFF6FF",
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
            ax.text(11200, 6250, "CENTRAL CLINICAL CORRIDOR (PLUMBING UTILITY RACK)",
                    ha="center", va="center", fontsize=8.0, fontweight="bold", color="#94A3B8", zorder=2)
        elif r["id"] == "ElecITRoom":
            ax.text(cx, cy - 350, "Elec / IT Room\nNO PLUMBING ZONE",
                    ha="center", va="center", fontsize=8.5, fontweight="bold", color="#B91C1C",
                    bbox=dict(boxstyle="round,pad=0.25", facecolor="#FEF2F2", edgecolor="#DC2626", lw=1.0), zorder=4)
        else:
            # Place room name cleanly away from wet fixture corner
            ty = r["min_y"] + 900 if r["min_y"] >= 7500 else r["min_y"] + 1350
            if r["id"] in ("PublicWC", "StaffWC"):
                ty = 8650
            elif r["id"] == "MechRoom":
                ty = 7950
            ax.text(cx, ty, r["id"], ha="center", va="center", fontsize=8.5, fontweight="bold", color="#334155", zorder=3)

    # Outer building shell border
    shell_pts = [(0, 0), (21600, 0), (21600, 5000), (27000, 5000), (27000, 11000), (0, 11000), (0, 0)]
    xs, ys = zip(*shell_pts)
    ax.plot(xs, ys, color="#1E293B", linewidth=3.0, zorder=3)

    # Architectural fixtures (muted background)
    for af in arch["fixtures"]:
        w = af["max_x"] - af["min_x"]
        h = af["max_y"] - af["min_y"]
        is_wet = any(k in af["kind"] for k in ("sink", "basin", "wc", "water_heater"))
        fc = "#DBEAFE" if is_wet else "#E2E8F0"
        ec = "#60A5FA" if is_wet else "#CBD5E1"
        ax.add_patch(patches.Rectangle((af["min_x"], af["min_y"]), w, h, facecolor=fc, edgecolor=ec, lw=0.9, zorder=2))

    # Doors (subtle arcs/boxes)
    for d in arch["doors"]:
        sb = d["swing_bbox"]
        ax.add_patch(patches.Rectangle(
            (sb[0], sb[1]), sb[2] - sb[0], sb[3] - sb[1],
            facecolor="none", edgecolor="#CBD5E1", lw=0.6, linestyle=":", zorder=2
        ))

    # Structural Columns (solid dark red/charcoal squares so 0-clash is visually obvious)
    for c in arch["columns"]:
        ax.add_patch(patches.Rectangle(
            (c["min_x"], c["min_y"]), c["max_x"] - c["min_x"], c["max_y"] - c["min_y"],
            facecolor="#991B1B", edgecolor="#450A0A", lw=1.0, zorder=5
        ))

    # Structural Gridlines
    grid_x = [(0, "A"), (5400, "B"), (10800, "C"), (16200, "D"), (21600, "E"), (27000, "F")]
    grid_y = [(0, "1"), (5000, "2"), (11000, "3")]
    for gx, glbl in grid_x:
        ax.plot([gx, gx], [-700, 11850], color="#CBD5E1", lw=0.7, linestyle="--", zorder=0)
        ax.add_patch(patches.Circle((gx, 12150), 320, facecolor="white", edgecolor="#334155", lw=1.2, zorder=6))
        ax.text(gx, 12150, glbl, ha="center", va="center", fontsize=9, fontweight="bold", color="#1E293B", zorder=7)
    for gy, glbl in grid_y:
        ax.plot([-800, 27600], [gy, gy], color="#CBD5E1", lw=0.7, linestyle="--", zorder=0)
        ax.add_patch(patches.Circle((-1150, gy), 320, facecolor="white", edgecolor="#334155", lw=1.2, zorder=6))
        ax.text(-1150, gy, glbl, ha="center", va="center", fontsize=9, fontweight="bold", color="#1E293B", zorder=7)

    # 2. Plumbing Piping Overlay
    style = {
        "CW":   {"color": "#0284C7", "ls": "-",  "z": 10},  # Domestic Cold Water (Blue solid)
        "HW":   {"color": "#DC2626", "ls": "--", "z": 11},  # Domestic Hot Water (Red dashed)
        "SAN":  {"color": "#15803D", "ls": "-",  "z": 12},  # Sanitary Drain (Dark Green heavy)
        "VENT": {"color": "#9333EA", "ls": "-.", "z": 9},   # Venting (Purple dash-dot)
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

    # Flow direction arrows on SAN trunk (East -> West)
    for ax_x in (22500, 18500, 12800, 7500, 2600):
        ax.annotate(
            "", xy=(ax_x - 600, Y_SAN_MAIN), xytext=(ax_x + 200, Y_SAN_MAIN),
            arrowprops=dict(arrowstyle="-|>", color="#15803D", lw=1.8, mutation_scale=13),
            zorder=15
        )

    # Pipe callout labels along the corridor rack (placed clear of zone valves)
    callouts = [
        (22500, Y_SAN_MAIN + 170, "SAN DN100 @ 2.0% (3 DFU)", "#15803D"),
        (18600, Y_SAN_MAIN + 170, "SAN DN100 @ 2.0% (6 DFU)", "#15803D"),
        (12600, Y_SAN_MAIN + 170, "SAN DN100 @ 2.0% (10 DFU)", "#15803D"),
        (7600,  Y_SAN_MAIN + 170, "SAN DN100 @ 2.0% (11 DFU)", "#15803D"),
        (2350,  Y_SAN_MAIN + 170, "SAN DN150 @ 1.5% (18–23 DFU)", "#15803D"),
        (18600, Y_VENT_MAIN - 175, "VENT HEADER DN50", "#9333EA"),
        (6000,  Y_VENT_MAIN - 175, "VENT HEADER DN75", "#9333EA"),
        (18600, Y_CW_MAIN + 145, "CW MAIN DN32", "#0284C7"),
        (6000,  Y_CW_MAIN + 145, "CW MAIN DN25", "#0284C7"),
        (18600, Y_HW_MAIN - 175, "HW MAIN DN25", "#DC2626"),
        (6000,  Y_HW_MAIN - 175, "HW MAIN DN25", "#DC2626"),
    ]
    for lx, ly, txt, col in callouts:
        ax.text(
            lx, ly, txt, ha="center", va="center", fontsize=6.8, fontweight="bold", color=col,
            bbox=dict(boxstyle="round,pad=0.14", facecolor="white", edgecolor="none", alpha=0.88),
            zorder=16
        )

    # 3. Valves (bowtie markers)
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
                "V-Z1-CW":   (0, 240),
                "V-Z1-HW":   (0, -240),
                "V-Z2-CW":   (0, 240),
                "V-Z2-HW":   (0, -240),
            }
            dx, dy = offsets[v["id"]]
            ax.text(vx + dx, vy + dy, v["id"], ha="center", va="center", fontsize=6.2, fontweight="bold",
                    color=col, bbox=dict(boxstyle="round,pad=0.12", facecolor="#FFFBEB", edgecolor=col, lw=0.7), zorder=18)

    # 4. Cleanouts
    for co in cleanouts:
        cx, cy = co["x"], co["y"]
        ax.add_patch(patches.Circle((cx, cy), 120, facecolor="#FEF08A", edgecolor="#854D0E", lw=1.4, zorder=18))
        ax.text(cx, cy, "CO", ha="center", va="center", fontsize=5.2, fontweight="bold", color="#713F12", zorder=19)

    # 5. Special Equipment: MAIN_CW, WH1, SAN_OUT, VTR-1, VTR-2
    ax.add_patch(patches.Circle(MAIN_CW_PT, 210, facecolor="#0284C7", edgecolor="#0C4A6E", lw=1.8, zorder=19))
    ax.text(MAIN_CW_PT[0], MAIN_CW_PT[1], "CW", ha="center", va="center", fontsize=6.5, fontweight="bold", color="white", zorder=20)
    ax.text(MAIN_CW_PT[0], MAIN_CW_PT[1] - 350, "MAIN_CW\n(DN32 Riser)", ha="center", va="top",
            fontsize=6.2, fontweight="bold", color="#0C4A6E",
            bbox=dict(boxstyle="round,pad=0.14", facecolor="#E0F2FE", edgecolor="#0284C7", lw=0.8), zorder=20)

    ax.add_patch(patches.Circle(SAN_OUT_PT, 175, facecolor="#15803D", edgecolor="#052E16", lw=1.6, zorder=19))
    ax.text(SAN_OUT_PT[0] - 850, SAN_OUT_PT[1] + 50, "SAN_OUT\nDN150 (23 DFU)", ha="center", va="center",
            fontsize=6.8, fontweight="bold", color="#14532D",
            bbox=dict(boxstyle="round,pad=0.18", facecolor="#DCFCE7", edgecolor="#15803D", lw=0.9), zorder=20)

    for vtr_id, pt in [("VTR-1 DN100", VTR1_PT), ("VTR-2 DN75", VTR2_PT)]:
        ax.add_patch(patches.Circle(pt, 120, facecolor="#F3E8FF", edgecolor="#7E22CE", lw=1.4, zorder=19))
        ax.text(pt[0], 11220, vtr_id, ha="center", va="bottom", fontsize=6.2, fontweight="bold", color="#6B21A8",
                bbox=dict(boxstyle="round,pad=0.12", facecolor="#FAF5FF", edgecolor="#9333EA", lw=0.6), zorder=20)

    # 6. Fixtures badges
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
            ax.text(
                fx["x"] + lbl_dx, fx["y"] + lbl_dy, f"{fx['id']}\n({fx['dfu']} DFU)",
                ha="center", va="top", fontsize=6.2, fontweight="bold", color="#0F172A",
                bbox=dict(boxstyle="round,pad=0.12", facecolor="white", edgecolor="#94A3B8", lw=0.6, alpha=0.92),
                zorder=19
            )

    # 7. Title Block & Legend in the open South-East notch (x = 22100..27400, y = -200..4650)
    tb = patches.FancyBboxPatch(
        (22100, -150), 5350, 4750,
        boxstyle="round,pad=120", facecolor="white", edgecolor="#1E293B", linewidth=1.6, zorder=15
    )
    ax.add_patch(tb)
    ax.text(24775, 4350, "OUTPATIENT SPECIALIST CLINIC", ha="center", va="center",
            fontsize=11.5, fontweight="bold", color="#0F172A", zorder=16)
    ax.text(24775, 3980, "PLUMBING PLAN — CW / HW / SAN / VENT (IPC)", ha="center", va="center",
            fontsize=8.2, fontweight="bold", color="#0369A1", zorder=16)
    ax.plot([22350, 27200], [3740, 3740], color="#CBD5E1", lw=1.0, zorder=16)

    # Pipe totals by system
    sys_len = defaultdict(float)
    for (s, dn, sl), d in pipe_agg.items():
        sys_len[s] += d["length_m"]

    summary_lines = [
        f"Wet Fixtures Served: {stats['total_fixtures']} / {stats['total_fixtures']} (100%)",
        f"Total Drainage Load: {stats['total_dfu']} DFU (WC=4, Mop=3, Sink=2, Lav=1)",
        f"SAN Main Trunk: DN100 @ 2.0% -> DN150 @ 1.5%",
        f"CW Main: DN32 / DN25  |  HW Main: DN25 (WH1)",
        f"Valves: {stats['total_valves']} (Main + Zone + Fixture)  |  Cleanouts: {stats['total_cleanouts']}",
        f"Pipe Lengths: CW {sys_len['CW']:.1f}m | HW {sys_len['HW']:.1f}m | SAN {sys_len['SAN']:.1f}m | VENT {sys_len['VENT']:.1f}m",
        f"Clash Audit: 0 ElecITRoom | 0 Column (min {stats['min_col_clearance_mm']:.0f} mm)",
    ]
    y_txt = 3440
    for ln in summary_lines:
        ax.text(22400, y_txt, ln, ha="left", va="center", fontsize=7.0, color="#1E293B", zorder=16)
        y_txt -= 285

    ax.plot([22350, 27200], [1370, 1370], color="#CBD5E1", lw=1.0, zorder=16)
    ax.text(22400, 1150, "LEGEND", ha="left", va="center", fontsize=8.0, fontweight="bold", color="#0F172A", zorder=16)

    legend_items = [
        (22400, 880, "#0284C7", "-",  2.6, "Domestic Cold Water (CW) — DN15–32"),
        (22400, 610, "#DC2626", "--", 2.6, "Domestic Hot Water (HW) — DN15–25"),
        (22400, 340, "#15803D", "-",  3.4, "Sanitary Drain (SAN) — DN50–150 (1.5–2%)"),
        (22400,  70, "#9333EA", "-.", 2.2, "Sanitary Vent (VENT) — DN40–100 + VTR"),
    ]
    for lx, ly, col, ls, lw, desc in legend_items:
        ax.plot([lx, lx + 700], [ly, ly], color=col, linestyle=ls, linewidth=lw, zorder=16)
        ax.text(lx + 850, ly, desc, ha="left", va="center", fontsize=6.9, color="#1E293B", zorder=16)

    # Bottom-left symbol legend
    ax.plot(1000, -900, marker="D", markersize=7, color="white", markeredgecolor="#0369A1", markeredgewidth=1.5, zorder=16)
    ax.text(1350, -900, "Isolation Valve (CW/HW)", ha="left", va="center", fontsize=7.5, color="#1E293B")
    ax.add_patch(patches.Circle((5600, -900), 130, facecolor="#FEF08A", edgecolor="#854D0E", lw=1.3, zorder=16))
    ax.text(5600, -900, "CO", ha="center", va="center", fontsize=5.2, fontweight="bold", color="#713F12", zorder=17)
    ax.text(5900, -900, "Sanitary Cleanout (IPC 708)", ha="left", va="center", fontsize=7.5, color="#1E293B")
    ax.add_patch(patches.Rectangle((10400, -1040), 280, 280, facecolor="#991B1B", edgecolor="#450A0A", lw=1.0, zorder=16))
    ax.text(10850, -900, "RC Column (300×300 — 0 Clashes)", ha="left", va="center", fontsize=7.5, color="#1E293B")
    ax.add_patch(patches.Rectangle((16200, -1040), 380, 280, facecolor="#FEE2E2", edgecolor="#DC2626", hatch="///", lw=1.0, zorder=16))
    ax.text(16750, -900, "Elec/IT Exclusion Zone (0 Pipes)", ha="left", va="center", fontsize=7.5, color="#B91C1C", fontweight="bold")

    ax.set_xlim(-1800, 27900)
    ax.set_ylim(-1500, 12800)
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

    print("=== PLUMBING VERIFICATION REPORT ===")
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

    # Write clean schema-exact plumbing_manifest.json
    manifest = {
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
    }
    manifest_path = OUT_DIR / "plumbing_manifest.json"
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    pipe_agg = write_schedule_csv(OUT_DIR / "plumbing_schedule.csv", fixtures, valves, cleanouts, pipes)
    write_dxf(ARCH_DXF_PATH, OUT_DIR / "clinic_plumbing.dxf", fixtures, valves, cleanouts, pipes)
    render_png(OUT_DIR / "clinic_plumbing.png", arch, fixtures, valves, cleanouts, pipes, pipe_agg, stats)
    print("Generated all deliverables successfully in:", OUT_DIR)


if __name__ == "__main__":
    main()
