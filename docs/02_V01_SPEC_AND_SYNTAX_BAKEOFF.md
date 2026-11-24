# 02 — EDL v0.1 Benchmark, Candidate Syntaxes & Minimal Canonical IR

This document pins down our first concrete deliverable: **a real 3-room architectural floor plan** that exercises exact units, frames, wall thicknesses, T-junctions, linearly solved dimensions, host-attached openings, exact circular door-swing arcs, clearance verification, annotated SVG preview, and DXF export.

---

## 1. Benchmark A: Three-Room Studio Plan (`Studio_A`)

### Exact Geometric Spec (Verified to the Millimeter)
- **Measurement convention:** `clear_interior` (all room widths/depths are clear finished-face to finished-face).
- **Default wall thicknesses:** `exterior = 200mm`, `interior = 120mm`.
- **Coordinate frame:** `FloorPlan` (`Frame2`, right-handed, `+X = east`, `+Y = north`).
- **Rooms:**
  1. `Living`: clear `4500mm × 4000mm`, `southwest` anchored at `FloorPlan.origin (0mm, 0mm)` → `x ∈ [0, 4500]`, `y ∈ [0, 4000]` (`18.00 m²`).
  2. `Kitchen`: clear `3000mm × 2400mm`, `east_of Living` separated by `120mm` partition, `south_face == Living.south_face` → `x ∈ [4620, 7620]`, `y ∈ [0, 2400]` (`7.20 m²`).
  3. `Bath`: **depth is NOT hardcoded**—it is linearly solved!
     - `west_face == Kitchen.west_face` (`4620mm`), `east_face == Kitchen.east_face` (`7620mm`) → `width = 3000mm`
     - `north_of Kitchen` separated by `120mm` partition → `south_face = 2520mm`
     - `north_face == Living.north_face` (`4000mm`) → **solved clear depth = `1480mm`** (`4.44 m²`).
- **Derived Wall Graph (automatic, zero manual overlap math):**
  - Exterior envelope (`200mm` outward): `x ∈ [-200, 7820]` (`8020mm` overall), `y ∈ [-200, 4200]` (`4400mm` overall).
  - Interior vertical partition (`120mm` wide): `x ∈ [4500, 4620]`, `y ∈ [0, 4000]` — shared by `Living | Kitchen` (south segment `y ∈ [0, 2400]`) and `Living | Bath` (north segment `y ∈ [2520, 4000]`), forming a **T-junction** with the horizontal partition.
  - Interior horizontal partition (`120mm` thick): `x ∈ [4620, 7620]`, `y ∈ [2400, 2520]` — shared by `Kitchen | Bath`.
- **Openings (hosted on semantic wall segments, never raw coordinates):**
  1. `D_Entry` (door, `900mm`): on `exterior(Living, south)`, `600mm` from west end, hinge `west`, swings `into Living`.
  2. `D_Kitchen` (door, `800mm`): on `wall_between(Living, Kitchen)`, centered, hinge `south`, swings `into Kitchen`.
  3. `D_Bath` (door, `750mm`): on `wall_between(Living, Bath)`, `300mm` from south end of shared segment, hinge `south`, swings `into Living`.
  4. `W_Living` (window, `1800mm`): on `exterior(Living, north)`, centered.
- **Verification Checks (Evidence Ledger):**
  - **V1 (Static):** All units dimensionally valid; all references exist; frame compatibility holds.
  - **V2 (Solver):** All room coordinates uniquely determined (`0` remaining DOF); no contradictory constraints.
  - **V3 (Geometry):** No overlapping rooms; all openings fit strictly inside their host wall segment with `>= 100mm` jamb margin on both sides.
  - **V4 (Clearance):** Exact analytical door swing arcs (`D_Entry`, `D_Kitchen`, `D_Bath`) do not intersect each other or any wall polygon.
- **Deliberate Negative Test Case (`Studio_A_Invalid`):**
  - Change `D_Bath` to `offset = 50mm` from south end (violates `>= 100mm` jamb margin) *or* flip `D_Kitchen` to swing `into Living` with hinge `north` so its swing arc collides with `D_Bath`'s swing arc → compiler emits `VIOLATED` diagnostics with exact measured vs. required millimeters and highlights the collision in red on the SVG preview.

---

## 2. Two Candidate Syntaxes (Acyclic & Ready to Compare)

Both candidates solve the circular `Room ↔ Wall` dependency from earlier drafts by using a **Space-First + Derived Wall Graph** model.

