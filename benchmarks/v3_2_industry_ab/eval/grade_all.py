#!/usr/bin/env python3
"""
Independent, Brutally Honest External Grader for EDL v3.2 Industry-Scale A/B Benchmarks.
NEVER trusts EDL's own verifier or agent self-reports.
Grades actual exported artifacts (.kicad_sch, .dxf, .svg, .png, .csv, manifests) using:
  - Real `kicad-cli sch erc` and `kicad-cli sch export netlist`
  - Independent KiCad S-expression sheet-frame & symbol-overlap analyzer
  - Independent `shapely` + `networkx` architectural & cross-discipline clash verifier
  - Independent IPC plumbing & IEC 60364 / IEC 60909 electrical & SLD calculators
"""
import csv
import json
import math
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import networkx as nx
from shapely.geometry import LineString, Point, box

ROOT = Path("/home/lindah/projects/EDL/benchmarks/v3_2_industry_ab")

KICAD_PAPERS = {
    "A4": (297.0, 210.0),
    "A3": (420.0, 297.0),
    "A2": (594.0, 420.0),
    "A1": (841.0, 594.0),
    "A0": (1189.0, 841.0),
    "USLetter": (279.4, 215.9),
    "USLegal": (355.6, 215.9),
    "USLedger": (431.8, 279.4),
}

# Standard IEC 60364-5-52 copper XLPE/PVC in conduit/tray reference ampacity (A)
CABLE_IZ_1P = {
    1.5: 17.5, 2.5: 24.0, 4.0: 32.0, 6.0: 41.0, 10.0: 57.0, 16.0: 76.0,
    25.0: 101.0, 35.0: 125.0, 50.0: 151.0, 70.0: 192.0, 95.0: 232.0,
    120.0: 269.0, 150.0: 309.0, 185.0: 353.0, 240.0: 415.0, 300.0: 477.0,
}
CABLE_IZ_3P = {
    1.5: 15.5, 2.5: 21.0, 4.0: 28.0, 6.0: 36.0, 10.0: 50.0, 16.0: 68.0,
    25.0: 89.0, 35.0: 110.0, 50.0: 134.0, 70.0: 171.0, 95.0: 207.0,
    120.0: 239.0, 150.0: 275.0, 185.0: 314.0, 240.0: 370.0, 300.0: 426.0,
}


def parse_notes_timing(arm_dir: Path) -> dict:
    notes = arm_dir / "notes.md"
    info = {"notes_exists": notes.is_file(), "elapsed_sec": None, "iterations": None}
    if not notes.is_file():
        return info
    txt = notes.read_text(errors="replace")
    m_el = re.search(r"elapsed_sec\s*[:=]\s*`?(\d+(?:\.\d+)?)`?", txt, re.I)
    if m_el:
        info["elapsed_sec"] = float(m_el.group(1))
    else:
        ms = re.search(r"start_epoch\s*[:=]\s*`?(\d+)`?", txt, re.I)
        me = re.search(r"end_epoch\s*[:=]\s*`?(\d+)`?", txt, re.I)
        if ms and me:
            info["elapsed_sec"] = float(int(me.group(1)) - int(ms.group(1)))
    m_it = re.search(r"iterations\s*[:=]\s*`?(\d+)`?", txt, re.I)
    if m_it:
        info["iterations"] = int(m_it.group(1))
    return info


def audit_dxf(path: Path) -> dict:
    if not path.is_file():
        return {"exists": False}
    txt = path.read_text(errors="replace")
    lines = txt.splitlines()
    has_entities = "ENTITIES" in txt
    has_eof = "EOF" in txt
    # Extract layer names (group code 8)
    layers = set()
    entity_count = 0
    entity_types = {"LINE", "LWPOLYLINE", "POLYLINE", "CIRCLE", "ARC", "TEXT", "MTEXT", "INSERT", "SOLID", "HATCH", "DIMENSION"}
    for i in range(len(lines) - 1):
        code = lines[i].strip()
        val = lines[i + 1].strip()
        if code == "0" and val in entity_types:
            entity_count += 1
        elif code == "8" and val:
            layers.add(val)
    return {
        "exists": True,
        "size_bytes": path.stat().st_size,
        "valid_structure": has_entities and has_eof and entity_count > 0,
        "entity_count": entity_count,
        "layer_count": len(layers),
        "layers": sorted(layers),
    }


