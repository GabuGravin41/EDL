#!/usr/bin/env python3
"""
Baseline (NO-EDL) KiCad 9 Schematic Generator
Benchmark 1: 24V Industrial IoT Edge Gateway & Isolated Relay Controller

Extracts real KiCad 9 library symbols directly from /usr/share/kicad/symbols/,
flattens symbol inheritance (extends), computes exact transformed pin coordinates
and bounding boxes, checks for zero overlaps and title-block/border clearance,
and emits a valid KiCad 9 schematic (gateway.kicad_sch).
"""

import copy
import math
import os
import re
import uuid

OUTPUT_DIR = "/home/lindah/projects/EDL/benchmarks/v3_2_industry_ab/b1_elec_gateway/baseline"
KICAD_SYM_DIR = "/usr/share/kicad/symbols"

# Deterministic UUID generator so output is reproducible
_uuid_counter = 0


def new_uuid(tag=""):
    global _uuid_counter
    _uuid_counter += 1
    return str(uuid.uuid5(uuid.NAMESPACE_DNS, f"edl-bench-b1-baseline-{_uuid_counter}-{tag}"))


def snap(v):
    """Round coordinate cleanly to 4 decimals (KiCad standard precision)."""
    r = round(float(v), 4)
    if abs(r) < 1e-9:
        return 0.0
    return r


def fmt_num(v):
    r = snap(v)
    if abs(r - round(r)) < 1e-6:
        return str(int(round(r)))
    return f"{r:.4f}".rstrip("0").rstrip(".")


# ---------------------------------------------------------------------------
# S-expression parser & serializer for KiCad 9 .kicad_sym / .kicad_sch
# ---------------------------------------------------------------------------
def parse_sexp(text):
    tokens = re.findall(r'"(?:\\.|[^"\\])*"|\(|\)|[^\s()]+', text)
    stack = [[]]
    for t in tokens:
        if t == "(":
            lst = []
            stack[-1].append(lst)
            stack.append(lst)
        elif t == ")":
            stack.pop()
        else:
            stack[-1].append(t)
    return stack[0][0]


def unquote(s):
    if isinstance(s, str) and len(s) >= 2 and s[0] == '"' and s[-1] == '"':
        return s[1:-1]
    return s


def quote(s):
    s = str(s).replace("\\", "\\\\").replace('"', '\\"')
    return f'"{s}"'


def dump_sexp(node, indent=0):
    if not isinstance(node, list):
        return str(node)
    if not node:
        return "()"
    # Inline short leaf lists
    has_sublist = any(isinstance(x, list) for x in node)
    if not has_sublist:
        return "(" + " ".join(str(x) for x in node) + ")"
    head = str(node[0])
    # Inline small attribute lists like (at x y a), (xy x y), (size w h)
    if head in ("at", "xy", "start", "end", "center", "size", "color", "pts") and len(node) <= 8:
        if all(not isinstance(x, list) or x[0] == "xy" for x in node[1:]):
            return "(" + " ".join(dump_sexp(x, 0) for x in node) + ")"
    pad = "\t" * indent
    pad_child = "\t" * (indent + 1)
    parts = [f"({head}"]
    for ch in node[1:]:
        if isinstance(ch, list):
            parts.append("\n" + pad_child + dump_sexp(ch, indent + 1))
        else:
            parts.append(" " + str(ch))
    parts.append("\n" + pad + ")")
    return "".join(parts)


# ---------------------------------------------------------------------------
# KiCad 9 Library Loader with `extends` flattening
# ---------------------------------------------------------------------------
class LibCache:
    def __init__(self):
        self._libs = {}
        self.embedded = {}  # "Lib:Name" -> flattened sexp node
        self.pins = {}  # "Lib:Name" -> {pin_num: (name, ptype, x, y, angle)}
        self.bboxes = {}  # "Lib:Name" -> (xmin, ymin, xmax, ymax) in symbol Y-up coords

    def _load_lib(self, lib_name):
        if lib_name not in self._libs:
            path = os.path.join(KICAD_SYM_DIR, f"{lib_name}.kicad_sym")
            root = parse_sexp(open(path, "r", encoding="utf-8").read())
            syms = {}
            for item in root[1:]:
                if isinstance(item, list) and item and item[0] == "symbol":
                    syms[unquote(item[1])] = item
            self._libs[lib_name] = syms
        return self._libs[lib_name]

    def _flatten(self, lib_name, sym_name):
        syms = self._load_lib(lib_name)
        raw = copy.deepcopy(syms[sym_name])
        ext = None
        for item in raw[2:]:
            if isinstance(item, list) and item and item[0] == "extends":
                ext = unquote(item[1])
                break
        if ext is None:
            return raw

        parent = self._flatten(lib_name, ext)
        # Merge child properties over parent properties, keep parent graphics/pins
        child_props = {}
        for item in raw[2:]:
            if isinstance(item, list) and item and item[0] == "property":
                child_props[unquote(item[1])] = item

        merged = ["symbol", quote(sym_name)]
        seen_props = set()
        for item in parent[2:]:
            if isinstance(item, list) and item and item[0] == "property":
                pname = unquote(item[1])
                if pname in child_props:
                    merged.append(child_props[pname])
                else:
                    merged.append(item)
                seen_props.add(pname)
            elif isinstance(item, list) and item and item[0] == "symbol":
                sub = copy.deepcopy(item)
                sub_name = unquote(sub[1])
                # Rename prefix from ext to sym_name
                if sub_name.startswith(ext + "_"):
                    sub[1] = quote(sym_name + sub_name[len(ext):])
                merged.append(sub)
            else:
                merged.append(item)
        for pname, pnode in child_props.items():
            if pname not in seen_props:
                merged.append(pnode)
        return merged

    def ensure(self, lib_id):
        if lib_id in self.embedded:
            return
        lib_name, sym_name = lib_id.split(":", 1)
        flat = self._flatten(lib_name, sym_name)
        flat[1] = quote(lib_id)
        self.embedded[lib_id] = flat

        # Extract pins and graphic bounding box
        pins = {}
        xs, ys = [], []

        def walk(node):
            for ch in node:
                if not isinstance(ch, list) or not ch:
                    continue
                tag = ch[0]
                if tag == "pin":
                    ptype = ch[1]
                    at = [x for x in ch if isinstance(x, list) and x and x[0] == "at"][0]
                    pname = [x for x in ch if isinstance(x, list) and x and x[0] == "name"][0]
                    pnum = [x for x in ch if isinstance(x, list) and x and x[0] == "number"][0]
                    px, py, pang = float(at[1]), float(at[2]), float(at[3])
                    pins[unquote(pnum[1])] = (unquote(pname[1]), ptype, px, py, pang)
                    xs.append(px)
                    ys.append(py)
                elif tag == "rectangle":
                    for sub in ch[1:]:
                        if isinstance(sub, list) and sub and sub[0] in ("start", "end"):
                            xs.append(float(sub[1]))
                            ys.append(float(sub[2]))
                elif tag == "polyline":
                    for sub in ch[1:]:
                        if isinstance(sub, list) and sub and sub[0] == "pts":
                            for pt in sub[1:]:
                                if isinstance(pt, list) and pt and pt[0] == "xy":
                                    xs.append(float(pt[1]))
                                    ys.append(float(pt[2]))
                elif tag == "circle":
                    cx = cy = r = 0.0
                    for sub in ch[1:]:
                        if isinstance(sub, list) and sub and sub[0] == "center":
                            cx, cy = float(sub[1]), float(sub[2])
                        elif isinstance(sub, list) and sub and sub[0] == "radius":
                            r = float(sub[1])
                    xs.extend([cx - r, cx + r])
                    ys.extend([cy - r, cy + r])
                elif tag == "symbol":
                    walk(ch[2:])

        walk(flat[2:])
        self.pins[lib_id] = pins
        if xs and ys:
            self.bboxes[lib_id] = (min(xs), min(ys), max(xs), max(ys))
        else:
            self.bboxes[lib_id] = (-2.54, -2.54, 2.54, 2.54)


