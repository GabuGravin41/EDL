# EDL — Engineering Description Language (`v3.6.0`)

An AI-native, typed, declarative language, Rust compiler, and MCP server for expressing engineering intent, enforcing hard physical/electrical/code non-negotiables (**"Rust for Engineering Schematics & MEP Plans"**), and compiling directly to native industry tools (**KiCad 9 `.kicad_sch`**, **AutoCAD R12 DXF**, **SPICE3**, **CSV Schedules/BOM**, and **layered SVG**).

## Philosophy — Total Creative Freedom + Hard Compiler Non-Negotiables
Just like Rust never dictates how you structure your code while refusing to compile memory bugs, EDL gives engineers and AI agents **complete freedom over spatial layout and design style** (`rect:`, `place ... center (x, y)`, `right_of`/`below`, `divider`, `overlay "..."`, `pack: order`) while strictly enforcing the non-negotiables:
- **Zero bad physics or unsafe circuits** (`E1`–`E16`: shorts, multi-drivers, voltage compatibility, galvanic isolation, IC decoupling, I2C pull-ups, inductive flyback, 80% DC voltage & thermal power derating, barrier divider crossings, real KiCad 9 footprint library checks).
- **Zero geometric or code-compliance collisions** (`E11` sheet/title-block fit, `E12` zero symbol overlap, `V1`–`V6` arch overlaps/jambs/swings/reachability/windows, `P1`–`P9` plumbing slope/trap-arm/DFU/dry-room/column clashes, `M1`–`M8` IEC 60364 breaker/ampacity/voltage-drop/RCD/3-phase balance/900mm DB working clearance, `S1`–`S6` IEC 60909 short-circuit/N-1/interlocks).
- **Zero-warning native exports & bidirectional sync:** Real KiCad 9 flattened library symbols + native `power.kicad_sym` rails + automatic `PWR_FLAG`s (**0 KiCad ERC errors, 0 warnings**), consultant-grade dimmed architectural underlays on MEP drawings, and 100% lossless round-trip across **36/36 official KiCad 9 corpus demos** (including the 1,507-component CERN VME-WREN FPGA board).

---

## Verified Capabilities (`v3.5.0` — 71 Rust Tests, 0 Warnings, 16 MCP Tools, 36/36 KiCad 9 Corpus)

### 1. Electronics Schematics (`elec` → KiCad 9, SPICE3, CSV BOM, SVG)
- **Total spatial freedom + progressive layout:** Explicit zone rectangles (`group Stage1 { rect: (x0, y0, x1, y1)  color: blue|red  border: dashed  note: "..." }`), exact symbol-center placement (`place U_BUCK center (180mm, 65mm) [orient: 90|180|270]`), relative pin-anchored placement (`left_of`, `right_of`, `above`, `below`, `align_y`, `array`), isolation barrier dividers (`divider { from: ... to: ... label: "ISOLATION BARRIER" color: red }`), and multi-sheet hierarchical export (`--kicad-dir`).
- **Human-grade KiCad 9 output:** Verbatim flattened KiCad 9 system symbols (resolving `(extends ...)` chains), native `power.kicad_sym` rails (`+24V`, `+5V`, `+3V3`, `GND`) with automatic multi-sheet `PWR_FLAG`, shared horizontal power/GND trunks with junction dots, 100% horizontal RefDes/Value labels on `90°`/`270°` rotated passives, and crisp red-outlined `(global_label ...)` flags for cross-stage & cross-sheet signals.
- **16 Formal Proofs (`E1`–`E16`):** Connectivity, shorts, multi-drivers, voltage compatibility with series-passive rail propagation, single-pin nets, galvanic isolation (`galvanic_isolation(GND, ISO_GND)`), IC decoupling, I2C pull-ups, inductive flyback (including multi-pin `Relay:Relay_SPDT`), BOM manufacturability, `E11` frame & title-block fit, `E12` zero component overlap, `E13`/`E14` MNA-backed 80% voltage/power derating, `E15` barrier divider non-crossing, and `E16` KiCad 9 footprint existence check with fuzzy suggestions.
- **Built-in MNA DC/RC Simulation & Live KiCad 9 ERC:** `edl simulate`, `edl build --kicad-erc`, `edl import-kicad --lift`.