def grade_b1_kicad(arm_dir: Path) -> dict:
    res = {"arm": arm_dir.name, "timing": parse_notes_timing(arm_dir)}
    sch_files = sorted(arm_dir.glob("*.kicad_sch"))
    root_sch = arm_dir / "gateway.kicad_sch"
    if not root_sch.is_file() and sch_files:
        root_sch = sch_files[0]
    res["kicad_sch_exists"] = root_sch.is_file()
    res["png_exists"] = (arm_dir / "gateway.png").is_file() or bool(list(arm_dir.glob("*.png")))
    res["sheet_count"] = len(sch_files)

    # Measure authored source lines (excluding generated .kicad_sch)
    src_lines = 0
    src_files = []
    for ext in ("*.edl", "*.py"):
        for p in sorted(arm_dir.glob(ext)):
            cnt = len(p.read_text(errors="replace").splitlines())
            src_lines += cnt
            src_files.append(f"{p.name}:{cnt}L")
    res["source_lines"] = src_lines
    res["source_files"] = src_files

    if not root_sch.is_file():
        res["fatal"] = "Missing gateway.kicad_sch"
        return res

    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        # 1. Real KiCad 9 CLI ERC
        erc_json = td / "erc.json"
        p_erc = subprocess.run(
            ["kicad-cli", "sch", "erc", "--format", "json", "--output", str(erc_json), str(root_sch)],
            capture_output=True, text=True, timeout=60,
        )
        res["erc_cli_rc"] = p_erc.returncode
        if erc_json.is_file():
            erc_data = json.loads(erc_json.read_text())
            errs, warns = [], []
            for sh in erc_data.get("sheets", []):
                for v in sh.get("violations", []):
                    sev = v.get("severity", "")
                    if sev == "error":
                        errs.append(f"{v.get('type')}: {v.get('description')}")
                    elif sev == "warning":
                        warns.append(f"{v.get('type')}: {v.get('description')}")
            res["erc_errors"] = len(errs)
            res["erc_warnings"] = len(warns)
            res["erc_error_samples"] = errs[:8]
            res["erc_warning_samples"] = warns[:8]
        else:
            res["erc_errors"] = -1
            res["erc_stderr"] = p_erc.stderr.strip()[:400]

        # 2. Real KiCad 9 CLI Netlist export
        nl_file = td / "netlist.net"
        p_nl = subprocess.run(
            ["kicad-cli", "sch", "export", "netlist", "--format", "kicadsexpr", "--output", str(nl_file), str(root_sch)],
            capture_output=True, text=True, timeout=60,
        )
        res["netlist_cli_rc"] = p_nl.returncode
        if nl_file.is_file():
            nl_txt = nl_file.read_text(errors="replace")
            comps = re.findall(r'\(comp\s+\(ref\s+"([^"]+)"\)', nl_txt)
            vals = re.findall(r'\(value\s+"([^"]+)"\)', nl_txt)
            # Parse nets
            net_blocks = re.split(r'\(net\s+\(code\s+"[^"]+"\)', nl_txt)[1:]
            multi_pin_nets = 0
            single_pin_nets = 0
            net_names = []
            for nb in net_blocks:
                mn = re.search(r'\(name\s+"([^"]+)"\)', nb)
                nname = mn.group(1) if mn else "?"
                nodes = re.findall(r'\(node\s+\(ref\s+"([^"]+)"\)\s+\(pin\s+"([^"]+)"\)', nb)
                if nname.startswith("unconnected-"):
                    continue
                net_names.append((nname, len(nodes)))
                if len(nodes) >= 2:
                    multi_pin_nets += 1
                elif len(nodes) == 1:
                    single_pin_nets += 1
            res["component_count"] = len(comps)
            res["multi_pin_nets"] = multi_pin_nets
            res["single_pin_nets"] = single_pin_nets
            res["components_sample"] = comps[:20]

            # Functional coverage heuristics from real KiCad netlist
            blob = (nl_txt + " " + " ".join(comps) + " " + " ".join(vals)).lower()
            checks = {
                "has_fuse": any(c.upper().startswith("F") for c in comps) or "fuse" in blob,
                "has_tvs_or_esd": "tvs" in blob or "smbj" in blob or "smaj" in blob or "pesd" in blob,
                "has_buck_inductor": any(c.upper().startswith("L") for c in comps),
                "has_mcu": any(k in blob for k in ["stm32", "rp2040", "esp32", "mcu", "atmega"]),
                "has_crystal": any(c.upper().startswith("Y") for c in comps) or "crystal" in blob or "mhz" in blob,
                "has_i2c_bus": "sda" in blob and "scl" in blob,
                "has_rs485": "485" in blob or "max485" in blob or "sn65hvd" in blob,
                "has_120ohm_term": "120" in blob,
                "has_optocoupler": "opto" in blob or "pc817" in blob or "tlp" in blob or "el817" in blob or "hcpl" in blob,
                "has_relay": any(c.upper().startswith("K") for c in comps) or "relay" in blob,
                "has_mosfet_or_bjt": any(c.upper().startswith("Q") for c in comps),
                "has_flyback_diode": "fly" in blob or "1n4148" in blob or "1n400" in blob or "ss14" in blob or "mb" in blob or len([c for c in comps if c.upper().startswith("D")]) >= 3,
            }
            res["functional_checks"] = checks
            res["functional_score"] = f"{sum(checks.values())}/{len(checks)}"
        else:
            res["netlist_stderr"] = p_nl.stderr.strip()[:400]

    # 3. Independent S-expression Sheet Frame & Symbol Overlap Audit across all .kicad_sch files
    frame_overflows = 0
    title_block_hits = 0
    near_overlaps = 0
    for sf in sch_files:
        stxt = sf.read_text(errors="replace")
        mp = re.search(r'\(paper\s+"([^"]+)"(\s+portrait)?\)', stxt)
        if mp:
            pname, is_port = mp.group(1), bool(mp.group(2))
            pw, ph = KICAD_PAPERS.get(pname, (420.0, 297.0))
            if is_port:
                pw, ph = ph, pw
        else:
            pw, ph = 420.0, 297.0
        # Extract placed non-power symbols: (symbol (lib_id "...") (at X Y ANG) ...)
        placed = []
        for m in re.finditer(r'\(symbol\s+\(lib_id\s+"([^"]+)"\)\s+\(at\s+([-\d.]+)\s+([-\d.]+)', stxt):
            lid, sx, sy = m.group(1), float(m.group(2)), float(m.group(3))
            if lid.startswith("power:"):
                continue
            placed.append((lid, sx, sy))
            if sx < 10.0 or sx > pw - 10.0 or sy < 10.0 or sy > ph - 10.0:
                frame_overflows += 1
            if sx > pw - 190.0 and sy > ph - 44.0:
                title_block_hits += 1
        # Check severe center-to-center overlap (< 5.0 mm between distinct placed symbols)
        for i in range(len(placed)):
            for j in range(i + 1, len(placed)):
                d = math.hypot(placed[i][1] - placed[j][1], placed[i][2] - placed[j][2])
                if d < 5.0:
                    near_overlaps += 1

    res["frame_overflows"] = frame_overflows
    res["title_block_hits"] = title_block_hits
    res["severe_symbol_overlaps"] = near_overlaps
    return res


