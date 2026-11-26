#!/usr/bin/env python3
"""
Benchmark 2 (Baseline / NO-EDL Arm):
15-Room (+ StaffWC = 16-Room) Outpatient Specialist Medical Clinic Floor Plan
Architecture + Structural Column Grid Generator, Verifier, DXF Exporter, and PNG Renderer.

Units: millimeters (mm) everywhere in geometry, DXF, and manifest.
Coordinate system: +X East, +Y North.
"""

import json
import math
import os
from dataclasses import dataclass, field
from typing import Dict, List, Tuple, Optional

import networkx as nx
import numpy as np
from shapely.geometry import box, Polygon, LineString
from shapely.ops import unary_union
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle, Arc, Circle, Polygon as MplPolygon, FancyBboxPatch
from matplotlib.lines import Line2D


OUT_DIR = "/home/lindah/projects/EDL/benchmarks/v3_2_industry_ab/b2_clinic_arch/baseline"

# ---------------------------------------------------------------------------
# 1. PARAMETRIC ARCHITECTURAL & STRUCTURAL CONSTANTS (mm)
# ---------------------------------------------------------------------------
EXT_WALL_T = 300.0   # Exterior wall thickness (mm)
INT_WALL_T = 150.0   # Interior partition thickness (mm)
COL_SIZE = 300.0     # Structural column 300x300 mm

# Regular structural grid: 5 bays of 5.4 m in X; 2 bays (5.0 m, 6.0 m) in Y
# L-shaped stepped footprint: South-East bay (X=21600..27000, Y=0..5000) is stepped out.
GRID_X = [0.0, 5400.0, 10800.0, 16200.0, 21600.0, 27000.0]
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

# Exterior boundary polygon (centerline and outer face)
FOOTPRINT_OUTER_PTS = [
    (-150.0, -150.0),
    (21750.0, -150.0),
    (21750.0, 4850.0),
    (27150.0, 4850.0),
    (27150.0, 11150.0),
    (-150.0, 11150.0),
]

