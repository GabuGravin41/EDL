# EDL v1.0 Specification (Candidate — Compiler v0.9.0)

**EDL (Engineering Description Language)** is a typed, declarative engineering language and verification compiler designed to act as the **multi-domain source of truth** between AI agents and native visual engineering tools (KiCad, AutoCAD, Revit, FreeCAD, Onshape, Fusion 360, SPICE).

> **Design North Star:** EDL does **not** replace native CAD/EDA GUIs or 3D kernels. Humans stay in their visual tools; AI writes and edits EDL; the compiler solves geometry, checks physics and engineering codes, exports native artifacts tagged with stable `EDL_ID`s, and syncs human edits back into surgical `.edl` source patches.

---

## 1. Lexical Structure & Dimensional Units

- **Comments:** `// line comment` and `/* block comment */`.
- **Identifiers:** `[A-Za-z_][A-Za-z0-9_]*`.
- **Strings:** Double-quoted `"..."` with `\"` escapes.
- **Physical Quantities:** Numeric literal immediately followed by a typed unit suffix. Dimension mismatches (e.g., adding `5V` to `100mm`) are compile-time errors (`E_UNIT_MISMATCH`).

| Dimension | Supported Units | Canonical Internal Unit |
|---|---|---|
| Length | `mm`, `cm`, `m`, `in`, `ft` | `mm` |
| Area | `mm2`, `m2` | `mm²` |
| Angle | `deg`, `rad` | `deg` |
| Voltage | `uV` (via `mV`), `mV`, `V`, `kV` | `V` |
| Current | `uA`, `mA`, `A` | `A` |
| Resistance | `ohm` / `Ohm`, `kohm` / `kOhm`, `Mohm` / `MOhm` | `Ω` |
| Capacitance | `pF`, `nF`, `uF`, `mF`, `F` | `F` |
| Inductance | `nH`, `uH`, `mH`, `H` | `H` |
| Frequency | `Hz`, `kHz`, `MHz` | `Hz` |
| Power | `mW`, `W`, `kW` | `W` |
| Percentage | `%` | `%` |

---

## 2. Top-Level Project Structure

Every `.edl` file is either a **library file** (containing `import` and `module` declarations) or a **project file**:

```edl
import "./lib/power_blocks.edl"

project "MyProject" {
    units: mm
    frame Plan: Frame2
    parameter wall_t: 120mm

    // Built-in domain declarations:
    //   arch:     architecture.defaults, room, layout, opening, fixture
    //   mep.elec: building_electrical
    //   elec:     circuit, schematic
    // Composition:
    //   module, use
    // Verification & Output:
    //   verify, view
    // Extension domains (plumbing, hvac, pcb, mech, ...):
    //   <keyword> <Name> { ... }
}
```

### 2.1 Composition: `import`, `module`, and `use`

Reusable engineering blocks work identically across all domains via token-level parameterized elaboration with full `ModuleProvenanceIr` tracking:

```edl
module LedIndicator(r_val = 470ohm, led_name = "LED_Green") {
    component R: resistor {
        value: r_val
        pin A: passive
        pin B: passive
    }
    component D: led {
        value: led_name
        pin A: passive
        pin K: passive
    }
    net _MID { connect: [R.B, D.A] }
    bind IN  -> R.A
    bind GND -> D.K
}
```

Instantiation inside a `circuit` or `project`:

```edl
use StatusLed = LedIndicator(r_val = 1kohm, led_name = "LED_Blue") {
    IN:  GPIO_LED
    GND: GND
}
```

- Every elaborated entity receives a deterministic prefixed ID (`StatusLed_R`, `StatusLed_D`) and records `provenance: { "module": "LedIndicator", "instance": "StatusLed" }`.
- Internal module nets bound to external parent nets are cleanly absorbed into the parent net during elaboration.

---

## 3. Built-In Engineering Domains

### 3.1 Architecture (`arch` — Base Domain)

Constraint-based 2D architectural floor plans solved via exact Reduced Row Echelon Form (RREF) linear algebra with provenance and remaining degrees of freedom (DOF) diagnostics.

- `architecture.defaults { measurement_basis: clear_interior  exterior_wall: 200mm  interior_wall: 120mm }`
- `room <ID> { role: habitable|circulation|wet|service  clear_size: (W, D)  anchor: (X, Y)  notch: (NW, ND) at <corner> }`
- `layout { require <RoomA>.<side> == <RoomB>.<side> [+ offset]  require <RoomA>.<side> flush_with <RoomB>.<side> }`
- `opening <ID>: door|window|passage { host: <Room>.<side> | shared_wall(<A>, <B>[, <side>])  width: W  align: center | offset <d> from start|end  swing: into(<Room>) hinge_at start|end  entry: true }`
- `fixture <ID>: <category> { room: <Room>  size: (W, D)  against: <side> | at: center | at: (X, Y)  offset: <d> from start|end  clearance_front: <d>  clearance_all: <d> }`

