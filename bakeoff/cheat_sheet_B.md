# Cheat Sheet — Track B: EDL Candidate B (Relational / Flat)

## 1-Page Grammar Summary
- Top-level header statements (no outer braces):
  - `project "<Name>" (units = mm)`
  - `frame <Name>: Frame2`
  - `defaults arch (basis = clear_interior, exterior_wall = 200mm, interior_wall = 120mm)`
- Room declarations (`width` and `depth` are optional if determined by `require` constraints):
  - `let <Id> = room(width = <len>, depth = <len>)`
  - `let <Id> = room()`
- Linear constraints (edges: `.west`, `.east`, `.south`, `.north`; corners: `.southwest`, etc.):
  - `require <expr> == <expr>`
  - `prefer  <expr> == <expr>`
- Hosted openings (`exterior(<Room>, south|north|east|west)` or `shared(<RoomA>, <RoomB>)`):
  - Segment direction (`start` → `end`): west-to-east for horizontal walls, south-to-north for vertical walls.
  - Placement: `align = center` OR `offset = <len>` (measured from `start`, or `offset_end = <len>` from `end`).
  - `let <Id> = door(host = ..., width = <len>, offset = <len> | align = center, hinge = start | end, swing = <Room>)`
  - `let <Id> = window(host = ..., width = <len>, offset = <len> | align = center)`
- Verification statements:
  - `verify no_room_overlap`
  - `verify opening_jamb_margin >= <len>`
  - `verify no_door_swing_collision`
- View statement:
  - `view <Id> = plan_view(frame = <Frame>, sheet = A3_landscape, scale = 1:50, dimensions = true, labels = true)`

---

## Worked Example: `Studio_A`

```edl
project "Studio_A" (units = mm)
frame FloorPlan: Frame2

defaults arch (
    basis = clear_interior,
    exterior_wall = 200mm,
    interior_wall = 120mm
)

let Living  = room(width = 4500mm, depth = 4000mm)
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
let W_Living  = window(host = exterior(Living, north), width = 1800mm, align = center)

verify no_room_overlap
verify opening_jamb_margin >= 100mm
verify no_door_swing_collision

view GroundPlan = plan_view(frame = FloorPlan, sheet = A3_landscape, scale = 1:50, dimensions = true, labels = true)
```