### 2. Architectural Floor Plans & Structural Grids (`arch` → SVG, 19-Layer DXF)
- **Constraint-solved space-first plans (`0 DOF`):** RREF linear solver with exact DOF & contradiction provenance, L-shaped notched rooms (`notch:`), structural column grids (`structural_grid`), hosted doors/windows/passages with analytical swing arcs, and wall/freestanding fixtures with clearances (`V1`–`V6`).
- **Clean multi-discipline foundation:** `overlay "base_arch.edl"` lets plumbing and electrical files inherit the entire floor plan, structural grid, views, and deduplicated verification rules with zero copy-paste.

### 3. Building Plumbing (`mep.plumbing` → SVG, DXF, CSV Takeoff)
- **Consultant-grade dimmed underlay + `NO PLUMBING ZONE`:** Automatically dims architectural walls/fixtures and highlights `*Elec*`/`*IT*`/`*Server*` rooms in red diagonal hatching.
- **9 Exact Proofs (`P1`–`P9`):** Wet fixture supply, drainage sizing & slope, trap-arm distance, supply velocity, isolation valves, cross-discipline MEP clash, DFU header capacity, `P8` dry-room protection (`E_PLUMB_DRY_ROOM_CLASH`), and `P9` exact Liang-Barsky pipe-vs-column clash check (`E_PLUMB_COLUMN_CLASH`).

### 4. Building Electrical (`mep.elec` → SVG, DXF, CSV Panel Schedule)
- **Consultant-grade dimmed underlay + widened Panel Schedule sidebar + red hatched 900×900mm DB working clearance boxes.**
- **8 IEC 60364 Proofs (`M1`–`M8`):** Breaker load & IEC 60364-5-52 cable ampacity (`In <= Iz`), 3D copper voltage drop, wet-zone IP clearance, latch-side switch accessibility, device/opening clearance, 30 mA RCD on wet circuits, `M7` native 3-phase balance (`phase: L1|L2|L3|3P`, `<= 15%`), and `M8` 900mm DB panel working clearance.

### 5. Power Single-Line Diagrams (`elec.sld` → SVG, DXF)
- **Native 11 kV MV Intake & RMU:** `mv_intake RMU_11KV { utility_kv: 11  sk_mva: 250  vcb_a: 630  vcb_icu_ka: 25 }` computes exact IEC 60909 upstream grid impedance $Z_Q$ from $S_k''$, renders the red 11 kV Utility Intake → RMU Busbar → MV VCBs above transformers, and prints both the **IEC 60909 Short-Circuit & Switchgear Verification Table** and **N-1 Contingency Summary**.
- **6 IEC 60909 / 60364 Proofs (`S1`–`S6`):** Source/bus & N-1 contingency capacity, breaker/cable coordination, protection selectivity, short-circuit breaking capacity ($I_{cu} \ge I_{k3}''$), cumulative voltage drop, and ATS/coupler interlocks.

### 6. AI Ergonomics, Live Sync & MCP Server (`16` Tools)
- **`edl guide [elec|arch|plumbing|mep_elec|sld|all]`** — Copy-paste-ready domain cheat sheets so any AI agent writes idiomatic `v3.5.0` EDL on Turn 1.
- **`edl inspect <file> [--pins] [--zones] [--nets]`** — Sub-50 ms exact layout coordinates, pin tip positions, zone boxes, dividers, and routed net bounding boxes.
- **`edl symbols search` / `edl symbols inspect`** — Live KiCad 9 system library search & pin/footprint inspection.
- **`edl diff` / `edl query` / `edl measure` / `edl sync` / `edl watch` / `edl mcp`** — Semantic diffing, bidirectional KiCad/DXF sync, hot-reloading browser preview, and 16-tool JSON-RPC 2.0 MCP server (`edl_guide`, `edl_inspect_layout`, `edl_compile`, `edl_query`, `edl_measure`, `edl_render_svg`, `edl_export`, `edl_simulate`, `edl_bom`, `edl_schedule`, `edl_plumbing_schedule`, `edl_sync`, `edl_symbol_search`, `edl_symbol_inspect`, `edl_domains`, `edl_diff`).

---

## Quick Start

