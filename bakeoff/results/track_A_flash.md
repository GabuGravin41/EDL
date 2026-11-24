# Bake-Off Raw Output: Track A (Block-Declarative EDL) — Flash

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

1. **`Bath` solved clear dimensions and bounds:**
   - Clear dimensions (`width × depth`): **`3000mm × 1480mm`**
   - Bounds `(min_x, min_y, max_x, max_y)`: **`(4620mm, 2520mm, 7620mm, 4000mm)`**

2. **Overall exterior footprint:**
   - **`8020mm × 4400mm`**

3. **Effect of increasing `Living` clear width from `4500mm` to `5000mm` (`+500mm`):**
   - `Living.east`, `Kitchen`, `Bath`, `D_Kitchen`, `D_Bath`, partitions, east exterior wall shift east `+500mm`; `W_Living` shifts east `+250mm`; total width `8020 → 8520mm`.
   - `Living.west/south/north`, `D_Entry`, room sizes of `Kitchen` & `Bath`, and all `y`-coordinates stay unchanged.

## Task 3

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
        clear_size: (5200mm, 4000mm)
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
        width: 2200mm
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

## Task 4

- Violation 1: `D_Bath` south jamb `40mm < 100mm` (short by `60mm`).
- Violation 2: `D_Kitchen` north jamb `50mm < 100mm` (short by `50mm`).
- Door-swing arc overlap check: No violation (`210mm` vertical gap).
- Minimal repaired snippet: `D_Kitchen` offset `1500mm`, `D_Bath` offset `100mm`.
