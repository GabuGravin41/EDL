#!/usr/bin/env python3
"""
KiCad 9 Hierarchical Schematic Builder (Pure Python, No EDL).
Loads real symbols from /usr/share/kicad/symbols/*.kicad_sym, flattens `extends`,
tracks exact pin coordinates across rotations/mirrors, and emits valid KiCad 9
`.kicad_sch` files with full hierarchical instance paths.
"""

import copy
import math
import os
import re
import uuid
from typing import Dict, List, Optional, Tuple

KICAD_SYM_DIR = "/usr/share/kicad/symbols"


def gen_uuid(seed: str = "") -> str:
    if seed:
        return str(uuid.uuid5(uuid.NAMESPACE_DNS, f"plc-b7-baseline-{seed}"))
    return str(uuid.uuid4())


def fmt(v: float) -> str:
    r = round(v, 4)
    if abs(r) < 1e-9:
        r = 0.0
    s = f"{r:.4f}".rstrip("0").rstrip(".")
    return s if s else "0"


def esc(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def parse_sexp(text: str):
    tokens = re.findall(r'"(?:\\.|[^"\\])*"|\(|\)|[^\s()]+', text)
    stack = [[]]
    for tok in tokens:
        if tok == "(":
            lst = []
            stack[-1].append(lst)
            stack.append(lst)
        elif tok == ")":
            stack.pop()
        else:
            stack[-1].append(tok)
    return stack[0][0]


def serialize_sexp(node, indent: int = 0) -> str:
    if not isinstance(node, list):
        return str(node)
    if not node:
        return "()"
    # Keep short leaf lists on one line
    has_sublist = any(isinstance(x, list) for x in node[1:])
    if not has_sublist and len(node) <= 8:
        return "(" + " ".join(str(x) for x in node) + ")"
    pad = "\t" * indent
    pad1 = "\t" * (indent + 1)
    parts = [f"({node[0]}"]
    inline = []
    i = 1
    while i < len(node) and not isinstance(node[i], list):
        inline.append(str(node[i]))
        i += 1
    if inline:
        parts[0] += " " + " ".join(inline)
    for item in node[i:]:
        if isinstance(item, list):
            parts.append(f"\n{pad1}{serialize_sexp(item, indent + 1)}")
        else:
            parts.append(f" {item}")
    parts.append(f"\n{pad})")
    return "".join(parts)


class SymbolLibraryCache:
    """Loads KiCad 9 symbols from system libraries and custom local libraries."""

    def __init__(self, extra_libs: Optional[Dict[str, str]] = None):
        self.extra_libs = extra_libs or {}
        self._raw_libs: Dict[str, Dict[str, list]] = {}
        self._flat_cache: Dict[str, Tuple[list, Dict[int, Dict[str, dict]]]] = {}

    def _load_lib(self, lib_name: str) -> Dict[str, list]:
        if lib_name in self._raw_libs:
            return self._raw_libs[lib_name]
        if lib_name in self.extra_libs:
            path = self.extra_libs[lib_name]
        else:
            path = os.path.join(KICAD_SYM_DIR, f"{lib_name}.kicad_sym")
        with open(path, "r", encoding="utf-8") as f:
            root = parse_sexp(f.read())
        syms = {}
        for item in root[1:]:
            if isinstance(item, list) and item[0] == "symbol":
                sname = item[1].strip('"')
                syms[sname] = item
        self._raw_libs[lib_name] = syms
        return syms

    def get_symbol(self, lib_id: str) -> Tuple[list, Dict[int, Dict[str, dict]]]:
        """
        Returns (flattened_sexp_for_lib_symbols, pins_by_unit).
        pins_by_unit[unit_idx][pin_num] = {
            'num': str, 'name': str, 'etype': str,
            'x': float, 'y': float, 'angle': float, 'length': float
        }
        Note: pins in unit 0 apply to all units; we store them under unit 1 too if no unit>0.
        """
        if lib_id in self._flat_cache:
            return self._flat_cache[lib_id]

        lib_name, sym_name = lib_id.split(":", 1)
        syms = self._load_lib(lib_name)
        if sym_name not in syms:
            raise KeyError(f"Symbol {lib_id} not found")

        # Resolve extends chain (root base -> ... -> derived)
        chain = []
        cur_name = sym_name
        while cur_name is not None:
            s = syms[cur_name]
            chain.append((cur_name, s))
            ext = None
            for sub in s[2:]:
                if isinstance(sub, list) and sub[0] == "extends":
                    ext = sub[1].strip('"')
                    break
            cur_name = ext
        chain.reverse()

        # Merge chain: attributes, properties, sub-symbols
        flags = []
        flag_keys = set()
        props: Dict[str, list] = {}
        prop_order: List[str] = []
        sub_syms: List[list] = []
        embedded_fonts = None

        for cname, s in chain:
            for sub in s[2:]:
                if not isinstance(sub, list):
                    continue
                tag = sub[0]
                if tag == "extends":
                    continue
                elif tag == "embedded_fonts":
                    embedded_fonts = copy.deepcopy(sub)
                elif tag == "property":
                    if len(sub) >= 3 and sub[1] == "private":
                        pkey = ("private", sub[2].strip('"'))
                    else:
                        pkey = sub[1].strip('"')
                    if pkey not in props:
                        prop_order.append(pkey)
                    props[pkey] = copy.deepcopy(sub)
                elif tag == "symbol":
                    # Rename sub-symbol prefix to target sym_name
                    sc = copy.deepcopy(sub)
                    old_sub = sc[1].strip('"')
                    if old_sub.startswith(cname + "_"):
                        suffix = old_sub[len(cname):]
                        sc[1] = f'"{sym_name}{suffix}"'
                    sub_syms.append(sc)
                else:
                    if tag not in flag_keys:
                        flag_keys.add(tag)
                        flags.append(copy.deepcopy(sub))
                    else:
                        # Override earlier flag
                        for idx, fl in enumerate(flags):
                            if fl[0] == tag:
                                flags[idx] = copy.deepcopy(sub)
                                break

        flat = ["symbol", f'"{lib_id}"']
        flat.extend(flags)
        for pname in prop_order:
            flat.append(props[pname])
        flat.extend(sub_syms)
        if embedded_fonts is not None:
            flat.append(embedded_fonts)

        # Extract pin definitions by unit
        pins_by_unit: Dict[int, Dict[str, dict]] = {}
        unit0_pins: Dict[str, dict] = {}
        max_unit = 1
        for sc in sub_syms:
            sname_u = sc[1].strip('"')
            m = re.search(r"_(\d+)_(\d+)$", sname_u)
            unit_num = int(m.group(1)) if m else 1
            if unit_num > max_unit:
                max_unit = unit_num
            for el in sc[2:]:
                if isinstance(el, list) and el[0] == "pin":
                    etype = el[1]
                    at = [x for x in el if isinstance(x, list) and x[0] == "at"][0]
                    leng = [x for x in el if isinstance(x, list) and x[0] == "length"][0]
                    pname_el = [x for x in el if isinstance(x, list) and x[0] == "name"][0]
                    pnum_el = [x for x in el if isinstance(x, list) and x[0] == "number"][0]
                    pinfo = {
                        "num": pnum_el[1].strip('"'),
                        "name": pname_el[1].strip('"'),
                        "etype": etype,
                        "x": float(at[1]),
                        "y": float(at[2]),
                        "angle": float(at[3]),
                        "length": float(leng[1]),
                    }
                    if unit_num == 0:
                        unit0_pins[pinfo["num"]] = pinfo
                    else:
                        pins_by_unit.setdefault(unit_num, {})[pinfo["num"]] = pinfo

        for u in range(1, max_unit + 1):
            d = pins_by_unit.setdefault(u, {})
            for k, v in unit0_pins.items():
                d.setdefault(k, v)

        self._flat_cache[lib_id] = (flat, pins_by_unit)
        return flat, pins_by_unit


def transform_pin(px: float, py: float, cx: float, cy: float, rot: float = 0, mirror: str = "") -> Tuple[float, float]:
    """Transform symbol pin (px, py) [Y-up] to schematic (sx, sy) [Y-down]."""
    x, y = px, py
    if mirror == "x":
        y = -y
    elif mirror == "y":
        x = -x
    r = int(rot) % 360
    if r == 0:
        rx, ry = x, y
    elif r == 90:
        rx, ry = -y, x
    elif r == 180:
        rx, ry = -x, -y
    elif r == 270:
        rx, ry = y, -x
    else:
        rad = math.radians(rot)
        rx = x * math.cos(rad) - y * math.sin(rad)
        ry = x * math.sin(rad) + y * math.cos(rad)
    return (round(cx + rx, 4), round(cy - ry, 4))


def create_custom_power_lib(path: str):
    """Creates a valid KiCad 9 symbol library `plc_power.kicad_sym` for isolated domains."""
    def gnd_sym(name: str, style: str = "bar") -> str:
        if style == "bar":
            # Distinct isolated ground bar (triple-bar chassis/iso style)
            gfx = """\t\t\t(polyline
\t\t\t\t(pts (xy 0 0) (xy 0 -1.27))
\t\t\t\t(stroke (width 0) (type default))
\t\t\t\t(fill (type none))
\t\t\t)
\t\t\t(polyline
\t\t\t\t(pts (xy -1.905 -1.27) (xy 1.905 -1.27))
\t\t\t\t(stroke (width 0.254) (type default))
\t\t\t\t(fill (type none))
\t\t\t)
\t\t\t(polyline
\t\t\t\t(pts (xy -1.27 -2.032) (xy 1.27 -2.032))
\t\t\t\t(stroke (width 0.254) (type default))
\t\t\t\t(fill (type none))
\t\t\t)
\t\t\t(polyline
\t\t\t\t(pts (xy -0.635 -2.794) (xy 0.635 -2.794))
\t\t\t\t(stroke (width 0.254) (type default))
\t\t\t\t(fill (type none))
\t\t\t)"""
        else:
            # Diamond/hollow isolated bus ground
            gfx = """\t\t\t(polyline
\t\t\t\t(pts (xy 0 0) (xy 0 -1.27))
\t\t\t\t(stroke (width 0) (type default))
\t\t\t\t(fill (type none))
\t\t\t)
\t\t\t(polyline
\t\t\t\t(pts (xy -1.524 -1.27) (xy 1.524 -1.27) (xy 0 -3.048) (xy -1.524 -1.27))
\t\t\t\t(stroke (width 0.254) (type default))
\t\t\t\t(fill (type none))
\t\t\t)"""
        return f"""\t(symbol "{name}"
\t\t(power)
\t\t(pin_numbers (hide yes))
\t\t(pin_names (offset 0) (hide yes))
\t\t(exclude_from_sim no)
\t\t(in_bom yes)
\t\t(on_board yes)
\t\t(property "Reference" "#PWR" (at 0 -6.35 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(property "Value" "{name}" (at 0 -4.572 0) (effects (font (size 1.27 1.27))))
\t\t(property "Footprint" "" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(property "Datasheet" "" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(property "Description" "Isolated ground symbol {name}" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(symbol "{name}_0_1"
{gfx}
\t\t)
\t\t(symbol "{name}_1_1"
\t\t\t(pin power_in line (at 0 0 270) (length 0)
\t\t\t\t(name "~" (effects (font (size 1.27 1.27))))
\t\t\t\t(number "1" (effects (font (size 1.27 1.27))))
\t\t\t)
\t\t)
\t\t(embedded_fonts no)
\t)"""

    def vcc_sym(name: str) -> str:
        return f"""\t(symbol "{name}"
\t\t(power)
\t\t(pin_numbers (hide yes))
\t\t(pin_names (offset 0) (hide yes))
\t\t(exclude_from_sim no)
\t\t(in_bom yes)
\t\t(on_board yes)
\t\t(property "Reference" "#PWR" (at 0 -3.81 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(property "Value" "{name}" (at 0 3.556 0) (effects (font (size 1.27 1.27))))
\t\t(property "Footprint" "" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(property "Datasheet" "" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(property "Description" "Power rail symbol {name}" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
\t\t(symbol "{name}_0_1"
\t\t\t(polyline (pts (xy -0.762 1.27) (xy 0 2.54)) (stroke (width 0) (type default)) (fill (type none)))
\t\t\t(polyline (pts (xy 0 2.54) (xy 0.762 1.27)) (stroke (width 0) (type default)) (fill (type none)))
\t\t\t(polyline (pts (xy 0 0) (xy 0 2.54)) (stroke (width 0) (type default)) (fill (type none)))
\t\t)
\t\t(symbol "{name}_1_1"
\t\t\t(pin power_in line (at 0 0 90) (length 0)
\t\t\t\t(name "~" (effects (font (size 1.27 1.27))))
\t\t\t\t(number "1" (effects (font (size 1.27 1.27))))
\t\t\t)
\t\t)
\t\t(embedded_fonts no)
\t)"""

    content = (
        '(kicad_symbol_lib\n'
        '\t(version 20241209)\n'
        '\t(generator "plc_b7_baseline")\n'
        '\t(generator_version "9.0")\n'
        + gnd_sym("ISO_IO_GND", "bar") + "\n"
        + gnd_sym("ISO_BUS_GND", "diamond") + "\n"
        + vcc_sym("ISO_IO_24V") + "\n"
        + vcc_sym("ISO_BUS_5V") + "\n"
        + vcc_sym("+3V3A") + "\n"
        + ")\n"
    )
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)