def grade_b2_arch(arm_dir: Path) -> dict:
    res = {"arm": arm_dir.name, "timing": parse_notes_timing(arm_dir)}
    res["dxf"] = audit_dxf(arm_dir / "clinic_arch.dxf")
    res["png_exists"] = (arm_dir / "clinic_arch.png").is_file()
    mp = arm_dir / "arch_manifest.json"
    if not mp.is_file():
        res["fatal"] = "Missing arch_manifest.json"
        return res
    data = json.loads(mp.read_text())
    rooms = data.get("rooms", [])
    doors = data.get("doors", [])
    windows = data.get("windows", [])
    columns = data.get("columns", [])
    fixtures = data.get("fixtures", [])

    res["room_count"] = len(rooms)
    res["door_count"] = len(doors)
    res["window_count"] = len(windows)
    res["column_count"] = len(columns)
    res["fixture_count"] = len(fixtures)

    rboxes = {}
    total_area_m2 = 0.0
    for r in rooms:
        b = box(r["min_x"], r["min_y"], r["max_x"], r["max_y"])
        rboxes[r["id"]] = b
        total_area_m2 += b.area / 1e6
    res["total_area_m2"] = round(total_area_m2, 2)

    # 1. Room-to-room polygon overlap (independent shapely proof)
    overlaps = []
    rids = list(rboxes.keys())
    for i in range(len(rids)):
        for j in range(i + 1, len(rids)):
            inter = rboxes[rids[i]].intersection(rboxes[rids[j]])
            if inter.area > 10.0:  # > 10 mm^2 tolerance
                overlaps.append(f"{rids[i]}<->{rids[j]}({inter.area/1e6:.3f}m2)")
    res["room_overlaps"] = len(overlaps)
    res["room_overlap_details"] = overlaps[:5]

    # 2. Graph reachability from Vestibule / exterior
    G = nx.Graph()
    for rid in rids:
        G.add_node(rid)
    entry_node = None
    for d in doors:
        ra, rb = d.get("room_a"), d.get("room_b")
        if ra in rboxes and rb in rboxes:
            G.add_edge(ra, rb)
        elif ra in rboxes and (not rb or str(rb).lower() in ("exterior", "outside", "ext")):
            entry_node = entry_node or ra
        elif rb in rboxes and (not ra or str(ra).lower() in ("exterior", "outside", "ext")):
            entry_node = entry_node or rb
    if entry_node is None and rids:
        entry_node = next((r for r in rids if "vest" in r.lower() or "wait" in r.lower()), rids[0])
    reachable = nx.node_connected_component(G, entry_node) if entry_node in G else set()
    unreachable = sorted(set(rids) - reachable)
    res["reachable_rooms"] = f"{len(reachable)}/{len(rids)}"
    res["unreachable_rooms"] = unreachable

    # 3. Door swing collisions (swing vs swing, swing vs column, swing vs fixture)
    swing_boxes = []
    for d in doors:
        sb = d.get("swing_bbox")
        if sb and len(sb) == 4:
            x0, y0, x1, y1 = sb
            swing_boxes.append((d["id"], box(min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))))
    swing_vs_swing = []
    for i in range(len(swing_boxes)):
        for j in range(i + 1, len(swing_boxes)):
            if swing_boxes[i][1].intersection(swing_boxes[j][1]).area > 100.0:
                swing_vs_swing.append(f"{swing_boxes[i][0]}<->{swing_boxes[j][0]}")
    col_boxes = [(c["id"], box(c["min_x"], c["min_y"], c["max_x"], c["max_y"])) for c in columns]
    fix_boxes = [(f["id"], box(f["min_x"], f["min_y"], f["max_x"], f["max_y"])) for f in fixtures]
    col_swing_clashes = []
    for cid, cb in col_boxes:
        for did, sb in swing_boxes:
            if cb.intersection(sb).area > 100.0:
                col_swing_clashes.append(f"{cid}<->{did}")
    fix_swing_clashes = []
    for fid, fb in fix_boxes:
        for did, sb in swing_boxes:
            if fb.intersection(sb).area > 100.0:
                fix_swing_clashes.append(f"{fid}<->{did}")
    res["door_swing_collisions"] = len(swing_vs_swing)
    res["column_door_clashes"] = len(col_swing_clashes)
    res["fixture_door_clashes"] = len(fix_swing_clashes)

    # 4. Daylighting on required habitable/clinical rooms
    win_rooms = {w["room"].lower() for w in windows if w.get("room")}
    daylight_req = ["waiting", "breakroom", "exam1", "exam2", "exam3", "exam4", "procedure"]
    missing_daylight = [k for k in daylight_req if not any(k in wr for wr in win_rooms)]
    res["missing_daylight_rooms"] = missing_daylight

    # Source lines
    src_lines = sum(len(p.read_text(errors="replace").splitlines()) for ext in ("*.edl", "*.py") for p in arm_dir.glob(ext))
    res["source_lines"] = src_lines
    return res


