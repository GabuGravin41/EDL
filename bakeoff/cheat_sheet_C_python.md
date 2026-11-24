# Cheat Sheet — Track C: Control Baseline (Pure Python Builder / Coordinates)

## 1-Page Convention Summary
Represent the 2D floor plan as a clean, self-contained Python script (standard library only, millimeter coordinates in `FloorPlan` frame where `+X = east`, `+Y = north`).
- Define room rectangles `(x0, y0, x1, y1)` using `clear_interior` dimensions (`exterior_wall = 200`, `interior_wall = 120`).
- Define wall segments/rectangles derived from rooms or explicit coordinates.
- Define openings (doors/windows) with their host wall, exact position along the wall (or coordinates), width, hinge point, and swing direction/arc parameters.
- Include explicit check functions/assertions verifying:
  - `no_room_overlap`
  - `opening_jamb_margin >= 100`
  - `no_door_swing_collision`
- Define a `VIEW` config dict (`sheet = "A3_landscape"`, `scale = "1:50"`).

---

## Worked Example: `Studio_A`

```python
EXT_WALL = 200
INT_WALL = 120

# Rooms as (x0, y0, x1, y1) in mm (clear interior)
living_w, living_d = 4500, 4000
kitchen_w, kitchen_d = 3000, 2400

rooms = {
    "Living":  (0, 0, living_w, living_d),
    "Kitchen": (living_w + INT_WALL, 0, living_w + INT_WALL + kitchen_w, kitchen_d),
    "Bath":    (living_w + INT_WALL, kitchen_d + INT_WALL, living_w + INT_WALL + kitchen_w, living_d),
}

# Wall segments: (x0, y0, x1, y1, thickness, kind)
walls = {
    "W_Ext_South":     (-EXT_WALL, -EXT_WALL, rooms["Kitchen"][2] + EXT_WALL, 0, EXT_WALL, "exterior"),
    "W_Ext_North":     (-EXT_WALL, living_d, rooms["Bath"][2] + EXT_WALL, living_d + EXT_WALL, EXT_WALL, "exterior"),
    "W_Ext_West":      (-EXT_WALL, 0, 0, living_d, EXT_WALL, "exterior"),
    "W_Ext_East":      (rooms["Kitchen"][2], 0, rooms["Kitchen"][2] + EXT_WALL, living_d, EXT_WALL, "exterior"),
    "W_Living_Right":  (living_w, 0, living_w + INT_WALL, living_d, INT_WALL, "interior"),
    "W_Kitchen_Bath":  (living_w + INT_WALL, kitchen_d, rooms["Kitchen"][2], kitchen_d + INT_WALL, INT_WALL, "interior"),
}

openings = {
    "D_Entry":   {"kind": "door", "wall": "exterior(Living, south)", "seg_start": (0, 0), "seg_len": 4500, "offset": 600, "width": 900, "hinge": "start", "swing": "Living"},
    "D_Kitchen": {"kind": "door", "wall": "shared(Living, Kitchen)", "seg_start": (living_w, 0), "seg_len": 2400, "offset": (2400 - 800) / 2, "width": 800, "hinge": "start", "swing": "Kitchen"},
    "D_Bath":    {"kind": "door", "wall": "shared(Living, Bath)", "seg_start": (living_w, kitchen_d + INT_WALL), "seg_len": living_d - (kitchen_d + INT_WALL), "offset": 300, "width": 750, "hinge": "start", "swing": "Living"},
    "W_Living":  {"kind": "window", "wall": "exterior(Living, north)", "seg_start": (0, living_d), "seg_len": 4500, "offset": (4500 - 1800) / 2, "width": 1800},
}

checks = {
    "no_room_overlap": True,
    "opening_jamb_margin_min_mm": 100,
    "no_door_swing_collision": True,
}

view = {"name": "GroundPlan", "frame": "FloorPlan", "sheet": "A3_landscape", "scale": "1:50", "dimensions": True, "labels": True}
```
