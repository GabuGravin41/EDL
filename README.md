# EDL — Engineering Description Language

An AI-native, typed, declarative language and Rust compiler for expressing engineering intent, resolving exact geometry/constraints deterministically, running evidence-bearing verification, and compiling to native engineering tools (**SVG**, **AutoCAD DXF**, and **KiCad 9 `.kicad_sch`**).

## Core Principles
- **Intent over raw coordinates:** Declare rooms, walls, openings, fixtures, electronic components, typed pins, and nets semantically; let deterministic engines compute exact coordinates, wall solids, door arcs, and orthogonal wire routes.
- **Strict layer separation:** Semantic entities, analytical geometry/topology, stratified constraints, presentation/layout, and verification evidence stay cleanly separated inside one cross-domain Canonical IR.
- **Honest verification:** Every check produces a typed evidence record (`EXACTLY_CHECKED`, `VIOLATED`, etc.) with method, tolerance, affected entities, source spans, and repair hints.
- **One-way compilation + target-side validation:** `.edl` is the single source of truth; adapters export to DXF and KiCad 9 and validate against target tools (`kicad-cli sch erc`).

## What Works Today (v0.3.1 — 12/12 Rust Tests Passing)

### Domain 1 — 2D Architectural Floor Plans (`arch`)
- **Space-first rooms** with built-in `interior_wall`/`exterior_wall` and user `parameter` support, linearly solved dimensions (RREF solver with exact DOF & contradiction provenance), and clean **L-shaped footprints** with derived exterior/interior wall solids and T-junctions.
- **Openings & fixtures:** Hosted `door` (exact analytical circular swing arcs), `window`, open-plan `passage`, plus wall-anchored AND freestanding `fixture`s with `clearance_front` / `clearance_all` zones.
- **6 verification rules:** `no_room_overlap`, `opening_jamb_margin >= X`, `no_door_swing_collision` (door/wall/fixture), `all_rooms_reachable` (BFS graph), `no_fixture_collision`, and `habitable_rooms_have_window`.
- **Outputs:** Annotated architectural SVG preview (`--annotated` with red violation highlights) + layered AutoCAD DXF (`WALLS_EXT`, `WALLS_INT`, `ROOMS`, `DOORS` with exact `ARC` entities, `WINDOWS`, `PASSAGES`, `FIXTURES`, `ANNOTATIONS`).
- **Blind AI stress test:** A blind AI Architect designed a 6-room, `66.80 m²` L-shaped 2-bedroom apartment (`examples/apartment_2br_l.edl`) with 11 openings and 10 fixtures on the first compile (0 errors, 0 DOF).

### Domain 2 — Electronics Schematics (`elec`)
- **Typed electrical model:** Full electrical SI units (`V`, `mV`, `A`, `mA`, `ohm`, `kohm`, `uF`, `nF`, `pF`, `Hz`, `W`), typed pins (`power_out`, `power_in`, `ground`, `input`, `output`, `bidir`, `passive`, `nc`), voltage ratings & ranges (`7V..35V`), and functional `net` graphs strictly separated from schematic layout.
- **5 Electrical Rule Checks (ERC):** `all_required_pins_connected`, `no_power_ground_short`, `no_multiple_power_drivers`, `voltage_compatibility`, and `no_single_pin_nets`.
- **Deterministic schematic layout & routing:** Grouped left-to-right stage placement on KiCad's `2.54mm` (100 mil) grid, lane-isolated orthogonal Manhattan wire routing with junction dots, and crossing-free net label placement.
- **Outputs:** EDA-style SVG preview + **real KiCad 9 `.kicad_sch` S-expression export** verified with **0 errors** under `kicad-cli sch erc --severity-error --exit-code-violations` and `kicad-cli sch export svg`.
- **Blind AI stress test:** A blind AI Electrical Engineer designed a 10-component 5V USB → 3.3V LDO + STM32G030 MCU + SWD Debug + Status LED board (`examples/mcu_board_3v3.edl`) passing both EDL ERC and real KiCad 9 ERC with 0 errors.

## Quick Start

```bash
cargo test
cargo build --release

# 1) Architecture: build, preview SVG, export DXF, semantic diff
./target/release/edl build examples/apartment_2br_l.edl
./target/release/edl preview examples/apartment_2br_l.edl -o out/apartment_2br_l.svg --annotated
./target/release/edl export examples/apartment_2br_l.edl --dxf out/apartment_2br_l.dxf
./target/release/edl diff examples/studio_a.edl examples/studio_a_wide.edl

# 2) Electronics: build + ERC, preview SVG, export KiCad 9 .kicad_sch, validate with KiCad CLI
./target/release/edl build examples/mcu_board_3v3.edl
./target/release/edl preview examples/mcu_board_3v3.edl -o out/mcu_board_3v3.svg --annotated
./target/release/edl export examples/mcu_board_3v3.edl --kicad out/mcu_board_3v3.kicad_sch
kicad-cli sch erc --severity-error --exit-code-violations out/mcu_board_3v3.kicad_sch
```

## Repository Structure
- `src/` — Rust compiler (`parser`, `units`, `geom`, `solver`, `arch`, `elec`, `ir`, `render_svg`, `export_dxf`, `export_kicad`, `diff`, `main`)
- `examples/` — Reference `.edl` projects (both valid and deliberate failure cases for architecture & electronics)
- `docs/` — Prior art, specs, and unified language reference (`docs/EDL_V03_REFERENCE.md`)
- `bakeoff/` — Empirical 6-way LLM syntax bake-off results (`bakeoff/SCORECARD.md`)
