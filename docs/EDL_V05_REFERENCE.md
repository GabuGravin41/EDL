# EDL v0.5 Reference — Domain Depth & AI Tooling Ergonomics

EDL v0.5 completes the 4-pillar roadmap built on the v0.4 Core/Syntax/Domain compiler:
1. **Real IEEE/IEC Schematic Symbols** in both SVG previews and KiCad 9 `.kicad_sch` exports (`src/domains/elec/symbols.rs`).
2. **Automated KiCad 9 ERC Evidence Ingestion** (`--kicad-erc` / `run_and_ingest_kicad_erc`).
3. **Architecture Domain Depth**: L-shaped notched rooms (`notch: (w, d) at <corner>`) + architectural interior room dimension chains in SVG & DXF.
4. **AI Tooling Ergonomics**: `edl inspect`, `edl query`, `edl measure`, machine-readable `RepairAction` repair plans, and a zero-dependency **MCP JSON-RPC 2.0 server (`edl mcp`)**.

---

## 1. IEEE/IEC Schematic Symbols

`src/domains/elec/symbols.rs` generates grid-accurate vector primitives shared by both EDL SVG (`elec/svg.rs`) and KiCad 9 `lib_symbols` (`elec/kicad.rs`) while keeping all pin tips locked to KiCad's `2.54mm` (100 mil) grid:

| Category | Rendered Symbol |
| --- | --- |
| `resistor` / `r` | 6-peak IEEE zig-zag (auto-oriented horizontal or vertical) |
| `capacitor` / `c` | Parallel capacitor plates (with `+` polarity mark when `POS` pin present) |
| `inductor` / `l` | 4-lobe coil arcs |
| `diode` / `zener` / `schottky` | Filled diode triangle + cathode bar |
| `led` | Diode triangle + cathode bar + twin light-emission arrows |
| `switch` / `button` / `sw` | SPST tactile push-button switch |
| `opamp` / `comparator` | Triangular amplifier body with `+` / `-` input markers |
| `ic`, `regulator`, `connector`, custom | Shaded EDA block rectangle with internal pin names |

---

## 2. Automated Target-Side KiCad 9 ERC (`--kicad-erc`)

```bash
edl build examples/dual_rail_modular.edl --kicad-erc
edl inspect examples/dual_rail_modular.edl --kicad-erc
```

EDL exports to a temporary `.kicad_sch`, invokes `kicad-cli sch erc --severity-error --format json`, and ingests results directly into the Canonical IR evidence ledger as `K1_kicad_erc_external` (`EXACTLY_CHECKED`, `VIOLATED`, or `UNVERIFIED` if `kicad-cli` is not installed).

---

## 3. L-Shaped Notched Rooms & Interior Dimension Chains

Declare an L-shaped room directly with a corner notch (`northeast | northwest | southeast | southwest`):

```edl
room GreatRoom {
  role: habitable
  clear_size: (6400mm, 5200mm)
  notch: (2320mm, 2520mm) at northeast
  anchor: southwest at Plan.origin
}
```

- Another room (e.g. `use Bath = StandardBath(...)`) can be tucked directly inside the notch separated by `interior_wall`, with automatic `shared_wall(GreatRoom, Bath)` resolution, exact CSG walls, zero-overlap verification, and exact door-swing / fixture-in-notch collision checks (`E_VERIFY_DOOR_SWING_NOTCH`, `E_VERIFY_FIXTURE_IN_NOTCH`).
- Architectural SVG previews (`show_dimensions: true`) now render both outer overall building dimensions and secondary **interior room dimension chains** along all differing edges.
- See [loft_notched.edl](file:///home/lindah/projects/EDL/examples/loft_notched.edl).

---

## 4. AI Tooling CLI & MCP Server (`edl mcp`)

### CLI Commands

```bash
# Compact AI-friendly overview (bounds, module provenance, nets, evidence)
edl inspect examples/dual_rail_modular.edl --kicad-erc

# Targeted semantic queries (summary | entity | net | room | module | violations)
edl query examples/duplex_modular.edl room Bed1
edl query examples/dual_rail_modular.edl module LedIndicator
edl query examples/power_5v_invalid.edl violations   # includes machine-readable repair_plan!

# Exact analytical geometry & routing measurements
edl measure examples/loft_notched.edl GreatRoom Bath
edl measure examples/dual_rail_modular.edl STAT_SIG
edl measure examples/dual_rail_modular.edl U_MCU J_DBG

# Start Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio
edl mcp
```

### MCP Tools Exposed by `edl mcp`

Configure any MCP client (Antigravity, Claude Desktop, Cursor, etc.) to launch `edl mcp` over stdio:

| MCP Tool | Purpose |
| --- | --- |
| `edl_compile` | Compile & verify file or inline EDL (with optional `kicad_erc`), returning compact summary + violations + `repair_plan` |
| `edl_query` | Query `summary`, `entity`, `net`, `room`, `module`, or `violations` without dumping full IR |
| `edl_measure` | Measure entity dimensions, routed net wire length (mm), or pairwise clear gap / center distance / shared walls / shared nets |
| `edl_render_svg` | Render architectural or schematic SVG preview to disk |
| `edl_export` | Export AutoCAD `dxf` or KiCad 9 `kicad` schematic (with optional `kicad_erc`) |
| `edl_diff` | Compute semantic diff by stable entity ID between two revisions |
