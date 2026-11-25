# Changelog

All notable releases of **EDL (Engineering Description Language)** are documented here.

## [1.6.0] — 2026-11-25
### Added
- **Building Power Single-Line Diagram Domain (`elec.sld`, `src/domains/sld/`):** `power_sld` blocks modeling MV/LV transformers, standby generators, Solar PV inverters, BESS battery storage, split main LV switchboards (`LVMD`), ATS bus couplers (`QF_Tie`), sub-distribution boards (`SDB`), PFC capacitor banks, motors, UPS, and EV charging hubs.
- **6 Exact IEC 60909 / IEC 60364 / IEC 61439 Proofs (`S1`–`S6`):** bottom-up power flow & **N-1 single-transformer emergency contingency (`S1`)**, cable/breaker `Ib <= In <= Iz` coordination (`S2`), upstream/downstream breaker selectivity (`S3`), exact IEC 60909 prospective 3-phase short-circuit `Ik3'' <= Icu` breaking capacity (`S4`), cumulative voltage drop (`S5`), and formal ATS 2-of-3 coupler interlock proof (`S6`).
- **Dual IEC 60617 SLD SVG + 6-Layer AutoCAD R12 DXF Renderer** and benchmark `examples/building_power_sld_2x1600kva.edl`.

## [1.5.0] — 2026-11-25
### Added
- **Complex Architecture & Structural Axis Grids (`src/domains/arch/grid.rs`):** `structural_grid` blocks (`x_bays`, `y_bays`, `x_labels`, `y_labels`, `columns: all_intersections | perimeter | [...]`, `column_kind: concrete | steel_i`, `levels { Room: "±0.000" }`) injecting `grid_x_<label>` / `grid_y_<label>` directly into the RREF linear layout solver.
- **Structural Column Clearance Proof (`V7_structural_column_clearance`):** Exact analytical verification that structural columns never collide with door swings or opening cutouts.
- **Variable Shared-Wall Thicknesses & Building Core Drafting:** Seamless mixing of 380mm load-bearing spines, 240mm party walls, and 120mm interior partitions; crisp architectural SVG + DXF (`GRID_AXES`, `GRID_BUBBLES`, `COLUMNS`) rendering for `stair_u`, `stair_straight`, `elevator`, `shaft`, outdoor terraces/balconies, and large-building auto-scaling (up to 70m warehouses).
- **3 Large-Scale Architectural Benchmarks:** `examples/complex_villa_grid_stepped.edl` (15 rooms, stepped 19.45m×13.90m villa), `examples/complex_apartment_multiunit.edl` (27 rooms, L-shaped 32.3m×21.6m multi-unit apartment plate with 2 stair/elevator cores), and `examples/complex_warehouse_structural.edl` (48.3m×28.8m 8-bay warehouse with 36 steel I-beam columns).

## [1.4.0] — 2026-11-25
### Added
- **Built-In Building Plumbing Overlay (`mep.plumbing`):** `building_plumbing` layered on solved `arch` floor plans (and coordinated with `mep.elec`) via `DomainEngine`, supporting `riser`, `heater`, `stack`, `valve`, `floor_drain`, and semantic `connect <Fixture>` rules.
- **6 Exact EN 806 / EN 12056 Plumbing Verification Rules (`P1`–`P6`):** wet fixture supply coverage, soil/waste pipe sizing & self-cleansing gravity slopes, unvented trap-arm distance limits, hydraulic supply velocity (`<= 2.0 m/s`), isolation valve checks, and **automated cross-discipline `mep.plumbing` <-> `mep.elec` clash detection**.
- **Layer-Selectable Multi-Discipline Rendering (`--layers`):** Render ONE shared `.edl` project into pure Architecture (`--layers arch`), Architectural + Electrical (`--layers arch,elec`), Architectural + Plumbing (`--layers arch,plumbing`), or combined MEP Coordination (`--layers all`), plus 19-layer AutoCAD R12 DXF (`PLUMB_COLD/HOT/DRAIN/VENT/EQUIP`), `edl plumbing-schedule [--csv]`, and `edl_plumbing_schedule` MCP tool (14 MCP tools total).
- **Multi-Discipline Benchmark (`examples/villa_3br_arch_elec_plumbing.edl`):** 8-room family villa unifying `arch` + `mep.elec` + `mep.plumbing` with 19/19 rules `EXACTLY_CHECKED`.

## [1.3.0] — 2026-11-25
### Added
- **Industrial Electrical & Motor Control Domain (`elec.industrial`):** `industrial_control` blocks modeling split multi-unit IEC 60617 devices (`breaker`, `overload`, `contactor`, `pushbutton`, `motor`), 3-phase main power circuits with L1↔L3 phase reversal & Star-Delta starters, and vertical 220V control ladders with automatic coil↔contact cross-reference tables.
- **Formal Safety Verification (`I1`–`I4`):** Exhaustive Boolean mutual-interlock proof (`I1_mutual_interlock`), E-Stop/overload cutset authority (`I2_estop_overload_authority`), coil↔contact cross-reference audit (`I3`), and 3-phase sequence swap verification (`I4`).
- **Dual IEC 60617 SVG + 6-Layer AutoCAD R12 DXF Renderer** (`IEC_POWER_BUS`, `IEC_CONTROL_WIRE`, `IEC_SYMBOLS`, `IEC_MECH_LINK`, `IEC_TEXT`, `IEC_CROSSREF`) and Benchmark C (`examples/motor_forward_reverse_star_delta.edl`).

## [1.2.0] — 2026-11-25
### Added
- **Multi-Sheet KiCad 9 Hierarchy (`--kicad-dir`, `--sheet`, `--all-sheets`):** Automatic root `.kicad_sch` + child sheets + `(global_label)` cross-sheet nets, verified with real `kicad-cli sch erc`.
- **2D Recursive Relative Layout Combinators (`zone`/`group` with `hstack`/`vstack`/`subzone`/`note`)** and **obstacle-aware 2.54mm grid A* orthogonal schematic router** (`src/domains/elec/router.rs`).
- **Ergonomic `use Inst = Mod(p = v) { PORT: NET }` port-binding blocks** + numeric/`+`/`-` pin references.
- **Benchmarks A & B:** `examples/vectron_120_multisheet.edl` (121 components, 61 nets, 4 sheets, 0 KiCad 9 ERC errors) and `examples/smps_flyback_isolated.edl` (58 discrete components, galvanic isolation `E6`, 0 KiCad 9 ERC errors).

## [1.1.0] — 2026-11-25
### Added
- **Native KiCad 9 `.kicad_sym` Library Engine (`src/domains/elec/kicad_sym.rs`):** Scans installed KiCad 9 symbol libraries (224 libs / ~20,000+ symbols) with `extends` inheritance resolution, multi-unit extraction, `orient`/`mirror`, explicit `no_connect` markers, `edl symbols search|inspect`, and MCP `edl_symbol_search`/`edl_symbol_inspect`.
- **Galvanic Isolation Verification (`E6`):** `require galvanic_isolation(A, B)` proving transformer/optocoupler isolation boundaries between hot & cold grounds.

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
