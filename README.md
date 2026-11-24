# EDL — Engineering Description Language

An AI-native, typed, declarative language, Rust compiler, and native MCP server for expressing engineering intent, resolving exact geometry/constraints deterministically, running evidence-bearing verification, and compiling to native engineering tools (**SVG**, **AutoCAD DXF**, and **KiCad 9 `.kicad_sch`**).

## Core Principles
- **Intent over raw coordinates (with progressive control):** Declare rooms, walls, openings, fixtures, electronic components, typed pins, and nets semantically; let deterministic engines compute exact coordinates, wall solids, door arcs, and orthogonal wire routes — while exposing clean explicit overrides (`routing`, `side:`, `place ... at:`, `notch:`, `parameter`, `module` defaults) whenever a project needs custom layout or style.
- **Strict layer separation:** Domain-neutral core (`src/core/`), unified frontend & module elaboration (`src/syntax/`), self-contained engineering domains (`src/domains/arch/`, `src/domains/elec/`), and AI MCP server (`src/mcp.rs`) compile into one Canonical IR with full source spans and module provenance.
- **Honest verification + machine-readable repair plans:** Every check produces a typed evidence record (`EXACTLY_CHECKED`, `VIOLATED`, `UNVERIFIED`) plus structured `RepairAction` suggestions so AI agents can self-repair deterministically.
- **One-way compilation + automated target-side validation:** `.edl` is the single source of truth; adapters export to DXF and KiCad 9 and automatically ingest `kicad-cli sch erc` results (`--kicad-erc`) into EDL's evidence ledger.

## What Works Today (v0.5.0 — 22/22 Rust Tests Passing, 4/4 KiCad 9 ERCs Passing)

### Cross-Domain Core, Composition & AI Tooling (`v0.4`–`v0.5`)
- **Modular compiler architecture:** `src/core/` (spans + repair actions, SI units, 2D analytical geometry, Canonical IR, semantic query/measure, semantic diff), `src/syntax/` (lexer, AST, parser, import/module elaborator), `src/domains/{arch,elec}/`, and `src/mcp.rs`.
- **Reusable libraries & modules:** File `import "..."` (cycle/diamond-safe), parameterized `module Name(p = default, ...) { ... }`, electronics `bind PORT -> Comp.Pin` (with automatic internal-net merging), and `use Inst = Name(arg: val)` across **both** Architecture and Electronics, preserving `provenance: { module, instance }` on every IR entity.
- **AI Tooling Ergonomics:** `edl inspect`, `edl query` (`summary`, `entity`, `net`, `room`, `module`, `violations`), `edl measure` (exact gaps, distances, shared wall lengths, routed wire lengths), structured repair plans, and a zero-dependency **MCP stdio JSON-RPC 2.0 server (`edl mcp`)** exposing 6 tools (`edl_compile`, `edl_query`, `edl_measure`, `edl_render_svg`, `edl_export`, `edl_diff`).

### Domain 1 — 2D Architectural Floor Plans (`arch`)
- **Space-first rooms & L-shaped notched rooms:** Built-in `interior_wall`/`exterior_wall` and user `parameter` support, RREF linear solver with exact DOF & contradiction provenance, reusable room modules (`examples/lib/arch_units.edl`), **corner-notched L-shaped rooms** (`notch: (w, d) at northeast`, allowing other rooms to tuck right inside a notch!), and exact grid-CSG wall solids.
- **Openings & fixtures:** Hosted `door` (exact analytical circular swing arcs), `window`, open-plan `passage`, plus wall-anchored & freestanding `fixture`s with `clearance_front` / `clearance_all` zones.
- **6 verification rules:** `no_room_overlap` (notch-exact), `opening_jamb_margin >= X`, `no_door_swing_collision` (door/wall/fixture/notch), `all_rooms_reachable`, `no_fixture_collision` (bounds/clearance/opening/notch), and `habitable_rooms_have_window`.
- **Outputs:** Architectural SVG preview with **overall + interior room dimension chains** (`--annotated` for badges & violation highlights) + layered AutoCAD DXF (`WALLS_EXT`, `WALLS_INT`, `ROOMS`, `DOORS`, `WINDOWS`, `PASSAGES`, `FIXTURES`, `ANNOTATIONS`).

### Domain 2 — Electronics Schematics (`elec`)
- **Typed electrical model:** Electrical SI units (`V`, `mV`, `A`, `mA`, `ohm`, `kohm`, `uF`, `nF`, `pF`, `Hz`, `W`), typed pins, voltage ranges (`4.5V..12V`), reusable subcircuit modules (`examples/lib/elec_blocks.edl`), and functional `net` graphs strictly separated from schematic layout.
- **5 Semantic ERCs + Live KiCad 9 ERC Ingestion:** `all_required_pins_connected`, `no_power_ground_short`, `no_multiple_power_drivers`, `voltage_compatibility`, `no_single_pin_nets`, plus `K1_kicad_erc_external` via `--kicad-erc`.
- **IEEE/IEC Symbols & Layered Routing:** Shared vector symbol library (`resistor` zig-zag, polarized `capacitor` plates, `diode`/`led` triangles + light arrows, `inductor` coils, `switch`, `opamp`, shaded ICs/connectors) rendered identically in SVG and KiCad 9 `.kicad_sch`, with per-net routing styles (`direct`, `bus`, `rail`, `symbol`, `label_stub`), `side:` overrides, and `place <Comp> at: (x, y)` overrides.

## Quick Start

```bash
cargo test
cargo build --release

# 1) Architecture: modular 2BR & notched L-shaped loft
./target/release/edl build examples/loft_notched.edl
./target/release/edl preview examples/loft_notched.edl -o out/loft_notched.svg
./target/release/edl export examples/loft_notched.edl --dxf out/loft_notched.dxf

# 2) Electronics: modular board + real KiCad 9 ERC ingestion
./target/release/edl build examples/dual_rail_modular.edl --kicad-erc
./target/release/edl preview examples/dual_rail_modular.edl -o out/dual_rail_modular.svg
./target/release/edl export examples/dual_rail_modular.edl --kicad out/dual_rail_modular.kicad_sch

# 3) AI inspection, semantic queries, exact measurements, and MCP server
./target/release/edl inspect examples/dual_rail_modular.edl --kicad-erc
./target/release/edl query examples/power_5v_invalid.edl violations
./target/release/edl measure examples/loft_notched.edl GreatRoom Bath
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
