# EDL — Engineering Description Language

An AI-native, typed, declarative language and Rust compiler for expressing engineering intent, resolving exact geometry/constraints deterministically, running evidence-bearing verification, and compiling to native engineering formats (SVG, DXF, KiCad).

## Design Principles
- **Intent over raw coordinates:** Describe rooms, walls, openings, fixtures, components, pins, and nets semantically; let deterministic solvers compute exact coordinates and topologies.
- **Layered architecture:** Separate semantic entities, analytical geometry/topology, stratified constraints, presentation/layout, and verification evidence.
- **Honest verification:** Every check produces a typed evidence record (`EXACTLY_CHECKED`, `VIOLATED`, etc.) with method, tolerance, affected entity IDs, source spans, and repair hints.
- **One-way compilation + target-side validation:** `.edl` is the single source of truth; adapters export to native tools (DXF, KiCad) and ingest target-side checks (e.g. `kicad-cli sch erc`).

## Current Status (v0.2)
- **Language frontend & core:** Rust lexer/parser with spans, SI dimensional unit checker, RREF linear constraint solver with exact DOF & contradiction provenance, canonical JSON IR, and semantic revision diffing.
- **Vertical Slice 1 — 2D Architectural Floor Plans (`arch`):**
  - Space-first rooms with derived exterior/interior wall graph (clean L-shapes, corners, T-junctions)
  - Doors (exact analytical circular swing arcs), windows, open-plan passages, and fixtures with front clearances
  - 6 verification rules: room overlap, jamb margins, exact analytical door-swing collisions (door/wall/fixture), room reachability (BFS), fixture collisions/clearances, and habitable natural light
  - Architectural SVG preview (`--annotated` with red violation highlights) + layered AutoCAD DXF export

## Quick Start

```bash
cargo test
cargo build --release

# Build & verify (add --json for machine-readable Canonical IR)
./target/release/edl build examples/apartment_2br_l.edl

# Render annotated SVG preview
./target/release/edl preview examples/apartment_2br_l.edl -o out/apartment_2br_l.svg --annotated

# Export layered CAD DXF
./target/release/edl export examples/apartment_2br_l.edl --dxf out/apartment_2br_l.dxf

# Semantic diff between two revisions
./target/release/edl diff examples/studio_a.edl examples/studio_a_wide.edl
```

## Project Layout
- `src/` — Rust compiler, solver, geometry engine, verification ledger, SVG/DXF/KiCad exporters, CLI
- `examples/` — Reference `.edl` designs (valid & deliberate failure cases)
- `docs/` — Prior art, design decisions, v0.1/v0.2 specs, and language reference
- `bakeoff/` — Empirical 6-way LLM syntax bake-off (Block EDL vs Flat EDL vs Python control)
