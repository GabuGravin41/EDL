# EDL — Engineering Description Language

An AI-native, typed, declarative language, Rust compiler, and native MCP server for expressing engineering intent, resolving exact geometry/constraints deterministically, running evidence-bearing verification, and compiling to native engineering tools (**SVG**, **AutoCAD DXF**, and **KiCad 9 `.kicad_sch`**).

## Core Principles
- **Intent over raw coordinates (with progressive control):** Declare rooms, walls, openings, fixtures, electronic components, typed pins, and nets semantically; let deterministic engines compute exact coordinates, wall solids, door arcs, and orthogonal wire routes — while exposing clean explicit overrides (`routing`, `side:`, `place ... at:`, `notch:`, `parameter`, `module` defaults) whenever a project needs custom layout or style.
- **Strict layer separation:** Domain-neutral core (`src/core/`), unified frontend & module elaboration (`src/syntax/`), self-contained engineering domains (`src/domains/arch/`, `src/domains/elec/`), and AI MCP server (`src/mcp.rs`) compile into one Canonical IR with full source spans and module provenance.
- **Honest verification + machine-readable repair plans:** Every check produces a typed evidence record (`EXACTLY_CHECKED`, `VIOLATED`, `UNVERIFIED`) plus structured `RepairAction` suggestions so AI agents can self-repair deterministically.
- **One-way compilation + automated target-side validation:** `.edl` is the single source of truth; adapters export to DXF and KiCad 9 and automatically ingest `kicad-cli sch erc` results (`--kicad-erc`) into EDL's evidence ledger.

## What Works Today (v0.8.0 — 33 Rust Tests Passing, 6/6 KiCad 9 ERCs Passing)

### Cross-Domain Core, Composition & AI Tooling (`v0.4`–`v0.6`)
- **Modular compiler architecture:** `src/core/` (spans + repair actions, SI units including `H/mH/uH/nH`, 2D analytical geometry, Canonical IR, semantic query/measure, semantic diff), `src/syntax/` (lexer, AST, parser, import/module elaborator), `src/domains/{arch,elec}/`, and `src/mcp.rs`.
- **Reusable libraries & modules:** File `import "..."` (cycle/diamond-safe), parameterized `module Name(p = default, ...) { ... }`, universal project `parameter` support, electronics `bind PORT -> Comp.Pin` (with automatic internal-net merging), and `use Inst = Name(arg: val)` across **both** Architecture and Electronics, preserving `provenance: { module, instance }` on every IR entity.
- **AI Tooling Ergonomics:** `edl inspect`, `edl query` (`summary`, `entity`, `net`, `room`, `module`, `violations`), `edl measure` (exact gaps including notched rooms, distances, shared wall lengths, routed wire lengths), `edl simulate`, `edl bom`, structured repair plans, and a zero-dependency **MCP stdio JSON-RPC 2.0 server (`edl mcp`)** exposing **8 tools** (`edl_compile`, `edl_query`, `edl_measure`, `edl_simulate`, `edl_bom`, `edl_render_svg`, `edl_export`, `edl_diff`).

### Domain 1 — 2D Architectural Floor Plans (`arch`)
- **Space-first rooms & L-shaped notched rooms:** Built-in `interior_wall`/`exterior_wall` and universal `parameter` support, RREF linear solver with exact DOF & contradiction provenance, reusable room modules (`examples/lib/arch_units.edl`), **corner-notched L-shaped rooms** (`notch: (w, d) at northeast`, with optional `shared_wall(A, B, side)` disambiguation), and exact grid-CSG wall solids.
- **Openings & fixtures:** Hosted `door` (exact analytical circular swing arcs), `window`, open-plan `passage`, plus wall-anchored & freestanding `fixture`s with `clearance_front` / `clearance_all` zones.
- **6 verification rules:** `no_room_overlap` (notch-exact), `opening_jamb_margin >= X`, `no_door_swing_collision` (door/wall/fixture/notch), `all_rooms_reachable`, `no_fixture_collision` (bounds/clearance/opening/notch with exact overlap depth), and `habitable_rooms_have_window`.
- **Outputs:** Architectural SVG preview with **collision-free room labels** and **overall + interior room dimension chains** (supporting recessed wings & notched walls) + layered AutoCAD DXF.

### Domain 2 — Electronics Schematics, Simulation & BOM (`elec`)
- **Typed electrical model:** Electrical SI units (`V`, `mV`, `A`, `mA`, `ohm`, `kohm`, `uF`, `nF`, `pF`, `H`, `mH`, `uH`, `nH`, `Hz`, `W`), typed pins, voltage ranges (`4.5V..12V`), **series-passive DC rail propagation** (ferrite beads, inductors, fuses, switches) + `power_flag: true`, reusable subcircuit modules (`examples/lib/elec_blocks.edl`), and functional `net` graphs.
- **5 Semantic ERCs + Built-In MNA DC/RC Simulation + Live KiCad 9 ERC:** `all_required_pins_connected`, `no_power_ground_short`, `no_multiple_power_drivers`, `voltage_compatibility` (with passive rail propagation), `no_single_pin_nets`, declarative `require dc_voltage(NET) in MIN..MAX` & `require led_current(COMP) in MIN..MAX` solved via exact built-in Modified Nodal Analysis (MNA) + analytical RC `-3dB` cutoff frequencies, plus `K1_kicad_erc_external` via `--kicad-erc`.
- **IEEE/IEC Symbols & 2D Multi-Row Layered Routing:** Shared polarity-aware vector symbol library (`resistor`, polarized `capacitor`, `inductor`, `diode`/`led`, `switch`, `opamp`, shaded ICs) rendered identically in SVG and KiCad 9 `.kicad_sch`, with **2D multi-row schematic bands** (`row [...]`), `W_SHEET_OVERFLOW` warnings, per-net routing styles (`direct`, `bus`, `rail`, `symbol`, `label_stub`), `side:` overrides, and `place <Comp> at: (x, y)` overrides.
- **Manufacturing & Simulation Outputs:** EDA SVG preview, **real KiCad 9 `.kicad_sch`** (**6/6 schematics passing real `kicad-cli sch erc` with 0 errors**, including a 32-component mixed-signal board and a 2-row simulated board), **SPICE3 `.cir` netlist** (`--spice`), and **grouped CSV/JSON BOM** (`edl bom` / `--bom`).