def grade_b3_plumbing(arm_dir: Path, arch_dir: Path) -> dict:
    res = {"arm": arm_dir.name, "timing": parse_notes_timing(arm_dir)}
    res["dxf"] = audit_dxf(arm_dir / "clinic_plumbing.dxf")
    res["png_exists"] = (arm_dir / "clinic_plumbing.png").is_file()
    res["csv_exists"] = (arm_dir / "plumbing_schedule.csv").is_file()
    mp = arm_dir / "plumbing_manifest.json"
    if not mp.is_file():
        res["fatal"] = "Missing plumbing_manifest.json"
        return res
    data = json.loads(mp.read_text())
    fixtures = data.get("fixtures", [])
    valves = data.get("valves", [])
    cleanouts = data.get("cleanouts", [])
    pipes = data.get("pipes", [])

    res["wet_fixture_count"] = len(fixtures)
    res["valve_count"] = len(valves)
    res["cleanout_count"] = len(cleanouts)
    res["pipe_count"] = len(pipes)

    # Fixture service completeness
    unserved = []
    total_dfu = 0
    for f in fixtures:
        kind = str(f.get("kind", "")).lower()
        total_dfu += f.get("dfu", 0) or 0
        if kind == "water_heater":
            continue
        if not f.get("has_cw") or not f.get("has_san") or not f.get("has_vent"):
            unserved.append(f["id"])
        if kind != "wc" and not f.get("has_hw"):
            unserved.append(f"{f['id']}(no_hw)")
    res["unserved_fixtures"] = unserved
    res["total_dfu"] = total_dfu

    # IPC pipe sizing checks
    san_pipes = [p for p in pipes if str(p.get("system", "")).upper() == "SAN"]
    max_san_dn = max((p.get("dn_mm", 0) for p in san_pipes), default=0)
    bad_slopes = [p["id"] for p in san_pipes if not (0.5 <= (p.get("slope_pct") or 0) <= 5.0)]
    res["max_san_collector_dn_mm"] = max_san_dn
    res["san_bad_slope_count"] = len(bad_slopes)

    # Independent cross-discipline clash check against arm's B2 arch_manifest.json
    elec_room_clashes = 0
    col_clashes = 0
    arch_mp = arch_dir / "arch_manifest.json"
    if arch_mp.is_file():
        adata = json.loads(arch_mp.read_text())
        elec_boxes = [
            box(r["min_x"], r["min_y"], r["max_x"], r["max_y"]).buffer(-50.0)
            for r in adata.get("rooms", [])
            if "elec" in r["id"].lower() or "server" in r["id"].lower()
        ]
        col_boxes = [box(c["min_x"], c["min_y"], c["max_x"], c["max_y"]) for c in adata.get("columns", [])]
        for p in pipes:
            seg = LineString([(p["from_x"], p["from_y"]), (p["to_x"], p["to_y"])])
            if any(seg.intersects(eb) for eb in elec_boxes):
                elec_room_clashes += 1
            if any(seg.intersects(cb) for cb in col_boxes):
                col_clashes += 1
    res["pipes_entering_elec_room"] = elec_room_clashes
    res["pipes_clashing_columns"] = col_clashes
    res["source_lines"] = sum(len(p.read_text(errors="replace").splitlines()) for ext in ("*.edl", "*.py") for p in arm_dir.glob(ext))
    return res


