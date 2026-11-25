---
name: edl-design
description: Author, verify, simulate, inspect, export, and bidirectionally sync multi-domain engineering designs (Architecture, Building Electrical IEC 60364, Electronics Schematics) using the EDL MCP server.
---

# EDL Engineering Workflow

Use the bundled `edl` MCP tools to treat `.edl` as the verified source of truth while interoperating with KiCad 9, AutoCAD DXF, SPICE, and FreeCAD.

## Standard Workflow

1. **Discover capabilities:** Call `edl_domains` once if unsure which built-in or plugin domains and verification rules are registered.
2. **Author intent semantically:**
   - **Architecture (`arch`):** Define `room`, `layout` constraints (`flush_with`, `==`), `opening` (`shared_wall`, `door`/`window`/`passage`), and `fixture` (`against:` or freestanding `at:`). Reuse `module`s via `import`/`use`.
   - **Building Electrical (`mep.elec`):** Add a `building_electrical` block inside the same architectural project; anchor switches via `beside(Door, latch_side, 180mm)` and verify all 6 IEC 60364 rules (`M1`–`M6`).
   - **Electronics (`elec`):** Define typed `component` pins, `net` connectivity, 2D `row [...]` layout, and declarative `dc_voltage` / `branch_current` assertions.
3. **Compile & verify:** Call `edl_compile` (pass `kicad_erc: true` for electronics). Inspect `violations` and structured `repair` plans instead of guessing fixes.
4. **Query & measure precisely:** Use `edl_query` (`room`, `entity`, `net`, `module`, `violations`) and `edl_measure` for exact distances, gaps, shared walls, and routed wire lengths.
5. **Simulate & manufacture:** Call `edl_simulate` + `edl_bom` (electronics) or `edl_schedule` (building electrical), and `edl_render_svg` / `edl_export` (`kicad`, `dxf`, `spice`, `bom`).
6. **Co-edit with humans:** When the engineer edits a KiCad `.kicad_sch` or `.dxf` visually, call `edl_sync` (`apply: false` to preview, `apply: true` to patch and re-verify).
