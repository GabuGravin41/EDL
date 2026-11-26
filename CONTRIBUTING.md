# Contributing to EDL (Engineering Description Language)

Thank you for contributing to EDL! Before opening a Pull Request, please read the core engineering principles below so `main` stays fast, clean, and single-source-of-truth.

## Core Architecture & Design Philosophy

1. **The Rust Compiler Philosophy ("Strict Non-Negotiables, Total Creative Freedom"):**
   - EDL enforces **non-negotiable physical, electrical, and geometric invariants** as hard compile errors (`E1`–`E12`: bad physics, shorts, voltage mismatches, unconnected pins, sheet-frame/title-block overflows, component overlaps, layout cycles).
   - EDL gives human engineers and AI agents **total freedom** on style and placement — do not silently hijack explicit `place` statements; instead report crisp diagnostics and structured `RepairSuggestion`s.
2. **AI-Native Source Files (Zero Coordinate Bloat):**
   - `.edl` source files should stay short, semantic, and coordinate-free (or relative-offset). Exact machine/GUI coordinates belong in `.edl.layout` lockfiles, never mixed into AI-authored intent files.
3. **Single Source of Truth — Do Not Duplicate Existing Modules:**
   - Lexer/Parser: extend [src/syntax/lexer.rs](src/syntax/lexer.rs) and [src/syntax/parser.rs](src/syntax/parser.rs) directly.
   - KiCad 9 Export/Import/Lifting: extend [src/domains/elec/kicad.rs](src/domains/elec/kicad.rs), [src/domains/elec/kicad_import.rs](src/domains/elec/kicad_import.rs), and [src/domains/elec/kicad_lift.rs](src/domains/elec/kicad_lift.rs).
   - MCP Server: extend [src/mcp.rs](src/mcp.rs).
   - Never add un-wired parallel scaffolding or root-level progress-report markdown files.

## Mandatory Pre-PR Checklist

Every PR targeting `main` **must** pass all of the following locally (also enforced automatically by GitHub Actions CI):

```bash
# 1. Zero compiler warnings across library, CLI, and all integration tests
RUSTFLAGS="-D warnings" cargo check --all-targets

# 2. Full Rust test suite passing
cargo test

# 3. If you changed any struct in src/core/ir.rs, regenerate the checked-in JSON schema:
cargo build && ./target/debug/edl schema > docs/canonical_ir.schema.json

# 4. Python adapter tests passing
python3 adapters/test_adapters.py
```

If KiCad 9 (`kicad-cli`) is installed on your machine and you touched electronics routing/export/import, also run:

```bash
python3 scripts/verify_kicad_corpus.py
```
