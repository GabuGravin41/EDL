#!/usr/bin/env python3
"""
Reusable native Python KiCad 9 schematic generator for PCBBench baseline arm.
Parses verbatim symbol definitions from test.kicad_sym and KiCad 9 stock libraries,
computes exact pin world coordinates, routes wires/labels/power flags, and writes
valid KiCad 9 (.kicad_sch) + sym-lib-table files.
"""

import math
import os
import re
import uuid
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

TEST_SYM_LIB = "/home/lindah/projects/EDL/benchmarks/external/PCBSchemaGen_v2/library/test.kicad_sym"
STOCK_LIB_DIR = "/usr/share/kicad/symbols"

LIB_PATHS = {
    "test": TEST_SYM_LIB,
    "Device": os.path.join(STOCK_LIB_DIR, "Device.kicad_sym"),
    "power": os.path.join(STOCK_LIB_DIR, "power.kicad_sym"),
    "Connector": os.path.join(STOCK_LIB_DIR, "Connector.kicad_sym"),
    "Connector_Generic": os.path.join(STOCK_LIB_DIR, "Connector_Generic.kicad_sym"),
}


def snap(val: float, grid: float = 1.27) -> float:
    return round(round(val / grid) * grid, 4)


def fmt(val: float) -> str:
    if abs(val) < 1e-9:
        val = 0.0
    s = f"{val:.4f}".rstrip("0").rstrip(".")
    return s if s else "0"


def new_uuid() -> str:
    return str(uuid.uuid4())


@dataclass
class PinDef:
    number: str
    name: str
    ptype: str
    x: float
    y: float
    angle: float
    length: float


class SymbolCache:
    """Loads and caches verbatim S-expression symbol blocks and parsed pin metadata."""

    _file_cache: Dict[str, str] = {}
    _sym_cache: Dict[str, Tuple[str, List[PinDef], Tuple[float, float, float, float]]] = {}

    @classmethod
    def _read_lib(cls, lib_name: str) -> str:
        if lib_name not in cls._file_cache:
            path = LIB_PATHS[lib_name]
            with open(path, "r", encoding="utf-8") as f:
                cls._file_cache[lib_name] = f.read()
        return cls._file_cache[lib_name]

    @classmethod
    def get_symbol(cls, lib_id: str) -> Tuple[str, List[PinDef], Tuple[float, float, float, float]]:
        """Return (verbatim_sexpr_with_prefixed_outer_name, pins, (xmin, ymin, xmax, ymax) in sym coords)."""
        if lib_id in cls._sym_cache:
            return cls._sym_cache[lib_id]

        lib_name, sym_name = lib_id.split(":", 1)
        txt = cls._read_lib(lib_name)

        # Find exact top-level symbol definition
        needle = f'\t(symbol "{sym_name}"'
        idx = txt.find(needle)
        if idx == -1:
            # fallback regex for different indentation
            m = re.search(rf'^\s*\(symbol\s+"{re.escape(sym_name)}"(?:\s|\n)', txt, re.M)
            if not m:
                raise KeyError(f"Symbol {lib_id} not found in {LIB_PATHS[lib_name]}")
            idx = m.start()
            start = txt.find("(", idx)
        else:
            start = idx + 1

        depth = 0
        in_str = False
        esc = False
        i = start
        end = -1
        while i < len(txt):
            ch = txt[i]
            if in_str:
                if esc:
                    esc = False
                elif ch == "\\":
                    esc = True
                elif ch == '"':
                    in_str = False
            else:
                if ch == '"':
                    in_str = True
                elif ch == "(":
                    depth += 1
                elif ch == ")":
                    depth -= 1
                    if depth == 0:
                        end = i + 1
                        break
            i += 1
        if end == -1:
            raise ValueError(f"Unbalanced parentheses for symbol {lib_id}")

        raw = txt[start:end]

        # Handle (extends "BaseSym") in stock KiCad libraries if present
        ext_m = re.search(r'\(extends\s+"([^"]+)"\)', raw)
        if ext_m:
            base_name = ext_m.group(1)
            _, base_pins, base_bbox = cls.get_symbol(f"{lib_name}:{base_name}")
            # For embedded lib_symbols in KiCad 9, flatten or include base; avoid extended symbols when possible
            # Better: inline base's graphical subsymbols into raw if needed, or load base too.
            pins = base_pins
            bbox = base_bbox
        else:
            pin_re = re.compile(
                r'\(pin\s+(\w+)\s+(\w+)\s+\(at\s+([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)\)\s+\(length\s+([-\d.]+)\).*?'
                r'\(name\s+"([^"]*)".*?\(number\s+"([^"]*)"',
                re.S,
            )
            pins = []
            xs, ys = [], []
            for ptype, _shape, px, py, pang, plen, pname, pnum in pin_re.findall(raw):
                fx, fy = float(px), float(py)
                pins.append(PinDef(pnum, pname, ptype, fx, fy, float(pang), float(plen)))
                xs.append(fx)
                ys.append(fy)

            # Also scan rectangle / polyline coords for bbox
            for mx, my in re.findall(r'\((?:start|end|xy|center)\s+([-\d.]+)\s+([-\d.]+)\)', raw):
                xs.append(float(mx))
                ys.append(float(my))
            if xs and ys:
                bbox = (min(xs), min(ys), max(xs), max(ys))
            else:
                bbox = (-2.54, -2.54, 2.54, 2.54)

        # Prefix ONLY the outermost symbol name: (symbol "Name" -> (symbol "lib:Name"
        prefixed = raw.replace(f'(symbol "{sym_name}"', f'(symbol "{lib_id}"', 1)
        # Strip any legacy (id N) inside properties in older symbol blocks? KiCad 9 tolerates them, keep verbatim!
        cls._sym_cache[lib_id] = (prefixed, pins, bbox)
        return cls._sym_cache[lib_id]


