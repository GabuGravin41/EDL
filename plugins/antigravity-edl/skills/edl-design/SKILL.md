---
name: edl-design
description: Author, verify, simulate, inspect, export, and bidirectionally sync multi-domain engineering designs (Electronics Schematics KiCad 9, Architecture, Building Plumbing, Building Electrical IEC 60364, Power SLD IEC 60909) using the EDL MCP server or CLI.
---

# EDL v3.5.0 Engineering Workflow

Treat `.edl` as the typed, verified source of truth while interoperating natively with **KiCad 9**, **AutoCAD R12 DXF**, **SPICE3**, and **FreeCAD**.

## Standard Workflow

1. **Get instant idiomatic syntax:** Call `edl_guide` (or `edl guide <elec|arch|plumbing|mep_elec|sld>`) on turn 1 before authoring a new project. Call `edl_domains` to inspect registered domains and rules.
2. **Author intent with full spatial freedom:**
   - **Electronics (`elec`):** Discover real KiCad 9 parts via `edl_symbol_search` / `edl_symbol_inspect`. Arrange sheets freely using `group ... { rect: (x0, y0, x1, y1)  color: blue|red  border: dashed }`, `place Comp center (x, y) [orient: 90|180|270]`, `divider { from: ... to: ... label: "ISOLATION BARRIER" color: red }`, or coordinate-free `layout { pack: order }`. EDL enforces `E1`–`E16` (shorts, isolation, decoupling, I2C pull-ups, flyback, 80% DC voltage & thermal power derating, frame/title-block fit, zero overlap, barrier divider non-crossing, and real KiCad footprint existence) and exports **0-error, 0-warning KiCad 9** single-sheet (`--kicad`) or hierarchical multi-sheet (`--kicad-dir`) schematics.
   - **Architecture (`arch`):** Define `room`, `structural_grid`, `layout` constraints (`0 DOF`), `opening`, and `fixture` (`V1`–`V7`).
   - **Building Plumbing (`mep.plumbing`) & Building Electrical (`mep.elec`):** Use `overlay "base_arch.edl"` so you never duplicate architectural geometry! Plumbing proves `P1`–`P9` (incl. `NO PLUMBING ZONE` in Elec/IT rooms & column clash); Electrical proves `M1`–`M8` (incl. `phase: L1|L2|L3|3P` 3-phase balance `<= 15%`, `In <= Iz` ampacity, and 900mm DB working clearance).
   - **Power SLD (`elec.sld`):** Use `mv_intake` (`sk_mva`), `bus`, `source`, `coupler`, `load` for exact IEC 60909 short-circuit, N-1 contingency, and switchgear proofs (`S1`–`S6`).
3. **Compile, inspect layout in <50ms & verify:** Call `edl_compile` (`kicad_erc: true` for electronics) and `edl_inspect_layout` (`edl inspect <file> --pins --zones --nets`) to see exact placed pin tips, zone boxes, and net bounds before rendering PNGs.
4. **Query, measure & diff ECO revisions:** Use `edl_query`, `edl_measure`, and `edl_diff` across revisions.
5. **Simulate & manufacture:** Call `edl_simulate`, `edl_bom`, `edl_schedule`, `edl_plumbing_schedule`, `edl_render_svg`, and `edl_export`.
6. **Co-edit with humans:** Call `edl_sync` (`apply: false` to preview, `apply: true` to patch and re-verify).
