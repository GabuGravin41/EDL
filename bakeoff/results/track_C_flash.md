# Bake-Off Raw Output: Track C (Python Control Baseline) — Flash

## Task 1

```python
EXT_WALL = 200
INT_WALL = 120

hall_w, hall_d = 1500, 3600
living_w, living_d = 4200, 3600
bed_w, bed_d = 3400, 3000

hall_rect = (0, 0, hall_w, hall_d)
living_rect = (hall_rect[2] + INT_WALL, 0, hall_rect[2] + INT_WALL + living_w, living_d)
bedroom_rect = (
    living_rect[2] - bed_w,
    living_rect[3] + INT_WALL,
    living_rect[2],
    living_rect[3] + INT_WALL + bed_d,
)
bath_rect = (
    hall_rect[0],
    hall_rect[3] + INT_WALL,
    bedroom_rect[0] - INT_WALL,
    bedroom_rect[3],
)

rooms = {
    "Hall":    hall_rect,
    "Living":  living_rect,
    "Bedroom": bedroom_rect,
    "Bath":    bath_rect,
}

plan_w = rooms["Living"][2]
plan_d = rooms["Bedroom"][3]

walls = {
    "W_Ext_South":    (-EXT_WALL, -EXT_WALL, plan_w + EXT_WALL, 0, EXT_WALL, "exterior"),
    "W_Ext_North":    (-EXT_WALL, plan_d, plan_w + EXT_WALL, plan_d + EXT_WALL, EXT_WALL, "exterior"),
    "W_Ext_West":     (-EXT_WALL, 0, 0, plan_d, EXT_WALL, "exterior"),
    "W_Ext_East":     (plan_w, 0, plan_w + EXT_WALL, plan_d, EXT_WALL, "exterior"),
    "W_Hall_Living":  (rooms["Hall"][2], 0, rooms["Living"][0], rooms["Hall"][3], INT_WALL, "interior"),
    "W_South_North":  (0, rooms["Hall"][3], plan_w, rooms["Bath"][1], INT_WALL, "interior"),
    "W_Bath_Bedroom": (rooms["Bath"][2], rooms["Bath"][1], rooms["Bedroom"][0], rooms["Bedroom"][3], INT_WALL, "interior"),
}

living_bed_x0 = max(rooms["Living"][0], rooms["Bedroom"][0])
living_bed_len = min(rooms["Living"][2], rooms["Bedroom"][2]) - living_bed_x0

openings = {
    "D_Entry":  {"kind": "door", "wall": "exterior(Hall, south)", "seg_start": (0, 0), "seg_len": hall_w, "axis": "x", "offset": (hall_w - 900) / 2, "width": 900, "hinge": "start", "swing": "Hall"},
    "D_Living": {"kind": "door", "wall": "shared(Hall, Living)", "seg_start": (rooms["Hall"][2], 0), "seg_len": hall_d, "axis": "y", "offset": 500, "width": 850, "hinge": "start", "swing": "Living"},
    "D_Bath":   {"kind": "door", "wall": "shared(Hall, Bath)", "seg_start": (0, rooms["Hall"][3]), "seg_len": hall_w, "axis": "x", "offset": (hall_w - 750) / 2, "width": 750, "hinge": "start", "swing": "Bath"},
    "D_Bed":    {"kind": "door", "wall": "shared(Living, Bedroom)", "seg_start": (living_bed_x0, rooms["Living"][3]), "seg_len": living_bed_len, "axis": "x", "offset": 300, "width": 800, "hinge": "start", "swing": "Bedroom"},
    "W_Living": {"kind": "window", "wall": "exterior(Living, east)", "seg_start": (rooms["Living"][2], 0), "seg_len": living_d, "axis": "y", "offset": (living_d - 2000) / 2, "width": 2000},
}


def rects_overlap(a, b):
    return a[0] < b[2] and a[2] > b[0] and a[1] < b[3] and a[3] > b[1]


def verify_no_room_overlap(room_map):
    names = list(room_map.keys())
    for i in range(len(names)):
        for j in range(i + 1, len(names)):
            assert not rects_overlap(room_map[names[i]], room_map[names[j]])
    return True


def verify_opening_jamb_margin(opening_map, min_margin=100):
    for name, op in opening_map.items():
        start_margin = op["offset"]
        end_margin = op["seg_len"] - (op["offset"] + op["width"])
        assert start_margin >= min_margin and end_margin >= min_margin
    return True


def door_swing_bbox(op, room_map):
    sx, sy = op["seg_start"]
    w = op["width"]
    rx0, ry0, rx1, ry1 = room_map[op["swing"]]
    if op["axis"] == "x":
        x0 = sx + op["offset"]
        x1 = x0 + w
        y0, y1 = (ry0, ry0 + w) if sy <= ry0 else (ry1 - w, ry1)
    else:
        y0 = sy + op["offset"]
        y1 = y0 + w
        x0, x1 = (rx0, rx0 + w) if sx <= rx0 else (rx1 - w, rx1)
    return (x0, y0, x1, y1)


def verify_no_door_swing_collision(opening_map, room_map):
    doors = [(n, op) for n, op in opening_map.items() if op["kind"] == "door"]
    for i in range(len(doors)):
        for j in range(i + 1, len(doors)):
            n1, d1 = doors[i]
            n2, d2 = doors[j]
            if d1["swing"] == d2["swing"]:
                assert not rects_overlap(
                    door_swing_bbox(d1, room_map),
                    door_swing_bbox(d2, room_map),
                )
    return True


checks = {
    "no_room_overlap": verify_no_room_overlap(rooms),
    "opening_jamb_margin_min_mm": 100,
    "opening_jamb_margins_ok": verify_opening_jamb_margin(openings, 100),
    "no_door_swing_collision": verify_no_door_swing_collision(openings, rooms),
}

VIEW = {
    "name": "PlanView",
    "frame": "FloorPlan",
    "sheet": "A3_landscape",
    "scale": "1:50",
    "dimensions": True,
    "labels": True,
}
```

## Task 2

1. `Bath`: `3000mm × 1480mm`, bounds `(4620, 2520, 7620, 4000)`.
2. Exterior footprint: `8020mm × 4400mm`.
3. `Living` width `4500 → 5000mm`: `Kitchen`, `Bath`, partitions, `D_Kitchen`, `D_Bath` shift `+500mm`; `W_Living` shifts `+250mm` if centered; `D_Entry` unchanged.

## Task 3

3 separate lines had to be modified (`living_w`, `D_Entry.seg_len`, and `W_Living.seg_len` + `offset` + `width`).

## Task 4

- `D_Bath` start jamb `40mm < 100mm` (`-60mm`).
- `D_Kitchen` end jamb `50mm < 100mm` (`-50mm`).
- 90° quarter-circle sectors do not overlap (`210mm` gap).
- Repaired snippet: changed `D_Kitchen` offset to `1500` AND unnecessarily flipped `swing` back to `"Kitchen"`.
