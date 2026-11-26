# Benchmark 3 (Baseline / NO-EDL) — Medical Clinic Plumbing Engineering Notes

## Timing & Execution Metrics
- **start_epoch:** `1795711452`
- **end_epoch:** `1795712005`
- **elapsed_sec:** `553` (~9m 13s)
- **iterations:** `2` (Iteration 1: full parametric generation + verification; Iteration 2: visual PNG label de-confliction & exact valve DN/room metadata in CSV takeoff)
- **tools_used:** `python3` (`shapely`, `networkx`, `matplotlib`, `json`, `csv`, `math`), custom AC1009 (R12) DXF exporter (note: `ezdxf` was not installed in the Python environment, so DXF merging and writing were implemented natively in Python)

---

## System Architecture & IPC Compliance Summary
All **13 wet fixtures & mechanical equipment items** across the Baseline B2 15-room + StaffWC clinic floor plan are served at **100% completeness**:

| System | Main Header / Trunk Sizing | Branch Sizing | Routing Elevation / Corridor Lane | Connected Nodes | Total Length |
|---|---|---|---|---|---|
| **CW** | DN32 (East clinical) → DN25 (West core) | DN15 (basins/sinks), DN20 (WCs/scrub/mop), DN25 (WH1) | Ceiling plenum, Corridor `y = 5850 mm` | 13 fixtures + `MAIN_CW` (1 component) | 72.39 m |
| **HW** | DN25 full corridor header from `WH1` | DN15 (basins/sinks), DN20 (scrub/mop), DN25 (WH1 out) | Ceiling plenum, Corridor `y = 5500 mm` | 11 HW fixtures (1 component) | 60.99 m |
| **SAN** | DN100 @ 2.0% (3→13 DFU) → DN150 @ 1.5% (17→23 DFU) | DN100 @ 2.0% (WCs), DN75 @ 2.0% (mop), DN50 @ 2.0% (basins/sinks) | Under-slab, Corridor `y = 7050 mm` (East → West to `SAN_OUT`) | 12 SAN fixtures + `SAN_OUT` (1 component) | 62.01 m |
| **VENT** | DN50 (East wing) → DN75 (Central/WC zone) + `VTR-1` (DN100) & `VTR-2` (DN75) | DN50 (WCs/mop), DN40 (basins/sinks) | Ceiling plenum, Corridor `y = 6650 mm` | 12 VENT fixtures + 2 VTRs (1 component) | 64.39 m |

- **Total Fixture Load:** **23 DFU** (2× WC @ 4 DFU, 1× Mop Sink @ 3 DFU, 3× Sinks @ 2 DFU, 6× Basins @ 1 DFU, 1× Water Heater @ 0 DFU).
- **Isolation Valves:** **29 total** (`V-MAIN-CW`, `V-WH1-CW`, `V-WH1-HW`, 4 zone valves `V-Z1/Z2-CW/HW`, and 22 fixture shutoff valves — every valve located on an exact node of its system's pipe graph).
- **Sanitary Cleanouts:** **7 total** (`CO-MCH`, `CO-PROC`, `CO-LAB`, `CO-EX1`, `CO-BRK`, `CO-WC`, `CO-OUT`) at upper terminal, stack bases, major junctions, and building exit (spacing <= 10 m, well within IPC 708.1 30 m max).

---

## Honest Log of Routing, Clash & Sizing Issues Encountered

1. **`ElecITRoom` Adjacency to `MechRoom` (Critical Cross-Discipline Constraint):**
   - In the Baseline B2 floor plan, `MechRoom` (`x = 24150..26850, y = 7500..10850`) sits immediately east of `ElecITRoom` (`x = 21800..24000, y = 7500..10850`).
   - Routing CW, HW, SAN, or VENT horizontally westward out of `MechRoom` would directly violate the zero-plumbing prohibition in `ElecITRoom`.
   - **Resolution:** All MechRoom mains (`CW` drop at `x = 26450`, `HW` drop at `x = 25750`, `SAN` drop at `x = 24260`, `VENT` drop at `x = 24360`) exit **south into the Central Corridor** (`y = 5150..7350`) before running west. Verified minimum clearance to `ElecITRoom` = **260.0 mm** (0 intersections).

2. **Structural Column `C-C2` Pinch Point at `Exam1` (`LAV_EX1`):**
   - `F-EX1-SINK` (`x = 10650..11200, y = 3850..4550`) sits directly south of RC column `C-C2` (`x = 10650..10950, y = 4850..5150`). Routing straight north from the architectural basin centerline (`x = 10925`) would have collided directly with `C-C2`.
   - **Resolution:** Shifted the `LAV_EX1` plumbing stub-out to `(11100, 4200)` (still inside the basin bbox) and packed the 4-pipe vertical chase along `x = 11040..11205` (strictly east of `C-C2` `max_x = 10950` and west of the east partition wall `x = 11250`). Verified minimum clear edge-to-edge distance (accounting for pipe radius) across all 17 columns = **82.5 mm** (`P-CW-11` vs `C-C2`, 0 clashes).

3. **Doorway Swing & West Corridor Window (`W12`) Coordination:**
   - All vertical room chases were aligned inside wall zones outside door swing bounding boxes (`0` door-swing crossings verified by `shapely`).
   - The main sanitary trunk was placed at `y = 7050` so that even in 2D plan projection it runs north of the west corridor window `W12` (`y = 5550..6950`), while remaining 300 mm clear of the north corridor wall (`y = 7350`).

4. **Iteration 2 Polish (Visual + CSV Takeoff):**
   - Visual inspection of Iteration 1 `clinic_plumbing.png` revealed label overlaps between corridor pipe callouts and zone valves `V-Z2-CW`/`V-Z2-HW`, plus tight label spacing around `V-WH1-HW` and `MOP_MCH` in `MechRoom`, and `VTR-1`/`VTR-2` touching the north wall line.
   - Also noticed `plumbing_schedule.csv` defaulted fixture valve nominal sizes to DN20 instead of exact DN15/DN20 per fixture.
   - **Resolution:** Repositioned corridor labels, MechRoom valve badges, and VTR pills; stored exact `_dn_mm` and `_room` per valve so the CSV schedule and PNG are both completely clean.

5. **Native DXF Generation without `ezdxf`:**
   - Because `ezdxf` was unavailable, `generate_plumbing.py` parses `clinic_arch.dxf`, dims the 9 architectural layers to gray (ACI 8/9), appends 8 new plumbing layers (`P-CW`, `P-HW`, `P-SAN`, `P-VENT`, `P-VALVES`, `P-CLEANOUTS`, `P-EQUIP`, `P-LABELS`), and emits valid AC1009 (R12) DXF geometry verified structurally (`16,598` lines, even code/value pairs, valid `0/EOF`).

---

## Deliverables Checklist
- `clinic_plumbing.dxf` — Valid multi-layer AC1009 DXF (9 arch layers + 8 plumbing layers)
- `clinic_plumbing.png` — High-resolution rendered plumbing overlay on clinic plan
- `plumbing_schedule.csv` — Complete pipe, fixture, equipment, valve, cleanout & VTR takeoff
- `plumbing_manifest.json` — Exact schema-compliant JSON verified against DXF coordinates & graph connectivity
- `generate_plumbing.py` — Parametric plumbing generator, verifier, DXF writer & PNG renderer
- `notes.md` — This engineering log