class Schematic:
    """Represents one KiCad 9 schematic sheet (root or child)."""

    def __init__(
        self,
        title: str,
        lib_cache: SymbolLibraryCache,
        project_name: str = "plc_controller",
        sheet_uuid: Optional[str] = None,
        instance_path: str = "",
        page_num: str = "1",
        paper: str = "A2",
        comment1: str = "",
        comment2: str = "",
    ):
        self.title = title
        self.lib_cache = lib_cache
        self.project_name = project_name
        self.uuid = sheet_uuid or gen_uuid(title)
        self.instance_path = instance_path or f"/{self.uuid}"
        self.page_num = page_num
        self.paper = paper
        self.comment1 = comment1
        self.comment2 = comment2

        self.used_lib_ids: List[str] = []
        self.symbols: List[dict] = []
        self.wires: List[Tuple[float, float, float, float]] = []
        self.junctions: List[Tuple[float, float]] = []
        self.labels: List[dict] = []
        self.hlabels: List[dict] = []
        self.no_connects: List[Tuple[float, float]] = []
        self.graphics: List[str] = []
        self.sheets: List[dict] = []

        # Lookup placed instances: key = (ref, unit) -> placement dict
        self._placed: Dict[Tuple[str, int], dict] = {}
        self._pwr_idx = 1
        self._flg_idx = 1
        self._connected_pins: set = set()  # ((ref, unit), pin_num)

    def _use_lib(self, lib_id: str):
        if lib_id not in self.used_lib_ids:
            self.used_lib_ids.append(lib_id)

    def add_comp(
        self,
        lib_id: str,
        ref: str,
        val: str,
        x: float,
        y: float,
        rot: float = 0,
        mirror: str = "",
        unit: int = 1,
        footprint: str = "",
        ref_offset: Optional[Tuple[float, float]] = None,
        val_offset: Optional[Tuple[float, float]] = None,
        ref_hide: bool = False,
        val_hide: bool = False,
        is_power: bool = False,
    ) -> dict:
        self._use_lib(lib_id)
        _, pins_by_unit = self.lib_cache.get_symbol(lib_id)
        upins = pins_by_unit.get(unit, {})
        pin_coords = {}
        for pnum, pinfo in upins.items():
            px, py = transform_pin(pinfo["x"], pinfo["y"], x, y, rot, mirror)
            pin_coords[pnum] = {
                "x": px,
                "y": py,
                "name": pinfo["name"],
                "etype": pinfo["etype"],
            }

        # Sensible default text offsets so Ref/Val don't overlap symbol body
        if ref_offset is None:
            if is_power:
                ref_offset = (0.0, 5.0)
            elif rot in (90, 270):
                ref_offset = (0.0, -3.2)
            else:
                ref_offset = (3.8, -1.5)
        if val_offset is None:
            if is_power:
                # Match standard power symbol value positions
                if "GND" in val:
                    val_offset = (0.0, 4.5)
                else:
                    val_offset = (0.0, -3.8)
            elif rot in (90, 270):
                val_offset = (0.0, 3.2)
            else:
                val_offset = (3.8, 1.5)

        suuid = gen_uuid(f"{self.title}:{ref}:{unit}:{x}:{y}:{len(self.symbols)}")
        rec = {
            "lib_id": lib_id,
            "ref": ref,
            "val": val,
            "x": x,
            "y": y,
            "rot": rot,
            "mirror": mirror,
            "unit": unit,
            "footprint": footprint,
            "ref_pos": (x + ref_offset[0], y + ref_offset[1]),
            "val_pos": (x + val_offset[0], y + val_offset[1]),
            "ref_hide": ref_hide or is_power,
            "val_hide": val_hide,
            "is_power": is_power,
            "uuid": suuid,
            "pins": pin_coords,
        }
        self.symbols.append(rec)
        if not is_power:
            self._placed[(ref, unit)] = rec
        return rec

    def pin(self, ref: str, pnum: str, unit: int = 1) -> Tuple[float, float]:
        rec = self._placed[(ref, unit)]
        p = rec["pins"][str(pnum)]
        self._connected_pins.add(((ref, unit), str(pnum)))
        return (p["x"], p["y"])

    def pin_by_name(self, ref: str, pname: str, unit: int = 1) -> Tuple[float, float]:
        rec = self._placed[(ref, unit)]
        for pnum, p in rec["pins"].items():
            if p["name"] == pname:
                self._connected_pins.add(((ref, unit), pnum))
                return (p["x"], p["y"])
        raise KeyError(f"Pin name {pname} not found on {ref} unit {unit}")

    def pwr(self, rail: str, x: float, y: float, rot: float = 0) -> Tuple[float, float]:
        """Place a power or ground symbol at (x, y). Rail maps to power:* or plc_power:*."""
        custom_map = {
            "ISO_IO_GND": "plc_power:ISO_IO_GND",
            "ISO_BUS_GND": "plc_power:ISO_BUS_GND",
            "ISO_IO_24V": "plc_power:ISO_IO_24V",
            "ISO_BUS_5V": "plc_power:ISO_BUS_5V",
            "+3V3A": "plc_power:+3V3A",
        }
        lib_id = custom_map.get(rail, f"power:{rail}")
        ref = f"#PWR{self.page_num}{self._pwr_idx:03d}"
        self._pwr_idx += 1
        # Adjust label offset according to rotation
        if "GND" in rail:
            voff = (0.0, 4.6) if rot == 0 else (0.0, -4.6)
        else:
            voff = (0.0, -3.6) if rot == 0 else (0.0, 3.6)
        self.add_comp(
            lib_id, ref, rail, x, y, rot=rot,
            val_offset=voff, ref_hide=True, is_power=True
        )
        return (x, y)

    def pwr_flag(self, x: float, y: float, rot: float = 0) -> Tuple[float, float]:
        ref = f"#FLG{self.page_num}{self._flg_idx:02d}"
        self._flg_idx += 1
        voff = (0.0, -3.5) if rot == 0 else (0.0, 3.5)
        self.add_comp(
            "power:PWR_FLAG", ref, "PWR_FLAG", x, y, rot=rot,
            val_offset=voff, ref_hide=True, is_power=True
        )
        return (x, y)

    def wire(self, p1: Tuple[float, float], p2: Tuple[float, float]):
        x1, y1 = round(p1[0], 4), round(p1[1], 4)
        x2, y2 = round(p2[0], 4), round(p2[1], 4)
        if abs(x1 - x2) < 1e-6 and abs(y1 - y2) < 1e-6:
            return
        self.wires.append((x1, y1, x2, y2))

    def wire_L(self, p1: Tuple[float, float], p2: Tuple[float, float], horiz_first: bool = True):
        x1, y1 = p1
        x2, y2 = p2
        if horiz_first:
            mid = (x2, y1)
        else:
            mid = (x1, y2)
        self.wire(p1, mid)
        self.wire(mid, p2)

    def wire_pts(self, pts: List[Tuple[float, float]]):
        for a, b in zip(pts, pts[1:]):
            self.wire(a, b)

    def junction(self, p: Tuple[float, float]):
        pt = (round(p[0], 4), round(p[1], 4))
        if pt not in self.junctions:
            self.junctions.append(pt)

    def label(self, text: str, p: Tuple[float, float], angle: float = 0, justify: str = "left"):
        self.labels.append({
            "text": text, "x": p[0], "y": p[1], "angle": angle,
            "justify": justify, "uuid": gen_uuid(f"lbl:{self.title}:{text}:{p[0]}:{p[1]}:{len(self.labels)}")
        })

    def hlabel(self, text: str, p: Tuple[float, float], shape: str = "bidirectional", angle: float = 0, justify: str = "left"):
        self.hlabels.append({
            "text": text, "x": p[0], "y": p[1], "shape": shape, "angle": angle,
            "justify": justify, "uuid": gen_uuid(f"hlbl:{self.title}:{text}:{p[0]}:{p[1]}:{len(self.hlabels)}")
        })

    def no_connect(self, p: Tuple[float, float]):
        self.no_connects.append((round(p[0], 4), round(p[1], 4)))

    def auto_nc_unused(self, ref: str, unit: int = 1):
        """Place no_connect X on all unused pins of (ref, unit) except `no_connect`-typed pins."""
        rec = self._placed[(ref, unit)]
        seen_xy = set()
        for pnum, p in rec["pins"].items():
            if ((ref, unit), pnum) in self._connected_pins:
                continue
            if p["etype"] == "no_connect":
                continue
            xy = (round(p["x"], 4), round(p["y"], 4))
            if xy in seen_xy:
                continue
            seen_xy.add(xy)
            self.no_connect(xy)

    def stub_label(self, p: Tuple[float, float], name: str, dx: float = -5.08, dy: float = 0.0, hier: bool = False, shape: str = "bidirectional"):
        """Draw a short stub wire from pin p to p+(dx,dy) and attach a net or hierarchical label."""
        end = (round(p[0] + dx, 4), round(p[1] + dy, 4))
        self.wire(p, end)
        if dx < 0:
            ang = 180 if hier else 0
            just = "right" if not hier else "right"
        else:
            ang = 0
            just = "left"
        if hier:
            self.hlabel(name, end, shape=shape, angle=ang, justify=just)
        else:
            self.label(name, end, angle=0, justify=("right" if dx < 0 else "left"))
        return end

    def add_box(self, x1: float, y1: float, x2: float, y2: float, title: str = "",
                stroke_rgb: Tuple[int, int, int] = (40, 80, 140),
                fill_rgba: Optional[Tuple[int, int, int, float]] = None,
                stroke_width: float = 0.35, stroke_type: str = "dash"):
        r, g, b = stroke_rgb
        if fill_rgba:
            fr, fg, fb, fa = fill_rgba
            fill_s = f"(fill (type color) (color {fr} {fg} {fb} {fa}))"
        else:
            fill_s = "(fill (type none))"
        u = gen_uuid(f"box:{self.title}:{x1}:{y1}:{x2}:{y2}:{len(self.graphics)}")
        self.graphics.append(
            f'\t(rectangle (start {fmt(x1)} {fmt(y1)}) (end {fmt(x2)} {fmt(y2)})\n'
            f'\t\t(stroke (width {fmt(stroke_width)}) (type {stroke_type}) (color {r} {g} {b} 1))\n'
            f'\t\t{fill_s}\n'
            f'\t\t(uuid "{u}")\n'
            f'\t)'
        )
        if title:
            tu = gen_uuid(f"boxt:{self.title}:{title}:{x1}:{y1}:{len(self.graphics)}")
            self.graphics.append(
                f'\t(text "{esc(title)}" (exclude_from_sim no) (at {fmt(x1 + 3.0)} {fmt(y1 + 5.0)} 0)\n'
                f'\t\t(effects (font (size 2.54 2.54) (bold yes) (color {r} {g} {b} 1)) (justify left top))\n'
                f'\t\t(uuid "{tu}")\n'
                f'\t)'
            )

    def add_text(self, text: str, x: float, y: float, size: float = 2.0, bold: bool = False,
                 rgb: Tuple[int, int, int] = (30, 30, 30), angle: float = 0, justify: str = "left"):
        u = gen_uuid(f"txt:{self.title}:{x}:{y}:{len(self.graphics)}")
        b = " (bold yes)" if bold else ""
        r, g, b_c = rgb
        j_str = "" if justify == "center" else f" (justify {justify})"
        self.graphics.append(
            f'\t(text "{esc(text)}" (exclude_from_sim no) (at {fmt(x)} {fmt(y)} {fmt(angle)})\n'
            f'\t\t(effects (font (size {fmt(size)} {fmt(size)}){b} (color {r} {g} {b_c} 1)){j_str})\n'
            f'\t\t(uuid "{u}")\n'
            f'\t)'
        )

    def add_isolation_barrier(self, x: float, y1: float, y2: float, left_label: str, right_label: str, banner: str = "GALVANIC ISOLATION BARRIER (2.5 kVrms)"):
        u = gen_uuid(f"iso:{self.title}:{x}:{y1}:{y2}")
        self.graphics.append(
            f'\t(polyline (pts (xy {fmt(x)} {fmt(y1)}) (xy {fmt(x)} {fmt(y2)}))\n'
            f'\t\t(stroke (width 0.9) (type dash) (color 210 25 25 1))\n'
            f'\t\t(uuid "{u}")\n'
            f'\t)'
        )
        # Second parallel hairline for crisp industrial barrier look
        u2 = gen_uuid(f"iso2:{self.title}:{x}:{y1}:{y2}")
        self.graphics.append(
            f'\t(polyline (pts (xy {fmt(x + 1.5)} {fmt(y1)}) (xy {fmt(x + 1.5)} {fmt(y2)}))\n'
            f'\t\t(stroke (width 0.3) (type dot) (color 210 25 25 1))\n'
            f'\t\t(uuid "{u2}")\n'
            f'\t)'
        )
        self.add_text(banner, x, y1 - 3.5, size=2.8, bold=True, rgb=(200, 20, 20), justify="center")
        self.add_text(left_label, x - 6.0, y1 + 4.0, size=2.2, bold=True, rgb=(25, 70, 150), justify="right")
        self.add_text(right_label, x + 7.5, y1 + 4.0, size=2.2, bold=True, rgb=(180, 40, 20), justify="left")

    def add_sheet_block(
        self,
        name: str,
        filename: str,
        x: float,
        y: float,
        w: float,
        h: float,
        child_uuid: str,
        page_num: str,
        pins: List[Tuple[str, str, str, float]],
        fill_rgba: Tuple[int, int, int, float] = (242, 248, 255, 1.0),
    ) -> Dict[str, Tuple[float, float]]:
        """
        Add a hierarchical (sheet ...) block on the root schematic.
        pins: list of (pin_name, shape, side ['left'|'right'|'top'|'bottom'], offset_from_start)
        Returns dict mapping pin_name -> exact (sx, sy) coordinate on root sheet.
        """
        pin_entries = []
        pin_coords: Dict[str, Tuple[float, float]] = {}
        for pname, shape, side, off in pins:
            if side == "left":
                px, py, ang = x, y + off, 180
                just = "left"
            elif side == "right":
                px, py, ang = x + w, y + off, 0
                just = "right"
            elif side == "top":
                px, py, ang = x + off, y, 90
                just = "right"
            else:
                px, py, ang = x + off, y + h, 270
                just = "left"
            pin_coords[pname] = (round(px, 4), round(py, 4))
            pin_entries.append({
                "name": pname, "shape": shape, "x": px, "y": py, "angle": ang,
                "justify": just, "uuid": gen_uuid(f"spin:{name}:{pname}")
            })

        self.sheets.append({
            "name": name, "filename": filename, "x": x, "y": y, "w": w, "h": h,
            "uuid": child_uuid, "page_num": page_num, "pins": pin_entries,
            "fill_rgba": fill_rgba,
        })
        return pin_coords

    def _split_wires(self) -> List[Tuple[float, float, float, float]]:
        """Split horizontal/vertical wires at all interior junction/pin/label/endpoint intersections."""
        pts = set()
        for jx, jy in self.junctions:
            pts.add((round(jx, 4), round(jy, 4)))
        for lb in self.labels:
            pts.add((round(lb["x"], 4), round(lb["y"], 4)))
        for hl in self.hlabels:
            pts.add((round(hl["x"], 4), round(hl["y"], 4)))
        for sym in self.symbols:
            for p in sym["pins"].values():
                pts.add((round(p["x"], 4), round(p["y"], 4)))
        for sh in self.sheets:
            for sp in sh["pins"]:
                pts.add((round(sp["x"], 4), round(sp["y"], 4)))
        for x1, y1, x2, y2 in self.wires:
            pts.add((round(x1, 4), round(y1, 4)))
            pts.add((round(x2, 4), round(y2, 4)))

        out = []
        seen = set()
        eps = 1e-3
        for x1, y1, x2, y2 in self.wires:
            if abs(y1 - y2) < eps:
                # Horizontal wire
                ya = round(y1, 4)
                lo, hi = min(x1, x2), max(x1, x2)
                xs = [lo, hi]
                for px, py in pts:
                    if abs(py - ya) < eps and lo + eps < px < hi - eps:
                        xs.append(px)
                xs = sorted(set(round(v, 4) for v in xs))
                for a, b in zip(xs, xs[1:]):
                    if abs(b - a) > eps:
                        seg = (a, ya, b, ya)
                        if seg not in seen:
                            seen.add(seg)
                            out.append(seg)
            elif abs(x1 - x2) < eps:
                # Vertical wire
                xa = round(x1, 4)
                lo, hi = min(y1, y2), max(y1, y2)
                ys = [lo, hi]
                for px, py in pts:
                    if abs(px - xa) < eps and lo + eps < py < hi - eps:
                        ys.append(py)
                ys = sorted(set(round(v, 4) for v in ys))
                for a, b in zip(ys, ys[1:]):
                    if abs(b - a) > eps:
                        seg = (xa, a, xa, b)
                        if seg not in seen:
                            seen.add(seg)
                            out.append(seg)
            else:
                seg = (round(x1, 4), round(y1, 4), round(x2, 4), round(y2, 4))
                if seg not in seen:
                    seen.add(seg)
                    out.append(seg)
        return out

    def render(self, sheet_instances_block: str = "") -> str:
        lines = []
        lines.append("(kicad_sch")
        lines.append("\t(version 20250114)")
        lines.append('\t(generator "eeschema")')
        lines.append('\t(generator_version "9.0")')
        lines.append(f'\t(uuid "{self.uuid}")')
        lines.append(f'\t(paper "{self.paper}")')
        lines.append("\t(title_block")
        lines.append(f'\t\t(title "{esc(self.title)}")')
        lines.append('\t\t(date "2026-11-27")')
        lines.append('\t\t(rev "1.0")')
        lines.append('\t\t(company "Industrial Automation Engineering — PLC-4S Controller")')
        if self.comment1:
            lines.append(f'\t\t(comment 1 "{esc(self.comment1)}")')
        if self.comment2:
            lines.append(f'\t\t(comment 2 "{esc(self.comment2)}")')
        lines.append("\t)")

        # lib_symbols
        lines.append("\t(lib_symbols")
        for lib_id in self.used_lib_ids:
            flat_sexp, _ = self.lib_cache.get_symbol(lib_id)
            lines.append("\t\t" + serialize_sexp(flat_sexp, indent=2))
        lines.append("\t)")

        # graphics
        for g in self.graphics:
            lines.append(g)

        # junctions
        for jx, jy in self.junctions:
            ju = gen_uuid(f"j:{self.title}:{jx}:{jy}")
            lines.append(f'\t(junction (at {fmt(jx)} {fmt(jy)}) (diameter 0) (color 0 0 0 0) (uuid "{ju}"))')

        # no_connects
        for idx, (nx, ny) in enumerate(self.no_connects):
            nu = gen_uuid(f"nc:{self.title}:{idx}:{nx}:{ny}")
            lines.append(f'\t(no_connect (at {fmt(nx)} {fmt(ny)}) (uuid "{nu}"))')

        # wires (split cleanly at all interior T-junctions, pins, and labels)
        for idx, (x1, y1, x2, y2) in enumerate(self._split_wires()):
            wu = gen_uuid(f"w:{self.title}:{idx}:{x1}:{y1}:{x2}:{y2}")
            lines.append(
                f'\t(wire (pts (xy {fmt(x1)} {fmt(y1)}) (xy {fmt(x2)} {fmt(y2)}))\n'
                f'\t\t(stroke (width 0) (type default))\n'
                f'\t\t(uuid "{wu}")\n'
                f'\t)'
            )

        # local labels
        for lb in self.labels:
            lines.append(
                f'\t(label "{esc(lb["text"])}" (at {fmt(lb["x"])} {fmt(lb["y"])} {fmt(lb["angle"])})\n'
                f'\t\t(effects (font (size 1.27 1.27)) (justify {lb["justify"]} bottom))\n'
                f'\t\t(uuid "{lb["uuid"]}")\n'
                f'\t)'
            )

        # hierarchical labels
        for hl in self.hlabels:
            lines.append(
                f'\t(hierarchical_label "{esc(hl["text"])}" (shape {hl["shape"]}) (at {fmt(hl["x"])} {fmt(hl["y"])} {fmt(hl["angle"])})\n'
                f'\t\t(effects (font (size 1.524 1.524) (bold yes)) (justify {hl["justify"]}))\n'
                f'\t\t(uuid "{hl["uuid"]}")\n'
                f'\t)'
            )

        # symbols
        for s in self.symbols:
            mir = f"\n\t\t(mirror {s['mirror']})" if s["mirror"] else ""
            rx, ry = s["ref_pos"]
            vx, vy = s["val_pos"]
            rhide = " (hide yes)" if s["ref_hide"] else ""
            vhide = " (hide yes)" if s["val_hide"] else ""
            lines.append(
                f'\t(symbol\n'
                f'\t\t(lib_id "{esc(s["lib_id"])}")\n'
                f'\t\t(at {fmt(s["x"])} {fmt(s["y"])} {fmt(s["rot"])}){mir}\n'
                f'\t\t(unit {s["unit"]})\n'
                f'\t\t(exclude_from_sim no)\n'
                f'\t\t(in_bom {"no" if s["is_power"] else "yes"})\n'
                f'\t\t(on_board {"no" if s["is_power"] else "yes"})\n'
                f'\t\t(dnp no)\n'
                f'\t\t(uuid "{s["uuid"]}")\n'
                f'\t\t(property "Reference" "{esc(s["ref"])}" (at {fmt(rx)} {fmt(ry)} 0) (effects (font (size 1.27 1.27)){rhide}))\n'
                f'\t\t(property "Value" "{esc(s["val"])}" (at {fmt(vx)} {fmt(vy)} 0) (effects (font (size 1.27 1.27)){vhide}))\n'
                f'\t\t(property "Footprint" "{esc(s["footprint"])}" (at {fmt(s["x"])} {fmt(s["y"])} 0) (effects (font (size 1.27 1.27)) (hide yes)))\n'
                f'\t\t(property "Datasheet" "~" (at {fmt(s["x"])} {fmt(s["y"])} 0) (effects (font (size 1.27 1.27)) (hide yes)))'
            )
            for pnum in sorted(s["pins"].keys()):
                pu = gen_uuid(f"pin:{s['uuid']}:{pnum}")
                lines.append(f'\t\t(pin "{esc(pnum)}" (uuid "{pu}"))')
            lines.append(
                f'\t\t(instances\n'
                f'\t\t\t(project "{esc(self.project_name)}"\n'
                f'\t\t\t\t(path "{self.instance_path}"\n'
                f'\t\t\t\t\t(reference "{esc(s["ref"])}")\n'
                f'\t\t\t\t\t(unit {s["unit"]})\n'
                f'\t\t\t\t)\n'
                f'\t\t\t)\n'
                f'\t\t)\n'
                f'\t)'
            )

        # child sheets (on root)
        for sh in self.sheets:
            fr, fg, fb, fa = sh["fill_rgba"]
            lines.append(
                f'\t(sheet\n'
                f'\t\t(at {fmt(sh["x"])} {fmt(sh["y"])})\n'
                f'\t\t(size {fmt(sh["w"])} {fmt(sh["h"])})\n'
                f'\t\t(exclude_from_sim no)\n'
                f'\t\t(in_bom yes)\n'
                f'\t\t(on_board yes)\n'
                f'\t\t(dnp no)\n'
                f'\t\t(fields_autoplaced no)\n'
                f'\t\t(stroke (width 0.4) (type solid) (color 30 70 130 1))\n'
                f'\t\t(fill (color {fr} {fg} {fb} {fa}))\n'
                f'\t\t(uuid "{sh["uuid"]}")\n'
                f'\t\t(property "Sheetname" "{esc(sh["name"])}" (at {fmt(sh["x"] + 2.0)} {fmt(sh["y"] - 2.5)} 0)\n'
                f'\t\t\t(effects (font (size 2.54 2.54) (bold yes) (color 20 50 110 1)) (justify left bottom))\n'
                f'\t\t)\n'
                f'\t\t(property "Sheetfile" "{esc(sh["filename"])}" (at {fmt(sh["x"] + 2.0)} {fmt(sh["y"] + sh["h"] + 3.0)} 0)\n'
                f'\t\t\t(effects (font (size 1.778 1.778) (color 90 90 90 1)) (justify left top))\n'
                f'\t\t)'
            )
            for sp in sh["pins"]:
                lines.append(
                    f'\t\t(pin "{esc(sp["name"])}" {sp["shape"]} (at {fmt(sp["x"])} {fmt(sp["y"])} {fmt(sp["angle"])})\n'
                    f'\t\t\t(effects (font (size 1.524 1.524)) (justify {sp["justify"]}))\n'
                    f'\t\t\t(uuid "{sp["uuid"]}")\n'
                    f'\t\t)'
                )
            lines.append(
                f'\t\t(instances\n'
                f'\t\t\t(project "{esc(self.project_name)}"\n'
                f'\t\t\t\t(path "/{self.uuid}"\n'
                f'\t\t\t\t\t(page "{sh["page_num"]}")\n'
                f'\t\t\t\t)\n'
                f'\t\t\t)\n'
                f'\t\t)\n'
                f'\t)'
            )

        if sheet_instances_block:
            lines.append(sheet_instances_block)
        else:
            lines.append(
                f'\t(sheet_instances\n'
                f'\t\t(path "/"\n'
                f'\t\t\t(page "{self.page_num}")\n'
                f'\t\t)\n'
                f'\t)'
            )
        lines.append("\t(embedded_fonts no)")
        lines.append(")\n")
        return "\n".join(lines)
