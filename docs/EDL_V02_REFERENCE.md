# EDL v0.2 — 1-Page Language & CLI Reference

## CLI Commands
- `./target/release/edl build <file.edl> [--json]` — Parse, type-check, solve linear layout, derive walls/openings/fixtures, run verification rules, emit Canonical IR + diagnostics + evidence.
- `./target/release/edl preview <file.edl> -o <out.svg> [--annotated]` — Render architectural SVG (red highlights on any violations).
- `./target/release/edl export <file.edl> --dxf <out.dxf>` — Export layered CAD DXF (`WALLS_EXT`, `WALLS_INT`, `ROOMS`, `DOORS`, `WINDOWS`, `PASSAGES`, `FIXTURES`, `ANNOTATIONS`).
- `./target/release/edl diff <a.edl> <b.edl> [--json]` — Semantic revision diff.

## Candidate A Grammar (v0.2)
```edl
project "<Name>" {
    units: mm                               // mm | cm | m | in | ft
    frame FloorPlan: Frame2                 // +X = east, +Y = north

    architecture.defaults {
        measurement_basis: clear_interior
        exterior_wall: 200mm
        interior_wall: 120mm
    }

    room <Id> {
        role: habitable | circulation | wet | service | outdoor   // optional
        clear_size: (<width>, <depth>)                            // optional if constrained in layout
        anchor: southwest | southeast | northwest | northeast at FloorPlan.origin
    }

    layout FloorPlan {
        // Linear constraints on room edges (.west, .east, .south, .north) and dims (.width, .depth)
        require <RoomA>.west == <RoomB>.east + 120mm
        require <RoomA>.south == <RoomB>.south
    }

    opening <Id>: door | window | passage {
        host: exterior(<Room>, north|south|east|west) | shared_wall(<RoomA>, <RoomB>)
        width: <len>
        align: center | offset: <len> from start | offset: <len> from end
        // Door only:
        hinge: start | end                  // start = west (horiz wall) or south (vert wall)
        swing: into <Room>
    }

    fixture <Id>: bed | sofa | counter | table | toilet | vanity | shower | wardrobe | desk {
        room: <RoomId>
        size: (<width_along_wall>, <depth_into_room>)
        against: north | south | east | west
        align: center | offset: <len> from start | offset: <len> from end
        clearance_front: <len>              // optional; projects inward from front face
    }

    verify {
        require no_room_overlap
        require opening_jamb_margin >= 100mm
        require no_door_swing_collision
        require all_rooms_reachable
        require no_fixture_collision
        require habitable_rooms_have_window
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
