# Benchmark 2: Outpatient Specialist Medical Clinic — Baseline (NO-EDL) Notes

## Benchmark Timing & Telemetry
- **Arm:** Baseline (NO-EDL, pure Python + `shapely` + `networkx` + `matplotlib` + custom AutoCAD R12 DXF writer)
- **start_epoch:** `1795710879`
- **end_epoch:** `1795711380`
- **elapsed_sec:** `501` (~8 min 21 sec)
- **iterations:** `2` (Iteration 1: complete geometry, verifier, DXF, manifest, and initial PNG passing 100% of automated checks; Iteration 2: visual inspection polish fixing 4 room-label overlaps on clinical fixtures and adding fixture text callouts)
- **tools_used:** `python3`, `shapely`, `networkx`, `matplotlib`, `numpy` (no `ezdxf` installed, so a clean AC1009 AutoCAD R12 ASCII DXF writer was implemented in Python)

## Architectural & Structural Design Summary
- **Footprint:** Stepped L-shaped building (`27.0 m × 11.0 m` overall, with a `5.4 m × 5.0 m` South-East bay step-in).
  - **Gross Footprint Area:** `281.49 m²`
  - **Net Interior Room Area:** `243.83 m²` (both gross and net sit cleanly inside the `240–290 m²` target).
- **Structural Column Grid:** Regular 5×2 bay grid (`5 × 5.4 m` along X: grids `A–F`; `5.0 m` and `6.0 m` along Y: grids `1–3`), with **17** reinforced-concrete/steel columns (`300 × 300 mm`).
  - Exterior wall thickness: **300 mm** (perimeter columns sit flush inside exterior walls).
  - Interior partition thickness: **150 mm**.
  - Corridor South partition sits at `Y = 5000..5150`, keeping the North face of all Grid-2 columns flush with the corridor boundary (`0.0 mm²` column encroachment into the `2200 mm` wide corridor).
- **Room Schedule (16 Rooms: all 15 mandatory + bonus `StaffWC`):**
  - South Wing (`4.85 m` clear depth): `Vestibule` (`8.97 m²`), `Waiting` (`29.83 m²`), `Exam1` (`13.58 m²`), `Exam2` (`13.58 m²`), `Exam3` (`13.58 m²`), `Procedure` (`20.13 m²`).
  - Central Spine: `Corridor` (`2200 mm` clear width >= `1800 mm`, `58.74 m²`).
  - North Wing (`3.35 m` clear depth): `PublicWC` (`6.37 m²`), `StaffWC` (`5.03 m²`), `Breakroom` (`12.90 m²`), `NurseStation` (`11.72 m²`), `Exam4` (`12.90 m²`), `Lab` (`11.05 m²`), `Pharmacy` (`9.04 m²`), `ElecITRoom` (`7.37 m²`, strictly dry zone), `MechRoom` (`9.04 m²`).
- **Doors (16), Windows (13), Fixtures (36):**
  - Exterior entry `D01` = `1300 mm` (>= `1200 mm`), `PublicWC` `D08` = `950 mm` (>= `900 mm`), `Procedure` `D07` = `1200 mm` (>= `1100 mm`).
  - All doors have `>= 200 mm` jamb margins (`>= 100 mm` required) and swing inward into rooms (0 corridor obstructions, 0 swing-to-swing or swing-to-fixture collisions).
  - All 7 daylight-required rooms (`Waiting`, `Breakroom`, `Exam1`–`Exam4`, `Procedure`) plus `NurseStation`, `Lab`, and both corridor ends have exterior glazing clear of all columns.

## Honest Log of Geometric Challenges & Errors Encountered
1. **No `ezdxf` package available:** Checked environment upfront and wrote a self-contained AutoCAD R12 (`AC1009`) ASCII DXF serializer (`SimpleR12DXF`) supporting 9 AIA layers (`A-WALL-EXT`, `A-WALL-INT`, `S-COLS`, `S-GRID`, `A-DOOR`, `A-GLAZ`, `A-FURN`, `A-ANNO-TEXT`, `A-ANNO-DIMS`), `POLYLINE`/`VERTEX`/`SEQEND`, `LINE`, `ARC`, `CIRCLE`, and `TEXT`, verified via an independent line-pair DXF parser.
2. **Single-rectangle Corridor vs. L-shaped Footprint:** Because `arch_manifest.json` represents each room by an axis-aligned bounding box (`min_x, min_y, max_x, max_y`), an L-shaped corridor would cause its bbox to overlap adjacent rooms. Designing the L-shape via a South-East bay step-in (`21.6 m` South wing vs. `27.0 m` North wing) preserved a genuine stepped/L-shaped building envelope on a regular 5.4 m × (5.0 / 6.0 m) structural grid while keeping every single room (including `Corridor`) strictly rectangular with `0.0 mm²` pairwise overlap.
3. **Breakroom Sink/Counter Overlap Caught During Authoring:** Initially placed the kitchenette sink overlapping the counter bounding box; split the counter (`Y = 8900..10050`) and sink (`Y = 10060..10800`) into adjacent non-overlapping rectangles before the first run so fixture-to-fixture overlap is strictly `0.0 mm²`.
4. **Visual Inspection Caught Label Overlaps (Iteration 2):** On the first rendered PNG, room labels in `Exam1`–`Exam3`, `Procedure`, and `NurseStation` touched adjacent exam/procedure tables and the nurse counter. Shifted exam/procedure tables slightly north (`+200–350 mm`), adjusted label offsets, widened spacing between the `5,000 mm` dimension and title block, and added fixture text callouts, then re-verified both PNG and manifest (`0` errors).
5. **Interior Column Pilaster Coordination:** Three interior columns on Grid 2 (`B2`, `C2`, `D2`) sit along the South-wing/Corridor partition line; offsetting the Corridor South face to `Y = 5150` guaranteed `0.0 mm²` overlap with the Corridor while doors and fixtures in `Waiting`, `Exam1`, and `Exam3` were explicitly positioned `>= 500 mm` clear of each column pilaster.
