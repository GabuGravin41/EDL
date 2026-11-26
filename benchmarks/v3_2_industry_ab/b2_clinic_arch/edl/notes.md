# Benchmark 2 (EDL v3.2 Arm): 15-Room Outpatient Specialist Medical Clinic — Engineering Notes

## Timing & Telemetry
- `start_epoch`: `1795710882`
- `end_epoch`: `1795711286`
- `elapsed_sec`: `404` (6m 44s)
- `iterations`: `3` (Build 1: 3 fixture clearance violations caught by V5 -> Build 2: all 8 EDL checks green -> Build 3: visual polish pass relocating Breakroom kitchenette off East window, moving Waiting reception counter onto interior shared wall, improving PublicWC/MechRoom/StaffWC spacing)
- `tools_used`: `./target/release/edl` (`domains`, `build`, `export --dxf`, `preview --annotated`), `inkscape` (SVG -> 150 DPI PNG), `python3` (manifest extraction from EDL JSON IR + independent geometry/graph/DXF verification)

## Delivered Design Summary
- **Footprint:** Stepped L-shaped building, `23.70 m x 15.40 m` exterior envelope (`264.11 m²` total clear interior area — squarely inside the `240–290 m²` target).
- **Rooms:** 16 rooms total (all 15 mandatory rooms + optional bonus `StaffWC`, 0 remaining DOF across 64 linear layout equations):
  - `Vestibule` (`9.16 m²`), `Waiting` (`36.82 m²`), `PublicWC` (`5.24 m²`), `Corridor` (`46.00 m²`, `2200 mm` clear width), `Procedure` (`19.60 m²`, `1200 mm` wide door), `Exam1`–`Exam4` (`18.75–19.94 m²`), `NurseStation` (`11.35 m²`), `Lab` (`11.01 m²`), `Pharmacy` (`9.15 m²`), `Breakroom` (`13.76 m²`), `ElecITRoom` (`8.24 m²`, dry zone — 0 wet fixtures), `MechRoom` (`12.39 m²`), `StaffWC` (`4.51 m²`).
- **Structural Grid:** 6×4 concrete column grid (`X` bays `4.8 / 4.8 / 4.6 / 4.6 / 4.6 m`, `Y` bays `4.6 / 6.0 / 4.5 m`, all strictly within `4.5–6.0 m`), 21 columns (`300 x 300 mm`), 0 clashes with door swings, opening cutouts, fixtures, or corridor passage.
- **Wall Construction:** `300 mm` exterior walls vs `140 mm` interior partitions.
- **Verification:** All 8 EDL rules (`T2`, `V1`–`V7`) `[ExactlyChecked]`, PLUS independent Python verification confirming 0 room overlaps, 100% BFS reachability from `Exterior` via `Vestibule`, 0 swing/fixture/column overlaps, daylighting on all 7 required rooms, dry-zone compliance on `ElecITRoom`, and 16/16 exact door-arc and room-corner coordinate parity between `arch_manifest.json` and `clinic_arch.dxf`.

## Brutally Honest Assessment of EDL v3.2

### Where EDL Helped
1. **Symbolic Grid-Snapped RREF Solver (`T2`):** Expressing all 16 rooms relative to `grid_x_1..6`, `grid_y_A..D`, `ext_half`, `int_half`, and `interior_wall` eliminated manual coordinate bookkeeping and arithmetic drift completely. The L-shaped footprint compiled watertight with 0 DOF on the very first run.
2. **Immediate Pinpoint Formal Verification (`V1`–`V7`):** Build #1 caught 3 real fixture-clearance errors (`WaitingSofa1` clearance protruding outside room wall; `ScrubSink` and `ProcCounter` encroaching on `ProcedureTable` clearance) in <1 second while proving zero room overlaps, >=100 mm jamb margins across all 26 openings, 0 door-swing collisions across 16 doors, 100% graph reachability, habitable daylighting, and 0 column clashes across 21 columns.
3. **Zero-Boilerplate Layered CAD & Preview Pipeline:** One command each produced a valid 11-layer AutoCAD R12 DXF (`WALLS_EXT`, `WALLS_INT`, `DOORS`, `WINDOWS`, `COLUMNS`, `FIXTURES`, `ROOMS`, `GRID_AXES`, `GRID_BUBBLES`, `ANNOTATIONS`, `EDL_ANCHORS`) and a high-resolution annotated SVG/PNG (`2000 x 1561 px`) with automatic dimension chains, structural grid bubbles, door swing arcs, and elevation tags.
4. **Machine-Readable Canonical IR (`build --json`):** Extracting `arch_manifest.json` directly from the compiler's JSON IR guaranteed exact numeric agreement with the exported DXF without maintaining two parallel geometry generators.

### Where EDL Fought Us, Lacked Features, or Slowed Us Down
1. **No Clinical/Medical Fixture Library:** EDL's architectural fixture vocabulary is residential/generic (`table`, `desk`, `basin`, `toilet`, `counter`, `wardrobe`, `sofa`, `shaft`) with no dedicated clinical primitives (`exam_table`, `procedure_table`, `scrub_sink`, `sterilizer`, `shelving`, `server_rack`, `water_heater`). Clinical equipment had to be mapped onto generic categories with descriptive IDs.
2. **No Program-Level Assertions in `verify {}`:** EDL checks geometric/topological invariants (`V1`–`V7`), but lacks expressive program checks such as `require Waiting.area >= 28m2`, `require Corridor.min_dim >= 1800mm`, `require DoorProcedure.width >= 1100mm`, or `require ElecITRoom.has_no_wet_fixtures`. Those mandatory spec constraints still required an external Python verifier.
3. **No Fixture-vs-Window Wall Conflict Check:** In iteration 1, `KitchenCounter` sat directly in front of `WinBreakroomE` on the East exterior wall and passed `V5` with zero warnings. Only visual inspection of `clinic_arch.png` caught the architectural clash.
4. **Annotated SVG Label Collisions in Compact Rooms:** `--annotated` places fixture, door, window, and room labels at fixed anchors without text-box collision avoidance, producing minor overlapping text in compact rooms (`StaffWC`, `PublicWC`, `MechRoom`) and where `WinCorridorW` meets the outer left dimension chain.
5. **Rectangular Room Constraint at L-Shaped Re-Entrant Corners & IR Post-Processing:** Because `Waiting` is a single rectangle spanning both the projecting South-West exterior bay and the interior bay adjacent to `NurseStation`, snapping `Waiting.east == grid_x_3 - 150mm` and `NurseStation.west == grid_x_3 + 70mm` yields a 220 mm shared wall segment along Grid 3 instead of a pure 140 mm partition. Additionally, `build --json` does not emit pre-computed `swing_bbox` or structured `(room_a, room_b)` fields on `arch.door`, requiring string parsing of `host_wall` (`W_Shared_<A>_<B>`) and arc-sector bbox calculation in Python.
6. **Minor CLI Pipe Panic:** Piping `./target/release/edl build ... --json | head` triggers a Rust `Broken pipe` panic instead of graceful `SIGPIPE` termination.
