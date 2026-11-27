#!/usr/bin/env python3
"""
Benchmark 6 — Baseline Arm (NO-EDL Python + CAD):
Revision B ECO (Engineering Change Order) — 17-Room Outpatient Specialist Medical Clinic
Architecture + Structural Column Grid Generator, Verifier, DXF Exporter, and PNG Renderer.

Rev B ECO Changes from Rev A:
  - ECO-1: Expanded Lab by +1.0 m width (3300 -> 4300 mm; 11.05 -> 14.41 m²), shifting
           Pharmacy, ElecITRoom, MechRoom, and the eastern exterior wall / Gridline F east
           by +1.0 m (building width 27,000 -> 28,000 mm). Updated structural grid cleanly
           to 5 regular 5.6 m bays (GRID_X = [0, 5600, 11200, 16800, 22400, 28000]), 18 cols.
  - ECO-2: Added 5th Examination Room (Exam5, 14.57 m²) on the North Wing with north window
           W14, corridor door D17, exam table, doctor desk, and clinical wash basin (F-EX5-SINK);
           enclosed the SE structural bay to accommodate Exam4 (16.61 m²) alongside Exam1-3
           and Procedure (26.07 m²) with zero overlaps and zero collisions across all 17 rooms.
  - ECO-3: Prepared NurseStation East wall bay for relocated DB_CLINIC (>=2.75 m shift from
           Rev A position, >=1100x1000 mm unobstructed working clearance).

Units: millimeters (mm) everywhere in geometry, DXF, and manifest.
Coordinate system: +X East, +Y North.
"""

import json
import math
import os
from dataclasses import dataclass
from typing import Dict, List, Tuple

import networkx as nx
import numpy as np
from shapely.geometry import box, Polygon
from shapely.ops import unary_union
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle, Arc, Circle, Polygon as MplPolygon, FancyBboxPatch


OUT_DIR = "/home/lindah/projects/EDL/benchmarks/v3_5_collapse_ab/b6_clinic_eco/baseline"

# ---------------------------------------------------------------------------
# 1. PARAMETRIC ARCHITECTURAL & STRUCTURAL CONSTANTS (mm) — REV B
# ---------------------------------------------------------------------------
EXT_WALL_T = 300.0   # Exterior wall thickness (mm)
INT_WALL_T = 150.0   # Interior partition thickness (mm)
COL_SIZE = 300.0     # Structural column 300x300 mm

# Regular structural grid: 5 equal bays of 5.6 m in X (28.0 m overall); 2 bays (5.0 m, 6.0 m) in Y
GRID_X = [0.0, 5600.0, 11200.0, 16800.0, 22400.0, 28000.0]
GRID_X_LABELS = ["A", "B", "C", "D", "E", "F"]
GRID_Y = [0.0, 5000.0, 11000.0]
GRID_Y_LABELS = ["1", "2", "3"]

# Key Y coordinates (interior clear faces)
Y_SOUTH_INT = 150.0        # North face of South exterior wall (Y = -150..150)
Y_CORR_WALL_S0 = 5000.0    # South face of South corridor partition (Y = 5000..5150)
Y_CORR_S = 5150.0          # South clear face of Corridor (flush with North face of Y=5000 columns!)
Y_CORR_N = 7350.0          # North clear face of Corridor (2200 mm clear width >= 1800 mm)
Y_NORTH_INT_S = 7500.0     # South clear face of North rooms (partition Y = 7350..7500)
Y_NORTH_INT_N = 10850.0    # South face of North exterior wall (Y = 10850..11150)

# Exterior boundary polygon (full 28.0 m x 11.0 m centerline rectangle; outer +/-150 mm)
FOOTPRINT_OUTER_PTS = [
    (-150.0, -150.0),
    (28150.0, -150.0),
    (28150.0, 11150.0),
    (-150.0, 11150.0),
]

FOOTPRINT_INNER_PTS = [
    (150.0, 150.0),
    (27850.0, 150.0),
    (27850.0, 10850.0),
    (150.0, 10850.0),
]


@dataclass
class Room:
    id: str
    label: str
    zone: str
    min_x: float
    min_y: float
    max_x: float
    max_y: float
    habitable: bool
    min_area_req: float = 0.0

    @property
    def width(self) -> float:
        return self.max_x - self.min_x

    @property
    def depth(self) -> float:
        return self.max_y - self.min_y

    @property
    def area_m2(self) -> float:
        return round((self.width * self.depth) / 1e6, 2)

    @property
    def poly(self) -> Polygon:
        return box(self.min_x, self.min_y, self.max_x, self.max_y)


@dataclass
class Door:
    id: str
    room_a: str
    room_b: str
    wall_orient: str      # 'H' (horizontal wall) or 'V' (vertical wall)
    wall_min: float       # wall thickness start (y for H, x for V)
    wall_max: float       # wall thickness end (y for H, x for V)
    open_start: float     # opening start along wall (x for H, y for V)
    width_mm: float
    hinge_x: float
    hinge_y: float
    open_dir: str         # '+N', '+S', '+E', '+W' direction door swings INTO room
    hinge_side: str       # 'W' or 'E' (for H); 'S' or 'N' (for V)

    @property
    def open_end(self) -> float:
        return self.open_start + self.width_mm

    @property
    def swing_bbox(self) -> List[float]:
        w = self.width_mm
        hx, hy = self.hinge_x, self.hinge_y
        if self.wall_orient == "H":
            x0 = hx if self.hinge_side == "W" else hx - w
            x1 = hx + w if self.hinge_side == "W" else hx
            y0 = hy if self.open_dir == "+N" else hy - w
            y1 = hy + w if self.open_dir == "+N" else hy
        else:
            y0 = hy if self.hinge_side == "S" else hy - w
            y1 = hy + w if self.hinge_side == "S" else hy
            x0 = hx if self.open_dir == "+E" else hx - w
            x1 = hx + w if self.open_dir == "+E" else hx
        return [round(x0, 1), round(y0, 1), round(x1, 1), round(y1, 1)]

    @property
    def arc_angles(self) -> Tuple[float, float]:
        """Return (start_deg, end_deg) CCW arc angles in standard math coordinates."""
        if self.wall_orient == "H":
            if self.hinge_side == "W" and self.open_dir == "+N":
                return (0.0, 90.0)
            if self.hinge_side == "E" and self.open_dir == "+N":
                return (90.0, 180.0)
            if self.hinge_side == "W" and self.open_dir == "+S":
                return (270.0, 360.0)
            if self.hinge_side == "E" and self.open_dir == "+S":
                return (180.0, 270.0)
        else:
            if self.hinge_side == "S" and self.open_dir == "+E":
                return (0.0, 90.0)
            if self.hinge_side == "N" and self.open_dir == "+E":
                return (270.0, 360.0)
            if self.hinge_side == "S" and self.open_dir == "+W":
                return (90.0, 180.0)
            if self.hinge_side == "N" and self.open_dir == "+W":
                return (180.0, 270.0)
        raise ValueError(f"Invalid door swing config for {self.id}")

    @property
    def leaf_end(self) -> Tuple[float, float]:
        """Tip of the open door leaf (90 deg open)."""
        w = self.width_mm
        if self.open_dir == "+N":
            return (self.hinge_x, self.hinge_y + w)
        if self.open_dir == "+S":
            return (self.hinge_x, self.hinge_y - w)
        if self.open_dir == "+E":
            return (self.hinge_x + w, self.hinge_y)
        if self.open_dir == "+W":
            return (self.hinge_x - w, self.hinge_y)
        raise ValueError(self.open_dir)

    @property
    def opening_poly(self) -> Polygon:
        if self.wall_orient == "H":
            return box(self.open_start, self.wall_min, self.open_end, self.wall_max)
        return box(self.wall_min, self.open_start, self.wall_max, self.open_end)


@dataclass
class Window:
    id: str
    room: str
    wall_orient: str   # 'H' or 'V'
    x: float           # center x on exterior wall centerline
    y: float           # center y on exterior wall centerline
    width_mm: float

    @property
    def opening_poly(self) -> Polygon:
        hw = self.width_mm / 2.0
        ht = EXT_WALL_T / 2.0
        if self.wall_orient == "H":
            return box(self.x - hw, self.y - ht, self.x + hw, self.y + ht)
        return box(self.x - ht, self.y - hw, self.x + ht, self.y + hw)


@dataclass
class Column:
    id: str
    cx: float
    cy: float
    size: float = COL_SIZE

    @property
    def min_x(self) -> float:
        return self.cx - self.size / 2.0

    @property
    def min_y(self) -> float:
        return self.cy - self.size / 2.0

    @property
    def max_x(self) -> float:
        return self.cx + self.size / 2.0

    @property
    def max_y(self) -> float:
        return self.cy + self.size / 2.0

    @property
    def poly(self) -> Polygon:
        return box(self.min_x, self.min_y, self.max_x, self.max_y)


