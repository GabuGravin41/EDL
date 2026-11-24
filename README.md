# EDL — Engineering Description Language

An AI-native, typed, declarative language and Rust compiler for expressing engineering intent, resolving exact geometry/constraints deterministically, running evidence-bearing verification, and compiling to native engineering tools (**SVG**, **AutoCAD DXF**, and **KiCad 9 `.kicad_sch`**).

## Core Principles
- **Intent over raw coordinates (with progressive control):** Declare rooms, walls, openings, fixtures, electronic components, typed pins, and nets semantically; let deterministic engines compute exact coordinates, wall solids, door arcs, and orthogonal wire routes — while exposing clean explicit overrides (`routing`, `side:`, `place ... at:`, `parameter`, `module` defaults) whenever a project needs custom layout or style.
- **Strict layer separation:** Domain-neutral core (`core/`), unified frontend & module elaboration (`syntax/`), and self-contained engineering domains (`domains/arch/`, `domains/elec/`) compile into one Canonical IR with full source spans and module provenance.
- **Honest verification:** Every check produces a typed evidence record (`EXACTLY_CHECKED`, `VIOLATED`, etc.) with method, tolerance, affected entities, source spans, and repair hints.
- **One-way compilation + target-side validation:** `.edl` is the single source of truth; adapters export to DXF and KiCad 9 and validate against real target tools (`kicad-cli sch erc`).

## What Works Today (v0.4.0 — 17/17 Rust Tests Passing, 4/4 KiCad 9 ERCs Passing)

### Cross-Domain Core & Composition (v0.4)
- **Modular compiler architecture:** `src/core/` (spans, SI units, 2D analytical geometry, Canonical IR, semantic diff), `src/syntax/` (lexer, AST, parser, import/module elaborator), `src/domains/{arch,elec}/`.
- **Reusable libraries & modules:** File `import "..."` (with cycle/diamond protection), parameterized `module Name(p = default, ...) { ... }`, `bind PORT -> Comp.Pin` (with automatic internal-net merging for power/ground subcircuits), and `use Inst = Name(arg: val)` across **both** Architecture and Electronics, preserving `provenance: { module, instance }` on every IR entity.

### Domain 1 — 2D Architectural Floor Plans (`arch`)
- **Space-first rooms** with built-in `interior_wall`/`exterior_wall` and user `parameter` support, linearly solved dimensions (RREF solver with exact DOF & contradiction provenance), reusable room modules (`examples/lib/arch_units.edl`), and clean **L-shaped footprints** with derived exterior/interior wall solids and T-junctions.
- **Openings & fixtures:** Hosted `door` (exact analytical circular swing arcs), `window`, open-plan `passage`, plus wall-anchored AND freestanding `fixture`s with `clearance_front` / `clearance_all` zones.
- **6 verification rules:** `no_room_overlap`, `opening_jamb_margin >= X`, `no_door_swing_collision` (door/wall/fixture), `all_rooms_reachable` (BFS graph), `no_fixture_collision`, and `habitable_rooms_have_window`.
- **Outputs:** Annotated architectural SVG preview (`--annotated` with red violation highlights) + layered AutoCAD DXF (`WALLS_EXT`, `WALLS_INT`, `ROOMS`, `DOORS` with exact `ARC` entities, `WINDOWS`, `PASSAGES`, `FIXTURES`, `ANNOTATIONS`).

### Domain 2 — Electronics Schematics (`elec`)
- **Typed electrical model:** Full electrical SI units (`V`, `mV`, `A`, `mA`, `ohm`, `kohm`, `uF`, `nF`, `pF`, `Hz`, `W`), typed pins (`power_out`, `power_in`, `ground`, `input`, `output`, `bidir`, `passive`, `nc`), voltage ratings & ranges (`7V..35V`), reusable subcircuit modules (`examples/lib/elec_blocks.edl`), and functional `net` graphs strictly separated from schematic layout.
- **5 Electrical Rule Checks (ERC):** `all_required_pins_connected`, `no_power_ground_short`, `no_multiple_power_drivers`, `voltage_compatibility`, and `no_single_pin_nets`.
- **Layered schematic layout & routing:** Grouped left-to-right stage placement on KiCad's `2.54mm` (100 mil) grid, direct adjacent-column wiring, per-net routing styles (`direct`, `bus`, `rail`, `symbol`, `label_stub`), per-pin `side:` overrides, explicit `place <Comp> at: (x, y)` overrides, and crossing-free net label placement.
- **Outputs:** EDA-style SVG preview + **real KiCad 9 `.kicad_sch` S-expression export** verified with **0 errors** under `kicad-cli sch erc --severity-error --exit-code-violations` across all 4 schematic examples (`power_5v`, `mcu_board_3v3`, `mcu_board_3v3_symbols`, `dual_rail_modular`).

## Quick Start

```bash
cargo test
cargo build --release

# 1) Modular Architecture: build, preview SVG, export DXF
./target/release/edl build examples/duplex_modular.edl
./target/release/edl preview examples/duplex_modular.edl -o out/duplex_modular.svg --annotated
./target/release/edl export examples/duplex_modular.edl --dxf out/duplex_modular.dxf

# 2) Modular Electronics: build + ERC, preview SVG, export KiCad 9 .kicad_sch, validate with KiCad CLI
./target/release/edl build examples/dual_rail_modular.edl
./target/release/edl preview examples/dual_rail_modular.edl -o out/dual_rail_modular.svg
./target/release/edl export examples/dual_rail_modular.edl --kicad out/dual_rail_modular.kicad_sch
kicad-cli sch erc --severity-error --exit-code-violations out/dual_rail_modular.kicad_sch

# 3) Semantic diff between revisions
./target/release/edl diff examples/studio_a.edl examples/studio_a_wide.edl
```

## Repository Structure
- `src/core/` — Domain-neutral compiler kernel (`span`, `units`, `geom`, `ir`, `diff`)
- `src/syntax/` — Unified lexer, AST, parser, and cross-domain `import`/`module`/`use` elaborator
- `src/domains/arch/` — Architecture AST, RREF linear solver, wall CSG & 6-rule verifier, SVG & DXF backends
- `src/domains/elec/` — Electronics AST, 5-rule ERC & multi-style router, EDA SVG & KiCad 9 `.kicad_sch` backends
- `examples/` — Reference `.edl` projects and reusable libraries (`examples/lib/`)
- `docs/` — Language references (`docs/EDL_V04_REFERENCE.md`, `docs/EDL_V03_REFERENCE.md`) and design notes
- `bakeoff/` — Empirical 6-way LLM syntax bake-off results (`bakeoff/SCORECARD.md`)