def grade_b4_electrical(arm_dir: Path, arch_dir: Path, plumbing_dir: Path) -> dict:
    res = {"arm": arm_dir.name, "timing": parse_notes_timing(arm_dir)}
    res["dxf"] = audit_dxf(arm_dir / "clinic_electrical.dxf")
    res["png_exists"] = (arm_dir / "clinic_electrical.png").is_file()
    res["csv_exists"] = (arm_dir / "panel_schedule.csv").is_file()
    mp = arm_dir / "electrical_manifest.json"
    if not mp.is_file():
        res["fatal"] = "Missing electrical_manifest.json"
        return res
    data = json.loads(mp.read_text())
    panels = data.get("panels", [])
    devices = data.get("devices", [])
    circuits = data.get("circuits", [])
    wires = data.get("wires", [])

    res["panel_count"] = len(panels)
    res["device_count"] = len(devices)
    res["circuit_count"] = len(circuits)
    res["wire_count"] = len(wires)

    # Independent IEC 60364 coordination & voltage drop verification
    coord_fails = []
    vdrop_fails = []
    phase_loads = {"L1": 0.0, "L2": 0.0, "L3": 0.0}
    for c in circuits:
        cid = c["id"]
        kind = str(c.get("kind", "")).lower()
        ph = str(c.get("phase", "L1")).upper()
        load_w = float(c.get("load_w", 0))
        brk = float(c.get("breaker_a", 0))
        mm2 = float(c.get("cable_mm2", 1.5))
        length_m = float(c.get("route_len_m", 0))
        if kind != "submain":
            if ph in phase_loads:
                phase_loads[ph] += load_w
            elif ph == "3P":
                for k in phase_loads:
                    phase_loads[k] += load_w / 3.0
        pf = 0.90
        if ph == "3P":
            ib = load_w / (math.sqrt(3) * 400.0 * pf) if load_w > 0 else 0.0
            iz = CABLE_IZ_3P.get(mm2, mm2 * 5.0)
            dv_pct = (math.sqrt(3) * ib * length_m * (0.0225 / mm2) * pf / 400.0) * 100.0
        else:
            ib = load_w / (230.0 * pf) if load_w > 0 else 0.0
            iz = CABLE_IZ_1P.get(mm2, mm2 * 6.0)
            dv_pct = (2.0 * ib * length_m * (0.0225 / mm2) * pf / 230.0) * 100.0
        if ib > brk + 0.1 or brk > iz + 0.1:
            coord_fails.append(f"{cid}(Ib={ib:.1f},In={brk:.0f},Iz={iz:.1f})")
        lim = 3.0 if "light" in kind else 5.0
        if dv_pct > lim + 0.05:
            vdrop_fails.append(f"{cid}(calc_dV={dv_pct:.2f}%>{lim}%)")

    avg_ph = sum(phase_loads.values()) / 3.0 if sum(phase_loads.values()) > 0 else 1.0
    imbalance_pct = ((max(phase_loads.values()) - min(phase_loads.values())) / avg_ph) * 100.0
    res["iec_coord_violations"] = len(coord_fails)
    res["iec_coord_details"] = coord_fails[:5]
    res["vdrop_violations"] = len(vdrop_fails)
    res["vdrop_details"] = vdrop_fails[:5]
    res["phase_loads_w"] = {k: round(v, 1) for k, v in phase_loads.items()}
    res["phase_imbalance_pct"] = round(imbalance_pct, 2)

    # Room lighting coverage & cross-discipline panel clearance check
    panel_clashes = 0
    unlit_rooms = []
    arch_mp = arch_dir / "arch_manifest.json"
    if arch_mp.is_file():
        adata = json.loads(arch_mp.read_text())
        lit_rooms = {d["room"].lower() for d in devices if "light" in str(d.get("kind", "")).lower() and d.get("room")}
        for r in adata.get("rooms", []):
            if r["id"].lower() not in lit_rooms:
                unlit_rooms.append(r["id"])
        obstacles = [box(c["min_x"], c["min_y"], c["max_x"], c["max_y"]) for c in adata.get("columns", [])]
        for d in adata.get("doors", []):
            sb = d.get("swing_bbox")
            if sb and len(sb) == 4:
                obstacles.append(box(min(sb[0], sb[2]), min(sb[1], sb[3]), max(sb[0], sb[2]), max(sb[1], sb[3])))
        for p in panels:
            cb = p.get("clearance_bbox")
            if cb and len(cb) == 4:
                pbox = box(min(cb[0], cb[2]), min(cb[1], cb[3]), max(cb[0], cb[2]), max(cb[1], cb[3]))
                if any(pbox.intersection(ob).area > 100.0 for ob in obstacles):
                    panel_clashes += 1
    res["unlit_rooms"] = unlit_rooms
    res["panel_clearance_clashes"] = panel_clashes
    res["source_lines"] = sum(len(p.read_text(errors="replace").splitlines()) for ext in ("*.edl", "*.py") for p in arm_dir.glob(ext))
    return res