@dataclass
class PlacedSymbol:
    ref: str
    lib_id: str
    value: str
    x: float
    y: float
    rot: float = 0.0
    mirror: Optional[str] = None  # None, "x", "y"
    footprint: str = ""
    uid: str = field(default_factory=new_uuid)
    ref_offset: Optional[Tuple[float, float]] = None
    val_offset: Optional[Tuple[float, float]] = None
    in_bom: bool = True
    on_board: bool = True
    is_power: bool = False
    hide_ref: bool = False
    hide_val: bool = False
    pins: List[PinDef] = field(default_factory=list)
    bbox: Tuple[float, float, float, float] = (-2.54, -2.54, 2.54, 2.54)

    def _transform_point(self, px: float, py: float) -> Tuple[float, float]:
        """Transform symbol-local (Y-up) point to schematic (Y-down) world coordinates.
        KiCad applies rotation CCW in symbol space FIRST, then mirror (x flips Y, y flips X)."""
        rad = math.radians(self.rot)
        c = round(math.cos(rad))
        s = round(math.sin(rad))
        rx = px * c - py * s
        ry = px * s + py * c

        if self.mirror == "x":
            ry = -ry
        elif self.mirror == "y":
            rx = -rx

        wx = round(self.x + rx, 4)
        wy = round(self.y - ry, 4)
        return (wx, wy)

    def pin(self, key: str) -> Tuple[float, float]:
        """Get world (x, y) of a pin by pin number OR exact pin name."""
        # First match by exact pin number
        for p in self.pins:
            if p.number == str(key):
                return self._transform_point(p.x, p.y)
        # Next match by exact pin name
        matches = [p for p in self.pins if p.name == key]
        if matches:
            return self._transform_point(matches[0].x, matches[0].y)
        raise KeyError(
            f"Pin '{key}' not found on {self.ref} ({self.lib_id}). "
            f"Available: {[(p.number, p.name) for p in self.pins]}"
        )

    def pins_by_name(self, name: str) -> List[Tuple[float, float]]:
        """Get all unique world coordinates for pins matching `name` (handles stacked pins)."""
        coords = []
        for p in self.pins:
            if p.name == name:
                pt = self._transform_point(p.x, p.y)
                if pt not in coords:
                    coords.append(pt)
        if not coords:
            raise KeyError(f"No pins named '{name}' on {self.ref} ({self.lib_id})")
        return coords

    def all_pin_points(self) -> Dict[str, Tuple[float, float]]:
        return {p.number: self._transform_point(p.x, p.y) for p in self.pins}

    def pin_dir_vec(self, key: str) -> Tuple[float, float]:
        """Unit vector pointing OUTWARD from the pin tip (away from symbol body) in schematic Y-down coords."""
        target = None
        for p in self.pins:
            if p.number == str(key) or p.name == key:
                target = p
                break
        if target is None:
            raise KeyError(f"Pin '{key}' not found on {self.ref}")
        # In KiCad symbol, pin.angle points INTO the body from the pin tip!
        # So outward direction in symbol Y-up space is (angle + 180)
        ang = (target.angle + 180.0) % 360.0
        rad = math.radians(ang)
        dx, dy = round(math.cos(rad)), round(math.sin(rad))
        r2 = math.radians(self.rot)
        c, s = round(math.cos(r2)), round(math.sin(r2))
        rdx = dx * c - dy * s
        rdy = dx * s + dy * c
        if self.mirror == "x":
            rdy = -rdy
        elif self.mirror == "y":
            rdx = -rdx
        return (float(rdx), float(-rdy))  # Y-down