FOOTPRINT_INNER_PTS = [
    (150.0, 150.0),
    (21450.0, 150.0),
    (21450.0, 5150.0),
    (26850.0, 5150.0),
    (26850.0, 10850.0),
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
    hinge_side: str       # 'L' or 'R' (for H: West/East; for V: South/North)

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
# 2. BUILD MODEL (ROOMS, COLUMNS, DOORS, WINDOWS, FIXTURES, WALLS)
# ---------------------------------------------------------------------------

def build_model():
    # --- Rooms ---
    # South Wing: Y = 150 .. 5000 (depth 4850 mm), X = 150 .. 21450 (21300 mm)
    # Partitions (150 mm): 2000..2150, 8300..8450, 11250..11400, 14200..14350, 17150..17300
    south_specs = [
        ("Vestibule",  "Vestibule",         "public",   150,  2000, False, 6.0),
        ("Waiting",    "Reception &\nWaiting", "public", 2150,  8300, True,  28.0),
        ("Exam1",      "Exam 1",            "clinical", 8450, 11250, True,  12.0),
        ("Exam2",      "Exam 2",            "clinical", 11400, 14200, True, 12.0),
        ("Exam3",      "Exam 3",            "clinical", 14350, 17150, True, 12.0),
        ("Procedure",  "Minor\nProcedure",  "clinical", 17300, 21450, True, 18.0),
    ]

    # North Wing: Y = 7500 .. 10850 (depth 3350 mm), X = 150 .. 26850 (26700 mm)
    # Widths: 1900, 1500, 3850, 3500, 3850, 3300, 2700, 2200, 2700 (+ 8*150 = 1200 => 26700)
    north_widths = [
        ("PublicWC",     "Public\nWC",        "public",   1900, False, 4.5),
        ("StaffWC",      "Staff\nWC",         "staff",    1500, False, 3.5),
        ("Breakroom",    "Staff\nBreakroom",  "staff",    3850, True,  12.0),
        ("NurseStation", "Nurse\nStation",    "clinical", 3500, True,  10.0),
        ("Exam4",        "Exam 4",            "clinical", 3850, True,  12.0),
        ("Lab",          "Lab &\nSteril.",    "clinical", 3300, True,  10.0),
        ("Pharmacy",     "Pharmacy &\nClean", "service",  2700, False, 8.0),
        ("ElecITRoom",   "Elec /\nIT Room",   "service",  2200, False, 6.0),
        ("MechRoom",     "Mech /\nJanitor",   "service",  2700, False, 7.0),
    ]

    rooms: List[Room] = []
    for rid, lbl, zone, x0, x1, hab, req in south_specs:
        rooms.append(Room(rid, lbl, zone, float(x0), Y_SOUTH_INT, float(x1), Y_CORR_WALL_S0, hab, req))

    # Central Corridor: Y = 5150 .. 7350 (clear width 2200 mm), X = 150 .. 26850
    rooms.append(Room("Corridor", "Central Clinical Corridor", "circ",
                      150.0, Y_CORR_S, 26850.0, Y_CORR_N, False, 0.0))

    cur_x = 150.0
    for rid, lbl, zone, w, hab, req in north_widths:
        rooms.append(Room(rid, lbl, zone, cur_x, Y_NORTH_INT_S, cur_x + w, Y_NORTH_INT_N, hab, req))
        cur_x += w + INT_WALL_T

    assert abs((cur_x - INT_WALL_T) - 26850.0) < 1e-6, f"North strip end mismatch: {cur_x - INT_WALL_T}"

    rmap: Dict[str, Room] = {r.id: r for r in rooms}

    # --- Structural Columns (17 columns on regular grid inside L-footprint) ---
    columns: List[Column] = []
    for yi, y in enumerate(GRID_Y):
        for xi, x in enumerate(GRID_X):
            # Exclude SE cutout corner (X=27000, Y=0)
            if x == 27000.0 and y == 0.0:
                continue
            cid = f"C-{GRID_X_LABELS[xi]}{GRID_Y_LABELS[yi]}"
            columns.append(Column(cid, x, y, COL_SIZE))

    # --- Doors (16 doors, all interior doors swing inward into rooms, >=200 mm jamb margins) ---
    doors: List[Door] = [
        # Exterior -> Vestibule (South exterior wall Y = -150..150, width 1300 mm >= 1200 mm)
        Door("D01", "Exterior", "Vestibule", "H", -150.0, 150.0, 425.0, 1300.0,
             hinge_x=425.0, hinge_y=150.0, open_dir="+N", hinge_side="W"),
        # Vestibule -> Waiting (Vertical partition X = 2000..2150, width 1200 mm, swings East into Waiting)
        Door("D02", "Vestibule", "Waiting", "V", 2000.0, 2150.0, 1800.0, 1200.0,
             hinge_x=2150.0, hinge_y=1800.0, open_dir="+E", hinge_side="S"),
        # Waiting -> Corridor (Horizontal partition Y = 5000..5150, width 1200 mm, swings South into Waiting)
        Door("D03", "Waiting", "Corridor", "H", 5000.0, 5150.0, 6400.0, 1200.0,
             hinge_x=7600.0, hinge_y=5000.0, open_dir="+S", hinge_side="E"),

        # South Clinical Wing doors (all swing South into rooms, clear of Y=5000 columns!)
        # Exam1: X = 8450..11250; col at 10800 (10650..10950) -> door at 8750..9700 (950 mm)
        Door("D04", "Corridor", "Exam1", "H", 5000.0, 5150.0, 8750.0, 950.0,
             hinge_x=8750.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),
        # Exam2: X = 11400..14200; no col -> door at 11700..12650 (950 mm)
        Door("D05", "Corridor", "Exam2", "H", 5000.0, 5150.0, 11700.0, 950.0,
             hinge_x=11700.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),
        # Exam3: X = 14350..17150; col at 16200 (16050..16350) -> door at 14650..15600 (950 mm)
        Door("D06", "Corridor", "Exam3", "H", 5000.0, 5150.0, 14650.0, 950.0,
             hinge_x=14650.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),
        # Procedure: X = 17300..21450; wide door 1200 mm (>= 1100 mm!) at 17700..18900
        Door("D07", "Corridor", "Procedure", "H", 5000.0, 5150.0, 17700.0, 1200.0,
             hinge_x=17700.0, hinge_y=5000.0, open_dir="+S", hinge_side="W"),

        # North Wing doors (all on Y = 7350..7500, swing North into rooms)
        # PublicWC: X = 150..2050 -> door 950 mm (>= 900 mm) at 450..1400
        Door("D08", "Corridor", "PublicWC", "H", 7350.0, 7500.0, 450.0, 950.0,
             hinge_x=450.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # StaffWC: X = 2200..3700 -> door 900 mm at 2450..3350
        Door("D09", "Corridor", "StaffWC", "H", 7350.0, 7500.0, 2450.0, 900.0,
             hinge_x=2450.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # Breakroom: X = 3850..7700 -> door 950 mm at 4150..5100
        Door("D10", "Corridor", "Breakroom", "H", 7350.0, 7500.0, 4150.0, 950.0,
             hinge_x=4150.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # NurseStation: X = 7850..11350 -> wide 1100 mm door at 8200..9300
        Door("D11", "Corridor", "NurseStation", "H", 7350.0, 7500.0, 8200.0, 1100.0,
             hinge_x=8200.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # Exam4: X = 11500..15350 -> door 950 mm at 11800..12750
        Door("D12", "Corridor", "Exam4", "H", 7350.0, 7500.0, 11800.0, 950.0,
             hinge_x=11800.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # Lab: X = 15500..18800 -> door 950 mm at 15800..16750
        Door("D13", "Corridor", "Lab", "H", 7350.0, 7500.0, 15800.0, 950.0,
             hinge_x=15800.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # Pharmacy: X = 18950..21650 -> door 900 mm at 19250..20150
        Door("D14", "Corridor", "Pharmacy", "H", 7350.0, 7500.0, 19250.0, 900.0,
             hinge_x=19250.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # ElecITRoom: X = 21800..24000 -> lockable dedicated 900 mm door at 22100..23000
        Door("D15", "Corridor", "ElecITRoom", "H", 7350.0, 7500.0, 22100.0, 900.0,
             hinge_x=22100.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
        # MechRoom: X = 24150..26850 -> door 950 mm at 24450..25400
        Door("D16", "Corridor", "MechRoom", "H", 7350.0, 7500.0, 24450.0, 950.0,
             hinge_x=24450.0, hinge_y=7500.0, open_dir="+N", hinge_side="W"),
    ]

    # --- Exterior Windows (all strictly on exterior walls, clear of columns & corners) ---
    # South exterior wall centerline Y = 0 (cols at X = 0, 5400, 10800, 16200, 21600)
    # North exterior wall centerline Y = 11000 (cols at X = 0, 5400, 10800, 16200, 21600, 27000)
    # East exterior wall of Procedure: X = 21600, Y = 0..5000
    # Corridor end walls: West X = 0 (Y=5150..7350), East X = 27000 (Y=5150..7350)
    windows: List[Window] = [
        # Waiting (2 large storefront glazing windows on South wall, clear of col at X=5400)
        Window("W01", "Waiting",      "H", 3650.0,  0.0,     2000.0),
        Window("W02", "Waiting",      "H", 6950.0,  0.0,     1800.0),
        # Exam1 (X=8450..11250; col at 10800 -> window center 9550, width 1400 -> 8850..10250)
        Window("W03", "Exam1",        "H", 9550.0,  0.0,     1400.0),
        # Exam2 (X=11400..14200; no col -> window center 12800, width 1600 -> 12000..13600)
        Window("W04", "Exam2",        "H", 12800.0, 0.0,     1600.0),
        # Exam3 (X=14350..17150; col at 16200 -> window center 15250, width 1200 -> 14650..15850)
        Window("W05", "Exam3",        "H", 15250.0, 0.0,     1200.0),
        # Procedure (X=17300..21450; South window + East window)
        Window("W06", "Procedure",    "H", 19400.0, 0.0,     1800.0),
        Window("W07", "Procedure",    "V", 21600.0, 2500.0,  1600.0),
        # Breakroom (X=3850..7700; col at 5400 -> window center 6600, width 1500 -> 5850..7350)
        Window("W08", "Breakroom",    "H", 6600.0,  11000.0, 1500.0),
        # NurseStation (X=7850..11350; col at 10800 -> window center 9200, width 1500 -> 8450..9950)
        Window("W09", "NurseStation", "H", 9200.0,  11000.0, 1500.0),
        # Exam4 (X=11500..15350; no col -> window center 13425, width 1600 -> 12625..14225)
        Window("W10", "Exam4",        "H", 13425.0, 11000.0, 1600.0),
        # Lab (X=15500..18800; col at 16200 -> window center 17550, width 1400 -> 16850..18250)
        Window("W11", "Lab",          "H", 17550.0, 11000.0, 1400.0),
        # Corridor daylighting end windows (West & East ends)
        Window("W12", "Corridor",     "V", 0.0,     6250.0,  1400.0),
        Window("W13", "Corridor",     "V", 27000.0, 6250.0,  1400.0),
    ]

    # --- Fixtures (all strictly inside rooms, 0 collisions with swings/columns/other fixtures) ---
    fixtures: List[Fixture] = [
        # Waiting (X=2150..8300, Y=150..5000; D02 swing=[2150,1800,3350,3000], D03 swing=[6400,3800,7600,5000], col=[5250,4850,5550,5150])
        Fixture("F-WAIT-REC",   "Waiting", "reception_counter", 2500, 3850, 5050, 4650, wet=False),
        Fixture("F-WAIT-SEAT1", "Waiting", "waiting_seating",   3600,  450, 6200, 1100, wet=False),
        Fixture("F-WAIT-SEAT2", "Waiting", "waiting_seating",   7400,  700, 8100, 3200, wet=False),

        # Exam1 (X=8450..11250, Y=150..5000; D04 swing=[8750,4050,9700,5000], col=[10650,4850,10950,5150])
        Fixture("F-EX1-TBL",  "Exam1", "exam_table",     9650, 1750, 10450, 3650, wet=False),
        Fixture("F-EX1-SINK", "Exam1", "handwash_basin", 10650, 3850, 11200, 4550, wet=True),
        Fixture("F-EX1-DESK", "Exam1", "desk",           8600,  350, 10000,  950, wet=False),

        # Exam2 (X=11400..14200, Y=150..5000; D05 swing=[11700,4050,12650,5000])
        Fixture("F-EX2-TBL",  "Exam2", "exam_table",     12600, 1750, 13400, 3650, wet=False),
        Fixture("F-EX2-SINK", "Exam2", "handwash_basin", 13600, 3950, 14150, 4650, wet=True),
        Fixture("F-EX2-DESK", "Exam2", "desk",           11550,  350, 12950,  950, wet=False),

        # Exam3 (X=14350..17150, Y=150..5000; D06 swing=[14650,4050,15600,5000], col=[16050,4850,16350,5150])
        Fixture("F-EX3-TBL",  "Exam3", "exam_table",     15550, 1750, 16350, 3650, wet=False),
        Fixture("F-EX3-SINK", "Exam3", "handwash_basin", 16550, 3850, 17100, 4550, wet=True),
        Fixture("F-EX3-DESK", "Exam3", "desk",           14500,  350, 15900,  950, wet=False),

        # Procedure (X=17300..21450, Y=150..5000; D07 swing=[17700,3800,18900,5000])
        Fixture("F-PROC-TBL",  "Procedure", "procedure_table",    19050, 1650, 20000, 3750, wet=False),
        Fixture("F-PROC-SINK", "Procedure", "scrub_sink",         19700, 4300, 21100, 4950, wet=True),
        Fixture("F-PROC-CAB",  "Procedure", "instrument_cabinet", 17400,  550, 18050, 2650, wet=False),

        # PublicWC (X=150..2050, Y=7500..10850; D08 swing=[450,7500,1400,8450])
        Fixture("F-PWC-WC",   "PublicWC", "wc_accessible",  1350, 10050, 1950, 10800, wet=True),
        Fixture("F-PWC-SINK", "PublicWC", "handwash_basin",  200,  9600,  750, 10300, wet=True),

        # StaffWC (X=2200..3700, Y=7500..10850; D09 swing=[2450,7500,3350,8400])
        Fixture("F-SWC-WC",   "StaffWC", "wc",             2950, 10050, 3550, 10800, wet=True),
        Fixture("F-SWC-SINK", "StaffWC", "handwash_basin", 2250,  9600, 2750, 10250, wet=True),

        # Breakroom (X=3850..7700, Y=7500..10850; D10 swing=[4150,7500,5100,8450])
        Fixture("F-BRK-CTR",  "Breakroom", "kitchen_counter", 3900, 9100, 4550, 10800, wet=False),
        Fixture("F-BRK-SINK", "Breakroom", "kitchen_sink",    3925, 9600, 4525, 10300, wet=True), # wait: avoid overlap with counter! Split counter!
        # Let's make counter and sink adjacent (0 overlap!) below.
    ]

    # Fix Breakroom counter/sink so 0 fixture-to-fixture overlap:
    fixtures = [f for f in fixtures if not f.id.startswith("F-BRK")]
    fixtures.extend([
        Fixture("F-BRK-CTR",  "Breakroom", "kitchen_counter", 3900,  8900, 4550, 10050, wet=False),
        Fixture("F-BRK-SINK", "Breakroom", "kitchen_sink",    3900, 10060, 4550, 10800, wet=True),
        Fixture("F-BRK-TBL",  "Breakroom", "break_table",     5600,  8400, 7200,  9600, wet=False),

        # NurseStation (X=7850..11350, Y=7500..10850; D11 swing=[8200,7500,9300,8600])
        Fixture("F-NUR-CTR",  "NurseStation", "nurse_counter", 9550, 7800, 11200, 9700, wet=False),
        Fixture("F-NUR-DESK", "NurseStation", "workstation",   8100, 9950, 10800, 10650, wet=False),

        # Exam4 (X=11500..15350, Y=7500..10850; D12 swing=[11800,7500,12750,8450])
        Fixture("F-EX4-TBL",  "Exam4", "exam_table",     13100, 8250, 15000,  9050, wet=False),
        Fixture("F-EX4-SINK", "Exam4", "handwash_basin", 14750, 9750, 15300, 10450, wet=True),
        Fixture("F-EX4-DESK", "Exam4", "desk",           11650, 9850, 13050, 10550, wet=False),

        # Lab (X=15500..18800, Y=7500..10850; D13 swing=[15800,7500,16750,8450])
        Fixture("F-LAB-BNCH", "Lab", "lab_bench",          17100, 7650, 18750,  8350, wet=False),
        Fixture("F-LAB-SINK", "Lab", "sterilization_sink", 15550, 9800, 16650, 10650, wet=True),
        Fixture("F-LAB-BNCH2","Lab", "lab_bench",          16700, 9950, 18750, 10700, wet=False),

        # Pharmacy (X=18950..21650, Y=7500..10850; D14 swing=[19250,7500,20150,8400])
        Fixture("F-PHM-SHL1", "Pharmacy", "pharmacy_shelving", 19000, 10150, 21600, 10800, wet=False),
        Fixture("F-PHM-SHL2", "Pharmacy", "pharmacy_shelving", 20950,  7650, 21600,  9950, wet=False),

        # ElecITRoom (X=21800..24000, Y=7500..10850; D15 swing=[22100,7500,23000,8400]) -> STRICTLY DRY ZONE!
        Fixture("F-ELC-RACK", "ElecITRoom", "it_server_rack", 21900,  9850, 22950, 10750, wet=False),
        Fixture("F-ELC-PNL",  "ElecITRoom", "elec_panel",     23450,  8600, 23950, 10500, wet=False),

        # MechRoom (X=24150..26850, Y=7500..10850; D16 swing=[24450,7500,25400,8450])
        Fixture("F-MCH-MOP",  "MechRoom", "mop_sink",     24250, 9900, 25050, 10750, wet=True),
        Fixture("F-MCH-WH",   "MechRoom", "water_heater", 25650, 9650, 26650, 10650, wet=True),
    ])

    # --- Wall Polygons (for DXF & PNG rendering, with exact door & window cutouts) ---
    outer_poly = Polygon(FOOTPRINT_OUTER_PTS)
    inner_poly = Polygon(FOOTPRINT_INNER_PTS)
    ext_wall_solid = outer_poly.difference(inner_poly)

    # Cut exterior doors and windows from exterior wall solid
    ext_cutouts = [w.opening_poly for w in windows] + [d.opening_poly for d in doors if d.room_a == "Exterior"]
    ext_walls_geom = ext_wall_solid.difference(unary_union(ext_cutouts))

    # Interior partitions solid = inner_poly - union(rooms)
    rooms_union = unary_union([r.poly for r in rooms])
    int_wall_solid = inner_poly.difference(rooms_union)
    int_cutouts = [d.opening_poly for d in doors if d.room_a != "Exterior"]
    int_walls_geom = int_wall_solid.difference(unary_union(int_cutouts))

    return rooms, rmap, columns, doors, windows, fixtures, ext_walls_geom, int_walls_geom, outer_poly, inner_poly


# ---------------------------------------------------------------------------
# 3. RIGOROUS GEOMETRIC & CODE VERIFICATION SUITE
# ---------------------------------------------------------------------------

def verify_model(rooms, rmap, columns, doors, windows, fixtures, outer_poly, inner_poly):
    errors = []
    stats = {}

    # 1. Room count & minimum areas
    mandatory_ids = [
        "Vestibule", "Waiting", "PublicWC", "Breakroom", "Corridor",
        "Exam1", "Exam2", "Exam3", "Exam4", "Procedure",
        "NurseStation", "Lab", "Pharmacy", "ElecITRoom", "MechRoom"
    ]
    for mid in mandatory_ids:
        if mid not in rmap:
            errors.append(f"Missing mandatory room: {mid}")
    for r in rooms:
        if r.area_m2 < r.min_area_req:
            errors.append(f"Room {r.id} area {r.area_m2} m² < required {r.min_area_req} m²")

    # Corridor clear width >= 1800 mm
    corr = rmap["Corridor"]
    if corr.depth < 1800.0:
        errors.append(f"Corridor clear width {corr.depth} mm < 1800 mm")

    # Total area checks
    net_area_m2 = sum(r.area_m2 for r in rooms)
    gross_area_m2 = round(outer_poly.area / 1e6, 2)
    stats["room_count"] = len(rooms)
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
    # Verify exterior enters only via Vestibule
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

        # Check jamb margins against connected rooms
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
        # Door swing must stay inside its target room
        target_room = d.room_b if d.open_dir in ("+N", "+E") else (d.room_a if d.id == "D03" else d.room_b)
        if d.id == "D03":
            target_room = "Waiting"
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

    # 8. Daylighting check on exterior walls
    daylight_req = {"Waiting", "Breakroom", "Exam1", "Exam2", "Exam3", "Exam4", "Procedure"}
    win_rooms = set()
    ext_ring = outer_poly.exterior
    for w in windows:
        win_rooms.add(w.room)
        # Verify window actually intersects the exterior boundary wall
        if w.opening_poly.intersection(outer_poly.difference(inner_poly)).area < (w.width_mm * EXT_WALL_T - 10.0):
            errors.append(f"Window {w.id} ({w.room}) is not fully on an exterior wall!")
    missing_daylight = daylight_req - win_rooms
    if missing_daylight:
        errors.append(f"Missing daylight windows for: {missing_daylight}")

    # 9. Wall thickness check
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
        # Single-line clean ASCII text centered at (x, y)
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
        elif geom.geom_type == "MultiPolygon":
            for g in geom.geoms:
                self.add_shapely_geom(layer, g)
        elif geom.geom_type == "GeometryCollection":
            for g in geom.geoms:
                self.add_shapely_geom(layer, g)

    def save(self, path: str):
        lines = [
            "0", "SECTION",
            "2", "HEADER",
            "9", "$ACADVER", "1", "AC1009",
            "9", "$INSUNITS", "70", "4",
            "9", "$EXTMIN", "10", "-2500.0", "20", "-2500.0", "30", "0.0",
            "9", "$EXTMAX", "10", "29500.0", "20", "13500.0", "30", "0.0",
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
        y_start = -1400.0 if x <= 21600.0 else 3600.0
        y_end = 12400.0
        dxf.add_line("S-GRID", x, y_start, x, y_end)
        dxf.add_circle("S-GRID", x, y_end + 350.0, 350.0)
        dxf.add_text("A-ANNO-TEXT", x, y_end + 350.0, 260.0, GRID_X_LABELS[xi])

    for yi, y in enumerate(GRID_Y):
        x_start = -1400.0
        x_end = 22800.0 if y == 0.0 else 28400.0
        dxf.add_line("S-GRID", x_start, y, x_end, y)
        dxf.add_circle("S-GRID", x_start - 350.0, y, 350.0)
        dxf.add_text("A-ANNO-TEXT", x_start - 350.0, y, 260.0, GRID_Y_LABELS[yi])

    # 2. Walls (exterior & interior polygons)
    dxf.add_shapely_geom("A-WALL-EXT", ext_walls_geom)
    dxf.add_shapely_geom("A-WALL-INT", int_walls_geom)

    # 3. Structural columns (300x300 box + diagonal X)
    for c in columns:
        dxf.add_rect("S-COLS", c.min_x, c.min_y, c.max_x, c.max_y)
        dxf.add_line("S-COLS", c.min_x, c.min_y, c.max_x, c.max_y)
        dxf.add_line("S-COLS", c.min_x, c.max_y, c.max_x, c.min_y)

    # 4. Doors (leaf line + 90-deg swing arc + tag)
    for d in doors:
        lx, ly = d.leaf_end
        dxf.add_line("A-DOOR", d.hinge_x, d.hinge_y, lx, ly)
        sa, ea = d.arc_angles
        dxf.add_arc("A-DOOR", d.hinge_x, d.hinge_y, d.width_mm, sa, ea)
        # Tag near center of swing
        sb = d.swing_bbox
        dxf.add_text("A-DOOR", (sb[0] + sb[2]) / 2.0, (sb[1] + sb[3]) / 2.0, 140.0, d.id)

    # 5. Windows (outer frame + glazing double lines)
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

    # 8. Dimension Strings (overall & bay dimensions)
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

    # North bay dimensions & overall width
    for i in range(len(GRID_X) - 1):
        add_dim_h(GRID_X[i], GRID_X[i + 1], 11850.0, f"{int(GRID_X[i+1]-GRID_X[i])}")
    add_dim_h(0.0, 27000.0, 13450.0, "27000 (OVERALL)")
    add_dim_h(0.0, 21600.0, -900.0, "21600 (SOUTH WING)")
    # East vertical dimensions
    add_dim_v(0.0, 5000.0, 22450.0, "5000")
    add_dim_v(5000.0, 11000.0, 28100.0, "6000")
    add_dim_v(0.0, 11000.0, -1100.0, "11000")

    dxf.save(path)


# ---------------------------------------------------------------------------
# 5. PUBLICATION-GRADE ARCHITECTURAL PNG RENDERER
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
    fig, ax = plt.subplots(figsize=(22, 12.2), dpi=220)
    fig.patch.set_facecolor("#FAFBFD")
    ax.set_facecolor("#FAFBFD")

    zone_colors = {
        "public":   "#EEF5FC",
        "clinical": "#F0F9F4",
        "staff":    "#FDF6EC",
        "service":  "#F4F3F7",
        "circ":     "#F7F8FA",
    }

    # 1. Room fills
    for r in rooms:
        fc = zone_colors.get(r.zone, "#FFFFFF")
        ax.add_patch(Rectangle((r.min_x, r.min_y), r.width, r.depth,
                               facecolor=fc, edgecolor="none", zorder=1))

    # 2. Structural Gridlines & Bubbles
    for xi, x in enumerate(GRID_X):
        y0 = -1500.0 if x <= 21600.0 else 3500.0
        y1 = 12300.0
        ax.plot([x, x], [y0, y1], color="#6C8EB4", linestyle=(0, (8, 4, 2, 4)),
                linewidth=0.75, alpha=0.75, zorder=2)
        circ = Circle((x, y1 + 380), 360, facecolor="white", edgecolor="#3B5B82", linewidth=1.2, zorder=8)
        ax.add_patch(circ)
        ax.text(x, y1 + 380, GRID_X_LABELS[xi], ha="center", va="center",
                fontsize=9.5, fontweight="bold", color="#243B55", zorder=9)

    for yi, y in enumerate(GRID_Y):
        x0 = -1500.0
        x1 = 22900.0 if y == 0.0 else 28300.0
        ax.plot([x0, x1], [y, y], color="#6C8EB4", linestyle=(0, (8, 4, 2, 4)),
                linewidth=0.75, alpha=0.75, zorder=2)
        circ = Circle((x0 - 380, y), 360, facecolor="white", edgecolor="#3B5B82", linewidth=1.2, zorder=8)
        ax.add_patch(circ)
        ax.text(x0 - 380, y, GRID_Y_LABELS[yi], ha="center", va="center",
                fontsize=9.5, fontweight="bold", color="#243B55", zorder=9)

    # 3. Fixtures (detailed architectural symbols)
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

    # 4. Walls (Exterior 300 mm dark poché, Interior 150 mm medium poché)
    _plot_shapely_filled(ax, ext_walls_geom, facecolor="#24292F", edgecolor="#111418", linewidth=1.1, zorder=5)
    _plot_shapely_filled(ax, int_walls_geom, facecolor="#5A6470", edgecolor="#2C343D", linewidth=0.8, zorder=5)

    # 5. Structural Columns (300x300 mm crisp architectural red-brown steel/RC poché)
    for c in columns:
        ax.add_patch(Rectangle((c.min_x, c.min_y), c.size, c.size,
                               facecolor="#B83227", edgecolor="#5E140E", linewidth=1.0, zorder=7))
        ax.plot([c.min_x, c.max_x], [c.min_y, c.max_y], color="#FFE6E3", lw=0.65, zorder=7.2)
        ax.plot([c.min_x, c.max_x], [c.max_y, c.min_y], color="#FFE6E3", lw=0.65, zorder=7.2)

    # 6. Exterior Windows (cyan glazing assembly)
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

    # 7. Doors (open leaf + dashed 90-deg swing arc + door tag)
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
        "Exam3": (0, -1180),
        "Procedure": (200, -1550),
        "Corridor": (0, 0),
        "PublicWC": (0, -150),
        "StaffWC": (0, -150),
        "Breakroom": (450, 950),
        "NurseStation": (-850, 50),
        "Exam4": (50, 500),
        "Lab": (200, 150),
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
            fs = 7.5 if r.width >= 2500 else 6.4
            ax.text(cx, cy + 160, r.label, ha="center", va="center", fontsize=fs, fontweight="bold",
                    color="#102A43", zorder=8.5)
            ax.text(cx, cy - 260, f"{r.area_m2:.1f} m²", ha="center", va="center", fontsize=6.8,
                    color="#486581", zorder=8.5)

    # Entrance indicator arrow at Vestibule
    ax.annotate("MAIN ENTRY\n(1300 mm)", xy=(1075, -150), xytext=(1075, -1350),
                ha="center", va="top", fontsize=8.5, fontweight="bold", color="#1B7A43",
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
    draw_dim_h(0.0, 27000.0, 13450.0, "27,000 mm (OVERALL WIDTH)")
    draw_dim_h(0.0, 21600.0, -900.0, "21,600 mm (SOUTH CLINICAL WING)")
    draw_dim_v(0.0, 5000.0, 22300.0, "5,000 mm")
    draw_dim_v(5000.0, 11000.0, 27900.0, "6,000 mm")
    draw_dim_v(0.0, 11000.0, -950.0, "11,000 mm")

    # 10. Legend & Title Block (placed cleanly inside the SE stepped cutout!)
    tb_x, tb_y, tb_w, tb_h = 23250.0, -1400.0, 5400.0, 5600.0
    ax.add_patch(FancyBboxPatch((tb_x, tb_y), tb_w, tb_h, boxstyle="round,pad=120",
                                facecolor="white", edgecolor="#334E68", linewidth=1.3, zorder=8))
    ax.text(tb_x + tb_w / 2.0, tb_y + tb_h - 380,
            "OUTPATIENT SPECIALIST CLINIC", ha="center", va="top",
            fontsize=10.5, fontweight="bold", color="#102A43", zorder=9)
    ax.text(tb_x + tb_w / 2.0, tb_y + tb_h - 820,
            "ARCHITECTURAL & STRUCTURAL FLOOR PLAN", ha="center", va="top",
            fontsize=7.5, fontweight="bold", color="#486581", zorder=9)
    ax.plot([tb_x + 250, tb_x + tb_w - 250], [tb_y + tb_h - 1150, tb_y + tb_h - 1150],
            color="#BCCCDC", lw=1.0, zorder=9)

    summary_lines = [
        f"Rooms: {stats['room_count']} (15 Mandatory + StaffWC)",
        f"Net Area: {stats['net_area_m2']:.1f} m²  |  Gross: {stats['gross_area_m2']:.1f} m²",
        f"Grid: 5×5.4 m (X), 5.0 / 6.0 m (Y) — {stats['column_count']} Cols",
        f"Walls: 300 mm Ext / 150 mm Int",
        f"Corridor: {int(stats['corridor_clear_width_mm'])} mm Clear Width",
        f"Doors: {stats['door_count']}  |  Windows: {stats['window_count']}  |  Fixtures: {stats['fixture_count']}",
        f"Checks: 0 Overlaps | 0 Collisions | 100% BFS",
    ]
    for idx, sl in enumerate(summary_lines):
        ax.text(tb_x + 280, tb_y + tb_h - 1450 - idx * 380, sl, ha="left", va="top",
                fontsize=7.0, color="#243B55", zorder=9)

    # Legend swatches inside title block
    leg_y = tb_y + 1150
    swatches = [
        ("#24292F", "Exterior Wall (300 mm)"),
        ("#5A6470", "Interior Wall (150 mm)"),
        ("#B83227", "RC Column (300×300)"),
        ("#0277BD", "Exterior Glazing"),
        ("#1B7A43", "Door & 90° Swing"),
        ("#2C6E91", "Wet Fixture"),
    ]
    for i, (col, lbl) in enumerate(swatches):
        col_i = i % 2
        row_i = i // 2
        sx = tb_x + 280 + col_i * 2650
        sy = leg_y - row_i * 360
        ax.add_patch(Rectangle((sx, sy - 90), 320, 180, facecolor=col, edgecolor="#102A43", lw=0.6, zorder=9))
        ax.text(sx + 420, sy, lbl, ha="left", va="center", fontsize=6.5, color="#243B55", zorder=9)

    # Scale bar (0..5m) & North Arrow
    sb_x, sb_y = 2500.0, -1850.0
    for k in range(5):
        fc = "#24292F" if k % 2 == 0 else "white"
        ax.add_patch(Rectangle((sb_x + k * 1000, sb_y), 1000, 180, facecolor=fc, edgecolor="#24292F", lw=0.8, zorder=9))
    for k in [0, 1, 2, 5]:
        ax.text(sb_x + k * 1000, sb_y - 120, f"{k} m", ha="center", va="top", fontsize=7.0, color="#243B55", zorder=9)
    ax.text(sb_x + 2500, sb_y + 260, "SCALE BAR (mm / m)", ha="center", va="bottom",
            fontsize=6.8, fontweight="bold", color="#486581", zorder=9)

    # North arrow
    nx_c, ny_c = 20500.0, -1450.0
    ax.annotate("", xy=(nx_c, ny_c + 650), xytext=(nx_c, ny_c - 250),
                arrowprops=dict(arrowstyle="-|>", color="#102A43", lw=2.0), zorder=9)
    ax.text(nx_c, ny_c + 720, "N", ha="center", va="bottom", fontsize=10, fontweight="bold", color="#102A43", zorder=9)

    ax.set_aspect("equal")
    ax.set_xlim(-2500, 29300)
    ax.set_ylim(-2400, 14100)
    ax.axis("off")

    plt.tight_layout(pad=0.3)
    plt.savefig(path, dpi=220, facecolor=fig.get_facecolor())
    plt.close(fig)


# ---------------------------------------------------------------------------
# 6. MANIFEST EXPORT & MAIN ENTRY
# ---------------------------------------------------------------------------

def export_manifest(path: str, rooms, columns, doors, windows, fixtures):
    manifest = {
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

    print("=== CLINIC VERIFICATION SUMMARY ===")
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