### Candidate A — Block-Declarative (Structured, Hierarchical, JSON/CSS-like)

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
        // width and depth derived by linear constraints below
    }

    layout FloorPlan {
        // Tier 2 linear constraints (Cassowary)
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

### Candidate B — Relational / Expression-Oriented (Flat, Compact, Penrose/KCL-like)

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

// Explicit linear constraints
require Living.southwest == FloorPlan.origin
require Kitchen.west  == Living.east + 120mm
require Kitchen.south == Living.south
require Bath.west     == Kitchen.west
require Bath.east     == Kitchen.east
require Bath.south    == Kitchen.north + 120mm
require Bath.north    == Living.north

// Hosted openings
let D_Entry   = door(host = exterior(Living, south), width = 900mm, offset = 600mm, hinge = start, swing = Living)
let D_Kitchen = door(host = shared(Living, Kitchen), width = 800mm, align = center, hinge = start, swing = Kitchen)
let D_Bath    = door(host = shared(Living, Bath),    width = 750mm, offset = 300mm, hinge = start, swing = Living)
let W_Living  = window(host = exterior(Living, north), width = 1800mm, align = center)

// Verification & view
verify no_room_overlap
verify opening_jamb_margin >= 100mm
verify no_door_swing_collision

view GroundPlan = plan_view(frame = FloorPlan, sheet = A3_landscape, scale = 1:50)
```

### Quick Tradeoff Comparison (Before Testing with LLMs)

| Dimension | Candidate A (Block-Declarative) | Candidate B (Relational / Flat) |
|---|---|---|
| **Visual scanability** | Very clear entity boundaries; easy to find where `Kitchen` starts/ends | Compact, fewer braces; feels like Penrose/Python/KCL |
| **Span-based AI edits** | Easy to patch one block cleanly by line range | One-liners are compact, though properties can get long horizontally |
| **Scoping / nesting** | Naturally groups `layout { ... }`, `verify { ... }`, `view { ... }` | Flat top-level statements; needs explicit scoping rules for multi-floor/multi-module |
| **LLM familiarity** | Looks like Terraform/HCL/CSS/JSON-with-blocks (very high LLM fluency) | Looks like Rust/Python/Penrose (also high LLM fluency) |

---

## 3. Minimal Canonical IR Schema (Domain-Neutral Core)

Both Candidate A and Candidate B compile into the **exact same typed IR**. Notice that the core IR knows nothing about "rooms" or "resistors"—it only knows **entities, frames, quantities, analytical 2D geometry, relations, constraints, views, and evidence**.

```json
{
  "ir_version": "0.1.0",
  "project": { "name": "Studio_A", "default_unit": "mm" },
  "frames": [
    { "id": "FloorPlan", "dim": 2, "handedness": "right", "parent": null }
  ],
  "entities": [
    {
      "id": "Living",
      "kind": "arch.room",
      "span": { "file": "studio_a.edl", "start_line": 10, "end_line": 13 },
      "frame": "FloorPlan",
      "props": { "measurement_basis": "clear_interior", "area_m2": 18.0 },
      "geometry": {
        "type": "rect",
        "min": [0.0, 0.0],
        "max": [4500.0, 4000.0],
        "unit": "mm"
      }
    },
    {
      "id": "D_Kitchen",
      "kind": "arch.door",
      "span": { "file": "studio_a.edl", "start_line": 39, "end_line": 45 },
      "frame": "FloorPlan",
      "props": { "width_mm": 800.0, "host_wall": "W_Living_Kitchen", "swing_into": "Kitchen" },
      "geometry": {
        "opening_segment": { "type": "segment", "p0": [4560.0, 800.0], "p1": [4560.0, 1600.0] },
        "swing_arc": {
          "type": "arc",
          "center": [4620.0, 800.0],
          "radius": 800.0,
          "start_deg": 0.0,
          "sweep_deg": 90.0
        }
      }
    }
  ],
  "derived_entities": [
    {
      "id": "W_Living_Kitchen",
      "kind": "arch.wall.interior",
      "derived_from": ["Living", "Kitchen"],
      "props": { "thickness_mm": 120.0 },
      "geometry": { "type": "rect", "min": [4500.0, 0.0], "max": [4620.0, 2400.0] }
    }
  ],
  "constraints": [
    {
      "id": "C05",
      "tier": 2,
      "strength": "require",
      "expr": "Bath.north == Living.north",
      "status": "EXACTLY_CHECKED",
      "residual_mm": 0.0
    }
  ],
  "evidence": [
    {
      "check_id": "V4_door_swing_clearance",
      "status": "EXACTLY_CHECKED",
      "method": "exact_arc_segment_intersection_v0.1",
      "tolerance_mm": 1e-6,
      "subjects": ["D_Entry", "D_Kitchen", "D_Bath"],
      "details": "0 collisions detected"
    }
  ],
  "diagnostics": []
}
```

### Cross-Domain Conformance Proof (Electronics Uses the Exact Same IR Shape)
Without changing a single top-level field of the IR, a future electronic circuit fits natively:

```json
{
  "entities": [
    { "id": "U1", "kind": "elec.component", "ports": ["INPUT", "OUTPUT", "GND"] },
    { "id": "J2", "kind": "elec.component", "ports": ["POSITIVE", "GND"] }
  ],
  "relations": [
    { "id": "VIN", "kind": "elec.net", "connects": ["J1.POSITIVE", "U1.INPUT", "C1.POSITIVE"] }
  ],
  "evidence": [
    {
      "check_id": "ERC_unconnected_pins",
      "status": "VIOLATED",
      "subjects": ["J2.GND"],
      "details": "Required power pin J2.GND is not connected to any net"
    }
  ]
}
```

---

## 4. Draft Definition of "First Satisfactory Floor Plan"

We can refine this as we go, but here is a concrete initial bar for our first working milestone:

1. **Compiles cleanly from `.edl` source** with unit + frame checking and clear error spans.
2. **Solves `Bath`'s missing dimensions linearly** (and reports exact DOF / conflict diagnostics if a constraint is missing or contradictory).
3. **Derives clean walls with real thicknesses** (exterior `200mm`, interior `120mm`) and clean L/T-junctions without overlapping garbage lines.
4. **Places doors & windows semantically** on host walls with exact analytical swing arcs (never tessellated in IR).
5. **Catches real geometric bugs** (jamb margin violations, overlapping rooms, colliding door swings) with structured JSON evidence + red overlays in SVG.
6. **Produces two real visual/CAD outputs** you can open and inspect right away:
   - `studio_a.svg` (clean, legible architectural plan with room names, areas, dimensions, wall fill, door swing arcs, and optional debug/violation overlay)
   - `studio_a.dxf` (clean layered DXF openable in LibreCAD / QCAD / AutoCAD)
7. **Supports one surgical edit + `edl diff`:** e.g., change `Living.width` from `4500mm` to `5200mm`, rebuild, and watch `Kitchen`, `Bath`, walls, doors, dimensions, SVG, and DXF update cleanly with a semantic diff report.
