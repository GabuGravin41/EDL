# Bake-Off Raw Output: Track B (Relational/Flat EDL) — Pro

## Task 1

```edl
project "Apartment_B" (units = mm)
frame FloorPlan: Frame2

defaults arch (
    basis = clear_interior,
    exterior_wall = 200mm,
    interior_wall = 120mm
)

let Hall    = room(width = 1500mm, depth = 3600mm)
let Living  = room(width = 4200mm, depth = 3600mm)
let Bedroom = room(width = 3400mm, depth = 3000mm)
let Bath    = room()

require Hall.southwest == FloorPlan.origin
require Living.west    == Hall.east + 120mm
require Living.south   == Hall.south
require Bedroom.south  == Living.north + 120mm
require Bedroom.east   == Living.east
require Bath.south     == Hall.north + 120mm
require Bath.west      == Hall.west
require Bedroom.west   == Bath.east + 120mm
require Bath.north     == Bedroom.north

let D_Entry  = door(host = exterior(Hall, south),   width = 900mm,  align = center, hinge = start, swing = Hall)
let D_Living = door(host = shared(Hall, Living),    width = 850mm,  offset = 500mm, hinge = start, swing = Living)
let D_Bath   = door(host = shared(Hall, Bath),      width = 750mm,  align = center, hinge = start, swing = Bath)
let D_Bed    = door(host = shared(Living, Bedroom), width = 800mm,  offset = 300mm, hinge = start, swing = Bedroom)
let W_Living = window(host = exterior(Living, east), width = 2000mm, align = center)

verify no_room_overlap
verify opening_jamb_margin >= 100mm
verify no_door_swing_collision

view PlanView = plan_view(frame = FloorPlan, sheet = A3_landscape, scale = 1:50, dimensions = true, labels = true)
```

## Task 2

1. `Bath`: `3000mm × 1480mm`, bounds `(4620, 2520, 7620, 4000)` mm.
2. Exterior footprint: `8020mm × 4400mm`.
3. `Living` width `+500mm`: `Kitchen`, `Bath`, partitions, `D_Kitchen`, `D_Bath`, east wall shift `+500mm`; `W_Living` shifts `+250mm`; `D_Entry` & west/south/north anchors stay fixed.

## Task 3

2 exact surgical token edits (`4500mm → 5200mm`, `1800mm → 2200mm`), 0 unrelated lines changed.

## Task 4

- `D_Bath` south jamb: `40mm < 100mm` (`-60mm`).
- `D_Kitchen` north jamb: `50mm < 100mm` (`-50mm`).
- Door-swing overlap: No violation (`210mm` gap).
- Repaired: `D_Kitchen` offset `1500mm` (preserving `hinge = end, swing = Living`), `D_Bath` offset `100mm`.