**Exact Verification Rules (`arch`):**
| Check ID | Rule Keyword | What It Verifies |
|---|---|---|
| `T2_linear_layout_solve` | *(always run)* | All rooms uniquely determined (`0` remaining DOF); reports exact over/under-constrained equations |
| `V1_no_room_overlap` | `no_room_overlap` | Exact AABB + notch interior overlap check across all room pairs |
| `V2_opening_jamb_margin` | `opening_jamb_margin >= <len>` | Every door/window/passage respects minimum jamb distance from wall ends |
| `V3_no_door_swing_collision` | `no_door_swing_collision` | Exact analytical quarter-disk intersection between door swings and walls/fixtures |
| `V4_all_rooms_reachable` | `all_rooms_reachable` | BFS room connectivity graph from `entry: true` doors |
| `V5_no_fixture_collision` | `no_fixture_collision` | Exact fixture body + clearance rectangle containment and non-overlap (including notched voids) |
| `V6_habitable_rooms_have_window` | `habitable_rooms_have_window` | Every `role: habitable` room has at least one exterior window |

### 3.2 Building Electrical (`mep.elec` — Overlay Domain on `arch`)

Layered directly on top of solved architectural IR rooms, openings, doors, and wet fixtures inside the same project file:

```edl
building_electrical MainInstall {
    supply: 230V 50Hz
    ceiling_height: 2700mm

    panel DB1 { host: Hall.east  align: center  height: 1500mm }
    breaker C1_Lights: lighting { from: DB1  rating: 10A  cable: 1.5mm2  rcd: 30mA  max_voltage_drop: 3% }

    light  L_Living: ceiling { room: Living  at: center  load: 36W  circuit: C1_Lights }
    switch SW_Entry: 2gang   { host: beside(EntryDoor, latch_side, 180mm)  room: Living  controls: [L_Living]  circuit: C1_Lights }
    outlet S_Liv_N:  duplex  { host: Living.north  offset: 600mm from start  load: 600W  circuit: C2_Sockets }
}
```

**Exact Verification Rules (`mep.elec`, IEC 60364):**
| Check ID | Rule Keyword | What It Verifies |
|---|---|---|
| `M1_breaker_load_capacity` | `breaker_load_capacity` | Circuit design current ≤ 80% of breaker continuous rating |
| `M2_voltage_drop_limit` | `voltage_drop_limit` | 3D routed copper run voltage drop $\Delta V\% = \frac{2 \rho L I}{A V}\times 100 \le \text{limit}$ |
| `M3_wet_zone_clearance` | `wet_zone_clearance >= <len>` | IEC 60364-7-701 Zone 2 clearance (e.g. `600mm`) around showers/tubs (exempting IP44+ ceiling lights & shaver sockets) |
| `M4_switch_door_accessibility` | `switch_door_accessibility` | Every switch sits outside all door swing arcs, and every light is controlled by ≥ 1 switch |
| `M5_device_wall_clearance` | `device_wall_clearance` | Wall-mounted panels, switches, and sockets stay clear of door/window/passage cutouts |
| `M6_wet_circuit_rcd` | `wet_circuit_rcd` | All socket, appliance, and wet-area circuits have ≤ 30 mA RCD protection |

### 3.3 Electronics Schematics, Simulation & BOM (`elec` — Base Domain)

Typed schematic capture, 2D multi-row layout, orthogonal routing on a `2.54mm` (100 mil) KiCad grid, ERC, built-in exact MNA DC + analytical RC simulation, KiCad 9 export, SPICE3 `.cir` export, and grouped BOM generation.

- `circuit <Name> { component <ID>: <category> { value: ... footprint: "..." pin <P>: power_out|power_in|ground|input|output|bidir|passive|nc [(V | Vmin..Vmax)] }  net <N> { connect: [C.P, ...]  power_flag: true } }`
- `schematic <Name> { sheet: A4|A3|A2 landscape|portrait  group <G> { members: [...] }  layout { arrange: [...] | row [...]  group_gap: ...  member_gap: ...  row_gap: ... }  routing { default: direct|bus|rail|symbol|label_stub  power: ...  ground: ...  net <N>: ... }  place <Comp> at: (<X>, <Y>) }`

**Exact Verification Rules (`elec`):**
| Check ID | Rule Keyword | What It Verifies |
|---|---|---|
| `E1_all_required_pins_connected` | `all_required_pins_connected` | Every non-`nc` pin is connected to a net |
| `E2_no_power_ground_short` | `no_power_ground_short` | No net contains both `power_out` and `ground` pins |
| `E3_no_multiple_power_drivers` | `no_multiple_power_drivers` | No net has > 1 independent `power_out` driver |
| `E4_voltage_compatibility` | `voltage_compatibility` | Driven rail voltage (propagated across series passives: ferrite beads, inductors, fuses, switches, ≤10Ω resistors) falls within every connected `power_in` range |
| `E5_no_single_pin_nets` | `no_single_pin_nets` | Every net connects ≥ 2 pins |
| `S1_dc_voltage_*` | `dc_voltage(<NET>) in <Vmin>..<Vmax>` | Built-in Modified Nodal Analysis (MNA) DC operating point assertion |
| `S2_branch_current_*` | `branch_current(<COMP>) in <Imin>..<Imax>` | Built-in MNA component branch current (incl. PWL LED/diode currents) |
| `K1_kicad_erc_external` | *(`--kicad-erc`)* | Invokes real `kicad-cli sch erc --severity-error --format json` and ingests violations into the evidence ledger |

