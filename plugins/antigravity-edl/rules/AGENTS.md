# EDL Engineering Guardrails

- Always verify every `.edl` change with `edl_compile` (and `kicad_erc: true` for electronics projects) before reporting a design as complete.
- Never claim a rule passed unless its evidence record status is `ExactlyChecked` or `FormallyChecked`.
- Use `edl_measure` and `edl_query` for exact geometry/clearance values rather than eyeballing or approximating coordinates.
- When syncing human edits from KiCad or DXF via `edl_sync`, preview first (`apply: false`) when `unsupported` or `report_only` actions exist, and explain constraint drift instead of overwriting semantic constraints.
