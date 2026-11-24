# Cheat Sheet — Track A: EDL Candidate A (Block-Declarative)

## 1-Page Grammar Summary
- Top-level container: `project "<Name>" { ... }`
- Units & frames: `units: mm` (or `m`), `frame <Name>: Frame2`
- Domain defaults block:
  ```edl
  architecture.defaults {
      measurement_basis: clear_interior
      exterior_wall: 200mm
      interior_wall: 120mm
  }
  ```
- Room block (`clear_size` is optional if constrained in `layout`):
  ```edl
  room <Id> {
      clear_size: (<width>, <depth>)
      anchor: <corner> at <Frame>.origin   // optional
  }
  ```
- Linear layout block (edges: `.west`, `.east`, `.south`, `.north`; corners: `.southwest`, etc.):
  ```edl
  layout <Frame> {
      require <expr> == <expr>
      prefer  <expr> == <expr>
  }
  ```
- Openings hosted on derived walls (`exterior(<Room>, <side>)` or `shared_wall(<RoomA>, <RoomB>)`):
  - Segment direction (`start` → `end`): west-to-east for horizontal walls, south-to-north for vertical walls.
  - Placement: either `align: center` OR `offset: <len> from start` (or `from end`).
  - Doors also specify `hinge: start | end` and `swing: into <Room>`.
  ```edl
  opening <Id>: door | window {
      host: exterior(<Room>, south|north|east|west) | shared_wall(<RoomA>, <RoomB>)
      width: <len>
      align: center | offset: <len> from start
      hinge: start | end          // door only
      swing: into <Room>          // door only
  }
  ```
- Verification block: `verify { require <rule> ... }` (rules: `no_room_overlap`, `opening_jamb_margin >= <len>`, `no_door_swing_collision`).
- View block: `view <Id>: PlanView { frame: <Frame>, sheet: A3 landscape, scale: 1:50, show_dimensions: true, show_labels: true }`.

---

## Worked Example: `Studio_A`

```edl
project "Studio_A" {
    units: mm
    frame FloorPlan: Frame2

    architecture.defaults {
        measurement_basis: clear_interior
        exterior_wall: 200mm
        interior_wall: 120mm
    }

    room Living {
        clear_size: (4500mm, 4000mm)
        anchor: southwest at FloorPlan.origin
    }

    room Kitchen {
        clear_size: (3000mm, 2400mm)
    }

    room Bath {
    }

    layout FloorPlan {
        require Kitchen.west == Living.east + 120mm
        require Kitchen.south == Living.south

        require Bath.west == Kitchen.west
        require Bath.east == Kitchen.east
        require Bath.south == Kitchen.north + 120mm
        require Bath.north == Living.north
    }

    opening D_Entry: door {
        host: exterior(Living, south)
        width: 900mm
        offset: 600mm from start
        hinge: start
        swing: into Living
    }

    opening D_Kitchen: door {
        host: shared_wall(Living, Kitchen)
        width: 800mm
        align: center
        hinge: start
        swing: into Kitchen
    }

    opening D_Bath: door {
        host: shared_wall(Living, Bath)
        width: 750mm
        offset: 300mm from start
        hinge: start
        swing: into Living
    }

    opening W_Living: window {
        host: exterior(Living, north)
        width: 1800mm
        align: center
    }

    verify {
        require no_room_overlap
        require opening_jamb_margin >= 100mm
        require no_door_swing_collision
    }

    view GroundPlan: PlanView {
        frame: FloorPlan
        sheet: A3 landscape
        scale: 1:50
        show_dimensions: true
        show_labels: true
    }
}
```