@dataclass
class Fixture:
    id: str
    room: str
    kind: str
    min_x: float
    min_y: float
    max_x: float
    max_y: float
    wet: bool = False

    @property
    def poly(self) -> Polygon:
        return box(self.min_x, self.min_y, self.max_x, self.max_y)


# ---------------------------------------------------------------------------
# 2. BUILD MODEL (17 ROOMS, 18 COLUMNS, 17 DOORS, 14 WINDOWS, 33 FIXTURES)
# ---------------------------------------------------------------------------

def build_model():
    # --- South Wing: Y = 150 .. 5000 (depth 4850 mm), X = 150 .. 27850 (27700 mm) ---
    # 7 rooms + 6 partitions (900 mm) => net room widths = 26800 mm
    south_specs = [
        ("Vestibule",  "Vestibule",            "public",     150,  2000, False,  6.0),
        ("Waiting",    "Reception &\nWaiting", "public",    2150,  8000, True,  28.0),
        ("Exam1",      "Exam 1",               "clinical",  8150, 11550, True,  12.0),
        ("Exam2",      "Exam 2",               "clinical", 11700, 15150, True,  12.0),
        ("Exam3",      "Exam 3",               "clinical", 15300, 18750, True,  12.0),
        ("Exam4",      "Exam 4",               "clinical", 18900, 22325, True,  12.0),
        ("Procedure",  "Minor\nProcedure",     "clinical", 22475, 27850, True,  18.0),
    ]

    # --- North Wing: Y = 7500 .. 10850 (depth 3350 mm), X = 150 .. 27850 (27700 mm) ---
    # 9 rooms + 8 partitions (1200 mm) => net room widths = 26500 mm
    # ECO-1: Lab expanded +1.0 m (3300 -> 4300 mm); Pharmacy, ElecITRoom, MechRoom shifted +1.0 m East
    # ECO-2: Exam5 added on North Wing (4350 mm x 3350 mm = 14.57 m²)
    north_widths = [
        ("PublicWC",     "Public\nWC",        "public",   1900, False,  4.5),
        ("StaffWC",      "Staff\nWC",         "staff",    1500, False,  3.5),
        ("Breakroom",    "Staff\nBreakroom",  "staff",    3600, True,  12.0),
        ("NurseStation", "Nurse\nStation",    "clinical", 3250, True,  10.0),
        ("Exam5",        "Exam 5\n(REV B)",   "clinical", 4350, True,  14.0),
        ("Lab",          "Lab &\nSteril.",    "clinical", 4300, True,  10.0),
        ("Pharmacy",     "Pharmacy &\nClean", "service",  2700, False,  8.0),
        ("ElecITRoom",   "Elec /\nIT Room",   "service",  2200, False,  6.0),
        ("MechRoom",     "Mech /\nJanitor",   "service",  2700, False,  7.0),
    ]

    rooms: List[Room] = []
    for rid, lbl, zone, x0, x1, hab, req in south_specs:
        rooms.append(Room(rid, lbl, zone, float(x0), Y_SOUTH_INT, float(x1), Y_CORR_WALL_S0, hab, req))

    # Central Corridor: Y = 5150 .. 7350 (clear width 2200 mm), X = 150 .. 27850
    rooms.append(Room("Corridor", "Central Clinical Corridor", "circ",
                      150.0, Y_CORR_S, 27850.0, Y_CORR_N, False, 0.0))

    cur_x = 150.0
    for rid, lbl, zone, w, hab, req in north_widths:
        rooms.append(Room(rid, lbl, zone, cur_x, Y_NORTH_INT_S, cur_x + w, Y_NORTH_INT_N, hab, req))
        cur_x += w + INT_WALL_T

    assert abs((cur_x - INT_WALL_T) - 27850.0) < 1e-6, f"North strip end mismatch: {cur_x - INT_WALL_T}"
    assert abs(south_specs[-1][4] - 27850.0) < 1e-6, f"South strip end mismatch: {south_specs[-1][4]}"

    rmap: Dict[str, Room] = {r.id: r for r in rooms}

    # --- Structural Columns (18 columns on regular 6x3 grid: 5x5.6m X, 5.0/6.0m Y) ---
    columns: List[Column] = []
    for yi, y in enumerate(GRID_Y):
        for xi, x in enumerate(GRID_X):
            cid = f"C-{GRID_X_LABELS[xi]}{GRID_Y_LABELS[yi]}"
            columns.append(Column(cid, x, y, COL_SIZE))

    # --- Doors (17 doors, all interior doors swing inward into rooms, >=100 mm jamb margins) ---
    doors: List[Door] = [
        # Exterior -> Vestibule (width 1300 mm >= 1200 mm)
        Door("D01", "Exterior", "Vestibule", "H", -150.0, 150.0, 425.0, 1300.0,
             hinge_x=425.0, hinge_y=150.0, open_dir="+N", hinge_side="W"),
        # Vestibule -> Waiting (width 1200 mm)
        Door("D02", "Vestibule", "Waiting", "V", 2000.0, 2150.0, 1800.0, 1200.0,
             hinge_x=2150.0, hinge_y=1800.0, open_dir="+E", hinge_side="S"),
        # Waiting -> Corridor (width 1200 mm, swings South into Waiting; clear of C-B2 at 5450..5750)
        Door("D03", "Waiting", "Corridor", "H", 5000.0, 5150.0, 6400.0, 1200.0,
             hinge_x=7600.0, hinge_y=5000.0, open_dir="+S", hinge_side="E"),

        # South Clinical Wing doors (all swing South into rooms, clear of Y=5000 columns!)
        # Exam1: X = 8150..11550; col C-C2 at 11050..11350 -> door at 8450..9400
        Door("D04", "Corridor", "Exam1", "H", 5000.0, 5150.0, 8450.0, 950.0,
             hinge_x=8450.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),
        # Exam2: X = 11700..15150; no col -> door at 12000..12950
        Door("D05", "Corridor", "Exam2", "H", 5000.0, 5150.0, 12000.0, 950.0,
             hinge_x=12000.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),
        # Exam3: X = 15300..18750; col C-D2 at 16650..16950 -> door east of col at 17450..18400
        Door("D06", "Corridor", "Exam3", "H", 5000.0, 5150.0, 17450.0, 950.0,
             hinge_x=18400.0, hinge_y=5000.0, open_dir="+S", hinge_side="E"),
        # Exam4: X = 18900..22325; col C-E2 at 22250..22550 -> door at 19200..20150
        Door("D12", "Corridor", "Exam4", "H", 5000.0, 5150.0, 19200.0, 950.0,
             hinge_x=19200.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),
        # Procedure: X = 22475..27850; wide door 1200 mm (>= 1100 mm) at 23000..24200
        Door("D07", "Corridor", "Procedure", "H", 5000.0, 5150.0, 23000.0, 1200.0,
             hinge_x=23000.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),

        # North Wing doors (all on Y = 7350..7500, swing North into rooms)
        Door("D08", "Corridor", "PublicWC", "H", 7350.0, 7500.0, 450.0, 950.0,
             hinge_x=450.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        Door("D09", "Corridor", "StaffWC", "H", 7350.0, 7500.0, 2450.0, 900.0,
             hinge_x=2450.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        Door("D10", "Corridor", "Breakroom", "H", 7350.0, 7500.0, 4150.0, 950.0,
             hinge_x=4150.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # NurseStation: X = 7600..10850 -> wide 1100 mm door at 7850..8950
        Door("D11", "Corridor", "NurseStation", "H", 7350.0, 7500.0, 7850.0, 1100.0,
             hinge_x=7850.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # Exam5 (NEW REV B): X = 11000..15350 -> 950 mm door at 11300..12250
        Door("D17", "Corridor", "Exam5", "H", 7350.0, 7500.0, 11300.0, 950.0,
             hinge_x=11300.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # Lab (EXPANDED +1.0 m): X = 15500..19800 -> 950 mm door at 15800..16750
        Door("D13", "Corridor", "Lab", "H", 7350.0, 7500.0, 15800.0, 950.0,
             hinge_x=15800.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # Pharmacy (+1.0 m East): X = 19950..22650 -> 900 mm door at 20250..21150
        Door("D14", "Corridor", "Pharmacy", "H", 7350.0, 7500.0, 20250.0, 900.0,
             hinge_x=20250.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # ElecITRoom (+1.0 m East): X = 22800..25000 -> 900 mm door at 23100..24000
        Door("D15", "Corridor", "ElecITRoom", "H", 7350.0, 7500.0, 23100.0, 900.0,
             hinge_x=23100.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # MechRoom (+1.0 m East): X = 25150..27850 -> 950 mm door at 25450..26400
        Door("D16", "Corridor", "MechRoom", "H", 7350.0, 7500.0, 25450.0, 950.0,
             hinge_x=25450.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
    ]

    # --- Exterior Windows (14 windows, all strictly on exterior walls & clear of columns) ---
    # Columns at X = 0, 5600, 11200, 16800, 22400, 28000 (+/- 150 mm)
    windows: List[Window] = [
        # South exterior wall (Y = 0)
        Window("W01", "Waiting",      "H",  3650.0,     0.0, 2000.0),  # 2650..4650
        Window("W02", "Waiting",      "H",  6850.0,     0.0, 1600.0),  # 6050..7650 (clear of C-B1)
        Window("W03", "Exam1",        "H",  9600.0,     0.0, 1400.0),  # 8900..10300 (clear of C-C1)
        Window("W04", "Exam2",        "H", 13425.0,     0.0, 1600.0),  # 12625..14225
        Window("W05", "Exam3",        "H", 17850.0,     0.0, 1400.0),  # 17150..18550 (clear of C-D1)
        Window("W10", "Exam4",        "H", 20600.0,     0.0, 1600.0),  # 19800..21400 (clear of C-E1)
        Window("W06", "Procedure",    "H", 25150.0,     0.0, 2000.0),  # 24150..26150
        # East exterior wall of Procedure (X = 28000)
        Window("W07", "Procedure",    "V", 28000.0,  2500.0, 1800.0),
        # North exterior wall (Y = 11000)
        Window("W08", "Breakroom",    "H",  6550.0, 11000.0, 1300.0),  # 5900..7200 (clear of C-B3)
        Window("W09", "NurseStation", "H",  9225.0, 11000.0, 1500.0),  # 8475..9975
        Window("W14", "Exam5",        "H", 13400.0, 11000.0, 1600.0),  # 12600..14200 (clear of C-C3)
        Window("W11", "Lab",          "H", 18350.0, 11000.0, 1600.0),  # 17550..19150 (clear of C-D3)
        # Corridor daylighting end windows (West X = 0, East X = 28000)
        Window("W12", "Corridor",     "V",     0.0,  6250.0, 1400.0),
        Window("W13", "Corridor",     "V", 28000.0,  6250.0, 1400.0),
    ]

    # --- Fixtures (33 fixtures, strictly inside rooms, 0 collisions with swings/columns/fixtures) ---
    fixtures: List[Fixture] = [
        # Waiting (X=2150..8000, Y=150..5000; D02=[2150,1800,3350,3000], D03=[6400,3800,7600,5000], C-B2=[5450,4850,5750,5150])
        Fixture("F-WAIT-REC",   "Waiting", "reception_counter", 2500, 3850, 5050, 4650, wet=False),
        Fixture("F-WAIT-SEAT1", "Waiting", "waiting_seating",   3500,  450, 6100, 1100, wet=False),
        Fixture("F-WAIT-SEAT2", "Waiting", "waiting_seating",   7200,  700, 7900, 3200, wet=False),

        # Exam1 (X=8150..11550, Y=150..5000; D04=[8450,4050,9400,5000], C-C2=[11050,4850,11350,5150])
        Fixture("F-EX1-TBL",  "Exam1", "exam_table",     9650, 1750, 10450, 3650, wet=False),
        Fixture("F-EX1-SINK", "Exam1", "handwash_basin", 10150, 4400, 10850, 4950, wet=True),
        Fixture("F-EX1-DESK", "Exam1", "desk",           8300,  350,  9700,  950, wet=False),

        # Exam2 (X=11700..15150, Y=150..5000; D05=[12000,4050,12950,5000])
        Fixture("F-EX2-TBL",  "Exam2", "exam_table",     13200, 1750, 14000, 3650, wet=False),
        Fixture("F-EX2-SINK", "Exam2", "handwash_basin", 14100, 4400, 14800, 4950, wet=True),
        Fixture("F-EX2-DESK", "Exam2", "desk",           11850,  350, 13250,  950, wet=False),

        # Exam3 (X=15300..18750, Y=150..5000; D06=[17450,4050,18400,5000], C-D2=[16650,4850,16950,5150])
        Fixture("F-EX3-TBL",  "Exam3", "exam_table",     16400, 1750, 17200, 3650, wet=False),
        Fixture("F-EX3-SINK", "Exam3", "handwash_basin", 15500, 4400, 16200, 4950, wet=True),
        Fixture("F-EX3-DESK", "Exam3", "desk",           17200,  350, 18600,  950, wet=False),

        # Exam4 (X=18900..22325, Y=150..5000; D12=[19200,4050,20150,5000], C-E2=[22250,4850,22550,5150])
        Fixture("F-EX4-TBL",  "Exam4", "exam_table",     20400, 1750, 21200, 3650, wet=False),
        Fixture("F-EX4-SINK", "Exam4", "handwash_basin", 21150, 4400, 21850, 4950, wet=True),
        Fixture("F-EX4-DESK", "Exam4", "desk",           19050,  350, 20450,  950, wet=False),

        # Procedure (X=22475..27850, Y=150..5000; D07=[23000,3800,24200,5000])
        Fixture("F-PROC-TBL",  "Procedure", "procedure_table",    24650, 1650, 25600, 3750, wet=False),
        Fixture("F-PROC-SINK", "Procedure", "scrub_sink",         25500, 4300, 26900, 4950, wet=True),
        Fixture("F-PROC-CAB",  "Procedure", "instrument_cabinet", 22650,  550, 23300, 2650, wet=False),

        # PublicWC (X=150..2050, Y=7500..10850; D08=[450,7500,1400,8450])
        Fixture("F-PWC-WC",   "PublicWC", "wc_accessible",  1350, 10050, 1950, 10800, wet=True),
        Fixture("F-PWC-SINK", "PublicWC", "handwash_basin",  200,  9600,  750, 10300, wet=True),

        # StaffWC (X=2200..3700, Y=7500..10850; D09=[2450,7500,3350,8400])
        Fixture("F-SWC-WC",   "StaffWC", "wc",             2950, 10050, 3550, 10800, wet=True),
        Fixture("F-SWC-SINK", "StaffWC", "handwash_basin", 2250,  9600, 2750, 10250, wet=True),

        # Breakroom (X=3850..7450, Y=7500..10850; D10=[4150,7500,5100,8450])
        Fixture("F-BRK-CTR",  "Breakroom", "kitchen_counter", 3900,  8900, 4550, 10050, wet=False),
        Fixture("F-BRK-SINK", "Breakroom", "kitchen_sink",    3900, 10060, 4550, 10800, wet=True),
        Fixture("F-BRK-TBL",  "Breakroom", "break_table",     5550,  8400, 7150,  9600, wet=False),

        # NurseStation (X=7600..10850, Y=7500..10850; D11=[7850,7500,8950,8600])
        # East wall [10600..10850, 8950..9550] + front clearance [9500..10600, 8750..9750] reserved for DB_CLINIC!
        Fixture("F-NUR-CTR",  "NurseStation", "nurse_counter", 7700,  8800,  9250,  9800, wet=False),
        Fixture("F-NUR-DESK", "NurseStation", "workstation",   7750, 10050, 10200, 10700, wet=False),

        # Exam5 (NEW REV B: X=11000..15350, Y=7500..10850; D17=[11300,7500,12250,8450])
        Fixture("F-EX5-TBL",  "Exam5", "exam_table",     12850, 8150, 14750,  8950, wet=False),
        Fixture("F-EX5-SINK", "Exam5", "handwash_basin", 14750, 9750, 15300, 10450, wet=True),
        Fixture("F-EX5-DESK", "Exam5", "desk",           11200, 9950, 12600, 10650, wet=False),

        # Lab (EXPANDED +1.0 m: X=15500..19800, Y=7500..10850; D13=[15800,7500,16750,8450])
        Fixture("F-LAB-BNCH",  "Lab", "lab_bench",          17100, 7650, 19750,  8350, wet=False),
        Fixture("F-LAB-SINK",  "Lab", "sterilization_sink", 15550, 9800, 16650, 10650, wet=True),
        Fixture("F-LAB-BNCH2", "Lab", "lab_bench",          16700, 9950, 19750, 10700, wet=False),

        # Pharmacy (+1.0 m East: X=19950..22650, Y=7500..10850; D14=[20250,7500,21150,8400])
        Fixture("F-PHM-SHL1", "Pharmacy", "pharmacy_shelving", 20000, 10150, 22600, 10800, wet=False),
        Fixture("F-PHM-SHL2", "Pharmacy", "pharmacy_shelving", 21950,  7650, 22600,  9950, wet=False),

        # ElecITRoom (+1.0 m East: X=22800..25000, Y=7500..10850; D15=[23100,7500,24000,8400]) -> STRICTLY DRY ZONE!
        Fixture("F-ELC-RACK", "ElecITRoom", "it_server_rack", 22900, 9850, 23950, 10750, wet=False),
        Fixture("F-ELC-PNL",  "ElecITRoom", "elec_panel",     24450, 8600, 24950, 10500, wet=False),

        # MechRoom (+1.0 m East: X=25150..27850, Y=7500..10850; D16=[25450,7500,26400,8450])
        Fixture("F-MCH-MOP",  "MechRoom", "mop_sink",     25250, 9900, 26050, 10750, wet=True),
        Fixture("F-MCH-WH",   "MechRoom", "water_heater", 26650, 9650, 27650, 10650, wet=True),
    ]

    # --- Wall Polygons (with exact door & window cutouts) ---
    outer_poly = Polygon(FOOTPRINT_OUTER_PTS)
    inner_poly = Polygon(FOOTPRINT_INNER_PTS)
    ext_wall_solid = outer_poly.difference(inner_poly)

    ext_cutouts = [w.opening_poly for w in windows] + [d.opening_poly for d in doors if d.room_a == "Exterior"]
    ext_walls_geom = ext_wall_solid.difference(unary_union(ext_cutouts))

    rooms_union = unary_union([r.poly for r in rooms])
    int_wall_solid = inner_poly.difference(rooms_union)
    int_cutouts = [d.opening_poly for d in doors if d.room_a != "Exterior"]
    int_walls_geom = int_wall_solid.difference(unary_union(int_cutouts))

    return rooms, rmap, columns, doors, windows, fixtures, ext_walls_geom, int_walls_geom, outer_poly, inner_poly


# ---------------------------------------------------------------------------
# 3. RIGOROUS GEOMETRIC & CODE VERIFICATION SUITE (REV B)
# ---------------------------------------------------------------------------

def verify_model(rooms, rmap, columns, doors, windows, fixtures, outer_poly, inner_poly):
    errors = []
    stats = {}

    # 1. Room count (17 rooms in Rev B) & minimum areas
    mandatory_ids = [
        "Vestibule", "Waiting", "PublicWC", "StaffWC", "Breakroom", "Corridor",
        "Exam1", "Exam2", "Exam3", "Exam4", "Exam5", "Procedure",
        "NurseStation", "Lab", "Pharmacy", "ElecITRoom", "MechRoom"
    ]
    for mid in mandatory_ids:
        if mid not in rmap:
            errors.append(f"Missing mandatory room: {mid}")
    if len(rooms) != 17:
        errors.append(f"Expected 17 rooms in Rev B, got {len(rooms)}")
    for r in rooms:
        if r.area_m2 < r.min_area_req:
            errors.append(f"Room {r.id} area {r.area_m2} m² < required {r.min_area_req} m²")

    # ECO-1 check: Lab width == 4300 mm (+1.0 m over Rev A 3300 mm)
    if abs(rmap["Lab"].width - 4300.0) > 1.0:
        errors.append(f"ECO-1 fail: Lab width is {rmap['Lab'].width} mm, expected 4300 mm")

    # ECO-2 check: Exam5 area in 14..18 m²
    if not (14.0 <= rmap["Exam5"].area_m2 <= 18.0):
        errors.append(f"ECO-2 fail: Exam5 area {rmap['Exam5'].area_m2} m² outside 14-18 m²")

    # Corridor clear width >= 1800 mm
    corr = rmap["Corridor"]
    if corr.depth < 1800.0:
        errors.append(f"Corridor clear width {corr.depth} mm < 1800 mm")

    # Total area checks
    net_area_m2 = sum(r.area_m2 for r in rooms)
    gross_area_m2 = round(outer_poly.area / 1e6, 2)
    stats["room_count"] = len(rooms)
    stats["building_width_mm"] = round(GRID_X[-1] - GRID_X[0], 1)
    stats["building_height_mm"] = round(GRID_Y[-1] - GRID_Y[0], 1)
    stats["net_area_m2"] = round(net_area_m2, 2)
    stats["gross_area_m2"] = gross_area_m2
    stats["corridor_clear_width_mm"] = corr.depth

    # 2. Watertight Geometry: 0 overlapping room polygons (> 1 mm²)
    for i in range(len(rooms)):
        for j in range(i + 1, len(rooms)):
            inter = rooms[i].poly.intersection(rooms[j].poly).area
            if inter > 1.0:
                errors.append(f"Room overlap: {rooms[i].id} & {rooms[j].id} = {inter:.1f} mm²")

    # Verify every room is strictly inside inner footprint
    for r in rooms:
        outside = r.poly.difference(inner_poly).area
        if outside > 1.0:
            errors.append(f"Room {r.id} extends outside interior footprint by {outside:.1f} mm²")

    # 3. 100% Graph Reachability via Vestibule (networkx BFS)
    G = nx.Graph()
    G.add_node("Exterior")
    for r in rooms:
        G.add_node(r.id)
    for d in doors:
        G.add_edge(d.room_a, d.room_b, door=d.id)
    reachable = set(nx.bfs_tree(G, "Exterior").nodes())
    unreachable = set(rmap.keys()) - reachable
    if unreachable:
        errors.append(f"Unreachable rooms from Exterior: {unreachable}")
    ext_neighbors = list(G.neighbors("Exterior"))
    if ext_neighbors != ["Vestibule"]:
        errors.append(f"Exterior connects to {ext_neighbors}, expected ['Vestibule']")

    # 4. Door width requirements & jamb margins (>= 100 mm)
    for d in doors:
        if d.room_a == "Exterior" and d.width_mm < 1200.0:
            errors.append(f"Exterior door {d.id} width {d.width_mm} < 1200 mm")
        if "PublicWC" in (d.room_a, d.room_b) and d.width_mm < 900.0:
            errors.append(f"PublicWC door {d.id} width {d.width_mm} < 900 mm")
        if "Procedure" in (d.room_a, d.room_b) and d.width_mm < 1100.0:
            errors.append(f"Procedure door {d.id} width {d.width_mm} < 1100 mm")

        for rid in (d.room_a, d.room_b):
            if rid == "Exterior":
                continue
            r = rmap[rid]
            if d.wall_orient == "H":
                m1 = d.open_start - r.min_x
                m2 = r.max_x - d.open_end
            else:
                m1 = d.open_start - r.min_y
                m2 = r.max_y - d.open_end
            if m1 < 100.0 or m2 < 100.0:
                errors.append(f"Door {d.id} jamb margin < 100 mm in {rid}: ({m1:.1f}, {m2:.1f})")

    # 5. Door swing collisions (door-to-door, door-to-fixture, door-to-column)
    swing_polys = [(d, box(*d.swing_bbox)) for d in doors]
    for i in range(len(swing_polys)):
        for j in range(i + 1, len(swing_polys)):
            di, pi = swing_polys[i]
            dj, pj = swing_polys[j]
            if pi.intersection(pj).area > 1.0:
                errors.append(f"Door swing collision: {di.id} & {dj.id}")

    for d, sp in swing_polys:
        target_room = "Waiting" if d.id == "D03" else d.room_b
        if sp.difference(rmap[target_room].poly).area > 1.0:
            errors.append(f"Door {d.id} swing leaves target room {target_room}")

        for f in fixtures:
            if sp.intersection(f.poly).area > 1.0:
                errors.append(f"Door swing {d.id} collides with fixture {f.id}")
        for c in columns:
            if sp.intersection(c.poly).area > 1.0:
                errors.append(f"Door swing {d.id} collides with column {c.id}")
            if d.opening_poly.intersection(c.poly).area > 1.0:
                errors.append(f"Door opening {d.id} blocked by column {c.id}")

    # 6. Column checks (windows, fixtures, corridor)
    for c in columns:
        if c.poly.intersection(corr.poly).area > 1.0:
            errors.append(f"Column {c.id} encroaches into Corridor")
        for w in windows:
            if c.poly.intersection(w.opening_poly).area > 1.0:
                errors.append(f"Column {c.id} blocks window {w.id}")
        for f in fixtures:
            if c.poly.intersection(f.poly).area > 1.0:
                errors.append(f"Column {c.id} collides with fixture {f.id}")

    # 7. Fixture containment, overlap, and ElecITRoom dry-zone
    for i, f in enumerate(fixtures):
        r = rmap[f.room]
        if f.poly.difference(r.poly).area > 1.0:
            errors.append(f"Fixture {f.id} extends outside room {f.room}")
        if f.room == "ElecITRoom" and f.wet:
            errors.append(f"Wet fixture {f.id} placed inside ElecITRoom!")
        for j in range(i + 1, len(fixtures)):
            if f.poly.intersection(fixtures[j].poly).area > 1.0:
                errors.append(f"Fixture overlap: {f.id} & {fixtures[j].id}")

    # 8. Daylighting check on exterior walls (including Exam5 on North wall!)
    daylight_req = {"Waiting", "Breakroom", "Exam1", "Exam2", "Exam3", "Exam4", "Exam5", "Procedure", "Lab", "NurseStation"}
    win_rooms = set()
    for w in windows:
        win_rooms.add(w.room)
        if w.opening_poly.intersection(outer_poly.difference(inner_poly)).area < (w.width_mm * EXT_WALL_T - 10.0):
            errors.append(f"Window {w.id} ({w.room}) is not fully on an exterior wall!")
    missing_daylight = daylight_req - win_rooms
    if missing_daylight:
        errors.append(f"Missing daylight windows for: {missing_daylight}")

    # Verify Exam5 specifically has a north exterior window
    exam5_north_wins = [w for w in windows if w.room == "Exam5" and abs(w.y - 11000.0) < 1.0]
    if not exam5_north_wins:
        errors.append("ECO-2 fail: Exam5 missing north exterior window!")

    assert EXT_WALL_T > INT_WALL_T, "Exterior walls must be thicker than interior walls"

    stats["column_count"] = len(columns)
    stats["door_count"] = len(doors)
    stats["window_count"] = len(windows)
    stats["fixture_count"] = len(fixtures)
    stats["errors"] = errors
    return errors, stats


# ---------------------------------------------------------------------------
# 4. CLEAN AUTOCAD R12 (AC1009) LAYERED DXF WRITER
# ---------------------------------------------------------------------------

class SimpleR12DXF:
    """Minimal, strictly valid AutoCAD R12 (AC1009) ASCII DXF generator."""

    def __init__(self):
        self.layers: List[Tuple[str, int, str]] = []
        self.entities: List[str] = []

    def add_layer(self, name: str, color: int, ltype: str = "CONTINUOUS"):
        self.layers.append((name, color, ltype))

    def _emit(self, pairs: List[Tuple[int, object]]):
        self.entities.append("\n".join(f"{code}\n{val}" for code, val in pairs))

    def add_line(self, layer: str, x1: float, y1: float, x2: float, y2: float):
        self._emit([
            (0, "LINE"), (8, layer),
            (10, f"{x1:.2f}"), (20, f"{y1:.2f}"), (30, "0.0"),
            (11, f"{x2:.2f}"), (21, f"{y2:.2f}"), (31, "0.0"),
        ])

    def add_polyline(self, layer: str, pts: List[Tuple[float, float]], closed: bool = True):
        if len(pts) < 2:
            return
        self._emit([
            (0, "POLYLINE"), (8, layer),
            (66, 1), (10, "0.0"), (20, "0.0"), (30, "0.0"),
            (70, 1 if closed else 0),
        ])
        for x, y in pts:
            self._emit([
                (0, "VERTEX"), (8, layer),
                (10, f"{x:.2f}"), (20, f"{y:.2f}"), (30, "0.0"),
            ])
        self._emit([(0, "SEQEND"), (8, layer)])

    def add_rect(self, layer: str, min_x: float, min_y: float, max_x: float, max_y: float):
        self.add_polyline(layer, [
            (min_x, min_y), (max_x, min_y), (max_x, max_y), (min_x, max_y)
        ], closed=True)

    def add_arc(self, layer: str, cx: float, cy: float, r: float, start_deg: float, end_deg: float):
        self._emit([
            (0, "ARC"), (8, layer),
            (10, f"{cx:.2f}"), (20, f"{cy:.2f}"), (30, "0.0"),
            (40, f"{r:.2f}"),
            (50, f"{start_deg:.2f}"), (51, f"{end_deg:.2f}"),
        ])

    def add_circle(self, layer: str, cx: float, cy: float, r: float):
        self._emit([
            (0, "CIRCLE"), (8, layer),
            (10, f"{cx:.2f}"), (20, f"{cy:.2f}"), (30, "0.0"),
            (40, f"{r:.2f}"),
        ])

    def add_text(self, layer: str, x: float, y: float, height: float, text: str, halign: int = 1, valign: int = 2):
        clean = text.replace("\n", " ").strip()
        self._emit([
            (0, "TEXT"), (8, layer),
            (10, f"{x:.2f}"), (20, f"{y:.2f}"), (30, "0.0"),
            (40, f"{height:.2f}"),
            (1, clean),
            (72, halign), (73, valign),
            (11, f"{x:.2f}"), (21, f"{y:.2f}"), (31, "0.0"),
        ])

    def add_shapely_geom(self, layer: str, geom):
        if geom.is_empty:
            return
        if geom.geom_type == "Polygon":
            coords = list(geom.exterior.coords)[:-1]
            self.add_polyline(layer, coords, closed=True)
            for hole in geom.interiors:
                hcoords = list(hole.coords)[:-1]
                self.add_polyline(layer, hcoords, closed=True)
        elif geom.geom_type in ("MultiPolygon", "GeometryCollection"):
            for g in geom.geoms:
                self.add_shapely_geom(layer, g)

    def save(self, path: str):
        lines = [
            "0", "SECTION",
            "2", "HEADER",
            "9", "$ACADVER", "1", "AC1009",
            "9", "$INSUNITS", "70", "4",
            "9", "$EXTMIN", "10", "-2500.0", "20", "-2500.0", "30", "0.0",
            "9", "$EXTMAX", "10", "30500.0", "20", "14000.0", "30", "0.0",
            "0", "ENDSEC",
            "0", "SECTION",
            "2", "TABLES",
            "0", "TABLE", "2", "LTYPE", "70", "2",
            "0", "LTYPE", "2", "CONTINUOUS", "70", "0", "3", "Solid line", "72", "65", "73", "0", "40", "0.0",
            "0", "LTYPE", "2", "DASHED", "70", "0", "3", "Dashed", "72", "65", "73", "2", "40", "400.0", "49", "250.0", "49", "-150.0",
            "0", "ENDTAB",
            "0", "TABLE", "2", "LAYER", "70", str(len(self.layers)),
        ]
        for name, color, ltype in self.layers:
            lines.extend([
                "0", "LAYER", "2", name, "70", "0", "62", str(color), "6", ltype
            ])
        lines.extend([
            "0", "ENDTAB",
            "0", "ENDSEC",
            "0", "SECTION",
            "2", "ENTITIES",
        ])
        lines.extend(self.entities)
        lines.extend([
            "0", "ENDSEC",
            "0", "EOF",
        ])
        with open(path, "w", encoding="ascii", errors="replace") as f:
            f.write("\n".join(lines) + "\n")


def export_dxf(path: str, rooms, columns, doors, windows, fixtures, ext_walls_geom, int_walls_geom):
    dxf = SimpleR12DXF()
    dxf.add_layer("A-WALL-EXT",  7, "CONTINUOUS")
    dxf.add_layer("A-WALL-INT",  8, "CONTINUOUS")
    dxf.add_layer("S-COLS",      1, "CONTINUOUS")
    dxf.add_layer("S-GRID",      9, "DASHED")
    dxf.add_layer("A-DOOR",      3, "CONTINUOUS")
    dxf.add_layer("A-GLAZ",      4, "CONTINUOUS")
    dxf.add_layer("A-FURN",      6, "CONTINUOUS")
    dxf.add_layer("A-ANNO-TEXT", 2, "CONTINUOUS")
    dxf.add_layer("A-ANNO-DIMS", 5, "CONTINUOUS")

    # 1. Structural gridlines & bubbles
    for xi, x in enumerate(GRID_X):
        dxf.add_line("S-GRID", x, -1400.0, x, 12400.0)
        dxf.add_circle("S-GRID", x, 12750.0, 350.0)
        dxf.add_text("A-ANNO-TEXT", x, 12750.0, 260.0, GRID_X_LABELS[xi])

    for yi, y in enumerate(GRID_Y):
        dxf.add_line("S-GRID", -1400.0, y, 29400.0, y)
        dxf.add_circle("S-GRID", -1750.0, y, 350.0)
        dxf.add_text("A-ANNO-TEXT", -1750.0, y, 260.0, GRID_Y_LABELS[yi])

    # 2. Walls
    dxf.add_shapely_geom("A-WALL-EXT", ext_walls_geom)
    dxf.add_shapely_geom("A-WALL-INT", int_walls_geom)

    # 3. Structural columns
    for c in columns:
        dxf.add_rect("S-COLS", c.min_x, c.min_y, c.max_x, c.max_y)
        dxf.add_line("S-COLS", c.min_x, c.min_y, c.max_x, c.max_y)
        dxf.add_line("S-COLS", c.min_x, c.max_y, c.max_x, c.min_y)

    # 4. Doors
    for d in doors:
        lx, ly = d.leaf_end
        dxf.add_line("A-DOOR", d.hinge_x, d.hinge_y, lx, ly)
        sa, ea = d.arc_angles
        dxf.add_arc("A-DOOR", d.hinge_x, d.hinge_y, d.width_mm, sa, ea)
        sb = d.swing_bbox
        dxf.add_text("A-DOOR", (sb[0] + sb[2]) / 2.0, (sb[1] + sb[3]) / 2.0, 140.0, d.id)

    # 5. Windows
    for w in windows:
        hw = w.width_mm / 2.0
        ht = EXT_WALL_T / 2.0
        if w.wall_orient == "H":
            dxf.add_rect("A-GLAZ", w.x - hw, w.y - ht, w.x + hw, w.y + ht)
            dxf.add_line("A-GLAZ", w.x - hw, w.y - 35.0, w.x + hw, w.y - 35.0)
            dxf.add_line("A-GLAZ", w.x - hw, w.y + 35.0, w.x + hw, w.y + 35.0)
        else:
            dxf.add_rect("A-GLAZ", w.x - ht, w.y - hw, w.x + ht, w.y + hw)
            dxf.add_line("A-GLAZ", w.x - 35.0, w.y - hw, w.x - 35.0, w.y + hw)
            dxf.add_line("A-GLAZ", w.x + 35.0, w.y - hw, w.x + 35.0, w.y + hw)

    # 6. Fixtures
    for f in fixtures:
        dxf.add_rect("A-FURN", f.min_x, f.min_y, f.max_x, f.max_y)
        cx = (f.min_x + f.max_x) / 2.0
        cy = (f.min_y + f.max_y) / 2.0
        if "sink" in f.kind or "basin" in f.kind:
            r = min(f.max_x - f.min_x, f.max_y - f.min_y) * 0.30
            dxf.add_circle("A-FURN", cx, cy, r)
        elif "wc" in f.kind:
            dxf.add_circle("A-FURN", cx, cy - 80.0, 160.0)
        elif "water_heater" in f.kind:
            r = min(f.max_x - f.min_x, f.max_y - f.min_y) * 0.42
            dxf.add_circle("A-FURN", cx, cy, r)
        elif "table" in f.kind or "rack" in f.kind:
            dxf.add_line("A-FURN", f.min_x + 60, f.min_y + 60, f.max_x - 60, f.max_y - 60)
            dxf.add_line("A-FURN", f.min_x + 60, f.max_y - 60, f.max_x - 60, f.min_y + 60)

    # 7. Room Labels & Areas
    for r in rooms:
        cx = (r.min_x + r.max_x) / 2.0
        cy = (r.min_y + r.max_y) / 2.0
        dxf.add_text("A-ANNO-TEXT", cx, cy + 180.0, 200.0, r.id)
        dxf.add_text("A-ANNO-TEXT", cx, cy - 160.0, 150.0, f"{r.area_m2:.1f} m2")

    # 8. Dimension Strings
    def add_dim_h(x1, x2, y_line, label):
        dxf.add_line("A-ANNO-DIMS", x1, y_line, x2, y_line)
        dxf.add_line("A-ANNO-DIMS", x1, y_line - 150, x1, y_line + 150)
        dxf.add_line("A-ANNO-DIMS", x2, y_line - 150, x2, y_line + 150)
        dxf.add_text("A-ANNO-DIMS", (x1 + x2) / 2.0, y_line + 220, 180.0, label)

    def add_dim_v(y1, y2, x_line, label):
        dxf.add_line("A-ANNO-DIMS", x_line, y1, x_line, y2)
        dxf.add_line("A-ANNO-DIMS", x_line - 150, y1, x_line + 150, y1)
        dxf.add_line("A-ANNO-DIMS", x_line - 150, y2, x_line + 150, y2)
        dxf.add_text("A-ANNO-DIMS", x_line + 380, (y1 + y2) / 2.0, 180.0, label)

    for i in range(len(GRID_X) - 1):
        add_dim_h(GRID_X[i], GRID_X[i + 1], 11850.0, f"{int(GRID_X[i+1]-GRID_X[i])}")
    add_dim_h(0.0, 28000.0, 13450.0, "28000 (OVERALL REV B)")
    add_dim_v(0.0, 5000.0, 29100.0, "5000")
    add_dim_v(5000.0, 11000.0, 29100.0, "6000")
    add_dim_v(0.0, 11000.0, -1100.0, "11000")

    dxf.save(path)


# ---------------------------------------------------------------------------
# 5. PUBLICATION-GRADE ARCHITECTURAL PNG RENDERER (REV B)
# ---------------------------------------------------------------------------

def _plot_shapely_filled(ax, geom, facecolor, edgecolor, linewidth=0.8, zorder=5):
    if geom.is_empty:
        return
    if geom.geom_type == "Polygon":
        ext = np.array(geom.exterior.coords)
        ax.add_patch(MplPolygon(ext, closed=True, facecolor=facecolor, edgecolor=edgecolor,
                                linewidth=linewidth, zorder=zorder))
        for hole in geom.interiors:
            hc = np.array(hole.coords)
            ax.add_patch(MplPolygon(hc, closed=True, facecolor="white", edgecolor=edgecolor,
                                    linewidth=linewidth, zorder=zorder + 0.1))
    elif geom.geom_type in ("MultiPolygon", "GeometryCollection"):
        for g in geom.geoms:
            _plot_shapely_filled(ax, g, facecolor, edgecolor, linewidth, zorder)


def render_png(path: str, rooms, columns, doors, windows, fixtures, ext_walls_geom, int_walls_geom, stats):
    fig, ax = plt.subplots(figsize=(24, 13.0), dpi=200)
    fig.patch.set_facecolor("#FAFBFD")
    ax.set_facecolor("#FAFBFD")

    zone_colors = {
        "public":   "#EEF5FC",
        "clinical": "#F0F9F4",
        "staff":    "#FDF6EC",
        "service":  "#F4F3F7",
        "circ":     "#F7F8FA",
    }

    # 1. Room fills (highlight Rev B ECO rooms subtly)
    for r in rooms:
        fc = zone_colors.get(r.zone, "#FFFFFF")
        if r.id == "Exam5":
            fc = "#DCFCE7"  # Highlight new Exam5 room
        elif r.id == "Lab":
            fc = "#E0F2FE"  # Highlight expanded Lab (+1.0 m)
        ax.add_patch(Rectangle((r.min_x, r.min_y), r.width, r.depth,
                               facecolor=fc, edgecolor="none", zorder=1))

    # 2. Structural Gridlines & Bubbles
    for xi, x in enumerate(GRID_X):
        y0 = -1500.0
        y1 = 12300.0
        ax.plot([x, x], [y0, y1], color="#6C8EB4", linestyle=(0, (8, 4, 2, 4)),
                linewidth=0.75, alpha=0.75, zorder=2)
        circ = Circle((x, y1 + 380), 360, facecolor="white", edgecolor="#3B5B82", linewidth=1.2, zorder=8)
        ax.add_patch(circ)
        ax.text(x, y1 + 380, GRID_X_LABELS[xi], ha="center", va="center",
                fontsize=9.5, fontweight="bold", color="#243B55", zorder=9)

    for yi, y in enumerate(GRID_Y):
        x0 = -1500.0
        x1 = 29200.0
        ax.plot([x0, x1], [y, y], color="#6C8EB4", linestyle=(0, (8, 4, 2, 4)),
                linewidth=0.75, alpha=0.75, zorder=2)
        circ = Circle((x0 - 380, y), 360, facecolor="white", edgecolor="#3B5B82", linewidth=1.2, zorder=8)
        ax.add_patch(circ)
        ax.text(x0 - 380, y, GRID_Y_LABELS[yi], ha="center", va="center",
                fontsize=9.5, fontweight="bold", color="#243B55", zorder=9)

    # 3. Fixtures
    for f in fixtures:
        w = f.max_x - f.min_x
        h = f.max_y - f.min_y
        cx, cy = f.min_x + w / 2.0, f.min_y + h / 2.0
        if f.wet:
            fc, ec = "#D8EFFA", "#2C6E91"
        elif "table" in f.kind:
            fc, ec = "#E4F2E7", "#366B44"
        elif "counter" in f.kind or "bench" in f.kind or "desk" in f.kind or "workstation" in f.kind:
            fc, ec = "#EFE5D6", "#7A5B36"
        elif "rack" in f.kind or "panel" in f.kind:
            fc, ec = "#E8E0F0", "#5B3E7A"
        else:
            fc, ec = "#E9ECEF", "#55606E"

        ax.add_patch(Rectangle((f.min_x, f.min_y), w, h, facecolor=fc, edgecolor=ec,
                               linewidth=0.95, zorder=3))
        if "sink" in f.kind or "basin" in f.kind:
            ax.add_patch(Circle((cx, cy), min(w, h) * 0.28, facecolor="white", edgecolor=ec, linewidth=0.8, zorder=3.2))
            ax.add_patch(Circle((cx, cy), min(w, h) * 0.07, facecolor=ec, edgecolor="none", zorder=3.3))
        elif "wc" in f.kind:
            ax.add_patch(Rectangle((f.min_x + 40, f.max_y - 200), w - 80, 170,
                                   facecolor="white", edgecolor=ec, linewidth=0.7, zorder=3.2))
            ax.add_patch(Circle((cx, f.min_y + 260), min(w, h) * 0.30,
                                facecolor="white", edgecolor=ec, linewidth=0.8, zorder=3.2))
        elif "water_heater" in f.kind:
            ax.add_patch(Circle((cx, cy), min(w, h) * 0.40, facecolor="white", edgecolor=ec, linewidth=0.9, zorder=3.2))
            ax.text(cx, cy, "WH", ha="center", va="center", fontsize=6.5, fontweight="bold", color=ec, zorder=3.3)
        elif "table" in f.kind:
            ax.plot([f.min_x + 60, f.max_x - 60], [f.min_y + 60, f.max_y - 60], color=ec, lw=0.5, alpha=0.45, zorder=3.2)
            ax.plot([f.min_x + 60, f.max_x - 60], [f.max_y - 60, f.min_y + 60], color=ec, lw=0.5, alpha=0.45, zorder=3.2)
            lbl_t = "PROC" if "procedure" in f.kind else ("BREAK" if "break" in f.kind else "EXAM")
            ax.text(cx, cy, lbl_t, ha="center", va="center", fontsize=5.2, fontweight="bold", color=ec,
                    bbox=dict(boxstyle="square,pad=0.1", fc=fc, ec="none", alpha=0.85), zorder=3.3)
        elif "reception" in f.kind:
            ax.text(cx, cy, "RECEPTION", ha="center", va="center", fontsize=6.0, fontweight="bold", color=ec, zorder=3.3)
        elif "nurse_counter" in f.kind:
            ax.text(cx, cy, "NURSE\nCOUNTER", ha="center", va="center", fontsize=5.5, fontweight="bold", color=ec, zorder=3.3)
        elif "rack" in f.kind:
            ax.text(cx, cy, "IT RACK", ha="center", va="center", fontsize=5.8, fontweight="bold", color=ec, zorder=3.3)
        elif "panel" in f.kind:
            ax.text(cx, cy, "EL", ha="center", va="center", fontsize=6.0, fontweight="bold", color=ec, zorder=3.3)

    # 4. Walls
    _plot_shapely_filled(ax, ext_walls_geom, facecolor="#24292F", edgecolor="#111418", linewidth=1.1, zorder=5)
    _plot_shapely_filled(ax, int_walls_geom, facecolor="#5A6470", edgecolor="#2C343D", linewidth=0.8, zorder=5)

    # 5. Structural Columns (18 cols)
    for c in columns:
        ax.add_patch(Rectangle((c.min_x, c.min_y), c.size, c.size,
                               facecolor="#B83227", edgecolor="#5E140E", linewidth=1.0, zorder=7))
        ax.plot([c.min_x, c.max_x], [c.min_y, c.max_y], color="#FFE6E3", lw=0.65, zorder=7.2)
        ax.plot([c.min_x, c.max_x], [c.max_y, c.min_y], color="#FFE6E3", lw=0.65, zorder=7.2)

    # 6. Exterior Windows
    for w in windows:
        hw = w.width_mm / 2.0
        ht = EXT_WALL_T / 2.0
        if w.wall_orient == "H":
            ax.add_patch(Rectangle((w.x - hw, w.y - ht), w.width_mm, EXT_WALL_T,
                                   facecolor="#E1F5FE", edgecolor="#0277BD", linewidth=1.0, zorder=6))
            ax.plot([w.x - hw, w.x + hw], [w.y - 35, w.y - 35], color="#0277BD", lw=0.85, zorder=6.2)
            ax.plot([w.x - hw, w.x + hw], [w.y + 35, w.y + 35], color="#0277BD", lw=0.85, zorder=6.2)
        else:
            ax.add_patch(Rectangle((w.x - ht, w.y - hw), EXT_WALL_T, w.width_mm,
                                   facecolor="#E1F5FE", edgecolor="#0277BD", linewidth=1.0, zorder=6))
            ax.plot([w.x - 35, w.x - 35], [w.y - hw, w.y + hw], color="#0277BD", lw=0.85, zorder=6.2)
            ax.plot([w.x + 35, w.x + 35], [w.y - hw, w.y + hw], color="#0277BD", lw=0.85, zorder=6.2)

    # 7. Doors
    for d in doors:
        lx, ly = d.leaf_end
        ax.plot([d.hinge_x, lx], [d.hinge_y, ly], color="#1B7A43", linewidth=2.0, solid_capstyle="butt", zorder=6.5)
        sa, ea = d.arc_angles
        ax.add_patch(Arc((d.hinge_x, d.hinge_y), 2 * d.width_mm, 2 * d.width_mm,
                         angle=0, theta1=sa, theta2=ea, color="#2E9E5B",
                         linestyle="--", linewidth=0.95, zorder=6.2))
        sb = d.swing_bbox
        tx, ty = (sb[0] + sb[2]) / 2.0, (sb[1] + sb[3]) / 2.0
        ax.text(tx, ty, d.id, ha="center", va="center", fontsize=6.2, fontweight="bold",
                color="#145A32", zorder=6.8)

    # 8. Room Labels & Exact Areas
    label_offsets = {
        "Waiting": (0, -350),
        "Vestibule": (0, 1200),
        "Exam1": (0, -1180),
        "Exam2": (0, -1180),
        "Exam3": (-400, -1180),
        "Exam4": (0, -1180),
        "Procedure": (350, -1550),
        "Corridor": (0, 0),
        "PublicWC": (0, -150),
        "StaffWC": (0, -150),
        "Breakroom": (400, 950),
        "NurseStation": (700, 120),
        "Exam5": (250, 500),
        "Lab": (250, 150),
        "Pharmacy": (-150, -100),
        "ElecITRoom": (0, -50),
        "MechRoom": (250, -350),
    }
    for r in rooms:
        dx, dy = label_offsets.get(r.id, (0, 0))
        cx = (r.min_x + r.max_x) / 2.0 + dx
        cy = (r.min_y + r.max_y) / 2.0 + dy
        if r.id == "Corridor":
            txt = f"CENTRAL CLINICAL CORRIDOR  |  Clear Width: {int(r.depth)} mm  ({r.area_m2:.1f} m²)"
            ax.text(cx, cy, txt, ha="center", va="center", fontsize=8.5, fontweight="bold",
                    color="#334E68", bbox=dict(boxstyle="round,pad=0.25", fc="white", ec="#BCCCDC", alpha=0.9),
                    zorder=8.5)
        else:
            fs = 7.4 if r.width >= 2500 else 6.3
            ax.text(cx, cy + 160, r.label, ha="center", va="center", fontsize=fs, fontweight="bold",
                    color="#102A43", zorder=8.5)
            ax.text(cx, cy - 260, f"{r.area_m2:.1f} m²", ha="center", va="center", fontsize=6.7,
                    color="#486581", zorder=8.5)

    # Entrance indicator arrow at Vestibule
    ax.annotate("MAIN ENTRY\n(1300 mm)", xy=(1075, -150), xytext=(1075, -1350),
                ha="center", va="top", fontsize=8.2, fontweight="bold", color="#1B7A43",
                arrowprops=dict(arrowstyle="-|>", color="#1B7A43", lw=2.0), zorder=9)

    # 9. Exterior Dimension Chains
    def draw_dim_h(x1, x2, y_line, label, color="#334E68"):
        ax.plot([x1, x2], [y_line, y_line], color=color, lw=0.85, zorder=7)
        ax.plot([x1, x1], [y_line - 140, y_line + 140], color=color, lw=0.85, zorder=7)
        ax.plot([x2, x2], [y_line - 140, y_line + 140], color=color, lw=0.85, zorder=7)
        ax.text((x1 + x2) / 2.0, y_line + 120, label, ha="center", va="bottom",
                fontsize=7.5, fontweight="bold", color=color, zorder=8)

    def draw_dim_v(y1, y2, x_line, label, color="#334E68"):
        ax.plot([x_line, x_line], [y1, y2], color=color, lw=0.85, zorder=7)
        ax.plot([x_line - 140, x_line + 140], [y1, y1], color=color, lw=0.85, zorder=7)
        ax.plot([x_line - 140, x_line + 140], [y2, y2], color=color, lw=0.85, zorder=7)
        ax.text(x_line + 160, (y1 + y2) / 2.0, label, ha="left", va="center",
                fontsize=7.5, fontweight="bold", color=color, rotation=90, zorder=8)

    for i in range(len(GRID_X) - 1):
        draw_dim_h(GRID_X[i], GRID_X[i + 1], 11650.0, f"{int(GRID_X[i+1]-GRID_X[i])}")
    draw_dim_h(0.0, 28000.0, 13450.0, "28,000 mm (OVERALL WIDTH — REV B +1.0 m)")
    draw_dim_v(0.0, 5000.0, 28750.0, "5,000 mm")
    draw_dim_v(5000.0, 11000.0, 28750.0, "6,000 mm")
    draw_dim_v(0.0, 11000.0, -950.0, "11,000 mm")

    # 10. Architectural Title Block & Legend Strip along bottom margin (clear of 28x11 m footprint)
    tb_x, tb_y, tb_w, tb_h = 3200.0, -4300.0, 24800.0, 2550.0
    ax.add_patch(FancyBboxPatch((tb_x, tb_y), tb_w, tb_h, boxstyle="round,pad=100",
                                facecolor="white", edgecolor="#334E68", linewidth=1.3, zorder=8))
    # Left section: Title
    ax.text(tb_x + 350, tb_y + tb_h - 380,
            "OUTPATIENT SPECIALIST CLINIC — REV B ECO", ha="left", va="top",
            fontsize=11.0, fontweight="bold", color="#102A43", zorder=9)
    ax.text(tb_x + 350, tb_y + tb_h - 860,
            "ARCHITECTURAL & STRUCTURAL FLOOR PLAN (NO-EDL BASELINE)", ha="left", va="top",
            fontsize=7.8, fontweight="bold", color="#0369A1", zorder=9)
    ax.text(tb_x + 350, tb_y + tb_h - 1320,
            "ECO-1: Lab +1.0 m (4.3 m) & East Wing +1.0 m (28.0 m overall)\n"
            "ECO-2: Added Exam5 (14.6 m²) on North Wing w/ W14, D17 & Basin\n"
            "ECO-3: NurseStation East Wall reserved for Relocated DB_CLINIC",
            ha="left", va="top", fontsize=6.9, color="#334E68", zorder=9)

    # Middle section: Metrics
    mid_x = tb_x + 10400.0
    ax.plot([mid_x - 300, mid_x - 300], [tb_y + 200, tb_y + tb_h - 200], color="#CBD5E1", lw=1.0, zorder=9)
    summary_lines = [
        f"Rooms: {stats['room_count']} (Rev A 16 -> Rev B 17) | 100% BFS Reachable",
        f"Net Area: {stats['net_area_m2']:.1f} m²  |  Gross: {stats['gross_area_m2']:.1f} m² (28.0 × 11.0 m)",
        f"Structural Grid: 5×5.6 m (X), 5.0 / 6.0 m (Y) — {stats['column_count']} RC Cols",
        f"Walls: 300 mm Ext / 150 mm Int  |  Corridor: {int(stats['corridor_clear_width_mm'])} mm Clear",
        f"Doors: {stats['door_count']} | Windows: {stats['window_count']} | Fixtures: {stats['fixture_count']} | 0 Collisions",
    ]
    for idx, sl in enumerate(summary_lines):
        ax.text(mid_x, tb_y + tb_h - 320 - idx * 410, sl, ha="left", va="top",
                fontsize=7.2, color="#243B55", zorder=9)

    # Right section: Swatches
    rig_x = tb_x + 18800.0
    ax.plot([rig_x - 300, rig_x - 300], [tb_y + 200, tb_y + tb_h - 200], color="#CBD5E1", lw=1.0, zorder=9)
    swatches = [
        ("#24292F", "Ext Wall (300 mm)"),
        ("#5A6470", "Int Wall (150 mm)"),
        ("#B83227", "RC Col (300×300)"),
        ("#0277BD", "Exterior Glazing"),
        ("#1B7A43", "Door & 90° Swing"),
        ("#2C6E91", "Wet Fixture"),
        ("#DCFCE7", "Exam5 (NEW ECO-2)"),
        ("#E0F2FE", "Lab (+1.0m ECO-1)"),
    ]
    for i, (col, lbl) in enumerate(swatches):
        col_i = i % 2
        row_i = i // 2
        sx = rig_x + col_i * 2950
        sy = tb_y + tb_h - 420 - row_i * 510
        ax.add_patch(Rectangle((sx, sy - 110), 360, 220, facecolor=col, edgecolor="#102A43", lw=0.6, zorder=9))
        ax.text(sx + 460, sy, lbl, ha="left", va="center", fontsize=6.6, color="#243B55", zorder=9)

    # Scale bar & North Arrow on bottom-left
    sb_x, sb_y = -1400.0, -3950.0
    for k in range(4):
        fc = "#24292F" if k % 2 == 0 else "white"
        ax.add_patch(Rectangle((sb_x + k * 1000, sb_y), 1000, 180, facecolor=fc, edgecolor="#24292F", lw=0.8, zorder=9))
    for k in [0, 1, 2, 4]:
        ax.text(sb_x + k * 1000, sb_y - 120, f"{k} m", ha="center", va="top", fontsize=7.0, color="#243B55", zorder=9)
    ax.text(sb_x + 2000, sb_y + 260, "SCALE BAR (m)", ha="center", va="bottom",
            fontsize=6.8, fontweight="bold", color="#486581", zorder=9)

    nx_c, ny_c = 28900.0, -3100.0
    ax.annotate("", xy=(nx_c, ny_c + 750), xytext=(nx_c, ny_c - 300),
                arrowprops=dict(arrowstyle="-|>", color="#102A43", lw=2.2), zorder=9)
    ax.text(nx_c, ny_c + 820, "N", ha="center", va="bottom", fontsize=11, fontweight="bold", color="#102A43", zorder=9)

    ax.set_aspect("equal")
    ax.set_xlim(-2500, 30200)
    ax.set_ylim(-4700, 14200)
    ax.axis("off")

    plt.tight_layout(pad=0.3)
    plt.savefig(path, dpi=200, facecolor=fig.get_facecolor())
    plt.close(fig)


# ---------------------------------------------------------------------------
# 6. MANIFEST EXPORT & MAIN ENTRY
# ---------------------------------------------------------------------------

def export_manifest(path: str, rooms, columns, doors, windows, fixtures):
    manifest = {
        "revision": "B",
        "building_width_mm": GRID_X[-1] - GRID_X[0],
        "building_height_mm": GRID_Y[-1] - GRID_Y[0],
        "grid_x": GRID_X,
        "grid_y": GRID_Y,
        "rooms": [
            {
                "id": r.id,
                "min_x": round(r.min_x, 1),
                "min_y": round(r.min_y, 1),
                "max_x": round(r.max_x, 1),
                "max_y": round(r.max_y, 1),
                "area_m2": r.area_m2,
                "habitable": r.habitable,
            }
            for r in rooms
        ],
        "doors": [
            {
                "id": d.id,
                "room_a": d.room_a,
                "room_b": d.room_b,
                "width_mm": round(d.width_mm, 1),
                "hinge_x": round(d.hinge_x, 1),
                "hinge_y": round(d.hinge_y, 1),
                "swing_bbox": d.swing_bbox,
            }
            for d in doors
        ],
        "windows": [
            {
                "id": w.id,
                "room": w.room,
                "width_mm": round(w.width_mm, 1),
                "x": round(w.x, 1),
                "y": round(w.y, 1),
            }
            for w in windows
        ],
        "columns": [
            {
                "id": c.id,
                "min_x": round(c.min_x, 1),
                "min_y": round(c.min_y, 1),
                "max_x": round(c.max_x, 1),
                "max_y": round(c.max_y, 1),
            }
            for c in columns
        ],
        "fixtures": [
            {
                "id": f.id,
                "room": f.room,
                "kind": f.kind,
                "min_x": round(f.min_x, 1),
                "min_y": round(f.min_y, 1),
                "max_x": round(f.max_x, 1),
                "max_y": round(f.max_y, 1),
            }
            for f in fixtures
        ],
    }
    with open(path, "w", encoding="utf-8") as fp:
        json.dump(manifest, fp, indent=2)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    rooms, rmap, columns, doors, windows, fixtures, ext_walls_geom, int_walls_geom, outer_poly, inner_poly = build_model()
    errors, stats = verify_model(rooms, rmap, columns, doors, windows, fixtures, outer_poly, inner_poly)

    print("=== CLINIC REV B ARCHITECTURAL VERIFICATION SUMMARY ===")
    for k, v in stats.items():
        if k != "errors":
            print(f"  {k}: {v}")
    if errors:
        print(f"ERRORS ({len(errors)}):")
        for e in errors:
            print("  -", e)
        raise SystemExit(1)
    else:
        print("ALL GEOMETRIC, STRUCTURAL, GRAPH & CODE CHECKS PASSED (0 ERRORS)!")

    dxf_path = os.path.join(OUT_DIR, "clinic_arch.dxf")
    png_path = os.path.join(OUT_DIR, "clinic_arch.png")
    json_path = os.path.join(OUT_DIR, "arch_manifest.json")

    export_dxf(dxf_path, rooms, columns, doors, windows, fixtures, ext_walls_geom, int_walls_geom)
    render_png(png_path, rooms, columns, doors, windows, fixtures, ext_walls_geom, int_walls_geom, stats)
    export_manifest(json_path, rooms, columns, doors, windows, fixtures)

    print(f"Wrote: {dxf_path} ({os.path.getsize(dxf_path):,} bytes)")
    print(f"Wrote: {png_path} ({os.path.getsize(png_path):,} bytes)")
    print(f"Wrote: {json_path} ({os.path.getsize(json_path):,} bytes)")


if __name__ == "__main__":
    main()