```bash
cargo test
cargo build --release

# 0) Instant domain cheat sheet for humans & AI agents
./target/release/edl guide elec
./target/release/edl guide plumbing

# 1) Electronics (single-sheet or multi-sheet KiCad 9 + 0-warning ERC + sub-50ms layout inspection)
./target/release/edl build benchmarks/v3_4_verified/b1_elec_gateway/gateway.edl --kicad-erc
./target/release/edl inspect benchmarks/v3_4_verified/b1_elec_gateway/gateway.edl --pins --zones --nets
./target/release/edl export benchmarks/v3_4_verified/b1_elec_gateway/gateway.edl --kicad out/gateway.kicad_sch
./target/release/edl bom benchmarks/v3_4_verified/b1_elec_gateway/gateway.edl --csv

# 2) Multi-Discipline Building (Arch + Plumbing + Electrical via `overlay`, 0 duplication)
./target/release/edl build benchmarks/v3_3_verified/b3_clinic_plumbing/clinic_plumbing.edl
./target/release/edl preview benchmarks/v3_3_verified/b3_clinic_plumbing/clinic_plumbing.edl --layers arch,plumbing -o out/clinic_plumbing.svg
./target/release/edl build benchmarks/v3_3_verified/b4_clinic_electrical/clinic_electrical.edl
./target/release/edl schedule benchmarks/v3_3_verified/b4_clinic_electrical/clinic_electrical.edl --csv

# 3) Hospital 11 kV / 400 V Power SLD (IEC 60909 / 60364)
./target/release/edl build benchmarks/v3_3_verified/b5_hospital_sld/hospital_sld.edl
./target/release/edl preview benchmarks/v3_3_verified/b5_hospital_sld/hospital_sld.edl -o out/hospital_sld.svg
./target/release/edl export benchmarks/v3_3_verified/b5_hospital_sld/hospital_sld.edl --dxf out/hospital_sld.dxf

# 4) Bidirectional Sync, Live Watch & MCP Server (16 tools)
./target/release/edl sync examples/mcu_board_3v3.edl --from-kicad out/edited.kicad_sch --apply
./target/release/edl watch examples/mcu_board_3v3.edl --port 8765 --kicad out/live.kicad_sch
./target/release/edl mcp
```

---

## Repository Structure
- `src/core/` — Domain-neutral kernel (`span` + `RepairSuggestion`, `units`, `geom`, `ir` + Draft-7 JSON Schema, `query`, `diff`, `sync`)
- `src/syntax/` — Unified lexer, AST, parser, `overlay` + `import`/`module`/`use` elaborator, and extension blocks (`ext`)
- `src/guide.rs` — Built-in domain guides (`edl guide`) & sub-50 ms layout introspection (`edl inspect --pins --zones --nets`)
- `src/domains/arch/` — Architecture AST, RREF linear solver, structural column grid, dimmed MEP underlay SVG & DXF backends
- `src/domains/elec/` — Electronics compiler (`E1`–`E16`), KiCad 9 symbol/footprint loader, free & relative layout, trunk/A* router, MNA SPICE simulator, BOM, KiCad 9 single & multi-sheet exporter, and KiCad semantic lifter
- `src/domains/mep_elec/` — Building Electrical overlay (`M1`–`M8`, IEC 60364 ampacity, 3-phase balance, DB clearance, panel schedule)
- `src/domains/mep_plumbing/` — Building Plumbing overlay (`P1`–`P9`, DFU headers, dry-room protection, column clash, takeoff schedule)
- `src/domains/sld/` — Power Single-Line Diagram engine (`S1`–`S6`, native `mv_intake`, IEC 60909 short-circuit table, N-1 contingency)
- `src/domains/industrial/` — Industrial Control ladder/contactor safety engine (`I1`–`I4`, IEC 60617)
- `src/mcp.rs` & `src/watch.rs` — 16-tool MCP JSON-RPC 2.0 server & live hot-reload watch bridge
- `benchmarks/` — Reproducible industry-scale A/B benchmarks comparing No-EDL Python vs EDL (`v3_2_industry_ab/`, `v3_3_verified/`, `v3_4_verified/`, `v3_5_collapse_ab/`)
- `examples/`, `adapters/`, `plugins/antigravity-edl/`, `docs/` — Reference designs, KiCad/FreeCAD adapters, Antigravity plugin bundle, and specs
