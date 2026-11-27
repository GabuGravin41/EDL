# Benchmark 6 — Baseline Arm (NO-EDL Python + CAD) Notes

## Timing & Effort Metrics
- **start_epoch**: `1795746327`
- **end_epoch**: `1795747393`
- **elapsed_sec**: `1066` (~17 min 46 sec)
- **iterations**: `4`
  1. **Iteration 1**: Re-architected `generate_clinic.py` (ECO-1 + ECO-2 + ECO-3 spatial prep, 28.0×11.0 m footprint, 5×5.6 m structural grid, 18 columns, 17 rooms, 17 doors, 14 windows, 39 fixtures, updated DXF/PNG title block) — passed 0 errors.
  2. **Iteration 2**: Re-synced `generate_plumbing.py` (shifted `MAIN_CW`, `WH1`, `MOP_MCH`, `VTR-2` +1.0 m East; added `Exam5Basin`; manually recomputed all South Wing Exam1–4 & Procedure wet chases, zone valves, cleanouts, DFU callouts, building envelope, and bottom title block) — passed 0 errors.
  3. **Iteration 3**: Re-synced `generate_electrical.py` (relocated `DB_CLINIC` +2.75 m East inside `NurseStation` with `1100×1000 mm` clearance; shifted `DB_MAIN` & East Wing devices +1.0 m; added `Exam5` 2×36W lighting, `SW-EX5`, and 3 duplex sockets on new balanced circuit `C-P5`; re-balanced 3-phase loads across 20 circuits) — passed 0 errors & 0 warnings.
  4. **Iteration 4**: Visual inspection of all 3 rendered PNGs caught minor annotation overlaps in `clinic_plumbing.png` (`V-Z1-CW`/`V-Z1-HW` labels near `SINK_PROC` chases, `MechRoom` label near `MAIN_CW`, corridor watermark over `Exam2` chases, `Exam5Basin` badge near chases) and `clinic_electrical.png` (`ECO-3` arrow callout near `CLR` hatch, corridor watermark over `L-COR-4/5`, `DB_MAIN` badge near `L-ELC-1` tag); manually tweaked hardcoded label offsets and re-rendered & re-verified both PNGs.

## Code Churn vs. Rev A (`difflib.unified_diff`, `n=0`)
| File | Rev A Lines | Rev B Lines | Lines Added | Lines Removed | Total Churn |
|---|---:|---:|---:|---:|---:|
| `generate_clinic.py` | 1,245 | 1,252 | +301 | -294 | **595** |
| `generate_plumbing.py` | 1,222 | 1,206 | +202 | -218 | **420** |
| `generate_electrical.py` | 1,160 | 1,151 | +273 | -282 | **555** |
| **TOTAL** | **3,627** | **3,609** | **+776** | **-794** | **1,570** |

**1,570 lines of code churn across 3 independent ~1,200-line Python scripts** were required to execute a 3-item Revision B ECO without a unified parametric cross-discipline model.

## Brutally Honest Log of Manual Coordinate & Re-Sync Pain Points

1. **Zero Single Source of Truth Across Disciplines (Triple Coordinate Duplication)**:
   - Even though `generate_plumbing.py` and `generate_electrical.py` read `arch_manifest.json` for verification, **neither script derives fixture, chase, valve, cleanout, switch, socket, light, or panel placement from room/wall/door constraints**. Every single coordinate is manually hardcoded as raw millimeter literals across all three files.
   - Moving `Lab` width by `+1.0 m` (ECO-1) required manually propagating `+1000 mm` to:
     - `generate_clinic.py`: `GRID_X`, `FOOTPRINT_OUTER_PTS`, `FOOTPRINT_INNER_PTS`, `Corridor`, `Lab`, `Pharmacy`, `ElecITRoom`, `MechRoom`, doors `D14..D16`, windows `W11/W13`, fixtures `F-LAB-*`, `F-PHM-*`, `F-ELC-*`, `F-MCH-*`, DXF `$EXTMAX`, gridline extents, dimension chains, and Matplotlib `xlim`.
     - `generate_plumbing.py`: `MAIN_CW_PT`, `VTR2_PT`, `MOP_MCH`, `WH1`, `cw_v_main`, `cw_dist_tee`, `cw_mop_tee`, `cw_wh_valve`, `cw_mop_valve`, `cw_corr_start_x`, `hw_drop_x`, `hw_mop_valve`, `V-Z1-CW/HW`, `CO-MCH`, `bldg_poly`, `shell_pts`, `grid_x`, DXF `$EXTMAX`, and every corridor SAN DFU callout string (`23 DFU` -> `24 DFU`, plus per-segment DFU numbers!).
     - `generate_electrical.py`: `DB_MAIN` (`x`, `body_bbox`, `clearance_bbox`, `tray_entry_x`), all lights/switches/sockets/dedicated loads in `Lab`, `Pharmacy`, `ElecITRoom`, `MechRoom`, `Corridor` (`cor_xs`, `SW-COR-E`, `EM-COR-4`, `SK-COR-2`), `ext_poly`, DXF `$EXTMAX`, and `xlim`.

