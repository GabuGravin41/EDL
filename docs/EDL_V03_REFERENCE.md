# EDL v0.3 — Unified Language & CLI Reference (Architecture + Electronics)

## CLI Commands
- `./target/release/edl build <file.edl> [--json]` — Parse, type-check, solve/route, run verification rules, emit Canonical IR + diagnostics + evidence.
- `./target/release/edl preview <file.edl> -o <out.svg> [--annotated]` — Render architectural floor plan SVG or electronics schematic SVG (red highlights on any violations).
- `./target/release/edl export <file.edl> --dxf <out.dxf>` — Export layered AutoCAD DXF (architecture).
- `./target/release/edl export <file.edl> --kicad <out.kicad_sch>` — Export KiCad 8/9 S-expression schematic (electronics; validated with `kicad-cli sch erc`).
- `./target/release/edl diff <a.edl> <b.edl> [--json]` — Semantic revision diff.

---

## Domain 1 — 2D Architectural Floor Plans (`arch`)

```edl
project "<Name>" {
    units: mm                               // mm | cm | m | in | ft
    frame FloorPlan: Frame2                 // +X = east, +Y = north
    parameter corridor_w: 1500mm            // optional named length parameters

    architecture.defaults {
        measurement_basis: clear_interior
        exterior_wall: 200mm
        interior_wall: 120mm
    }

    room <Id> {
        role: habitable | circulation | wet | service | outdoor   // optional
        clear_size: (<width>, <depth>)                            // optional if solved in layout
        anchor: southwest | southeast | northwest | northeast at FloorPlan.origin
    }

    layout FloorPlan {
        // Linear constraints on .west, .east, .south, .north, .width, .depth
        // Built-in parameters: interior_wall, exterior_wall (plus any user `parameter`)
        require <RoomA>.west == <RoomB>.east + interior_wall
        require <RoomA>.south == <RoomB>.south
    }

    opening <Id>: door | window | passage {
        host: exterior(<Room>, north|south|east|west) | shared_wall(<RoomA>, <RoomB>)
        width: <len>
        // Placement — use ONE of the following standalone keys:
        align: center
        // OR: offset: <len> from start
        // OR: offset: <len> from end
        // Door only:
        hinge: start | end                  // start = west (horiz wall) or south (vert wall)
        swing: into <Room>
    }

    fixture <Id>: bed | sofa | counter | table | toilet | vanity | shower | wardrobe | desk {
        room: <RoomId>
        size: (<width>, <depth>)
        // Option A (wall-anchored):
        against: north | south | east | west
        align: center                       // OR: offset: <len> from start | end
        clearance_front: <len>              // optional
        // Option B (freestanding — omit `against`):
        // at: center                       // OR: at: (<x>, <y>) relative to room southwest
        // clearance_all: <len>             // optional 4-sided clearance
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

---

## Domain 2 — Electronics Schematics (`elec`)

Supported electrical units: `V`, `mV`, `kV`, `A`, `mA`, `uA`, `ohm`, `kohm`, `Mohm`, `F`, `mF`, `uF`, `nF`, `pF`, `Hz`, `kHz`, `MHz`, `W`, `mW`.

```edl
project "<Name>" {
    units: mm
    frame Sheet: Frame2

    circuit <CircuitName> {
        component <RefDes>: connector | regulator | ic | resistor | capacitor | inductor | diode | led | switch | crystal | transistor {
            value: "LM7805"                 // string OR dimensional quantity (e.g. 10uF, 330ohm)
            footprint: "Package:Name"       // optional KiCad footprint string
            pin <PinName>: power_out | power_in | ground | input | output | bidir | passive | nc
            // Optional voltage spec and/or explicit pin-side override:
            //   pin VIN: power_out (12V)
            //   pin IN:  power_in (7V..35V)
            //   pin SWDIO: bidir side: west
            //   pin GND: ground (0V)
        }

        net <NetName> {
            connect: [<CompA>.<PinA>, <CompB>.<PinB>, ...]
        }
    }

    schematic <SchematicName> {
        sheet: A4 landscape

        group <GroupName> { members: [<Ref1>, <Ref2>] }

        // Optional explicit placement override for custom/nuanced layouts:
        // place U2 at: (140mm, 90mm)

        layout {
            arrange: [<Group1>, <Group2>, ...] left_to_right
            group_gap: 25.4mm               // optional (snapped to 2.54mm KiCad grid)
            member_gap: 15.24mm             // optional
        }

        // Optional presentation routing styles (never alters functional circuit or ERC):
        // Styles: direct | bus | rail | symbol | label_stub
        routing {
            default: direct                 // direct adjacent pin-to-pin wires + tight local lanes
            power_nets: direct              // OR: symbol (local power bars) | bus | label_stub
            ground_nets: rail               // OR: symbol (local 3-bar GND symbols) | label_stub
            // Optional per-net overrides:
            // net V3V3: symbol
            // net SWDIO: label_stub
        }
    }

    verify {
        require all_required_pins_connected
        require no_power_ground_short
        require no_multiple_power_drivers
        require voltage_compatibility
        require no_single_pin_nets
    }
}
```
