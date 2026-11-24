# Bake-Off Raw Output: Track A (Block-Declarative EDL) — Pro

## Task 1

```edl
project "Apartment_B" {
    units: mm
    frame FloorPlan: Frame2

    architecture.defaults {
        measurement_basis: clear_interior
        exterior_wall: 200mm
        interior_wall: 120mm
    }

    room Hall {
        clear_size: (1500mm, 3600mm)
        anchor: southwest at FloorPlan.origin
    }

    room Living {
        clear_size: (4200mm, 3600mm)
    }

    room Bedroom {
        clear_size: (3400mm, 3000mm)
    }

    room Bath {
    }

    layout FloorPlan {
        require Living.west == Hall.east + 120mm
        require Living.south == Hall.south

        require Bedroom.south == Living.north + 120mm
        require Bedroom.east == Living.east

        require Bath.south == Hall.north + 120mm
        require Bath.west == Hall.west
        require Bath.east == Bedroom.west - 120mm
        require Bath.north == Bedroom.north
    }

    opening D_Entry: door {
        host: exterior(Hall, south)
        width: 900mm
        align: center
        hinge: start
        swing: into Hall
    }

    opening D_Living: door {
        host: shared_wall(Hall, Living)
        width: 850mm
        offset: 500mm from start
        hinge: start
        swing: into Living
    }

    opening D_Bath: door {
        host: shared_wall(Hall, Bath)
        width: 750mm
        align: center
        hinge: start
        swing: into Bath
    }

    opening D_Bed: door {
        host: shared_wall(Living, Bedroom)
        width: 800mm
        offset: 300mm from start
        hinge: start
        swing: into Bedroom
    }

    opening W_Living: window {
        host: exterior(Living, east)
        width: 2000mm
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

## Task 2

1. `Bath`: `3000mm × 1480mm`, bounds `(4620mm, 2520mm, 7620mm, 4000mm)`.
2. Exterior footprint: `8020mm × 4400mm`.
3. `Living` width `+500mm`: `Kitchen`, `Bath`, partitions, `D_Kitchen`, `D_Bath`, east wall shift `+500mm`; `W_Living` shifts `+250mm`; `D_Entry` & west/south/north anchors stay fixed.

## Task 3

2 exact surgical token edits (`4500mm → 5200mm`, `1800mm → 2200mm`), 0 unrelated lines changed.

## Task 4

- `D_Bath` south jamb: `40mm < 100mm` (`-60mm`).
- `D_Kitchen` north jamb: `50mm < 100mm` (`-50mm`).
- 90° quarter-circle swing arcs: No overlap (`210mm` vertical gap).
- Minimal repaired snippet: `D_Kitchen` offset `1500mm` (preserving `hinge: end`, `swing: into Living`), `D_Bath` offset `100mm`.