2. **Brittle Column-vs-Door / Column-vs-Chase Manual Arithmetic**:
   - Updating `GRID_X` from `5×5400 mm` to `5×5600 mm` (`28,000 mm`) shifted interior columns `C-B*`, `C-C*`, `C-D*`, `C-E*` by `+200`, `+400`, `+600`, `+800 mm`.
   - Every single South Wing door (`D04`, `D05`, `D06`, `D12`, `D07`), exterior window (`W01..W14`), wet fixture (`F-EX1..4-SINK`), and 4-pipe vertical chase (`CW`, `HW`, `SAN`, `VENT` × 5 South rooms = 20 individual X-coordinates!) had to be hand-checked against the new `300×300 mm` column footprints so no door swing, window opening, or pipe radius (`dn_mm/2`) collided with a column.
   - In `Exam3`, column `C-D2` (`16650..16950`) landed directly where a west-side corridor door would normally sit — requiring manually flipping `D06` to the east side (`17450..18400`, `hinge_side="E"`), flipping `SW-EX3` to its new strike side (`17300`), and moving `F-EX3-SINK` + its 4 chases to the west side (`15500..16200`) while ensuring those chases didn't share identical X-coordinates with North Wing `SINK_LAB` (`15560..15750`).

3. **L-Shape Cutout Collapse Forced Full Title-Block & Legend Relocation Across All 3 Scripts**:
   - In Rev A, all 3 scripts placed their title blocks and legends inside the empty SE stepped cutout (`X=21600..27000, Y=0..5000`).
   - Enclosing the SE bay in Rev B to house `Exam4` + `Procedure` (so `Exam5` could fit cleanly at `14.57 m²` in the North Wing without violating minimum areas) broke the title-block layout in **all three** renderers simultaneously — forcing complete manual rewriting of the title-block geometry, text cursors, swatches, scale bar, and `ylim` in `generate_clinic.py`, `generate_plumbing.py`, and `generate_electrical.py`.

4. **Manual 3-Phase Load Re-Balancing & Cable Sizing Propagation**:
   - Adding `Exam5`'s 3 duplex sockets (`1,050 W`) + 2 luminaires (`72 W`) pushed `DB_CLINIC` over the `15%` 3-phase imbalance threshold if simply appended to an existing phase.
   - Had to manually split `C-P4` (`Procedure` vs `NurseStation`), introduce circuit `C-P5` (`NurseStation` + `Exam5`), permute phase assignments (`L1/L2/L3`) across `C-L1..C-D2` to achieve `2.80%` Whole Clinic / `3.74%` DB_MAIN / `8.43%` DB_CLINIC imbalance, AND manually upsize `C-L1` (`1.5 -> 2.5 mm²`) and `C-P3` (`2.5 -> 4.0 mm²`) to preserve comfortable voltage-drop margins on longer Rev B conduit runs.

5. **Fragile Manual Visual Label Placement (Required Extra Iteration)**:
   - Hardcoded Matplotlib text coordinates (`label_offsets`, `callouts`, `offsets`, room watermark positions) have zero awareness of adjacent symbols or pipes. Visual inspection immediately caught 6 overlapping annotations across the plumbing and electrical drawings that passed all geometric/code assertions, requiring a dedicated Iteration 4 cleanup pass.

## Final Verification Results (All Pass, 0 Errors)
- **Architecture**: `17 rooms` (`279.68 m²` net, `319.79 m²` gross, `28.0 × 11.0 m`), `18 RC columns` (`5×5.6 m` X, `5.0/6.0 m` Y), `17 doors`, `14 windows` (incl. `Exam5` north window `W14`), `39 fixtures`, `0 overlaps`, `0 collisions`, `100% BFS reachability`.
- **Plumbing**: `14 wet fixtures` (incl. `Exam5Basin`), `24 DFU` total (`max_san_trunk_dfu = 24`), `188 pipes`, `31 valves`, `8 cleanouts`, `ElecITRoom = 0 pipes/equip (min 260 mm clear)`, `0 pipe-vs-column clashes (min 337.5 mm clear)`, `0 door-swing crossings`.
- **Electrical**: `17/17 rooms` covered, `2 panels`, `DB_CLINIC` relocated `+2,750 mm` (`2.75 m >= 1.0 m`) with `1100×1000 mm` unobstructed clearance (`DB_MAIN` also `1100×1000 mm`), `116 devices`, `20 circuits`, `31.25 kW` total load, `3-phase imbalance = 2.80%` (DB_MAIN `3.74%`, DB_CLINIC `8.43%`, all `<= 15%`), `max dU lighting = 0.37% <= 3%`, `max dU power = 1.49% <= 5%`, `30 mA RCD` on all 19 final circuits, `20/20` IEC `Ib <= In <= Iz` PASS.