# ---------------------------------------------------------------------------
# Schematic Builder
# ---------------------------------------------------------------------------
class SchematicBuilder:
    def __init__(self, project="gateway", paper="A2", sheet_w=594.0, sheet_h=420.0):
        self.project = project
        self.paper = paper
        self.sheet_w = sheet_w
        self.sheet_h = sheet_h
        self.root_uuid = new_uuid("root-sheet")
        self.lib = LibCache()
        self.symbols = []  # dicts describing placed symbols
        self.wires = []  # ((x1,y1), (x2,y2))
        self.junctions = set()  # (x, y)
        self.no_connects = []  # (x, y)
        self.labels = []  # (kind, text, x, y, rot, shape)
        self.graphics = []  # raw sexp strings for rectangles/polylines/text
        self._pwr_idx = 0

    def _transform_pt(self, px, py, sx, sy, rot=0, mirror=None):
        """Convert symbol local (px, py) [Y-up] to schematic (wx, wy) [Y-down]."""
        lx, ly = float(px), float(py)
        # KiCad mirror: mirror x flips Y; mirror y flips X (applied before rotation? Let's avoid mirror or handle exact KiCad order!)
        if mirror == "x":
            ly = -ly
        elif mirror == "y":
            lx = -lx
        rad = math.radians(rot)
        c = round(math.cos(rad))
        s = round(math.sin(rad))
        rx = lx * c - ly * s
        ry = lx * s + ly * c
        return (snap(sx + rx), snap(sy - ry))

    def place(
        self,
        ref,
        lib_id,
        value,
        x,
        y,
        rot=0,
        mirror=None,
        footprint="",
        ref_offset=(0, -5.08),
        val_offset=(0, 5.08),
        is_power=False,
        in_bom=True,
        on_board=True,
        extra_props=None,
    ):
        self.lib.ensure(lib_id)
        x, y = snap(x), snap(y)
        pins_world = {}
        for pnum, (pname, ptype, px, py, pang) in self.lib.pins[lib_id].items():
            wx, wy = self._transform_pt(px, py, x, y, rot, mirror)
            pins_world[pnum] = (wx, wy, pname, ptype)

        # Compute body bbox in schematic coords
        bx0, by0, bx1, by1 = self.lib.bboxes[lib_id]
        corners = [
            self._transform_pt(bx0, by0, x, y, rot, mirror),
            self._transform_pt(bx0, by1, x, y, rot, mirror),
            self._transform_pt(bx1, by0, x, y, rot, mirror),
            self._transform_pt(bx1, by1, x, y, rot, mirror),
        ]
        sbbox = (
            min(c[0] for c in corners),
            min(c[1] for c in corners),
            max(c[0] for c in corners),
            max(c[1] for c in corners),
        )

        sym = {
            "ref": ref,
            "lib_id": lib_id,
            "value": value,
            "x": x,
            "y": y,
            "rot": rot,
            "mirror": mirror,
            "footprint": footprint,
            "ref_pos": (snap(x + ref_offset[0]), snap(y + ref_offset[1])),
            "val_pos": (snap(x + val_offset[0]), snap(y + val_offset[1])),
            "is_power": is_power,
            "in_bom": in_bom,
            "on_board": on_board,
            "uuid": new_uuid(f"sym-{ref}"),
            "pins": pins_world,
            "bbox": sbbox,
            "extra_props": extra_props or {},
        }
        self.symbols.append(sym)
        return sym

    def pwr(self, net_name, x, y, rot=0):
        self._pwr_idx += 1
        ref = f"#PWR{self._pwr_idx:03d}"
        lib_id = f"power:{net_name}"
        val_dy = -3.81 if net_name.startswith("+") else 3.81
        if rot == 180:
            val_dy = -val_dy
        return self.place(
            ref=ref,
            lib_id=lib_id,
            value=net_name,
            x=x,
            y=y,
            rot=rot,
            ref_offset=(0, 0),
            val_offset=(0, val_dy),
            is_power=True,
            in_bom=False,
            on_board=False,
        )

    def pwr_flag(self, x, y, rot=0):
        self._pwr_idx += 1
        ref = f"#FLG{self._pwr_idx:03d}"
        return self.place(
            ref=ref,
            lib_id="power:PWR_FLAG",
            value="PWR_FLAG",
            x=x,
            y=y,
            rot=rot,
            ref_offset=(0, 0),
            val_offset=(0, -3.81 if rot == 0 else 3.81),
            is_power=True,
            in_bom=False,
            on_board=False,
        )

    def pin(self, sym, pnum):
        wx, wy, _, _ = sym["pins"][str(pnum)]
        return (wx, wy)

    def wire(self, p1, p2):
        x1, y1 = snap(p1[0]), snap(p1[1])
        x2, y2 = snap(p2[0]), snap(p2[1])
        if (x1, y1) != (x2, y2):
            self.wires.append(((x1, y1), (x2, y2)))

    def wire_path(self, pts):
        for a, b in zip(pts, pts[1:]):
            self.wire(a, b)

    def wire_hv(self, p1, p2):
        """Horizontal then vertical L-wire."""
        mid = (snap(p2[0]), snap(p1[1]))
        self.wire(p1, mid)
        self.wire(mid, p2)

    def wire_vh(self, p1, p2):
        """Vertical then horizontal L-wire."""
        mid = (snap(p1[0]), snap(p2[1]))
        self.wire(p1, mid)
        self.wire(mid, p2)

    def junction(self, pt):
        self.junctions.add((snap(pt[0]), snap(pt[1])))

    def no_connect(self, pt):
        self.no_connects.append((snap(pt[0]), snap(pt[1])))

    def label(self, text, pt, rot=0):
        self.labels.append(("label", text, snap(pt[0]), snap(pt[1]), rot, None))

    def glabel(self, text, pt, rot=0, shape="bidirectional"):
        self.labels.append(("global_label", text, snap(pt[0]), snap(pt[1]), rot, shape))

    def stub_glabel(self, pt, dx, text, shape="bidirectional"):
        """Draw horizontal wire stub from pt by dx and place global label at tip."""
        tip = (snap(pt[0] + dx), snap(pt[1]))
        self.wire(pt, tip)
        rot = 0 if dx > 0 else 180
        self.glabel(text, tip, rot=rot, shape=shape)
        return tip

    def stub_label(self, pt, dx, text):
        tip = (snap(pt[0] + dx), snap(pt[1]))
        self.wire(pt, tip)
        rot = 0 if dx > 0 else 180
        self.label(text, tip, rot=rot)
        return tip

    def add_section_box(self, title, x1, y1, x2, y2, color=(40, 80, 140, 1)):
        r, g, b, a = color
        self.graphics.append(
            f'\t(rectangle (start {fmt_num(x1)} {fmt_num(y1)}) (end {fmt_num(x2)} {fmt_num(y2)})\n'
            f'\t\t(stroke (width 0.35) (type dash) (color {r} {g} {b} {a}))\n'
            f'\t\t(fill (type none))\n'
            f'\t\t(uuid "{new_uuid("rect")}")\n'
            f'\t)'
        )
        self.graphics.append(
            f'\t(text {quote(title)} (exclude_from_sim no)\n'
            f'\t\t(at {fmt_num(x1 + 2.54)} {fmt_num(y1 + 4.0)} 0)\n'
            f'\t\t(effects (font (size 2.2 2.2) (thickness 0.35) (bold yes) (color {r} {g} {b} {a})) (justify left))\n'
            f'\t\t(uuid "{new_uuid("title")}")\n'
            f'\t)'
        )

    def add_note(self, text, x, y, size=1.4, bold=False, color=(60, 60, 60, 1)):
        r, g, b, a = color
        b_str = " (bold yes)" if bold else ""
        self.graphics.append(
            f'\t(text {quote(text)} (exclude_from_sim no)\n'
            f'\t\t(at {fmt_num(x)} {fmt_num(y)} 0)\n'
            f'\t\t(effects (font (size {fmt_num(size)} {fmt_num(size)}){b_str} (color {r} {g} {b} {a})) (justify left))\n'
            f'\t\t(uuid "{new_uuid("note")}")\n'
            f'\t)'
        )

    def add_dashed_line(self, x1, y1, x2, y2, color=(180, 30, 30, 1)):
        r, g, b, a = color
        self.graphics.append(
            f'\t(polyline (pts (xy {fmt_num(x1)} {fmt_num(y1)}) (xy {fmt_num(x2)} {fmt_num(y2)}))\n'
            f'\t\t(stroke (width 0.4) (type dash_dot) (color {r} {g} {b} {a}))\n'
            f'\t\t(uuid "{new_uuid("line")}")\n'
            f'\t)'
        )

    def split_wires_at_points(self):
        """Split every orthogonal wire at all pins, labels, and other wire endpoints
        that lie strictly inside the wire segment, and deduplicate overlapping wires.
        KiCad 9 requires wires to meet at endpoints for proper T-junction & bus connectivity."""
        # Collect all candidate split points
        pts = set()
        for p1, p2 in self.wires:
            pts.add(p1)
            pts.add(p2)
        for sym in self.symbols:
            for _, (wx, wy, _, _) in sym["pins"].items():
                pts.add((wx, wy))
        for _, _, lx, ly, _, _ in self.labels:
            pts.add((lx, ly))

        new_wires = []
        seen = set()
        for p1, p2 in self.wires:
            x1, y1 = p1
            x2, y2 = p2
            if abs(y1 - y2) < 1e-6:  # horizontal
                xmin, xmax = min(x1, x2), max(x1, x2)
                xs = sorted({xmin, xmax} | {px for px, py in pts if abs(py - y1) < 1e-6 and xmin < px < xmax})
                for a, b in zip(xs, xs[1:]):
                    if b - a > 1e-6:
                        seg = ((snap(a), snap(y1)), (snap(b), snap(y1)))
                        key = tuple(sorted(seg))
                        if key not in seen:
                            seen.add(key)
                            new_wires.append(seg)
            elif abs(x1 - x2) < 1e-6:  # vertical
                ymin, ymax = min(y1, y2), max(y1, y2)
                ys = sorted({ymin, ymax} | {py for px, py in pts if abs(px - x1) < 1e-6 and ymin < py < ymax})
                for a, b in zip(ys, ys[1:]):
                    if b - a > 1e-6:
                        seg = ((snap(x1), snap(a)), (snap(x1), snap(b)))
                        key = tuple(sorted(seg))
                        if key not in seen:
                            seen.add(key)
                            new_wires.append(seg)
            else:
                key = tuple(sorted((p1, p2)))
                if key not in seen:
                    seen.add(key)
                    new_wires.append((p1, p2))
        self.wires = new_wires

    def auto_junctions(self):
        """Split wires at interior intersections first, then place KiCad junctions
        wherever >=3 wire endpoints (or >=2 wires + pin) meet cleanly."""
        self.split_wires_at_points()
        endpoint_counts = {}
        for p1, p2 in self.wires:
            endpoint_counts[p1] = endpoint_counts.get(p1, 0) + 1
            endpoint_counts[p2] = endpoint_counts.get(p2, 0) + 1
        for pt, cnt in endpoint_counts.items():
            if cnt >= 3:
                self.junctions.add(pt)

    def validate_geometry(self):
        """Check for overlapping component bodies, sheet border violations, and title block collisions."""
        errors = []
        # Usable interior inside 10mm KiCad border: [12, 582] x [12, 408]
        # Title block on A2 occupies bottom-right ~[464, 584] x [360, 410]
        real_syms = [s for s in self.symbols if not s["is_power"]]
        for s in self.symbols:
            x0, y0, x1, y1 = s["bbox"]
            if x0 < 12.0 or y0 < 12.0 or x1 > self.sheet_w - 12.0 or y1 > self.sheet_h - 12.0:
                errors.append(f"BORDER VIOLATION: {s['ref']} bbox={s['bbox']}")
            if x1 > 460.0 and y1 > 356.0:
                errors.append(f"TITLE BLOCK COLLISION: {s['ref']} bbox={s['bbox']}")

        # Check pairwise body overlap among real (non-power) components
        for i in range(len(real_syms)):
            a = real_syms[i]
            ax0, ay0, ax1, ay1 = a["bbox"]
            for j in range(i + 1, len(real_syms)):
                b = real_syms[j]
                bx0, by0, bx1, by1 = b["bbox"]
                # Strict overlap check (with small 0.5mm margin)
                if ax0 < bx1 - 0.1 and ax1 > bx0 + 0.1 and ay0 < by1 - 0.1 and ay1 > by0 + 0.1:
                    errors.append(f"BODY OVERLAP: {a['ref']} {a['bbox']} vs {b['ref']} {b['bbox']}")

        # Check all wires within border and outside title block
        for p1, p2 in self.wires:
            for px, py in (p1, p2):
                if px < 12.0 or py < 12.0 or px > self.sheet_w - 12.0 or py > self.sheet_h - 12.0:
                    errors.append(f"WIRE BORDER VIOLATION: {(px, py)}")
                if px > 460.0 and py > 356.0:
                    errors.append(f"WIRE TITLE BLOCK COLLISION: {(px, py)}")

        return errors, len(real_syms)

    def render(self):
        self.auto_junctions()
        lines = []
        lines.append('(kicad_sch')
        lines.append('\t(version 20250114)')
        lines.append('\t(generator "baseline_gateway_py")')
        lines.append('\t(generator_version "1.0")')
        lines.append(f'\t(uuid "{self.root_uuid}")')
        lines.append(f'\t(paper "{self.paper}")')
        lines.append('\t(title_block')
        lines.append('\t\t(title "24V Industrial IoT Edge Gateway & Isolated Relay Controller")')
        lines.append('\t\t(date "2026-11-26")')
        lines.append('\t\t(rev "1.0")')
        lines.append('\t\t(company "Industrial Control Systems")')
        lines.append('\t\t(comment 1 "Domains: 24V_IN -> +24V -> +5V -> +3V3 | ISO_24V / ISO_GND (3.75kVrms)")')
        lines.append('\t\t(comment 2 "MCU: STM32F103C8T6 | I2C: LM75B + 24LC16 | RS-485: SP3485EN | 0-10V AFE")')
        lines.append('\t)')

        # Embedded lib_symbols
        lines.append('\t(lib_symbols')
        for lib_id in sorted(self.lib.embedded.keys()):
            lines.append("\t\t" + dump_sexp(self.lib.embedded[lib_id], indent=2))
        lines.append('\t)')

        # Graphics (section frames, titles, isolation line, notes)
        for g in self.graphics:
            lines.append(g)

        # Junctions
        for jx, jy in sorted(self.junctions):
            lines.append(
                f'\t(junction (at {fmt_num(jx)} {fmt_num(jy)}) (diameter 0) (color 0 0 0 0) (uuid "{new_uuid("junc")}"))'
            )

        # No-connects
        for nx, ny in self.no_connects:
            lines.append(f'\t(no_connect (at {fmt_num(nx)} {fmt_num(ny)}) (uuid "{new_uuid("nc")}"))')

        # Wires
        for (x1, y1), (x2, y2) in self.wires:
            lines.append(
                f'\t(wire (pts (xy {fmt_num(x1)} {fmt_num(y1)}) (xy {fmt_num(x2)} {fmt_num(y2)}))\n'
                f'\t\t(stroke (width 0) (type default))\n'
                f'\t\t(uuid "{new_uuid("wire")}")\n'
                f'\t)'
            )

        # Labels & Global Labels
        for kind, text, lx, ly, rot, shape in self.labels:
            if kind == "label":
                just = "left" if rot == 0 else "right"
                lines.append(
                    f'\t(label {quote(text)} (at {fmt_num(lx)} {fmt_num(ly)} {rot})\n'
                    f'\t\t(effects (font (size 1.27 1.27)) (justify {just} bottom))\n'
                    f'\t\t(uuid "{new_uuid("lbl")}")\n'
                    f'\t)'
                )
            else:
                just = "left" if rot == 0 else "right"
                lines.append(
                    f'\t(global_label {quote(text)} (shape {shape}) (at {fmt_num(lx)} {fmt_num(ly)} {rot})\n'
                    f'\t\t(effects (font (size 1.27 1.27)) (justify {just}))\n'
                    f'\t\t(uuid "{new_uuid("glbl")}")\n'
                    f'\t\t(property "Intersheetrefs" "${{INTERSHEET_REFS}}" (at {fmt_num(lx)} {fmt_num(ly)} 0)\n'
                    f'\t\t\t(effects (font (size 1.27 1.27)) (hide yes))\n'
                    f'\t\t)\n'
                    f'\t)'
                )

        # Placed symbols
        for s in self.symbols:
            mir_str = f'\n\t\t(mirror {s["mirror"]})' if s["mirror"] else ""
            ref_hide = " (hide yes)" if s["is_power"] else ""
            # In KiCad 9, property angle is relative to symbol rotation (mod 180);
            # setting prop_rot = 90 for rot in (90, 270) renders all text cleanly horizontal!
            prop_rot = 90 if s["rot"] in (90, 270) else 0
            lines.append(
                f'\t(symbol\n'
                f'\t\t(lib_id {quote(s["lib_id"])})\n'
                f'\t\t(at {fmt_num(s["x"])} {fmt_num(s["y"])} {s["rot"]}){mir_str}\n'
                f'\t\t(unit 1)\n'
                f'\t\t(exclude_from_sim no)\n'
                f'\t\t(in_bom {"yes" if s["in_bom"] else "no"})\n'
                f'\t\t(on_board {"yes" if s["on_board"] else "no"})\n'
                f'\t\t(dnp no)\n'
                f'\t\t(uuid "{s["uuid"]}")\n'
                f'\t\t(property "Reference" {quote(s["ref"])} (at {fmt_num(s["ref_pos"][0])} {fmt_num(s["ref_pos"][1])} {prop_rot})\n'
                f'\t\t\t(effects (font (size 1.27 1.27)){ref_hide})\n'
                f'\t\t)\n'
                f'\t\t(property "Value" {quote(s["value"])} (at {fmt_num(s["val_pos"][0])} {fmt_num(s["val_pos"][1])} {prop_rot})\n'
                f'\t\t\t(effects (font (size 1.27 1.27)))\n'
                f'\t\t)\n'
                f'\t\t(property "Footprint" {quote(s["footprint"])} (at {fmt_num(s["x"])} {fmt_num(s["y"])} 0)\n'
                f'\t\t\t(effects (font (size 1.27 1.27)) (hide yes))\n'
                f'\t\t)\n'
                f'\t\t(property "Datasheet" "~" (at {fmt_num(s["x"])} {fmt_num(s["y"])} 0)\n'
                f'\t\t\t(effects (font (size 1.27 1.27)) (hide yes))\n'
                f'\t\t)'
            )
            for k, v in s["extra_props"].items():
                lines.append(
                    f'\t\t(property {quote(k)} {quote(v)} (at {fmt_num(s["x"])} {fmt_num(s["y"])} 0)\n'
                    f'\t\t\t(effects (font (size 1.27 1.27)) (hide yes))\n'
                    f'\t\t)'
                )
            for pnum in sorted(s["pins"].keys(), key=lambda z: (int(z) if z.isdigit() else 9999, z)):
                lines.append(f'\t\t(pin {quote(pnum)} (uuid "{new_uuid("pin")}"))')
            lines.append(
                f'\t\t(instances\n'
                f'\t\t\t(project {quote(self.project)}\n'
                f'\t\t\t\t(path "/{self.root_uuid}"\n'
                f'\t\t\t\t\t(reference {quote(s["ref"])})\n'
                f'\t\t\t\t\t(unit 1)\n'
                f'\t\t\t\t)\n'
                f'\t\t\t)\n'
                f'\t\t)'
            )
            lines.append('\t)')

        lines.append('\t(sheet_instances')
        lines.append('\t\t(path "/" (page "1"))')
        lines.append('\t)')
        lines.append(')')
        return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# Build the 5-Stage Industrial IoT Edge Gateway & Isolated Relay Controller