def grade_b5_sld(arm_dir: Path) -> dict:
    res = {"arm": arm_dir.name, "timing": parse_notes_timing(arm_dir)}
    res["dxf"] = audit_dxf(arm_dir / "hospital_sld.dxf")
    res["svg_exists"] = (arm_dir / "hospital_sld.svg").is_file()
    res["png_exists"] = (arm_dir / "hospital_sld.png").is_file()
    mp = arm_dir / "sld_manifest.json"
    if not mp.is_file():
        res["fatal"] = "Missing sld_manifest.json"
        return res
    data = json.loads(mp.read_text())
    sources = data.get("sources", [])
    buses = data.get("buses", [])
    interlocks = data.get("interlocks", [])
    breakers = data.get("breakers", [])
    feeders = data.get("feeders", [])

    # Independent IEC 60909 Ik'' calculation for 1000kVA 400V uk=6%, Sk''=250MVA (c_max=1.05)
    c_max = 1.05
    u_n = 400.0
    z_q = (c_max * u_n * u_n) / 250e6
    z_t = (0.06 * u_n * u_n) / 1000e3
    ik_single_ka = (c_max * u_n) / (math.sqrt(3) * (z_q + z_t)) / 1000.0  # ~23.7 kA

    res["independent_ik_single_tx_ka"] = round(ik_single_ka, 2)
    res["source_count"] = len(sources)
    res["bus_count"] = len(buses)
    res["interlock_count"] = len(interlocks)
    res["feeder_count"] = len(feeders)

    # Check breaker Icu >= Ik''
    icu_fails = []
    for b in breakers:
        if float(b.get("icu_ka", 0)) < ik_single_ka:
            icu_fails.append(f"{b['id']}(Icu={b.get('icu_ka')}kA<{ik_single_ka:.1f}kA)")
    for f in feeders:
        if float(f.get("icu_ka", 0)) < ik_single_ka:
            icu_fails.append(f"{f['id']}(Icu={f.get('icu_ka')}kA<{ik_single_ka:.1f}kA)")

    # Independent IEC 60364 feeder cable coordination & voltage drop <= 4.0%
    feeder_coord_fails = []
    feeder_vdrop_fails = []
    essential_kw = 0.0
    total_kw = 0.0
    for f in feeders:
        fid = f["id"]
        kw = float(f.get("load_kw", 0))
        pf = float(f.get("pf", 0.9))
        length_m = float(f.get("length_m", 0))
        brk = float(f.get("breaker_a", 0))
        runs = max(1, int(f.get("cable_runs", 1)))
        mm2 = float(f.get("cable_mm2", 16))
        iz = float(f.get("iz_a", CABLE_IZ_3P.get(mm2, mm2 * 2.0) * runs))
        ib = (kw * 1000.0) / (math.sqrt(3) * 400.0 * pf)
        # 3-phase copper XLPE R+X voltage drop (R=0.0225/S, X=0.00008 ohm/m)
        r_per_m = 0.0225 / (mm2 * runs)
        x_per_m = 0.00008 / runs
        sin_phi = math.sqrt(max(0.0, 1.0 - pf * pf))
        dv_pct = (math.sqrt(3) * ib * length_m * (r_per_m * pf + x_per_m * sin_phi) / 400.0) * 100.0
        if ib > brk + 1.0 or brk > iz + 1.0:
            feeder_coord_fails.append(f"{fid}(Ib={ib:.0f}A,In={brk:.0f}A,Iz={iz:.0f}A)")
        if dv_pct > 4.05:
            feeder_vdrop_fails.append(f"{fid}(calc_dV={dv_pct:.2f}%>4.0%)")
        total_kw += kw
        if "emsb" in str(f.get("bus", "")).lower() or "ess" in str(f.get("bus", "")).lower():
            essential_kw += kw

    res["icu_violations"] = len(icu_fails)
    res["icu_details"] = icu_fails[:5]
    res["feeder_coord_violations"] = len(feeder_coord_fails)
    res["feeder_coord_details"] = feeder_coord_fails[:5]
    res["feeder_vdrop_violations"] = len(feeder_vdrop_fails)
    res["feeder_vdrop_details"] = feeder_vdrop_fails[:5]
    res["essential_kw"] = round(essential_kw, 1)
    res["total_kw"] = round(total_kw, 1)
    res["gen_covers_essential"] = essential_kw <= 800.0
    res["source_lines"] = sum(len(p.read_text(errors="replace").splitlines()) for ext in ("*.edl", "*.py") for p in arm_dir.glob(ext))
    return res