---

## 4. Extension Blocks (Open-Ended Domain Syntax)

Any top-level `<keyword> [<Name>] { ... }` inside a `project` that is not a built-in keyword is captured verbatim by the core parser as an `ExtensionBlock` (with exact source `Span` and balanced tokens) and dispatched to the registered `DomainEngine` claiming `<keyword>`:

```edl
plumbing DomesticWater {
    source: (6800mm, 300mm)
    max_run: 9m
    feed F_Shower: cold { fixture: Bath1_Shower }
    feed F_Vanity: hot  { fixture: Bath1_Vanity }
}
```

- Unclaimed extension keywords emit `E_UNKNOWN_DOMAIN_BLOCK` with exact source span and a hint listing available built-in and registered keywords.
- Extension entities are stored in `CanonicalIr.entities` using `EntityGeometryIr::Custom { shape, data }`. Publishing `"bbox": [min_x, min_y, max_x, max_y]` inside `data` automatically unlocks `edl query` and `edl measure` for the custom domain.

---

## 5. Canonical IR & Verification Evidence Ledger

Every compilation emits a deterministic, versioned JSON `CanonicalIr` whose formal JSON Schema (Draft 7) is generated directly from the Rust compiler types in [`docs/canonical_ir.schema.json`](./canonical_ir.schema.json) (and via `edl schema`).

Key top-level sections:
- `ir_version`: Schema version string.
- `project`: Project metadata, domain identifier (`architecture`, `architecture+mep.elec`, `electronics`), footprint/bounds, counts, remaining DOF, and `has_errors`.
- `frames`: Coordinate frames (`Frame2` Y-up for architecture/MEP; `Frame2` Y-down 2.54mm grid for electronics).
- `entities`: Typed entities with stable `id`, `kind`, optional `span`, optional `provenance` (`module` + `instance`), arbitrary `props`, and tagged `geometry`.
- `relations`: Functional connectivity (`elec.net` pin lists — strictly separated from drawn wire routes).
- `derived_walls` & `wall_solids`: Semantic wall graph and mitered wall poché solids.
- `schematic_groups` & `schematic_routes`: Physical schematic layout boxes, routed wire segments, junctions, and symbol/label terminals.
- `mep_circuits` & `mep_wires`: Building electrical circuits (load, current, 3D run length, voltage drop) and routed conduit segments.
- `constraints`: RREF linear equation records with residuals.
- `evidence`: **Honest Verification Ledger.** Every check carries an explicit status:
  - `FormallyChecked` / `ExactlyChecked` / `NumericallyChecked`
  - `Assumed` / `NotModeled` / `Unverified`
  - `Violated`
- `diagnostics`: Structured errors/warnings with exact `Span`, `measured` vs `required` quantities, human-readable `suggestion`, and machine-readable `repair` (`RepairAction`) plans.

---

## 6. CLI Command Reference

| Command | Purpose |
|---|---|
| `edl build <file> [--json] [--kicad-erc]` | Compile, verify, and print summary or full Canonical IR JSON |
| `edl preview <file> -o <out.svg> [--annotated]` | Render SVG preview (with optional dimension/pin annotations) |
| `edl export <file> [--dxf ...] [--kicad ...] [--spice ...] [--bom ...]` | Export to native CAD/EDA formats tagged with `EDL_ID` |
| `edl sync <file> (--from-kicad <f> \| --from-dxf <f>) [--apply] [--json]` | Diff host-tool edits back into surgical `.edl` patches (re-verified before write) |
| `edl watch <file> [--port 8765] [--kicad <f>]` | Live hot-reloading browser viewer + optional two-way KiCad file sync |
| `edl simulate <file> [--json]` | Built-in exact MNA DC + analytical RC `-3dB` simulator |
| `edl bom <file> [--csv]` | Grouped Bill of Materials (JSON or CSV) |
| `edl schedule <file> [--csv]` | Building Electrical DB Panel & Circuit Schedule (JSON or CSV) |
| `edl inspect <file> [--kicad-erc]` | Compact AI-friendly structural inspection summary |
| `edl query <file> <summary\|entity\|net\|room\|module\|violations> [target]` | Targeted semantic query without dumping the full IR |
| `edl measure <file> <target_a> [target_b]` | Exact geometry bounds, net wire length, clear gap, shared wall, or shared nets |
| `edl diff <before.edl> <after.edl> [--json]` | Semantic diff by stable entity ID |
| `edl domains [--json]` | List registered domain engines, export formats, and verification rules |
| `edl schema` | Emit the live Canonical IR JSON Schema |
| `edl mcp` | Run the stdio JSON-RPC 2.0 MCP server (11 tools) |
