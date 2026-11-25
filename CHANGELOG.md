# Changelog

All notable releases of **EDL (Engineering Description Language)** are documented here.

## [1.0.0] — 2026-11-25
### Added
- **Native Host-Tool Adapters (`adapters/`):**
  - `adapters/kicad/edl_kicad_bridge.py` + `metadata.json`: Thin bidirectional KiCad 9 bridge & ActionPlugin (`pull`, `status`, `push`, `watch`) with live ERC verification.
  - `adapters/freecad/EDL_Import_Sync.FCMacro`: FreeCAD 3D extrusion macro building floor slab, mitered wall solids, fixtures, and 3D-elevated MEP electrical devices tagged with `EDL_ID` properties + headless `--plan-3d` mode and round-trip fixture sync.
  - `adapters/test_adapters.py`: End-to-end Python integration tests for both adapters.
- **Antigravity Plugin Bundle (`plugins/antigravity-edl/`):** Complete plugin packaging `edl mcp`, `edl-design` skill, verification guardrails (`rules/AGENTS.md`), and SVG icon.
- **Open-Source Release Infrastructure:** MIT `LICENSE`, `CONTRIBUTING.md`, `CHANGELOG.md`, GitHub Actions CI (`.github/workflows/ci.yml`) and multi-platform tagged release workflow (`.github/workflows/release.yml`).

## [0.9.0] — 2026-11-25
### Added
- **Extensible Domain Plugin Architecture:** `DomainEngine` trait (`Base`/`Overlay`, `keywords`, `compile`, model-space `render_overlay` via `ViewTransform`, custom `exports`, `rules`) and `DomainRegistry` (`src/domains/registry.rs`).
- **Generic Extension Blocks (`src/syntax/ext.rs`):** Zero-parser-edit syntax for external domains (`<keyword> <Name> { ... }`) + `EntityGeometryIr::Custom { shape, data }`.
- **Discovery & Live JSON Schema:** `edl domains [--json]`, `edl_domains` MCP tool (11 tools total), and `edl schema` emitting Draft-7 JSON Schema locked to `docs/canonical_ir.schema.json`.
- **Formal v1.0 Documentation:** `docs/EDL_V1_SPEC.md`, `docs/ADAPTER_AUTHORING_GUIDE.md`, and `docs/MCP_INTEGRATION.md`.

## [0.8.0] — 2026-11-25
### Added
- **Stable `EDL_ID` Tags in Native Exports:** Hidden `EDL_ID` properties on KiCad 9 symbols; registered `APPID` (`EDL`) + `EDL_ANCHORS` XDATA points in AutoCAD DXF.
- **Verified Bidirectional Sync (`edl sync` & `edl_sync` MCP tool):** Surgical line-level patching of `.edl` sources from edited `.kicad_sch` and `.dxf` files, re-compiled and verified before writing, with honest `unsupported` / `report_only` classification.
- **Live Co-Editing Bridge (`edl watch`):** Zero-dependency local HTTP pan/zoom SVG viewer with hot reload and two-way `--kicad` file sync.

## [0.7.0] — 2026-11-25
### Added
- **Multi-Domain Layering (`arch` + `mep.elec`):** Building Electrical (`building_electrical`) solved directly on top of architectural IR rooms, doors, and wet fixtures with 6 IEC 60364 checks (`M1`–`M6`), layered SVG (with arced switch legs), 5-layer electrical DXF, and `edl schedule [--csv]` / `edl_schedule`.

## [0.6.0] — 2026-11-25
### Added
- 2D multi-row schematic bands (`row [...]`), `W_SHEET_OVERFLOW` warnings, series-passive DC power rail propagation + `power_flag: true`, built-in exact MNA DC + analytical RC `-3dB` simulator (`edl simulate` + declarative `dc_voltage`/`branch_current` assertions), SPICE3 `.cir` export, and grouped BOM (`edl bom`).

## [0.5.0] – [0.5.1] — 2026-11-24
### Added
- Shared IEEE/IEC schematic vector symbol library, live `kicad-cli sch erc` ingestion (`--kicad-erc`), L-shaped notched rooms, `RepairAction` repair plans, `edl inspect`/`query`/`measure`, and zero-dependency MCP stdio JSON-RPC 2.0 server (`edl mcp`).

## [0.1.0] – [0.4.0] — 2026-11-24
### Added
- Core/Syntax/Domain compiler architecture, RREF linear layout solver, wall CSG, door swing arcs, electronics schematic router + KiCad 9 export, and cross-domain `import`/`module`/`use` composition with provenance tracking.
