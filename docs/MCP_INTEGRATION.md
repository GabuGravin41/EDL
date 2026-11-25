# EDL MCP Server Integration Guide

EDL ships a **zero-dependency, native stdio JSON-RPC 2.0 MCP server** (`edl mcp`) that gives any MCP-compatible AI agent (Antigravity, Claude Desktop, Cursor, Windsurf, Cline, etc.) full access to compile, query, measure, simulate, export, diff, and bidirectionally sync engineering designs.

---

## 1. Build the Release Binary

```bash
cargo build --release
# Binary is at: ./target/release/edl
```

Optional external tool for hardware-grade electronics verification:
- **KiCad 9 (`kicad-cli`)**: enables real `kicad-cli sch erc` ingestion when passing `kicad_erc: true` to `edl_compile` or `edl_export`.

---

## 2. Client Configurations

Replace `/ABS/PATH/TO/EDL/target/release/edl` with the absolute path on your machine.

### Antigravity / Cursor (`mcp.json`)

```json
{
  "mcpServers": {
    "edl": {
      "command": "/ABS/PATH/TO/EDL/target/release/edl",
      "args": ["mcp"]
    }
  }
}
```

### Claude Desktop (`claude_desktop_config.json`)

- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Linux:** `~/.config/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "edl": {
      "command": "/ABS/PATH/TO/EDL/target/release/edl",
      "args": ["mcp"]
    }
  }
}
```

---

## 3. Available MCP Tools (11 Tools)

| Tool | Purpose | Key Inputs |
|---|---|---|
| `edl_domains` | Discover registered domain engines (`arch`, `elec`, `mep.elec`, plugins), export formats, and verification rules | *(none)* |
| `edl_compile` | Compile & verify an `.edl` file or inline source; returns compact summary, evidence ledger, diagnostics, and `RepairAction` plans | `file_path` or `source`, optional `kicad_erc` |
| `edl_query` | Targeted semantic lookup without dumping full IR (`summary`, `entity`, `net`, `room`, `module`, `violations`) | `kind`, optional `target`, `file_path`/`source` |
| `edl_measure` | Exact analytical geometry: bounds/area, routed net wire length (mm), clear gap, shared wall length, shared nets | `target_a`, optional `target_b`, `file_path`/`source` |
| `edl_render_svg` | Render architectural, building-electrical, or schematic SVG preview to disk | `output_path`, optional `annotated`, `file_path`/`source` |
| `edl_export` | Export to `dxf`, `kicad`, `spice`, or `bom` (optionally verifying with real `kicad-cli` ERC) | `format`, `output_path`, optional `kicad_erc` |
| `edl_simulate` | Run built-in exact MNA DC operating-point + analytical RC `-3dB` cutoff simulation (electronics) | `file_path` or `source` |
| `edl_bom` | Generate grouped Bill of Materials with RefDes lists, values, footprints, and module provenance | `file_path` or `source` |
| `edl_schedule` | Generate Building Electrical DB Panel & Circuit Schedule (breaker ratings, RCDs, 3D run lengths, loads, voltage drops) | `file_path` or `source`, optional `csv` |
| `edl_sync` | Bidirectional sync: diff an edited KiCad `.kicad_sch` or `.dxf` back into verified, surgical `.edl` patches | `file_path`, `from_kicad` or `from_dxf`, optional `apply` |
| `edl_diff` | Compute a semantic diff by stable entity ID between two `.edl` revisions | `before_path`/`before_source`, `after_path`/`after_source` |

---

## 4. Recommended AI Engineering Loop

1. **Discover** — call `edl_domains` once to see active domains and verification rules.
2. **Draft & Verify** — write `.edl` (reusing `module`s via `import`/`use`), then call `edl_compile` (with `kicad_erc: true` for schematics).
3. **Repair Deterministically** — if any check is `Violated`, inspect `violations` + `repair` actions, and use `edl_measure` / `edl_query` to get exact numbers rather than guessing coordinates.
4. **Simulate & Schedule** — run `edl_simulate` + `edl_bom` (electronics) or `edl_schedule` (building electrical).
5. **Co-Edit with Humans** — export to native tools (`edl_export`) or run `edl watch`; when the human adjusts layout or component values visually, call `edl_sync` (`apply: false` to preview, `apply: true` to patch and re-verify).