### Domain 3 — Building Electrical layered on Architecture (`mep.elec`, v0.7)
One project, two domains, one shared Canonical IR: `building_electrical` is solved *on top of* the architectural floor plan
(rooms, doors, wet fixtures). DB panels, breakers, ceiling lights, switches anchored `beside(Door, latch_side, 150mm)`, and sockets
are placed on real wall faces; orthogonal circuit runs and switch legs are routed with 3D cable lengths and IEC 60364 copper voltage drop.
Six exact rules: `M1` breaker load, `M2` voltage drop, `M3` wet-zone clearance, `M4` switch/door accessibility, `M5` device/opening clearance, `M6` RCD on wet circuits.
Outputs: layered SVG, 5-layer DXF (`ELEC_PANEL/LIGHTING/SWITCHES/POWER/WIRING`), and `edl schedule [--csv]` DB circuit schedule.
See `examples/apartment_2br_electrical.edl` (13/13 rules `EXACTLY_CHECKED`).

### Bidirectional Sync & Live Co-Editing (`v0.8`)
Humans stay in KiCad / AutoCAD; EDL stays the typed source of truth.
* Every export carries the stable entity id (`EDL_ID` property in `.kicad_sch`; tagged `EDL_ANCHORS` POINTs with registered `EDL` XDATA in `.dxf`).
* `edl sync design.edl --from-kicad edited.kicad_sch [--apply]` turns a human's drag / value / footprint edit into a **minimal text patch**
  (`place U1 at: (...)`, `value: 22uF`) — comments, parameters and modules are untouched. `--from-dxf` patches freestanding fixtures (`at:`) and reports
  semantic drift (wall-anchored fixtures, devices, rooms) instead of guessing. Patches are re-compiled first and **rejected** if they would break verification.
* `edl watch design.edl [--kicad live.kicad_sch]` serves a hot-reloading, pan/zoom browser preview (last good render kept on errors) and keeps KiCad in two-way sync.
* MCP: new `edl_sync` tool (10 tools total).

## Quick Start

```bash
cargo test
cargo build --release

# 1) Architecture: 8-room modular L-notched home & semantic diff
./target/release/edl build examples/stress_family_home.edl
./target/release/edl diff examples/stress_family_home.edl examples/stress_family_home_v2.edl
./target/release/edl preview examples/stress_family_home_v2.edl -o out/stress_family_home_v2.svg
./target/release/edl export examples/stress_family_home_v2.edl --dxf out/stress_family_home_v2.dxf

# 2) Electronics: 2-row schematic + passive ferrite propagation + MNA DC/RC sim + KiCad 9 ERC + SPICE + BOM
./target/release/edl build examples/sensor_node_2row.edl --kicad-erc
./target/release/edl simulate examples/sensor_node_2row.edl
./target/release/edl bom examples/sensor_node_2row.edl --csv
./target/release/edl export examples/sensor_node_2row.edl \
  --kicad out/sensor_node_2row.kicad_sch \
  --spice out/sensor_node_2row.cir \
  --bom out/sensor_node_2row_bom.csv

# 3) AI inspection, semantic queries, exact measurements, and MCP server (8 tools)
./target/release/edl inspect examples/stress_sensor_node.edl --kicad-erc
./target/release/edl query examples/power_5v_invalid.edl violations
./target/release/edl measure examples/stress_family_home.edl LivingDining PowderRoom
./target/release/edl mcp
```

## Repository Structure
- `src/core/` — Domain-neutral kernel (`span` + repair plans, `units`, `geom`, `ir`, `query`, `diff`)
- `src/syntax/` — Unified lexer, AST, parser, and cross-domain `import`/`module`/`use` elaborator
- `src/domains/arch/` — Architecture AST, RREF linear solver, notch-aware wall CSG & 6-rule verifier, SVG & DXF backends
- `src/domains/elec/` — Electronics AST, 5-rule ERC & multi-style router, IEEE/IEC `symbols`, SVG, KiCad 9 `.kicad_sch`, and `kicad_erc` ingestion
- `src/mcp.rs` — Model Context Protocol (MCP) JSON-RPC 2.0 stdio server
- `examples/` — Reference `.edl` projects and reusable libraries (`examples/lib/`)
- `docs/` — Language references (`docs/EDL_V05_REFERENCE.md`, `docs/EDL_V04_REFERENCE.md`, `docs/EDL_V03_REFERENCE.md`)
- `bakeoff/` — Empirical 6-way LLM syntax bake-off results (`bakeoff/SCORECARD.md`)
