# EDL ↔ FreeCAD 3D Adapter

A thin Python macro that reuses FreeCAD's 3D kernel to extrude verified architectural + building-electrical `.edl` projects into live 3D solids (floor slab, exact mitered walls, fixtures, and 3D-elevated DB panels/lights/switches/sockets) — tagging every object with `EDL_ID` and `EDL_KIND` properties.

## Inside FreeCAD
1. Copy `adapters/freecad/EDL_Import_Sync.FCMacro` into your FreeCAD Macros folder.
2. Ensure `edl` is on your `PATH` (or set `EDL_BIN=/path/to/edl`).
3. Run **Macro → Macros → `EDL_Import_Sync.FCMacro`** and select any architectural `.edl` project (e.g., `examples/apartment_2br_electrical.edl`).

## Headless / Scriptable Mode

```bash
# Inspect exact 3D solid coordinates & group breakdown without launching FreeCAD GUI
python3 adapters/freecad/EDL_Import_Sync.FCMacro examples/apartment_2br_electrical.edl --plan-3d
```

Moved freestanding fixtures in FreeCAD can be synced straight back to `.edl` via `sync_freecad_centers_to_edl(...)` (reusing EDL's verified `edl sync --from-dxf --apply` pipeline).
