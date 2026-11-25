# Contributing to EDL

Welcome! EDL is designed so contributors can add **new engineering domains** and **new native host-tool adapters** without fighting the compiler core.

## Quick Verification Checklist

Before opening a pull request, run:

```bash
# 1. All Rust unit & integration tests (including Canonical IR JSON Schema parity)
cargo test

# 2. Release build with zero warnings
cargo build --release

# 3. Host-tool adapter end-to-end tests (KiCad 9 + FreeCAD)
python3 -m unittest adapters/test_adapters.py -v
```

If you have KiCad 9 (`kicad-cli`) installed locally, also verify live ERC on all reference schematics:

```bash
for f in power_5v mcu_board_3v3 mcu_board_3v3_symbols stress_sensor_node sensor_node_2row dual_rail_modular; do
  ./target/release/edl build examples/$f.edl --kicad-erc
done
```

## Updating the Canonical IR Schema

If you add or change any field on `CanonicalIr` (or types it references), regenerate the checked-in schema:

```bash
cargo run -- schema > docs/canonical_ir.schema.json
```

The `canonical_ir_json_schema_matches_checked_in_docs` test enforces that `docs/canonical_ir.schema.json` never drifts from the Rust compiler types.

## Where to Contribute

- **New Engineering Domain** (`plumbing`, `hvac`, `structural`, `pcb`, `mech`):
  Implement `edl::domains::registry::DomainEngine` using `ExtensionBlock` + `EntityGeometryIr::Custom`. See [`docs/ADAPTER_AUTHORING_GUIDE.md`](docs/ADAPTER_AUTHORING_GUIDE.md) and [`tests/plugin_domain.rs`](tests/plugin_domain.rs).
- **New Host-Tool Adapter** (AutoCAD, Revit, Onshape, Fusion 360, LibreCAD):
  Place a thin adapter under `adapters/<tool>/` that consumes `edl build --json` / `edl export`, embeds stable `EDL_ID` metadata, and syncs edits back via `edl sync`. See [`adapters/kicad/`](adapters/kicad/) and [`adapters/freecad/`](adapters/freecad/) for working templates.
- **Core Improvements & Bug Fixes:**
  Keep `src/core/` domain-neutral, emit honest `EvidenceRecord` statuses (`ExactlyChecked` / `Violated` / `Unverified`), and include a focused regression test in `src/lib.rs` or `tests/`.
