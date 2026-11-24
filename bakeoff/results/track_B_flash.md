# Bake-Off Raw Output: Track B (Relational/Flat EDL) — Flash

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
require Bath.east      == Bedroom.west - 120mm
require Bath.north     == Bedroom.north

let D_Entry  = door(host = exterior(Hall, south),       width = 900mm, align = center, hinge = start, swing = Hall)
let D_Living = door(host = shared(Hall, Living),        width = 850mm, offset = 500mm, hinge = start, swing = Living)
let D_Bath   = door(host = shared(Hall, Bath),          width = 750mm, align = center, hinge = start, swing = Bath)
let D_Bed    = door(host = shared(Living, Bedroom),     width = 800mm, offset = 300mm, hinge = start, swing = Bedroom)
let W_Living = window(host = exterior(Living, east),    width = 2000mm, align = center)

verify no_room_overlap
verify opening_jamb_margin >= 100mm
verify no_door_swing_collision

view PlanView = plan_view(frame = FloorPlan, sheet = A3_landscape, scale = 1:50, dimensions = true, labels = true)
```

## Task 2

1. `Bath`: `3000mm × 1480mm`, bounds `(4620, 2520, 7620, 4000)` mm.
2. Exterior footprint: `8020mm × 4400mm`.
3. Increasing `Living` width `4500 → 5000mm` (`+500mm`): `Kitchen`, `Bath`, `D_Kitchen`, `D_Bath`, partitions, east wall shift `+500mm` east; `W_Living` shifts `+250mm` east; `D_Entry` and west/south/north anchors stay fixed.

## Task 3

```edl
project "Studio_A" (units = mm)
frame FloorPlan: Frame2

defaults arch (
    basis = clear_interior,
    exterior_wall = 200mm,
    interior_wall = 120mm
)

let Living  = room(width = 5200mm, depth = 4000mm)
let Kitchen = room(width = 3000mm, depth = 2400mm)
let Bath    = room()

require Living.southwest == FloorPlan.origin
require Kitchen.west  == Living.east + 120mm
require Kitchen.south == Living.south
require Bath.west     == Kitchen.west
require Bath.east     == Kitchen.east
require Bath.south    == Kitchen.north + 120mm
require Bath.north    == Living.north

let D_Entry   = door(host = exterior(Living, south), width = 900mm, offset = 600mm, hinge = start, swing = Living)
let D_Kitchen = door(host = shared(Living, Kitchen), width = 800mm, align = center, hinge = start, swing = Kitchen)
let D_Bath    = door(host = shared(Living, Bath),    width = 750mm, offset = 300mm, hinge = start, swing = Living)
let W_Living  = window(host = exterior(Living, north), width = 2200mm, align = center)

verify no_room_overlap
verify opening_jamb_margin >= 100mm
verify no_door_swing_collision

view GroundPlan = plan_view(frame = FloorPlan, sheet = A3_landscape, scale = 1:50, dimensions = true, labels = true)
```

## Task 4

- `D_Bath` start jamb: `40mm < 100mm` (short by `60mm`).
- `D_Kitchen` end jamb: `50mm < 100mm` (short by `50mm`).
- Door-swing overlap: No violation (`210mm` gap).
- Repaired: `D_Kitchen` offset `1500mm`, `D_Bath` offset `100mm`.