class Schematic:
    def __init__(
        self,
        title: str,
        project_name: str,
        paper: str = "A2",
        rev: str = "1.0",
        comment1: str = "",
        comment2: str = "",
    ):
        self.title = title
        self.project_name = project_name
        self.paper = paper
        self.rev = rev
        self.comment1 = comment1
        self.comment2 = comment2
        self.root_uuid = new_uuid()
        self.used_lib_ids: List[str] = []
        self.symbols: List[PlacedSymbol] = []
        self.wires: List[Tuple[Tuple[float, float], Tuple[float, float]]] = []
        self.junctions: List[Tuple[float, float]] = []
        self.no_connects: List[Tuple[float, float]] = []
        self.labels: List[Tuple[str, float, float, float, str]] = []  # (text, x, y, rot, justify)
        self.global_labels: List[Tuple[str, float, float, float, str, str]] = []  # (text, x, y, rot, shape, justify)
        self.texts: List[Tuple[str, float, float, float, bool]] = []  # (text, x, y, size, bold)
        self.rects: List[Tuple[float, float, float, float]] = []
        self._pwr_counter = 1
        self._flg_counter = 1
        self._ref_counters: Dict[str, int] = {}

    def _register_lib(self, lib_id: str):
        if lib_id not in self.used_lib_ids:
            SymbolCache.get_symbol(lib_id)
            self.used_lib_ids.append(lib_id)

    def next_ref(self, prefix: str) -> str:
        n = self._ref_counters.get(prefix, 0) + 1
        self._ref_counters[prefix] = n
        return f"{prefix}{n}"

    def add_symbol(
        self,
        lib_id: str,
        ref: str,
        value: str,
        x: float,
        y: float,
        rot: float = 0.0,
        mirror: Optional[str] = None,
        footprint: str = "",
        ref_offset: Optional[Tuple[float, float]] = None,
        val_offset: Optional[Tuple[float, float]] = None,
    ) -> PlacedSymbol:
        self._register_lib(lib_id)
        _, pins, bbox = SymbolCache.get_symbol(lib_id)
        sym = PlacedSymbol(
            ref=ref,
            lib_id=lib_id,
            value=value,
            x=round(x, 4),
            y=round(y, 4),
            rot=rot,
            mirror=mirror,
            footprint=footprint,
            ref_offset=ref_offset,
            val_offset=val_offset,
            pins=pins,
            bbox=bbox,
        )
        self.symbols.append(sym)
        return sym

    def add_power(
        self,
        lib_id: str,
        x: float,
        y: float,
        rot: float = 0.0,
        value: Optional[str] = None,
    ) -> PlacedSymbol:
        """Place a power symbol (power:GND, power:PWR_FLAG, power:+3V3, etc.) at pin coordinate (x, y)."""
        self._register_lib(lib_id)
        _, pins, bbox = SymbolCache.get_symbol(lib_id)
        sym_name = lib_id.split(":", 1)[1]
        if sym_name == "PWR_FLAG":
            ref = f"#FLG{self._flg_counter:02d}"
            self._flg_counter += 1
        else:
            ref = f"#PWR{self._pwr_counter:02d}"
            self._pwr_counter += 1
        sym = PlacedSymbol(
            ref=ref,
            lib_id=lib_id,
            value=value if value is not None else sym_name,
            x=round(x, 4),
            y=round(y, 4),
            rot=rot,
            in_bom=False,
            on_board=False,
            is_power=True,
            hide_ref=True,
            hide_val=(sym_name == "GND"),
            pins=pins,
            bbox=bbox,
        )
        self.symbols.append(sym)
        return sym

    def wire(self, p1: Tuple[float, float], p2: Tuple[float, float]):
        x1, y1 = round(p1[0], 4), round(p1[1], 4)
        x2, y2 = round(p2[0], 4), round(p2[1], 4)
        if abs(x1 - x2) < 1e-6 and abs(y1 - y2) < 1e-6:
            return
        self.wires.append(((x1, y1), (x2, y2)))

    def wire_poly(self, pts: List[Tuple[float, float]]):
        for a, b in zip(pts, pts[1:]):
            self.wire(a, b)

    def wire_ortho(self, p1: Tuple[float, float], p2: Tuple[float, float], horiz_first: bool = True):
        x1, y1 = round(p1[0], 4), round(p1[1], 4)
        x2, y2 = round(p2[0], 4), round(p2[1], 4)
        if abs(x1 - x2) < 1e-6 or abs(y1 - y2) < 1e-6:
            self.wire((x1, y1), (x2, y2))
        elif horiz_first:
            mid = (x2, y1)
            self.wire((x1, y1), mid)
            self.wire(mid, (x2, y2))
        else:
            mid = (x1, y2)
            self.wire((x1, y1), mid)
            self.wire(mid, (x2, y2))

    def junction(self, pt: Tuple[float, float]):
        p = (round(pt[0], 4), round(pt[1], 4))
        if p not in self.junctions:
            self.junctions.append(p)

    def no_connect(self, pt: Tuple[float, float]):
        p = (round(pt[0], 4), round(pt[1], 4))
        if p not in self.no_connects:
            self.no_connects.append(p)

    def label(self, text: str, pt: Tuple[float, float], rot: float = 0.0, justify: str = "left bottom"):
        self.labels.append((text, round(pt[0], 4), round(pt[1], 4), rot, justify))

    def global_label(
        self,
        text: str,
        pt: Tuple[float, float],
        rot: float = 0.0,
        shape: str = "bidirectional",
        justify: str = "left",
    ):
        self.global_labels.append((text, round(pt[0], 4), round(pt[1], 4), rot, shape, justify))

    def stub_label(
        self,
        sym: PlacedSymbol,
        pin_key: str,
        net_name: str,
        length: float = 5.08,
        global_lbl: bool = False,
        shape: str = "bidirectional",
    ) -> Tuple[float, float]:
        """Draw a short outward wire stub from a pin and attach a net label at its tip."""
        p = sym.pin(pin_key)
        dx, dy = sym.pin_dir_vec(pin_key)
        tip = (round(p[0] + dx * length, 4), round(p[1] + dy * length, 4))
        self.wire(p, tip)
        if dx < 0:
            rot = 180.0
            just = "right bottom"
            gjust = "right"
        elif dx > 0:
            rot = 0.0
            just = "left bottom"
            gjust = "left"
        elif dy < 0:
            rot = 90.0
            just = "left bottom"
            gjust = "left"
        else:
            rot = 270.0
            just = "left bottom"
            gjust = "left"

        if global_lbl:
            self.global_label(net_name, tip, rot=rot, shape=shape, justify=gjust)
        else:
            # Keep horizontal text readable (rot=0 for left/right labels)
            if dx < 0:
                self.label(net_name, tip, rot=0.0, justify="right bottom")
            elif dx > 0:
                self.label(net_name, tip, rot=0.0, justify="left bottom")
            else:
                self.label(net_name, tip, rot=0.0, justify="left bottom")
        return tip

    def pin_gnd(self, sym: PlacedSymbol, pin_key: str, stub_len: float = 2.54):
        """Attach a primary power:GND symbol to a pin with a clean downward stub."""
        p = sym.pin(pin_key)
        dx, dy = sym.pin_dir_vec(pin_key)
        if dy > 0:
            tip = (round(p[0], 4), round(p[1] + stub_len, 4))
            self.wire(p, tip)
            self.add_power("power:GND", tip[0], tip[1], rot=0)
        elif dx != 0:
            mid = (round(p[0] + dx * stub_len, 4), round(p[1], 4))
            tip = (mid[0], round(mid[1] + 2.54, 4))
            self.wire(p, mid)
            self.wire(mid, tip)
            self.add_power("power:GND", tip[0], tip[1], rot=0)
        else:
            # Pin points up -> route sideways then down
            mid1 = (round(p[0], 4), round(p[1] - 2.54, 4))
            mid2 = (round(p[0] + 2.54, 4), mid1[1])
            tip = (mid2[0], round(mid2[1] + 2.54, 4))
            self.wire_poly([p, mid1, mid2, tip])
            self.add_power("power:GND", tip[0], tip[1], rot=0)

    def add_pwr_flag_on_net(self, net_name: str, x: float, y: float, is_gnd: bool = False):
        """Place a PWR_FLAG attached to a named net (or power:GND) so KiCad 9 ERC has 0 power_pin_not_driven errors."""
        if is_gnd:
            self.add_power("power:GND", x, y + 2.54, rot=0)
            self.wire((x, y), (x, y + 2.54))
            self.add_power("power:PWR_FLAG", x, y, rot=0)
        else:
            self.add_power("power:PWR_FLAG", x, y, rot=0)
            self.wire((x, y), (x + 5.08, y))
            self.label(net_name, (x + 5.08, y), rot=0, justify="left bottom")

    def add_section_box(self, title: str, x1: float, y1: float, x2: float, y2: float):
        self.rects.append((round(x1, 4), round(y1, 4), round(x2, 4), round(y2, 4)))
        self.texts.append((title, round(x1 + 2.54, 4), round(y1 + 3.81, 4), 2.54, True))

    def add_note(self, text: str, x: float, y: float, size: float = 1.524, bold: bool = False):
        self.texts.append((text, round(x, 4), round(y, 4), size, bold))

    def auto_junctions(self):
        """Split wires at every interior contact point (wire endpoints, pins, labels, junctions)
        and place junctions at every T-intersection / degree>=3 wire node."""
        eps = 1e-4

        def on_segment_interior(pt, a, b):
            px, py = pt
            x1, y1 = a
            x2, y2 = b
            if abs(x1 - x2) < eps and abs(px - x1) < eps:
                lo, hi = min(y1, y2), max(y1, y2)
                return lo + eps < py < hi - eps
            if abs(y1 - y2) < eps and abs(py - y1) < eps:
                lo, hi = min(x1, x2), max(x1, x2)
                return lo + eps < px < hi - eps
            return False

        # Collect all candidate split points (wire endpoints, pins, labels, global labels, existing junctions)
        pts = set()
        for a, b in self.wires:
            pts.add(a)
            pts.add(b)
        for s in self.symbols:
            for p in s.pins:
                pts.add(s._transform_point(p.x, p.y))
        for _t, lx, ly, _r, _j in self.labels:
            pts.add((lx, ly))
        for _t, lx, ly, _r, _sh, _j in self.global_labels:
            pts.add((lx, ly))
        for j in self.junctions:
            pts.add(j)

        # Split every wire segment at all interior points that lie on it
        split_wires: List[Tuple[Tuple[float, float], Tuple[float, float]]] = []
        seen_segs = set()
        for a, b in self.wires:
            interior = [pt for pt in pts if on_segment_interior(pt, a, b)]
            if not interior:
                seg = (a, b) if a <= b else (b, a)
                if seg not in seen_segs:
                    seen_segs.add(seg)
                    split_wires.append((a, b))
            else:
                # Sort along the wire direction
                if abs(a[0] - b[0]) < eps:
                    chain = sorted([a, b] + interior, key=lambda p: p[1])
                else:
                    chain = sorted([a, b] + interior, key=lambda p: p[0])
                for u, v in zip(chain, chain[1:]):
                    if abs(u[0] - v[0]) < eps and abs(u[1] - v[1]) < eps:
                        continue
                    seg = (u, v) if u <= v else (v, u)
                    if seg not in seen_segs:
                        seen_segs.add(seg)
                        split_wires.append((u, v))

        self.wires = split_wires

        # Count wire degree at every endpoint; place a junction where >=3 wire segments meet
        # (or >=2 collinear/orthogonal wire segments + a pin at a T)
        deg: Dict[Tuple[float, float], int] = {}
        for a, b in self.wires:
            deg[a] = deg.get(a, 0) + 1
            deg[b] = deg.get(b, 0) + 1
        pin_pts = set()
        for s in self.symbols:
            for p in s.pins:
                pin_pts.add(s._transform_point(p.x, p.y))

        for pt, d in deg.items():
            if d >= 3:
                self.junction(pt)
            elif d == 2 and pt in pin_pts:
                # Check if the 2 segments are collinear (wire passing straight through a pin)
                inc = [b if a == pt else a for a, b in self.wires if a == pt or b == pt]
                if len(inc) == 2:
                    same_x = abs(inc[0][0] - pt[0]) < eps and abs(inc[1][0] - pt[0]) < eps
                    same_y = abs(inc[0][1] - pt[1]) < eps and abs(inc[1][1] - pt[1]) < eps
                    if same_x or same_y:
                        self.junction(pt)

    def _emit_symbol_instance(self, sym: PlacedSymbol) -> str:
        # Compute clean Reference and Value positions if not explicitly supplied
        xmin, ymin, xmax, ymax = sym.bbox
        # Transform corners to world coords to get world bounding box
        corners = [
            sym._transform_point(xmin, ymin),
            sym._transform_point(xmin, ymax),
            sym._transform_point(xmax, ymin),
            sym._transform_point(xmax, ymax),
        ]
        wxmin = min(c[0] for c in corners)
        wxmax = max(c[0] for c in corners)
        wymin = min(c[1] for c in corners)
        wymax = max(c[1] for c in corners)
        wxc = round((wxmin + wxmax) / 2.0, 4)

        if sym.is_power:
            rx, ry = sym.x, sym.y + 3.81
            vx, vy = sym.x, sym.y + (3.81 if sym.rot == 0 else -3.81)
            if sym.lib_id == "power:PWR_FLAG":
                vx, vy = sym.x, sym.y - 3.81
            ref_just, val_just = "", ""
        else:
            two_pin_vert = sym.lib_id in {
                "Device:R", "Device:C", "Device:C_Polarized", "Device:L",
                "Device:Thermistor_NTC", "Device:FerriteBead",
            }
            two_pin_horiz = sym.lib_id in {
                "Device:D", "Device:D_Schottky", "Device:D_Zener", "Device:D_TVS",
                "Device:LED", "Device:Crystal",
            }
            ref_just, val_just = "", ""
            if sym.ref_offset is not None:
                rx, ry = sym.x + sym.ref_offset[0], sym.y + sym.ref_offset[1]
            elif two_pin_vert and int(sym.rot) % 180 == 0:
                rx, ry = sym.x + 2.54, sym.y - 1.27
                ref_just = "\n\t\t\t\t(justify left)"
            elif two_pin_horiz and int(sym.rot) % 180 == 90:
                rx, ry = sym.x + 2.54, sym.y - 1.27
                ref_just = "\n\t\t\t\t(justify left)"
            elif (two_pin_vert and int(sym.rot) % 180 == 90) or (two_pin_horiz and int(sym.rot) % 180 == 0):
                rx, ry = sym.x, sym.y - 2.54
            else:
                rx, ry = wxc, wymin - 2.54

            if sym.val_offset is not None:
                vx, vy = sym.x + sym.val_offset[0], sym.y + sym.val_offset[1]
            elif two_pin_vert and int(sym.rot) % 180 == 0:
                vx, vy = sym.x + 2.54, sym.y + 1.27
                val_just = "\n\t\t\t\t(justify left)"
            elif two_pin_horiz and int(sym.rot) % 180 == 90:
                vx, vy = sym.x + 2.54, sym.y + 1.27
                val_just = "\n\t\t\t\t(justify left)"
            elif (two_pin_vert and int(sym.rot) % 180 == 90) or (two_pin_horiz and int(sym.rot) % 180 == 0):
                vx, vy = sym.x, sym.y + 2.54
            else:
                vx, vy = wxc, wymax + 2.54

        mirror_str = f"\n\t\t(mirror {sym.mirror})" if sym.mirror else ""
        ref_hide = "\n\t\t\t\t(hide yes)" if sym.hide_ref else ""
        val_hide = "\n\t\t\t\t(hide yes)" if sym.hide_val else ""

        pin_lines = []
        seen_nums = set()
        for p in sym.pins:
            if p.number in seen_nums:
                continue
            seen_nums.add(p.number)
            pin_lines.append(f'\t\t(pin "{p.number}"\n\t\t\t(uuid "{new_uuid()}")\n\t\t)')
        pins_block = "\n".join(pin_lines)

        return f'''\t(symbol
\t\t(lib_id "{sym.lib_id}")
\t\t(at {fmt(sym.x)} {fmt(sym.y)} {fmt(sym.rot)}){mirror_str}
\t\t(unit 1)
\t\t(exclude_from_sim no)
\t\t(in_bom {"yes" if sym.in_bom else "no"})
\t\t(on_board {"yes" if sym.on_board else "no"})
\t\t(dnp no)
\t\t(fields_autoplaced yes)
\t\t(uuid "{sym.uid}")
\t\t(property "Reference" "{sym.ref}"
\t\t\t(at {fmt(rx)} {fmt(ry)} 0)
\t\t\t(effects
\t\t\t\t(font
\t\t\t\t\t(size 1.27 1.27)
\t\t\t\t){ref_just}{ref_hide}
\t\t\t)
\t\t)
\t\t(property "Value" "{sym.value}"
\t\t\t(at {fmt(vx)} {fmt(vy)} 0)
\t\t\t(effects
\t\t\t\t(font
\t\t\t\t\t(size 1.27 1.27)
\t\t\t\t){val_just}{val_hide}
\t\t\t)
\t\t)
\t\t(property "Footprint" "{sym.footprint}"
\t\t\t(at {fmt(sym.x)} {fmt(sym.y)} 0)
\t\t\t(effects
\t\t\t\t(font
\t\t\t\t\t(size 1.27 1.27)
\t\t\t\t)
\t\t\t\t(hide yes)
\t\t\t)
\t\t)
\t\t(property "Datasheet" ""
\t\t\t(at {fmt(sym.x)} {fmt(sym.y)} 0)
\t\t\t(effects
\t\t\t\t(font
\t\t\t\t\t(size 1.27 1.27)
\t\t\t\t)
\t\t\t\t(hide yes)
\t\t\t)
\t\t)
{pins_block}
\t\t(instances
\t\t\t(project "{self.project_name}"
\t\t\t\t(path "/{self.root_uuid}"
\t\t\t\t\t(reference "{sym.ref}")
\t\t\t\t\t(unit 1)
\t\t\t\t)
\t\t\t)
\t\t)
\t)'''

    def render(self) -> str:
        self.auto_junctions()

        lib_blocks = []
        for lib_id in self.used_lib_ids:
            raw, _, _ = SymbolCache.get_symbol(lib_id)
            indented = "\n".join("\t\t" + line for line in raw.splitlines())
            lib_blocks.append(indented)
        lib_symbols_str = "\n".join(lib_blocks)

        parts = [
            f'''(kicad_sch
\t(version 20250114)
\t(generator "pcbschemagen_baseline_py")
\t(generator_version "9.0")
\t(uuid "{self.root_uuid}")
\t(paper "{self.paper}")
\t(title_block
\t\t(title "{self.title}")
\t\t(rev "{self.rev}")
\t\t(comment 1 "{self.comment1}")
\t\t(comment 2 "{self.comment2}")
\t)
\t(lib_symbols
{lib_symbols_str}
\t)'''
        ]

        # Section rectangles
        for x1, y1, x2, y2 in self.rects:
            parts.append(
                f'''\t(rectangle
\t\t(start {fmt(x1)} {fmt(y1)})
\t\t(end {fmt(x2)} {fmt(y2)})
\t\t(stroke
\t\t\t(width 0.254)
\t\t\t(type dash)
\t\t\t(color 90 120 160 1)
\t\t)
\t\t(fill
\t\t\t(type none)
\t\t)
\t\t(uuid "{new_uuid()}")
\t)'''
            )

        # Text notes
        for text, x, y, size, bold in self.texts:
            esc_text = text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")
            bold_s = " (bold yes)" if bold else ""
            parts.append(
                f'''\t(text "{esc_text}"
\t\t(exclude_from_sim no)
\t\t(at {fmt(x)} {fmt(y)} 0)
\t\t(effects
\t\t\t(font
\t\t\t\t(size {fmt(size)} {fmt(size)}){bold_s}
\t\t\t)
\t\t\t(justify left)
\t\t)
\t\t(uuid "{new_uuid()}")
\t)'''
            )

        # Junctions
        for jx, jy in self.junctions:
            parts.append(
                f'''\t(junction
\t\t(at {fmt(jx)} {fmt(jy)})
\t\t(diameter 0)
\t\t(color 0 0 0 0)
\t\t(uuid "{new_uuid()}")
\t)'''
            )

        # No-connects
        for nx, ny in self.no_connects:
            parts.append(
                f'''\t(no_connect
\t\t(at {fmt(nx)} {fmt(ny)})
\t\t(uuid "{new_uuid()}")
\t)'''
            )

        # Wires
        for (x1, y1), (x2, y2) in self.wires:
            parts.append(
                f'''\t(wire
\t\t(pts
\t\t\t(xy {fmt(x1)} {fmt(y1)}) (xy {fmt(x2)} {fmt(y2)})
\t\t)
\t\t(stroke
\t\t\t(width 0)
\t\t\t(type default)
\t\t)
\t\t(uuid "{new_uuid()}")
\t)'''
            )

        # Net labels
        for text, x, y, rot, justify in self.labels:
            parts.append(
                f'''\t(label "{text}"
\t\t(at {fmt(x)} {fmt(y)} {fmt(rot)})
\t\t(fields_autoplaced yes)
\t\t(effects
\t\t\t(font
\t\t\t\t(size 1.27 1.27)
\t\t\t)
\t\t\t(justify {justify})
\t\t)
\t\t(uuid "{new_uuid()}")
\t)'''
            )

        # Global labels
        for text, x, y, rot, shape, justify in self.global_labels:
            parts.append(
                f'''\t(global_label "{text}"
\t\t(shape {shape})
\t\t(at {fmt(x)} {fmt(y)} {fmt(rot)})
\t\t(fields_autoplaced yes)
\t\t(effects
\t\t\t(font
\t\t\t\t(size 1.27 1.27)
\t\t\t)
\t\t\t(justify {justify})
\t\t)
\t\t(uuid "{new_uuid()}")
\t\t(property "Intersheetrefs" "${{INTERSHEET_REFS}}"
\t\t\t(at {fmt(x)} {fmt(y)} 0)
\t\t\t(effects
\t\t\t\t(font
\t\t\t\t\t(size 1.27 1.27)
\t\t\t\t)
\t\t\t\t(hide yes)
\t\t\t)
\t\t)
\t)'''
            )

        # Symbols
        for sym in self.symbols:
            parts.append(self._emit_symbol_instance(sym))

        parts.append(
            f'''\t(sheet_instances
\t\t(path "/"
\t\t\t(page "1")
\t\t)
\t)
)'''
        )
        return "\n".join(parts) + "\n"

    def write(self, out_dir: str, filename: str) -> str:
        os.makedirs(out_dir, exist_ok=True)
        # Write local sym-lib-table pointing to test.kicad_sym
        slt_path = os.path.join(out_dir, "sym-lib-table")
        slt = (
            "(sym_lib_table\n"
            "  (version 7)\n"
            f'  (lib (name "test")(type "KiCad")(uri "{TEST_SYM_LIB}")(options "")(descr "PCBBench IC library"))\n'
            ")\n"
        )
        with open(slt_path, "w", encoding="utf-8") as f:
            f.write(slt)

        sch_path = os.path.join(out_dir, filename)
        with open(sch_path, "w", encoding="utf-8") as f:
            f.write(self.render())

        # Write minimal .kicad_pro so kicad-cli loads the project-local sym-lib-table
        pro_name = os.path.splitext(filename)[0] + ".kicad_pro"
        pro_path = os.path.join(out_dir, pro_name)
        with open(pro_path, "w", encoding="utf-8") as f:
            f.write('{"meta": {"filename": "' + pro_name + '", "version": 1}}\n')
        return sch_path