def main():
    targets = sys.argv[1:] or ["b1", "b2", "b3", "b4", "b5"]
    out = {}
    if "b1" in targets:
        out["b1_elec_gateway"] = {
            arm: grade_b1_kicad(ROOT / "b1_elec_gateway" / arm) for arm in ("baseline", "edl")
        }
    if "b2" in targets:
        out["b2_clinic_arch"] = {
            arm: grade_b2_arch(ROOT / "b2_clinic_arch" / arm) for arm in ("baseline", "edl")
        }
    if "b3" in targets:
        out["b3_clinic_plumbing"] = {
            arm: grade_b3_plumbing(ROOT / "b3_clinic_plumbing" / arm, ROOT / "b2_clinic_arch" / arm)
            for arm in ("baseline", "edl")
        }
    if "b4" in targets:
        out["b4_clinic_electrical"] = {
            arm: grade_b4_electrical(
                ROOT / "b4_clinic_electrical" / arm,
                ROOT / "b2_clinic_arch" / arm,
                ROOT / "b3_clinic_plumbing" / arm,
            )
            for arm in ("baseline", "edl")
        }
    if "b5" in targets:
        out["b5_hospital_sld"] = {
            arm: grade_b5_sld(ROOT / "b5_hospital_sld" / arm) for arm in ("baseline", "edl")
        }
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
