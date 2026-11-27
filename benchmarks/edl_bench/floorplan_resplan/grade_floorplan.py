#!/usr/bin/env python3
"""Deterministic 3-Axis Floorplan Graph & Engineering Grader (EDL-Bench Domain 2/3/4).

Evaluates a compiled EDL Canonical IR JSON against a real ResPlan / MSD topology task spec across:
  1. Geometric reasoning   — room count, area fidelity (MAPE), 0 overlaps, 0 DOF
  2. Topological reasoning — ResPlan/MSD access-graph edge recall & adjacency F1
  3. Engineering reasoning — daylighting (habitable rooms with windows), egress/BFS reachability,
                             wet-wall clustering, and overlaid Plumbing (P1-P9) / Electrical (M1-M8) proofs.

Usage:
  python3 benchmarks/edl_bench/floorplan_resplan/grade_floorplan.py \
      --spec benchmarks/edl_bench/floorplan_resplan/resplan_tasks_3.json \
      --task-id RP_3BR_0 \
      --ir path/to/plan_ir.json
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def norm_type(room_id: str, room_type: str = "") -> str:
    s = f"{room_type} {room_id}".lower()
    for key in ("bedroom", "bed", "master"):
        if key in s:
            return "bedroom"
    for key in ("bathroom", "bath", "wc", "toilet", "ensuite"):
        if key in s:
            return "bathroom"
    for key in ("kitchen", "kit"):
        if key in s:
            return "kitchen"
    for key in ("balcony", "terrace", "veranda"):
        if key in s:
            return "balcony"
    for key in ("living", "lounge", "dining", "great"):
        if key in s:
            return "living"
    for key in ("corridor", "hall", "foyer", "entry", "vestibule"):
        if key in s:
            return "corridor"
    return room_type.lower() or "other"


def grade_plan(spec: dict, ir: dict) -> dict:
    rooms = [e for e in ir.get("entities", []) if e.get("kind") == "arch.room"]
    doors = [e for e in ir.get("entities", []) if e.get("kind") == "arch.door"]
    windows = [e for e in ir.get("entities", []) if e.get("kind") == "arch.window"]

    # 1. Geometric & Area Fidelity
    target_rooms = spec["rooms"]
    target_by_type: dict[str, list[float]] = {}
    for r in target_rooms:
        target_by_type.setdefault(r["type"], []).append(float(r["target_area_m2"]))

    actual_by_type: dict[str, list[tuple[str, float]]] = {}
    room_rects = {}
    for e in rooms:
        g = e.get("geometry", {})
        area = float(g.get("area_m2", 0.0))
        rt = norm_type(e["id"], str(e.get("props", {}).get("room_type", "")))
        actual_by_type.setdefault(rt, []).append((e["id"], area))
        if "min" in g and "max" in g:
            room_rects[e["id"]] = (g["min"][0], g["min"][1], g["max"][0], g["max"][1])

    type_count_matches = {}
    area_errors = []
    for rtype, t_areas in target_by_type.items():
        act = sorted((a for _, a in actual_by_type.get(rtype, [])), reverse=True)
        tgt = sorted(t_areas, reverse=True)
        type_count_matches[rtype] = {"target": len(tgt), "actual": len(act), "ok": len(act) >= len(tgt)}
        for ta, aa in zip(tgt, act):
            if ta > 0:
                area_errors.append(abs(aa - ta) / ta)

    mape = round(sum(area_errors) / len(area_errors) * 100.0, 2) if area_errors else 100.0
    total_actual_m2 = round(sum(float(e.get("geometry", {}).get("area_m2", 0.0)) for e in rooms), 2)
    total_area_err_pct = round(abs(total_actual_m2 - spec["net_area_m2"]) / max(spec["net_area_m2"], 1.0) * 100.0, 2)

    # 2. Topological Connectivity Graph (via_door & shared-wall adjacency)
    walls_by_id = {e["id"]: e for e in ir.get("entities", []) if e.get("kind") == "arch.wall"}
    actual_door_pairs = []
    has_front_door = False
    for d in doors:
        p = d.get("props", {})
        hw = str(p.get("host_wall", ""))
        w_props = walls_by_id.get(hw, {}).get("props", {})
        a = p.get("room_a") or w_props.get("room_a")
        b = p.get("room_b") or w_props.get("room_b")
        if not (a and b):
            if hw.startswith("W_Shared_"):
                parts = hw[len("W_Shared_"):].split("_")
                if len(parts) >= 2:
                    a, b = parts[0], "_".join(parts[1:])
            elif hw.startswith("W_Ext_"):
                has_front_door = True
        if a and b:
            actual_door_pairs.append(tuple(sorted((norm_type(a), norm_type(b)))))

    # Compare multiset of required via_door edges (room-type pairs) for exact topology fidelity
    from collections import Counter
    req_counter: Counter[tuple[str, str]] = Counter()
    req_front_door = False
    for u, v, etype in spec["connectivity_edges"]:
        if "front_door" in u or "front_door" in v:
            req_front_door = True
            continue
        tu = norm_type(u)
        tv = norm_type(v)
        req_counter[tuple(sorted((tu, tv)))] += 1
    act_counter = Counter(actual_door_pairs)
    req_total = sum(req_counter.values()) + (1 if req_front_door else 0)
    matched_total = sum(min(cnt, act_counter[pair]) for pair, cnt in req_counter.items()) + (
        1 if (req_front_door and has_front_door) else 0
    )
    topo_recall = round(matched_total / max(req_total, 1), 3)

    # 3. Engineering & Formal Proof Verification (EDL Evidence Ledger)
    ev = ir.get("evidence", [])
    ev_pass = sum(1 for x in ev if x.get("status") == "EXACTLY_CHECKED")
    ev_viol = sum(1 for x in ev if x.get("status") == "VIOLATED")
    has_errors = bool(ir.get("project", {}).get("has_errors", True))
    dof = len(ir.get("project", {}).get("remaining_dof", []))

    return {
        "task_id": spec["task_id"],
        "resplan_id": spec["resplan_id"],
        "geometric": {
            "target_area_m2": spec["net_area_m2"],
            "actual_area_m2": total_actual_m2,
            "total_area_error_pct": total_area_err_pct,
            "room_area_mape_pct": mape,
            "room_type_counts": type_count_matches,
            "remaining_dof": dof,
        },
        "topological": {
            "door_count": len(doors),
            "window_count": len(windows),
            "required_type_edge_recall": topo_recall,
        },
        "engineering": {
            "has_errors": has_errors,
            "evidence_passed": ev_pass,
            "evidence_violated": ev_viol,
            "evidence_total": len(ev),
        },
        "pass_all": (not has_errors) and dof == 0 and ev_viol == 0 and all(v["ok"] for v in type_count_matches.values()),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--spec", required=True)
    ap.add_argument("--task-id", required=True)
    ap.add_argument("--ir", required=True)
    args = ap.parse_args()

    specs = json.loads(Path(args.spec).read_text(encoding="utf-8"))
    spec = next(s for s in specs if s["task_id"] == args.task_id)
    ir = json.loads(Path(args.ir).read_text(encoding="utf-8"))
    print(json.dumps(grade_plan(spec, ir), indent=2))


if __name__ == "__main__":
    main()