# ---------------------------------------------------------------------------
def build_gateway():
    sch = SchematicBuilder(project="gateway", paper="A2", sheet_w=594.0, sheet_h=420.0)

    # =========================================================================
    # STAGE 1: 24V Industrial Power Input, Surge Protection & 24V -> 5V Buck
    # Box: X [20.32, 294.64], Y [20.32, 154.94]
    # =========================================================================
    sch.add_section_box(
        "STAGE 1: 24V INDUSTRIAL POWER INPUT, SURGE PROTECTION & 24V -> 5V BUCK CONVERTER",
        20.32, 20.32, 294.64, 154.94,
    )

    # J_PWR: 24V DC Screw Terminal (rot=180 so pins point RIGHT: pin 1 top-right, pin 2 bottom-right)
    # Screw_Terminal_01x02 local pins: 1=(-5.08, 0), 2=(-5.08, -2.54)
    # At rot=180: pin 1 = (+5.08, 0), pin 2 = (+5.08, -2.54) -> wait:
    # Let's check: px=-5.08, py=-2.54, rot=180 -> rx=+5.08, ry=+2.54 -> wy = sy - 2.54 (ABOVE pin 1!).
    # So at rot=180: pin 2 is at (sx+5.08, sy-2.54) and pin 1 is at (sx+5.08, sy).
    # Better: use rot=180 and connect pin 1 (bottom) = GND, pin 2 (top) = 24V_IN, OR mirror y so pin 1 is top!
    # Actually at rot=180, pin 2 is top and pin 1 is bottom, or even cleaner: use rot=0 with wires going left then up/down? No, rot=180 is super clean: pin 2 = 24V_IN (top), pin 1 = GND (bottom)!
    # Let's make pin 1 = 24V_IN, pin 2 = GND cleanly by placing J_PWR with mirror="y" (or rot=180 with clean routing).
    # Actually, let's avoid mirror so we don't have any KiCad mirror-rotation ambiguity; with rot=180, pin 2 is top and pin 1 is bottom, OR let's place J_PWR at x=35.56, y=68.58, rot=180:
    # pin 1=(40.64, 68.58), pin 2=(40.64, 66.04). Let's connect pin 1 (24V_IN) and pin 2 (GND)? Even better: rot=180, pin 2 = 24V_IN? Spec says "(J_PWR, 2-pin: 24V_IN, GND)".
    # With rot=0, pins exit LEFT: pin 1=(sx-5.08, sy), pin 2=(sx-5.08, sy+2.54). We can place J_PWR at rot=180 and cross nothing if pin 1 is 24V_IN going right at y=68.58 and pin 2 goes up/left? No, easiest: place J_PWR at rot=0 at x=43.18, y=68.58 -> pin 1 at (38.10, 68.58) goes left to 30.48, up to 55.88, then right to F1! Wait, that loops back.
    # Or use mirror="y", rot=0: in KiCad, (mirror y) mirrors horizontally (X -> -X), keeping Y intact! Let's check: pin 1=(-5.08, 0) -> (+5.08, 0), pin 2=(-5.08, -2.54) -> (+5.08, -2.54) -> wy = sy + 2.54 (BELOW pin 1!). We will verify with kicad-cli netlist & SVG!
    j_pwr = sch.place(
        "J_PWR", "Connector:Screw_Terminal_01x02", "24V_IN_TERM",
        33.02, 58.42, rot=0, mirror="y",
        footprint="TerminalBlock_Phoenix:TerminalBlock_Phoenix_MKDS-1,5-2-5.08_1x02_P5.08mm_Horizontal",
        ref_offset=(0, -6.35), val_offset=(0, 8.89),
    )

    # Main top 24V power bus Y = 58.42, GND rail Y = 114.30
    Y_24V = 58.42
    Y_GND1 = 114.30

    # F1: Resettable PTC Polyfuse (1.5A 30V), horizontal (rot=90: pin1 left, pin2 right)
    f1 = sch.place(
        "F1", "Device:Polyfuse", "1.5A 30V PTC",
        53.34, Y_24V, rot=90,
        footprint="Fuse:Fuse_1812_4532Metric",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )
    # D_REV: Reverse-polarity Schottky diode (SS34, 40V 3A).
    # Device:D_Schottky: pin 1=K (-3.81, 0), pin 2=A (+3.81, 0).
    # At rot=180: pin 2=A is LEFT (-3.81, 0), pin 1=K is RIGHT (+3.81, 0)! Forward conducts left -> right!
    d_rev = sch.place(
        "D_REV", "Device:D_Schottky", "SS34 40V/3A",
        73.66, Y_24V, rot=180,
        footprint="Diode_SMD:D_SMA",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )

    # Shunt protection & Pi EMI filter components between Y_24V (58.42) and Y_GND1 (114.30)
    # D_TVS: 28V TVS surge diode (SMBJ28A) vertical (rot=90: pin1=(0, +3.81) in Y-down? Wait:
    # Device:D_TVS: pin 1=(-3.81, 0), pin 2=(+3.81, 0).
    # At rot=90: rx = -py = 0, ry = px -> pin 1 ry=-3.81 -> wy = sy + 3.81 (BOTTOM), pin 2 ry=+3.81 -> wy = sy - 3.81 (TOP).
    d_tvs = sch.place(
        "D_TVS", "Device:D_TVS", "SMBJ28A 28V",
        91.44, 86.36, rot=90,
        footprint="Diode_SMD:D_SMB",
        ref_offset=(-5.08, -2.54), val_offset=(-5.08, 2.54),
    )
    # C_EMI1: 100uF 50V bulk electrolytic cap (>= 35V)
    c_emi1 = sch.place(
        "C_EMI1", "Device:C_Polarized", "100uF 50V",
        109.22, 86.36, rot=0,
        footprint="Capacitor_SMD:CP_Elec_8x10.5",
        ref_offset=(5.08, -2.54), val_offset=(5.08, 2.54),
        extra_props={"Voltage": "50V"},
    )
    # L_EMI: 10uH 2A EMI Pi-filter inductor, horizontal (rot=90)
    l_emi = sch.place(
        "L_EMI", "Device:L", "10uH 2A EMI",
        124.46, Y_24V, rot=90,
        footprint="Inductor_SMD:L_6.3x6.3_H3",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )
    # C_EMI2: 47uF 50V Pi-filter output bulk cap (>= 35V)
    c_emi2 = sch.place(
        "C_EMI2", "Device:C_Polarized", "47uF 50V",
        139.70, 86.36, rot=0,
        footprint="Capacitor_SMD:CP_Elec_6.3x7.7",
        ref_offset=(5.08, -2.54), val_offset=(5.08, 2.54),
        extra_props={"Voltage": "50V"},
    )
    # C_IN1: 100nF 50V ceramic decoupling cap for buck VIN (>= 35V)
    c_in1 = sch.place(
        "C_IN1", "Device:C", "100nF 50V",
        157.48, 86.36, rot=0,
        footprint="Capacitor_SMD:C_0805_2012Metric",
        ref_offset=(4.445, -2.54), val_offset=(4.445, 2.54),
        extra_props={"Voltage": "50V"},
    )

    # Wire Stage 1 input -> F1 -> D_REV -> D_TVS / C_EMI1 -> L_EMI -> C_EMI2 / C_IN1 -> +24V
    sch.wire(sch.pin(j_pwr, 1), sch.pin(f1, 1))
    sch.label("24V_IN", (43.18, Y_24V))
    sch.wire(sch.pin(f1, 2), sch.pin(d_rev, 2))
    sch.label("24V_FUSED", (63.50, Y_24V))
    sch.wire(sch.pin(d_rev, 1), sch.pin(l_emi, 1))
    sch.label("24V_PROT", (96.52, Y_24V))

    # Connect D_TVS & C_EMI1 to 24V_PROT and GND rail
    sch.wire((91.44, Y_24V), sch.pin(d_tvs, 2))
    sch.wire(sch.pin(d_tvs, 1), (91.44, Y_GND1))
    sch.wire((109.22, Y_24V), sch.pin(c_emi1, 1))
    sch.wire(sch.pin(c_emi1, 2), (109.22, Y_GND1))

    # Connect J_PWR pin 2 (GND) to GND rail
    p_jpwr2 = sch.pin(j_pwr, 2)
    sch.wire_path([p_jpwr2, (43.18, p_jpwr2[1]), (43.18, Y_GND1), (109.22, Y_GND1)])

    # Filtered +24V rail from L_EMI pin 2 -> U_BUCK VIN
    # U_BUCK: LM2576S-ADJ centered at (185.42, 60.96)
    # LM2576S-ADJ local pins:
    #   1:VIN (-12.7, +2.54) -> world (172.72, 58.42) = exact Y_24V!
    #   5:~ON/OFF (-12.7, -2.54) -> world (172.72, 63.50)
    #   3:GND (0, -7.62) -> world (185.42, 68.58)
    #   4:FB (+12.7, +2.54) -> world (198.12, 58.42)
    #   2:OUT (+12.7, -2.54) -> world (198.12, 63.50)
    u_buck = sch.place(
        "U_BUCK", "Regulator_Switching:LM2576S-ADJ", "LM2576S-ADJ (24V->5V 3A)",
        185.42, 60.96, rot=0,
        footprint="Package_TO_SOT_SMD:TO-263-5_TabPin3",
        ref_offset=(0, -10.16), val_offset=(0, -13.97),
    )

    sch.wire(sch.pin(l_emi, 2), sch.pin(u_buck, 1))
    # +24V power symbol + PWR_FLAG on filtered +24V rail
    sch.pwr("+24V", 149.86, 48.26)
    sch.wire((149.86, 48.26), (149.86, Y_24V))
    sch.pwr_flag(165.10, 48.26)
    sch.wire((165.10, 48.26), (165.10, Y_24V))

    sch.wire((139.70, Y_24V), sch.pin(c_emi2, 1))
    sch.wire(sch.pin(c_emi2, 2), (139.70, Y_GND1))
    sch.wire((157.48, Y_24V), sch.pin(c_in1, 1))
    sch.wire(sch.pin(c_in1, 2), (157.48, Y_GND1))

    # U_BUCK ~ON/OFF (pin 5) and GND (pin 3) -> GND rail
    p_onoff = sch.pin(u_buck, 5)
    sch.wire_path([p_onoff, (167.64, p_onoff[1]), (167.64, Y_GND1)])
    sch.wire(sch.pin(u_buck, 3), (185.42, Y_GND1))

    # D_CATCH: SS34 catch diode from BUCK_SW (K=top) to GND (A=bottom)
    # Device:D_Schottky pin1=K(-3.81,0), pin2=A(+3.81,0).
    # At rot=270: rx = py = 0, ry = -px -> pin1(K) ry=+3.81 -> wy=sy-3.81 (TOP!), pin2(A) ry=-3.81 -> wy=sy+3.81 (BOTTOM!)
    d_catch = sch.place(
        "D_CATCH", "Device:D_Schottky", "SS34 40V/3A",
        208.28, 86.36, rot=270,
        footprint="Diode_SMD:D_SMA",
        ref_offset=(-5.08, -2.54), val_offset=(-5.08, 2.54),
    )

    # L_BUCK: 33uH 3A switching inductor, horizontal at Y = 63.50 (matches U_BUCK OUT!)
    l_buck = sch.place(
        "L_BUCK", "Device:L", "33uH 3A",
        223.52, 63.50, rot=90,
        footprint="Inductor_SMD:L_12x12mm_H6mm",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )

    sch.wire(sch.pin(u_buck, 2), sch.pin(l_buck, 1))
    sch.label("BUCK_SW", (205.74, 63.50))
    sch.wire((208.28, 63.50), sch.pin(d_catch, 1))
    sch.wire(sch.pin(d_catch, 2), (208.28, Y_GND1))

    # Feedback divider R_FB1 (30.9k 1%) & R_FB2 (10k 1%) -> Vout = 1.23*(1+30.9/10) = 5.03V
    r_fb1 = sch.place(
        "R_FB1", "Device:R", "30.9k 1%",
        243.84, 78.74, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(4.445, -1.905), val_offset=(4.445, 1.905),
    )
    r_fb2 = sch.place(
        "R_FB2", "Device:R", "10k 1%",
        243.84, 99.06, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(4.445, -1.905), val_offset=(4.445, 1.905),
    )
    # Output bulk & ceramic capacitors on +5V (>= 10V)
    c_out1 = sch.place(
        "C_OUT1", "Device:C_Polarized", "220uF 16V",
        264.16, 86.36, rot=0,
        footprint="Capacitor_SMD:CP_Elec_8x10.5",
        ref_offset=(5.08, -2.54), val_offset=(5.08, 2.54),
        extra_props={"Voltage": "16V"},
    )
    c_out2 = sch.place(
        "C_OUT2", "Device:C", "100nF 25V",
        281.94, 86.36, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(4.445, -2.54), val_offset=(4.445, 2.54),
        extra_props={"Voltage": "25V"},
    )

    # Wire +5V output bus at Y = 63.50
    sch.wire(sch.pin(l_buck, 2), (281.94, 63.50))
    sch.pwr("+5V", 256.54, 48.26)
    sch.wire((256.54, 48.26), (256.54, 63.50))
    sch.pwr_flag(271.78, 48.26)
    sch.wire((271.78, 48.26), (271.78, 63.50))

    sch.wire((243.84, 63.50), sch.pin(r_fb1, 1))
    sch.wire(sch.pin(r_fb1, 2), sch.pin(r_fb2, 1))
    sch.wire(sch.pin(r_fb2, 2), (243.84, Y_GND1))

    # Connect U_BUCK FB pin 4 (198.12, 58.42) cleanly over the top to midpoint of R_FB1/R_FB2 (243.84, 88.90)
    # Actually route FB right from (198.12, 58.42) -> (236.22, 58.42) -> (236.22, 88.90) -> (243.84, 88.90)
    # Wait: (236.22, 63.50) crosses +5V wire! Instead, use net label BUCK_FB at both ends (zero crossing!).
    p_fb = sch.pin(u_buck, 4)
    sch.stub_label(p_fb, 10.16, "BUCK_FB")
    sch.stub_label((243.84, 88.90), -10.16, "BUCK_FB")

    sch.wire((264.16, 63.50), sch.pin(c_out1, 1))
    sch.wire(sch.pin(c_out1, 2), (264.16, Y_GND1))
    sch.wire((281.94, 63.50), sch.pin(c_out2, 1))
    sch.wire(sch.pin(c_out2, 2), (281.94, Y_GND1))

    # Continuous Stage 1 GND bus + GND symbol + PWR_FLAG
    sch.wire((109.22, Y_GND1), (281.94, Y_GND1))
    sch.pwr("GND", 185.42, 127.00)
    sch.wire((185.42, Y_GND1), (185.42, 127.00))
    sch.pwr_flag(167.64, 127.00, rot=180)
    sch.wire((167.64, Y_GND1), (167.64, 127.00))

    sch.add_note(
        "Design Notes: Vout = 1.23V * (1 + 30.9k/10k) = 5.03V | All 24V input caps rated 50V (>=35V) | Output caps rated 16V-25V (>=10V)",
        25.40, 144.78, size=1.4,
    )

    # =========================================================================
    # STAGE 2: 5V -> 3.3V Low-Noise LDO & Power Indicator
    # Box: X [304.80, 568.96], Y [20.32, 154.94]
    # =========================================================================
    sch.add_section_box(
        "STAGE 2: 5V -> 3.3V LOW-NOISE LDO REGULATOR & POWER INDICATOR",
        304.80, 20.32, 568.96, 154.94,
    )

    Y_LDO = 68.58
    Y_GND2 = 114.30

    # +5V input to C_LDO1 & U_LDO
    sch.pwr("+5V", 327.66, 53.34)
    sch.wire((327.66, 53.34), (327.66, Y_LDO))

    c_ldo1 = sch.place(
        "C_LDO1", "Device:C", "10uF 16V",
        353.06, 91.44, rot=0,
        footprint="Capacitor_SMD:C_0805_2012Metric",
        ref_offset=(4.445, -2.54), val_offset=(4.445, 2.54),
        extra_props={"Voltage": "16V"},
    )
    u_ldo = sch.place(
        "U_LDO", "Regulator_Linear:AMS1117-3.3", "AMS1117-3.3 (5V->3.3V 1A)",
        393.70, Y_LDO, rot=0,
        footprint="Package_TO_SOT_SMD:SOT-223-3_TabPin2",
        ref_offset=(0, -6.35), val_offset=(0, -10.16),
    )
    c_ldo2 = sch.place(
        "C_LDO2", "Device:C", "10uF 16V",
        434.34, 91.44, rot=0,
        footprint="Capacitor_SMD:C_0805_2012Metric",
        ref_offset=(4.445, -2.54), val_offset=(4.445, 2.54),
        extra_props={"Voltage": "16V"},
    )

    sch.wire((327.66, Y_LDO), sch.pin(u_ldo, 3))
    sch.wire((353.06, Y_LDO), sch.pin(c_ldo1, 1))
    sch.wire(sch.pin(c_ldo1, 2), (353.06, Y_GND2))
    sch.wire(sch.pin(u_ldo, 1), (393.70, Y_GND2))

    # +3V3 output bus to C_LDO2 and Power LED indicator
    sch.wire(sch.pin(u_ldo, 2), (485.14, Y_LDO))
    sch.pwr("+3V3", 459.74, 53.34)
    sch.wire((459.74, 53.34), (459.74, Y_LDO))
    sch.wire((434.34, Y_LDO), sch.pin(c_ldo2, 1))
    sch.wire(sch.pin(c_ldo2, 2), (434.34, Y_GND2))

    # Power-on Indicator LED: R_PWR (330R 0.25W) + D_PWR (Green LED)
    # Device:LED pins: 1=K(-3.81, 0), 2=A(+3.81, 0).
    # At rot=90: pin 2(A) ry=+3.81 -> wy = sy - 3.81 (TOP!), pin 1(K) ry=-3.81 -> wy = sy + 3.81 (BOTTOM!)
    r_pwr = sch.place(
        "R_PWR", "Device:R", "330R 0.25W",
        485.14, 81.28, rot=0,
        footprint="Resistor_SMD:R_0805_2012Metric",
        ref_offset=(5.08, -1.905), val_offset=(5.08, 1.905),
        extra_props={"Power": "250mW"},
    )
    d_pwr = sch.place(
        "D_PWR", "Device:LED", "Green LED",
        485.14, 101.60, rot=90,
        footprint="LED_SMD:LED_0805_2012Metric",
        ref_offset=(5.08, -1.905), val_offset=(5.08, 1.905),
    )
    sch.wire((485.14, Y_LDO), sch.pin(r_pwr, 1))
    sch.wire(sch.pin(r_pwr, 2), sch.pin(d_pwr, 2))
    sch.wire(sch.pin(d_pwr, 1), (485.14, Y_GND2))

    # Stage 2 GND bus
    sch.wire((353.06, Y_GND2), (485.14, Y_GND2))
    sch.pwr("GND", 393.70, 127.00)
    sch.wire((393.70, Y_GND2), (393.70, 127.00))

    sch.add_note(
        "Power LED Sizing: If = (3.3V - 2.0V) / 330R = 3.94 mA (within 2-10 mA) | P = 5.1 mW << 80% of 250 mW",
        312.42, 144.78, size=1.4,
    )

    # =========================================================================
    # STAGE 3: MCU Core (STM32F103C8Tx), Clock, Reset, SWD & UART Console
    # Box: X [20.32, 264.16], Y [165.10, 396.24]
    # =========================================================================
    sch.add_section_box(
        "STAGE 3: MCU CORE (STM32F103C8T6), 8MHz CLOCK, RESET, SWD & UART CONSOLE",
        20.32, 165.10, 264.16, 396.24,
    )

    # Place STM32F103C8Tx centered at (154.94, 287.02)
    # Body X: [137.16, 172.72], Y: [246.38, 327.66]
    u_mcu = sch.place(
        "U_MCU", "MCU_ST_STM32F1:STM32F103C8Tx", "STM32F103C8T6",
        154.94, 287.02, rot=0,
        footprint="Package_QFP:LQFP-48_7x7mm_P0.5mm",
        ref_offset=(16.51, -44.45), val_offset=(16.51, -40.64),
    )

    # Connect ALL MCU Top Power Pins (1:VBAT, 24:VDD, 36:VDD, 48:VDD, 9:VDDA) to +3V3 rail at Y=238.76
    Y_MCU_VDD = 238.76
    for p in (1, 24, 36, 48, 9):
        pt = sch.pin(u_mcu, p)
        sch.wire(pt, (pt[0], Y_MCU_VDD))
    sch.wire((sch.pin(u_mcu, 1)[0], Y_MCU_VDD), (sch.pin(u_mcu, 9)[0], Y_MCU_VDD))
    sch.pwr("+3V3", 154.94, 226.06)
    sch.wire((154.94, 226.06), (154.94, Y_MCU_VDD))

    # Connect ALL MCU Bottom GND Pins (23/35/47:VSS stacked at x=154.94, 8:VSSA at x=157.48) to GND rail at Y=335.28
    Y_MCU_VSS = 335.28
    for p in (23, 8):
        pt = sch.pin(u_mcu, p)
        sch.wire(pt, (pt[0], Y_MCU_VSS))
    sch.wire((sch.pin(u_mcu, 23)[0], Y_MCU_VSS), (sch.pin(u_mcu, 8)[0], Y_MCU_VSS))
    sch.pwr("GND", 154.94, 345.44)
    sch.wire((154.94, Y_MCU_VSS), (154.94, 345.44))

    # Local MCU Decoupling Bank (4 capacitors: 3x 100nF + 1x 10uF) at top-left of Stage 3
    Y_DEC_TOP = 185.42
    Y_DEC_BOT = 213.36
    dec_specs = [
        ("C_MCU1", "100nF 16V", 35.56),
        ("C_MCU2", "100nF 16V", 53.34),
        ("C_MCU3", "100nF 16V", 71.12),
        ("C_MCU4", "10uF 16V", 88.90),
    ]
    for cname, cval, cx in dec_specs:
        cap = sch.place(
            cname, "Device:C", cval,
            cx, 198.12, rot=0,
            footprint="Capacitor_SMD:C_0603_1608Metric",
            ref_offset=(4.445, -2.54), val_offset=(4.445, 2.54),
        )
        sch.wire((cx, Y_DEC_TOP), sch.pin(cap, 1))
        sch.wire(sch.pin(cap, 2), (cx, Y_DEC_BOT))
    sch.wire((35.56, Y_DEC_TOP), (88.90, Y_DEC_TOP))
    sch.wire((35.56, Y_DEC_BOT), (88.90, Y_DEC_BOT))
    sch.pwr("+3V3", 62.23 if False else 53.34, 177.80)
    sch.wire((53.34, 177.80), (53.34, Y_DEC_TOP))
    sch.pwr("GND", 53.34, 220.98)
    sch.wire((53.34, Y_DEC_BOT), (53.34, 220.98))

    # External RC Reset Network + Tactile Reset Button connected to MCU pin 7 (NRST)
    # pin 7 (NRST) is at (137.16, 254.00)
    p_nrst = sch.pin(u_mcu, 7)
    r_rst = sch.place(
        "R_RST", "Device:R", "10k",
        106.68, 241.30, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(4.445, -1.905), val_offset=(4.445, 1.905),
    )
    c_rst = sch.place(
        "C_RST", "Device:C", "100nF",
        91.44, 266.70, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(4.445, -2.54), val_offset=(4.445, 2.54),
    )
    sw_rst = sch.place(
        "SW_RST", "Switch:SW_Push", "RESET",
        63.50, 254.00, rot=0,
        footprint="Button_Switch_SMD:SW_SPST_TL3342",
        ref_offset=(0, -5.08), val_offset=(0, 5.08),
    )
    # Wire NRST bus at y=254.00 from SW_RST pin 2 -> C_RST -> R_RST -> U_MCU NRST
    sch.wire(sch.pin(sw_rst, 2), p_nrst)
    sch.wire(sch.pin(r_rst, 2), (106.68, 254.00))
    sch.pwr("+3V3", 106.68, 228.60)
    sch.wire((106.68, 228.60), sch.pin(r_rst, 1))
    sch.wire((91.44, 254.00), sch.pin(c_rst, 1))
    sch.wire(sch.pin(c_rst, 2), (91.44, 279.40))
    # SW_RST pin 1 to GND
    p_sw1 = sch.pin(sw_rst, 1)
    sch.wire_path([p_sw1, (53.34, p_sw1[1]), (53.34, 279.40), (91.44, 279.40)])
    sch.pwr("GND", 73.66, 287.02)
    sch.wire((73.66, 279.40), (73.66, 287.02))
    # Also global label NRST for J_SWD
    sch.glabel("NRST", (78.74, 254.00), rot=180, shape="input")

    # BOOT0 pull-down resistor R_BOOT0 (10k) on pin 44 (137.16, 259.08)
    p_boot0 = sch.pin(u_mcu, 44)
    r_boot0 = sch.place(
        "R_BOOT0", "Device:R", "10k",
        119.38, 271.78, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(-5.08, -1.905), val_offset=(-5.08, 1.905),
    )
    sch.wire_hv(p_boot0, sch.pin(r_boot0, 1))
    sch.pwr("GND", 119.38, 284.48)
    sch.wire(sch.pin(r_boot0, 2), (119.38, 284.48))

    # External 8MHz Crystal Oscillator (Y1 + C_XTAL1 + C_XTAL2) near PD0(pin 5) / PD1(pin 6)
    # PD0=(137.16, 264.16), PD1=(137.16, 266.70) -> route cleanly left/down to Y=312.42
    p_pd0 = sch.pin(u_mcu, 5)
    p_pd1 = sch.pin(u_mcu, 6)
    sch.stub_label(p_pd0, -10.16, "OSC_IN")
    sch.stub_label(p_pd1, -10.16, "OSC_OUT")

    # Place Crystal block cleanly at bottom-left of Stage 3 (X: 40-100, Y: 310-365)
    y1 = sch.place(
        "Y1", "Device:Crystal", "8MHz",
        71.12, 322.58, rot=0,
        footprint="Crystal:Crystal_SMD_HC49-SD",
        ref_offset=(0, -5.08), val_offset=(0, 5.08),
    )
    c_xtal1 = sch.place(
        "C_XTAL1", "Device:C", "20pF",
        55.88, 342.90, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(-5.08, -2.54), val_offset=(-5.08, 2.54),
    )
    c_xtal2 = sch.place(
        "C_XTAL2", "Device:C", "20pF",
        86.36, 342.90, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(5.08, -2.54), val_offset=(5.08, 2.54),
    )
    sch.wire_path([sch.pin(y1, 1), (55.88, 322.58), sch.pin(c_xtal1, 1)])
    sch.label("OSC_IN", (55.88, 322.58), rot=180)
    sch.wire_path([sch.pin(y1, 2), (86.36, 322.58), sch.pin(c_xtal2, 1)])
    sch.label("OSC_OUT", (86.36, 322.58), rot=0)
    sch.wire(sch.pin(c_xtal1, 2), (55.88, 355.60))
    sch.wire(sch.pin(c_xtal2, 2), (86.36, 355.60))
    sch.wire((55.88, 355.60), (86.36, 355.60))
    sch.pwr("GND", 71.12, 365.76)
    sch.wire((71.12, 355.60), (71.12, 365.76))

    # Connect active MCU signal pins to clean global labels
    mcu_left_signals = {
        42: ("I2C_SCL", "output"),
        43: ("I2C_SDA", "bidirectional"),
    }
    for pnum, (net, shape) in mcu_left_signals.items():
        sch.stub_glabel(sch.pin(u_mcu, pnum), -15.24, net, shape=shape)

    mcu_right_signals = {
        10: ("ADC_AIN", "input"),
        11: ("RS485_DE", "output"),
        12: ("RS485_TX", "output"),
        13: ("RS485_RX", "input"),
        14: ("RLY_CTRL", "output"),
        30: ("UART_TX", "output"),
        31: ("UART_RX", "input"),
        34: ("SWDIO", "bidirectional"),
        37: ("SWCLK", "input"),
    }
    for pnum, (net, shape) in mcu_right_signals.items():
        sch.stub_glabel(sch.pin(u_mcu, pnum), 15.24, net, shape=shape)

    # Mark all remaining unused STM32 pins with explicit KiCad no_connect
    used_mcu_pins = {1, 24, 36, 48, 9, 23, 35, 47, 8, 7, 44, 5, 6} | set(mcu_left_signals) | set(mcu_right_signals)
    for pnum in u_mcu["pins"]:
        if int(pnum) not in used_mcu_pins:
            sch.no_connect(sch.pin(u_mcu, pnum))

    # SWD Debug Header (J_SWD: 3V3, SWDIO, SWCLK, NRST, GND) at right of Stage 3
    j_swd = sch.place(
        "J_SWD", "Connector_Generic:Conn_01x05", "SWD_DEBUG",
        246.38, 223.52, rot=0,
        footprint="Connector_PinHeader_2.54mm:PinHeader_1x05_P2.54mm_Vertical",
        ref_offset=(0, -10.16), val_offset=(0, 10.16),
    )
    # Conn_01x05 pins exit LEFT: 1=top (+5.08 -> y-5.08), 2, 3, 4, 5=bottom
    p_swd1 = sch.pin(j_swd, 1)
    sch.wire_path([p_swd1, (226.06, p_swd1[1]), (226.06, 208.28)])
    sch.pwr("+3V3", 226.06, 208.28)
    sch.stub_glabel(sch.pin(j_swd, 2), -15.24, "SWDIO", shape="bidirectional")
    sch.stub_glabel(sch.pin(j_swd, 3), -15.24, "SWCLK", shape="output")
    sch.stub_glabel(sch.pin(j_swd, 4), -15.24, "NRST", shape="output")
    p_swd5 = sch.pin(j_swd, 5)
    sch.wire_path([p_swd5, (226.06, p_swd5[1]), (226.06, 238.76)])
    sch.pwr("GND", 226.06, 238.76)

    # UART Console Header (J_UART: TX, RX, GND) below J_SWD
    j_uart = sch.place(
        "J_UART", "Connector_Generic:Conn_01x03", "UART_CONSOLE",
        246.38, 355.60, rot=0,
        footprint="Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical",
        ref_offset=(0, -7.62), val_offset=(0, 7.62),
    )
    sch.stub_glabel(sch.pin(j_uart, 1), -15.24, "UART_TX", shape="input")
    sch.stub_glabel(sch.pin(j_uart, 2), -15.24, "UART_RX", shape="output")
    p_u3 = sch.pin(j_uart, 3)
    sch.wire_path([p_u3, (226.06, p_u3[1]), (226.06, 370.84)])
    sch.pwr("GND", 226.06, 370.84)

    # =========================================================================
    # STAGE 4: Dual Industrial Peripherals (I2C + RS-485) & 0-10V AFE
    # Box: X [274.32, 568.96], Y [165.10, 279.40]
    # =========================================================================
    sch.add_section_box(
        "STAGE 4: I2C SENSOR/EEPROM BUS, RS-485 TRANSCEIVER & 0-10V ANALOG FRONT-END",
        274.32, 165.10, 568.96, 279.40,
    )

    # --- 4A: Shared I2C Bus (U_TEMP=LM75B, U_EEPROM=24LC16, R_I2C1/2=4.7k) ---
    # R_I2C1 & R_I2C2 pull-ups at X=287.02, 299.72
    r_i2c1 = sch.place(
        "R_I2C1", "Device:R", "4.7k",
        287.02, 195.58, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(-4.445, -1.905), val_offset=(-4.445, 1.905),
    )
    r_i2c2 = sch.place(
        "R_I2C2", "Device:R", "4.7k",
        299.72, 195.58, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(4.445, -1.905), val_offset=(4.445, 1.905),
    )
    sch.wire(sch.pin(r_i2c1, 1), (287.02, 185.42))
    sch.wire(sch.pin(r_i2c2, 1), (299.72, 185.42))
    sch.wire((287.02, 185.42), (299.72, 185.42))
    sch.pwr("+3V3", 292.10, 177.80)
    sch.wire((292.10, 177.80), (292.10, 185.42))
    sch.wire(sch.pin(r_i2c1, 2), (287.02, 205.74))
    sch.glabel("I2C_SDA", (287.02, 205.74), rot=180, shape="bidirectional")
    sch.wire(sch.pin(r_i2c2, 2), (299.72, 210.82))
    sch.glabel("I2C_SCL", (299.72, 210.82), rot=180, shape="bidirectional")

    # U_TEMP: LM75C I2C Temperature Sensor centered at (332.74, 205.74)
    u_temp = sch.place(
        "U_TEMP", "Sensor_Temperature:LM75C", "LM75C (0x48)",
        332.74, 205.74, rot=0,
        footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
        ref_offset=(-8.89, -7.62), val_offset=(8.89, -7.62),
    )
    c_temp = sch.place(
        "C_TEMP", "Device:C", "100nF",
        314.96, 205.74, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(-4.445, -2.54), val_offset=(-4.445, 2.54),
    )
    # LM75B A0(7), A1(6), A2(5) -> GND; +Vs(8) -> +3V3; GND(4) -> GND; SDA(1)->I2C_SDA; SCL(2)->I2C_SCL; O.S.(3)->NC
    for p in (7, 6, 5):
        pt = sch.pin(u_temp, p)
        sch.wire(pt, (320.04, pt[1]))
    sch.wire((320.04, sch.pin(u_temp, 7)[1]), (320.04, 218.44))
    sch.wire((320.04, 218.44), (332.74, 218.44))
    sch.wire(sch.pin(u_temp, 4), (332.74, 223.52))
    sch.pwr("GND", 332.74, 223.52)
    sch.wire(sch.pin(u_temp, 8), (332.74, 190.50))
    sch.pwr("+3V3", 332.74, 190.50)
    # C_TEMP local decoupling
    sch.wire(sch.pin(c_temp, 1), (314.96, 193.04))
    sch.wire((314.96, 193.04), (332.74, 193.04))
    sch.wire(sch.pin(c_temp, 2), (314.96, 218.44))
    sch.wire((314.96, 218.44), (320.04, 218.44))
    # SDA/SCL/OS
    sch.stub_glabel(sch.pin(u_temp, 1), 10.16, "I2C_SDA", shape="bidirectional")
    sch.stub_glabel(sch.pin(u_temp, 2), 10.16, "I2C_SCL", shape="input")
    sch.no_connect(sch.pin(u_temp, 3))

    # U_EEPROM: 24LC16 I2C EEPROM centered at (332.74, 254.00)
    u_eeprom = sch.place(
        "U_EEPROM", "Memory_EEPROM:24LC16", "24LC16B 16Kb",
        332.74, 254.00, rot=0,
        footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
        ref_offset=(-8.89, -7.62), val_offset=(8.89, -7.62),
    )
    c_eeprom = sch.place(
        "C_EEPROM", "Device:C", "100nF",
        314.96, 254.00, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(-4.445, -2.54), val_offset=(-4.445, 2.54),
    )
    for p in (1, 2, 3):
        pt = sch.pin(u_eeprom, p)
        sch.wire(pt, (320.04, pt[1]))
    sch.wire((320.04, sch.pin(u_eeprom, 1)[1]), (320.04, 266.70))
    sch.wire((320.04, 266.70), (332.74, 266.70))
    sch.wire(sch.pin(u_eeprom, 4), (332.74, 271.78))
    sch.pwr("GND", 332.74, 271.78)
    # WP (pin 7) -> GND (write enabled)
    p_wp = sch.pin(u_eeprom, 7)
    sch.wire_path([p_wp, (345.44, p_wp[1]), (345.44, 266.70), (332.74, 266.70)])
    # VCC (pin 8) -> +3V3 & C_EEPROM
    sch.wire(sch.pin(u_eeprom, 8), (332.74, 238.76))
    sch.pwr("+3V3", 332.74, 238.76)
    sch.wire(sch.pin(c_eeprom, 1), (314.96, 241.30))
    sch.wire((314.96, 241.30), (332.74, 241.30))
    sch.wire(sch.pin(c_eeprom, 2), (314.96, 266.70))
    sch.wire((314.96, 266.70), (320.04, 266.70))
    sch.stub_glabel(sch.pin(u_eeprom, 5), 10.16, "I2C_SDA", shape="bidirectional")
    sch.stub_glabel(sch.pin(u_eeprom, 6), 10.16, "I2C_SCL", shape="input")

    # --- 4B: RS-485 Industrial Transceiver (U_485, C_485, R_TERM, J_RS485) ---
    # U_485: SP3485EN centered at (408.94, 223.52)
    u_485 = sch.place(
        "U_485", "Interface_UART:SP3485EN", "SP3485EN (3.3V RS-485)",
        408.94, 223.52, rot=0,
        footprint="Package_SO:SOIC-8_3.9x4.9mm_P1.27mm",
        ref_offset=(0, -17.78), val_offset=(0, -21.59),
    )
    c_485 = sch.place(
        "C_485", "Device:C", "100nF",
        388.62, 198.12, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(-4.445, -2.54), val_offset=(-4.445, 2.54),
    )
    # Power & decoupling on U_485: VCC(8) at (408.94, 208.28), GND(5) at (408.94, 238.76)
    sch.wire(sch.pin(u_485, 8), (408.94, 187.96))
    sch.pwr("+3V3", 408.94, 187.96)
    sch.wire(sch.pin(c_485, 1), (388.62, 190.50))
    sch.wire((388.62, 190.50), (408.94, 190.50))
    sch.wire(sch.pin(c_485, 2), (388.62, 205.74))
    sch.pwr("GND", 388.62, 205.74)
    sch.wire(sch.pin(u_485, 5), (408.94, 248.92))
    sch.pwr("GND", 408.94, 248.92)

    # UART/DE signals on left of U_485:
    # 1:RO -> RS485_RX, 2:~RE & 3:DE tied together -> RS485_DE, 4:DI -> RS485_TX
    sch.stub_glabel(sch.pin(u_485, 1), -12.70, "RS485_RX", shape="output")
    p_re = sch.pin(u_485, 2)
    p_de = sch.pin(u_485, 3)
    sch.wire(p_re, (393.70, p_re[1]))
    sch.wire(p_de, (393.70, p_de[1]))
    sch.wire((393.70, p_re[1]), (393.70, p_de[1]))
    sch.stub_glabel((393.70, p_de[1]), -7.62, "RS485_DE", shape="input")
    sch.stub_glabel(sch.pin(u_485, 4), -12.70, "RS485_TX", shape="input")

    # RS-485 bus lines A(pin 6: 419.10, 215.90), B(pin 7: 419.10, 220.98) -> R_TERM(120R) -> J_RS485
    # Place R_TERM vertically at X=436.88, centered between A & B -> Wait: A is 215.90, B is 220.98 (5.08mm apart),
    # while Device:R pin span is 7.62mm! So route A to Y=215.90 and B down to Y=228.60 (12.7mm apart) for clean R_TERM!
    r_term = sch.place(
        "R_TERM", "Device:R", "120R",
        436.88, 222.25 if False else 223.52, rot=0,
        footprint="Resistor_SMD:R_0805_2012Metric",
        ref_offset=(4.445, -1.905), val_offset=(4.445, 1.905),
    )
    j_rs485 = sch.place(
        "J_RS485", "Connector:Screw_Terminal_01x03", "RS485_BUS",
        462.28, 228.60, rot=0,
        footprint="TerminalBlock_Phoenix:TerminalBlock_Phoenix_MKDS-1,5-3-5.08_1x03_P5.08mm_Horizontal",
        ref_offset=(0, -7.62), val_offset=(0, 8.89),
    )
    # J_RS485 pins exit LEFT: 1=(457.20, 226.06), 2=(457.20, 228.60), 3=(457.20, 231.14)
    p_485a = sch.pin(u_485, 6)
    p_485b = sch.pin(u_485, 7)
    sch.wire_path([p_485a, (449.58, p_485a[1]), (449.58, sch.pin(j_rs485, 1)[1]), sch.pin(j_rs485, 1)])
    sch.label("RS485_A", (426.72, p_485a[1]))
    sch.wire((436.88, p_485a[1]), sch.pin(r_term, 1))

    sch.wire_path([p_485b, (426.72, p_485b[1]), (426.72, 228.60), sch.pin(j_rs485, 2)])
    sch.label("RS485_B", (429.26, 228.60))
    sch.wire(sch.pin(r_term, 2), (436.88, 228.60))

    p_j485_gnd = sch.pin(j_rs485, 3)
    sch.wire_path([p_j485_gnd, (449.58, p_j485_gnd[1]), (449.58, 241.30)])
    sch.pwr("GND", 449.58, 241.30)

    # --- 4C: 0-10V Industrial Analog Input Front-End (J_AIN, R_DIV1/2, R_AAF, C_AAF, D_CLAMP) ---
    # Place neatly at X: [478, 560], Y: [185, 270] (all labels strictly inside Stage 4 box!)
    Y_AIN = 213.36
    Y_AIN_GND = 259.08

    j_ain = sch.place(
        "J_AIN", "Connector:Screw_Terminal_01x02", "0-10V_IN",
        480.06, Y_AIN, rot=0, mirror="y",
        footprint="TerminalBlock_Phoenix:TerminalBlock_Phoenix_MKDS-1,5-2-5.08_1x02_P5.08mm_Horizontal",
        ref_offset=(0, -6.35), val_offset=(0, 8.89),
    )
    r_div1 = sch.place(
        "R_DIV1", "Device:R", "23.2k 0.1%",
        497.84, Y_AIN, rot=90,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )
    r_div2 = sch.place(
        "R_DIV2", "Device:R", "10k 0.1%",
        510.54, 236.22, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(-5.08, -1.905), val_offset=(-5.08, 1.905),
    )
    r_aaf = sch.place(
        "R_AAF", "Device:R", "1k 1%",
        523.24, Y_AIN, rot=90,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )
    c_aaf = sch.place(
        "C_AAF", "Device:C", "10nF 25V C0G",
        533.40, 236.22, rot=0,
        footprint="Capacitor_SMD:C_0603_1608Metric",
        ref_offset=(-6.35, -2.54), val_offset=(-6.35, 2.54),
    )
    d_clamp = sch.place(
        "D_CLAMP", "Diode:BAT54S", "BAT54S",
        543.56, 223.52, rot=90,
        footprint="Package_TO_SOT_SMD:SOT-23",
        ref_offset=(8.89, -7.62), val_offset=(8.89, 7.62),
    )

    # Wire 0-10V AFE
    sch.wire(sch.pin(j_ain, 1), sch.pin(r_div1, 1))
    sch.label("AIN_10V", (487.68, Y_AIN))
    sch.wire(sch.pin(r_div1, 2), sch.pin(r_aaf, 1))
    sch.wire((510.54, Y_AIN), sch.pin(r_div2, 1))
    sch.wire(sch.pin(r_div2, 2), (510.54, Y_AIN_GND))

    sch.wire(sch.pin(r_aaf, 2), (533.40, Y_AIN))
    sch.wire((533.40, Y_AIN), sch.pin(c_aaf, 1))
    sch.wire(sch.pin(c_aaf, 2), (533.40, Y_AIN_GND))

    # Connect ADC_AIN to D_CLAMP COM(pin 3: 548.64, 223.52) and global label ADC_AIN inside box
    sch.wire_path([(533.40, Y_AIN), (553.72, Y_AIN), (553.72, 223.52), sch.pin(d_clamp, 3)])
    sch.glabel("ADC_AIN", (553.72, Y_AIN), rot=0, shape="output")

    # D_CLAMP K(pin 2, top) -> +3V3, A(pin 1, bottom) -> GND
    sch.wire(sch.pin(d_clamp, 2), (543.56, 200.66))
    sch.pwr("+3V3", 543.56, 200.66)
    sch.wire(sch.pin(d_clamp, 1), (543.56, Y_AIN_GND))

    # J_AIN GND pin 2 -> Y_AIN_GND rail -> GND
    p_jain2 = sch.pin(j_ain, 2)
    sch.wire_path([p_jain2, (487.68, p_jain2[1]), (487.68, Y_AIN_GND), (543.56, Y_AIN_GND)])
    sch.pwr("GND", 510.54, 266.70)
    sch.wire((510.54, Y_AIN_GND), (510.54, 266.70))

    sch.add_note(
        "0-10V AFE: 10V->3.01V (23.2k/10k) | fc=1.99kHz | BAT54S clamp",
        474.98, 274.32, size=1.2,
    )

    # =========================================================================
    # STAGE 5: Galvanically Isolated 24V Relay / Solenoid Driver Output
    # Box: X [274.32, 452.12], Y [289.56, 396.24]  (strictly left of A2 title block!)
    # =========================================================================
    sch.add_section_box(
        "STAGE 5: GALVANICALLY ISOLATED 24V RELAY DRIVER (ISO_24V / ISO_GND)",
        274.32, 289.56, 452.12, 396.24,
        color=(140, 40, 40, 1),
    )

    # Isolation Barrier Dashed Line right down the middle of U_OPTO (X = 325.12)
    sch.add_dashed_line(325.12, 297.18, 325.12, 391.16)
    sch.add_note("ISOLATION BARRIER", 312.42, 388.62, size=1.3, bold=True, color=(180, 30, 30, 1))

    # --- Logic Side (3.3V / GND) ---
    r_opto = sch.place(
        "R_OPTO", "Device:R", "330R",
        304.80, 330.20, rot=90,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )
    # PC817 centered at (325.12, 332.74):
    # pin 1(LED A): (317.50, 330.20), pin 2(LED K): (317.50, 335.28)
    # pin 4(Col): (332.74, 330.20), pin 3(Emit): (332.74, 335.28)
    u_opto = sch.place(
        "U_OPTO", "Isolator:PC817", "PC817",
        325.12, 332.74, rot=0,
        footprint="Package_DIP:DIP-4_W7.62mm",
        ref_offset=(0, -6.35), val_offset=(0, -10.16),
    )
    sch.stub_glabel(sch.pin(r_opto, 1), -10.16, "RLY_CTRL", shape="input")
    sch.wire(sch.pin(r_opto, 2), sch.pin(u_opto, 1))
    p_opto2 = sch.pin(u_opto, 2)
    sch.wire_path([p_opto2, (309.88, p_opto2[1]), (309.88, 353.06)])
    sch.pwr("GND", 309.88, 353.06)

    # --- Isolated 24V Side (ISO_24V / ISO_GND — NO GND or +24V symbols!) ---
    Y_ISO24 = 307.34
    Y_ISOGND = 375.92

    # J_ISO_PWR: Isolated 24V input screw terminal
    j_iso_pwr = sch.place(
        "J_ISO_PWR", "Connector:Screw_Terminal_01x02", "ISO_24V_PWR",
        345.44, 307.34, rot=0, mirror="y",
        footprint="TerminalBlock_Phoenix:TerminalBlock_Phoenix_MKDS-1,5-2-5.08_1x02_P5.08mm_Horizontal",
        ref_offset=(-5.08, -6.35), val_offset=(12.70, -6.35),
    )
    # R_OPTO_PU: 10k collector current-limit/divider resistor (limits MOSFET Vgs to ~12V safe!)
    r_opto_pu = sch.place(
        "R_OPTO_PU", "Device:R", "10k",
        340.36, 322.58, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(6.35, -1.905), val_offset=(6.35, 1.905),
    )
    # R_GATE: 1k gate resistor
    r_gate = sch.place(
        "R_GATE", "Device:R", "1k",
        353.06, 353.06, rot=90,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(0, -4.445), val_offset=(0, 4.445),
    )
    # R_PD: 10k gate pull-down to ISO_GND
    r_pd = sch.place(
        "R_PD", "Device:R", "10k",
        363.22, 365.76, rot=0,
        footprint="Resistor_SMD:R_0603_1608Metric",
        ref_offset=(-5.08, -1.905), val_offset=(-5.08, 1.905),
    )
    # Q_RLY: 2N7000 N-MOSFET low-side relay driver
    # 2N7000 local pins: 2:G(-5.08, 0), 3:D(+2.54, +5.08), 1:S(+2.54, -5.08)
    # Centered at (375.92, 353.06): G=(370.84, 353.06), D=(378.46, 347.98), S=(378.46, 358.14)
    q_rly = sch.place(
        "Q_RLY", "Transistor_FET:2N7000", "2N7000",
        375.92, 353.06, rot=0,
        footprint="Package_TO_SOT_SMD:SOT-23",
        ref_offset=(10.16, -2.54), val_offset=(10.16, 2.54),
    )

    # D_FLY: 1N4007 flyback diode antiparallel across relay coil (K=top->ISO_24V, A=bottom->RLY_COIL_N)
    # Device:D rot=270 -> pin 1(K) TOP, pin 2(A) BOTTOM
    d_fly = sch.place(
        "D_FLY", "Device:D", "1N4007",
        396.24, 327.66, rot=270,
        footprint="Diode_SMD:D_SMA",
        ref_offset=(-6.35, -2.54), val_offset=(-6.35, 2.54),
    )

    # K1: 24V SPDT Relay (G5LE-1) centered at (414.02, 327.66)
    # G5LE-1 local pins:
    #   Coil: 5(-5.08, +7.62)->(408.94, 320.04), 2(-5.08, -7.62)->(408.94, 335.28)
    #   Contacts: 1:COM(+5.08, -7.62)->(419.10, 335.28), 4:NC(+2.54, +7.62)->(416.56, 320.04), 3:NO(+7.62, +7.62)->(421.64, 320.04)
    k1 = sch.place(
        "K1", "Relay:G5LE-1", "G5LE-1 24VDC",
        414.02, 327.66, rot=0,
        footprint="Relay_THT:Relay_SPDT_Omron-G5LE-1",
        ref_offset=(-1.27, 29.21), val_offset=(-1.27, 33.02),
    )

    # J_OUT: Isolated Relay Contacts Terminal Block (COM, NO, NC)
    j_out = sch.place(
        "J_OUT", "Connector:Screw_Terminal_01x03", "RELAY_OUT",
        441.96, 353.06, rot=0,
        footprint="TerminalBlock_Phoenix:TerminalBlock_Phoenix_MKDS-1,5-3-5.08_1x03_P5.08mm_Horizontal",
        ref_offset=(0, -7.62), val_offset=(0, 8.89),
    )

    # Wire Isolated 24V bus (ISO_24V) & Isolated Ground bus (ISO_GND)
    # J_ISO_PWR pin 1 = ISO_24V (350.52, 307.34), pin 2 = ISO_GND (350.52, 309.88)
    # Extend ISO_24V left to 340.36 for R_OPTO_PU and right to 408.94 for K1 coil!
    sch.wire((340.36, Y_ISO24), (408.94, Y_ISO24))
    sch.label("ISO_24V", (368.30, Y_ISO24))
    # Route J_ISO_PWR pin 2 cleanly down to ISO_GND rail at Y=375.92 (along X=330.20? Wait: X=330.20 is near U_OPTO!)
    # Instead, label ISO_GND at J_ISO_PWR pin 2 and on the ISO_GND bus! Zero crossing!
    sch.stub_label(sch.pin(j_iso_pwr, 2), 7.62, "ISO_GND")

    # R_OPTO_PU from ISO_24V -> U_OPTO collector(pin 4)
    sch.wire((340.36, Y_ISO24), sch.pin(r_opto_pu, 1))
    sch.wire_vh(sch.pin(r_opto_pu, 2), sch.pin(u_opto, 4))

    # U_OPTO emitter(pin 3) -> R_GATE -> RLY_GATE -> Q_RLY G & R_PD
    sch.wire_vh(sch.pin(u_opto, 3), sch.pin(r_gate, 1))
    sch.wire(sch.pin(r_gate, 2), sch.pin(q_rly, 2))
    sch.label("RLY_GATE", (363.22, 353.06))
    sch.wire((363.22, 353.06), sch.pin(r_pd, 1))
    sch.wire(sch.pin(r_pd, 2), (363.22, Y_ISOGND))

    # Q_RLY Source(pin 1) -> ISO_GND bus
    sch.wire(sch.pin(q_rly, 1), (378.46, Y_ISOGND))
    sch.wire((363.22, Y_ISOGND), (378.46, Y_ISOGND))
    sch.label("ISO_GND", (368.30, Y_ISOGND))

    # Relay coil & flyback diode: ISO_24V -> K1(5) & D_FLY(K,1); RLY_COIL_N -> K1(2), D_FLY(A,2), Q_RLY Drain(3)
    sch.wire((396.24, Y_ISO24), sch.pin(d_fly, 1))
    sch.wire((408.94, Y_ISO24), sch.pin(k1, 5))

    Y_COIL_N = 342.90
    sch.wire(sch.pin(k1, 2), (408.94, Y_COIL_N))
    sch.wire(sch.pin(d_fly, 2), (396.24, Y_COIL_N))
    sch.wire(sch.pin(q_rly, 3), (378.46, Y_COIL_N))
    sch.wire((378.46, Y_COIL_N), (408.94, Y_COIL_N))
    sch.label("RLY_COIL_N", (383.54, Y_COIL_N))

    # Relay contacts K1 -> J_OUT:
    # J_OUT pins exit LEFT: 1=COM(436.88, 350.52), 2=NO(436.88, 353.06), 3=NC(436.88, 355.60)
    # K1 contacts: 1=COM(419.10, 335.28), 3=NO(421.64, 320.04), 4=NC(416.56, 320.04)
    # Route cleanly with zero overlap!
    sch.wire_path([sch.pin(k1, 1), (419.10, 350.52), sch.pin(j_out, 1)])
    sch.label("RLY_COM", (421.64, 350.52))

    # NO (pin 3) goes up to 312.42, right to 429.26, down to 353.06 -> J_OUT pin 2
    sch.wire_path([sch.pin(k1, 3), (421.64, 314.96), (429.26, 314.96), (429.26, 353.06), sch.pin(j_out, 2)])
    sch.label("RLY_NO", (429.26, 314.96))

    # NC (pin 4) goes up to 309.88, right to 434.34, down to 355.60 -> J_OUT pin 3
    sch.wire_path([sch.pin(k1, 4), (416.56, 309.88), (434.34, 309.88), (434.34, 355.60), sch.pin(j_out, 3)])
    sch.label("RLY_NC", (434.34, 309.88))

    return sch


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    sch = build_gateway()
    errors, real_count = sch.validate_geometry()
    print(f"Real components placed: {real_count}")
    print(f"Total symbols (incl power/flags): {len(sch.symbols)}")
    print(f"Total wire segments: {len(sch.wires)}")
    if errors:
        print(f"GEOMETRY VALIDATION ISSUES ({len(errors)}):")
        for e in errors:
            print("  !", e)
    else:
        print("GEOMETRY VALIDATION: 0 overlaps, 0 border violations, 0 title-block collisions!")

    out_path = os.path.join(OUTPUT_DIR, "gateway.kicad_sch")
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(sch.render())
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
