# Benchmark 4 — EDL v3.2 Building Electrical Arm Notes

## Telemetry
- **start_epoch:** `1795711455`
- **end_epoch:** `1795711966`
- **elapsed_sec:** `511` (~8.5 minutes)
- **iterations:** `2` (Iteration 1 compiled on first shot with 0 diagnostics and 14/14 checks `EXACTLY_CHECKED`; Iteration 2 added a general duplex socket in `Procedure`, shortened circuit IDs to avoid SVG panel-table truncation, and adjusted troffer/emergency-light positions for clean visual readability)
- **tools_used:** `./target/release/edl` (`build --json`, `schedule --csv`, `export --dxf`, `preview --layers arch,elec --annotated`, `domains`), `inkscape`, `python3`

## Engineering Summary
- **Architectural Overlay:** Built directly on the unmodified B2 `clinic_arch.edl` 16-room (15 required + bonus `StaffWC`) L-shaped Medical Clinic floor plan (264.11 m² clear area, 21 structural columns, 26 openings, 33 fixtures).
- **Distribution Boards (2):**
  - `DB_MAIN` (Main 400V/230V 3P DB, 125A main breaker) on `ElecITRoom` west wall (`x=2390, y=6650`, facing east); 900 mm unobstructed working clearance `[2640, 6350, 3540, 6950]` verified clear of `ITServerRack`, `DoorElecIT`, columns, and wet zones.
  - `DB_CLINIC` (Clinical Sub-DB, 100A 3P local isolator, fed by 3-phase sub-main `SUB_CLINIC` 63A 3P / 16.0 mm² Cu) on `NurseStation` west wall (`x=9670, y=6700`, facing east, immediately adjacent to `Corridor`); 900 mm unobstructed working clearance `[9920, 6400, 10820, 7000]` verified clear of `NurseCounter`, `DoorNurse`, columns, and wet zones.
- **Device & Circuit Totals:** 94 devices (44 luminaires, 6 maintained emergency/exit luminaires, 17 wall switches including 2-way corridor switching, 27 general/clinical duplex sockets, 7 dedicated medical/heavy loads) across 15 circuits (14 final circuits + `SUB_CLINIC` 3P sub-main) and 163 routed orthogonal wire segments.
- **Verification Results:**
  - **EDL Built-in Compiler Proofs:** 14/14 `EXACTLY_CHECKED` (`T2`, `V1`–`V7`, `M1`–`M6`), 0 diagnostics.
  - **100% Room Coverage:** 16/16 rooms lit; emergency/exit lighting in `Corridor`, `Vestibule`, `Waiting`, and `Procedure`; general/medical power across all clinical/admin rooms + all 5 mandatory dedicated circuits (`D1_IT_UPS`, `D2_WtrHeater`, `D3_Proc_Med`, `D4_Autoclave`, `D5_Kitchen`).
  - **IEC 60364 Coordination ($I_b \le 0.8 I_n \le I_n \le I_z$):** Verified on all 15 circuits (including conservative Cu-in-conduit ampacities $I_z$).
  - **Voltage Drop:** Lighting worst `0.60%` (`0.79%` cumulative with sub-main) $\le 3.0\%$; Power/Dedicated worst `1.83%` (`2.02%` cumulative) $\le 5.0\%$; Sub-main `SUB_CLINIC` `0.19%` $\le 5.0\%$.
  - **3-Phase Load Balance:** `L1 = 9,810 W`, `L2 = 10,046 W`, `L3 = 10,448 W` (Total `30,304 W`), imbalance **`6.32%`** ($\le 15\%$).

---

## Brutally Honest Log: Where EDL Helped vs. Where EDL Fought Me / Lacked Features

### Where EDL Genuinely Helped
1. **Zero-Friction Semantic Anchoring to B2 Architecture (`wall(...)` and `beside(Door, latch_side, 180mm)`):**
   - Being able to anchor all 17 wall switches declaratively with `beside(DoorExam1, latch_side, 180mm)` and wall outlets with `wall(Room, side) offset: ...` eliminated manual global-coordinate math and guaranteed zero switches behind door swings on the first compile.
2. **Instant Shared-IR Multi-Discipline Compilation (`arch` + `mep.elec`):**
   - Extending `clinic_arch.edl` directly in one `.edl` file kept all 16 rooms, 26 openings, 33 fixtures, and 21 columns synchronized automatically. Exporting layered DXF (`ELEC_LIGHTING`, `ELEC_PANEL`, `ELEC_POWER`, `ELEC_SWITCHES`, `ELEC_WIRING` overlaid on `WALLS_EXT`, `WALLS_INT`, `DOORS`, `WINDOWS`, `COLUMNS`, `FIXTURES`), annotated SVG/PNG with embedded panel tables, and `panel_schedule.csv` took ~2 seconds total.
3. **Automatic 3D Run-Length Routing & Conservative Voltage-Drop Proof (`M2`):**
   - EDL automatically routed Manhattan daisy-chains from each DB panel across all load devices (including vertical ceiling risers and wall drops) and computed exact copper $\Delta V\%$ with zero manual length summation.
4. **Built-in Life-Safety Recognition (`M4`):**
   - `exit_sign` and `emergency_light` subtypes were automatically treated as unswitched 24/7 luminaires without tripping `E_MEP_UNSWITCHED_LIGHT`, while ensuring every regular luminaire was controlled by $\ge 1$ switch.

### Where EDL Fought Me, Lacked Features, or Required Workarounds
1. **No 3-Phase Modeling or Phase-Balance Verification in `mep.elec` (Major Gap):**
   - `building_electrical` only accepts a single scalar supply voltage (`supply: 230V 50Hz`) and `breaker` has **no `phase` property** (`L1`/`L2`/`L3`/`3P`).
   - Consequently, EDL **cannot** compute or verify 3-phase load balancing ($\frac{\max - \min}{\text{avg}} \le 15\%$), and `M1` linearly sums all circuit currents on a panel as if single-phase! On `DB_CLINIC` (`17.2 kW`, `74.8 A` single-phase sum vs. `24.8 A/phase` 3-phase), EDL would have rejected a `63A` 3P main breaker (`74.8 A > 63 A`), forcing us to specify a `100A` local isolator in `.edl` and verify 3-phase balance + 3P current in an external Python script.
2. **No Sub-Main Feeder / Panel-to-Panel Hierarchy in `mep.elec` (Major Gap):**
   - `panel` has no `fed_from:` property and `breaker` cannot target another `panel` — every panel is compiled as an independent root!
   - Thus, the mandatory 3-phase sub-main (`SUB_CLINIC`: `DB_MAIN -> DB_CLINIC`, 63A 3P, 16 mm² Cu, 15.79 m routed length, 0.19% $\Delta V$) could **not** be modeled natively inside `building_electrical`, does **not** appear in `edl schedule --csv` or the rendered DXF/SVG wiring, and had to be computed and injected programmatically into `electrical_manifest.json` from the panel coordinates.
3. **No Cable Ampacity ($I_z$) Coordination Check (`In <= Iz`):**
   - `M1_breaker_load_capacity` checks $I_b \le 0.80 I_n$, **not** $I_n \le I_z$. EDL would happily pass a `25A` breaker on a `1.5mm2` cable if $I_b$ and $\Delta V\%$ were small! Full IEC 60364 $I_b \le I_n \le I_z$ coordination had to be audited externally in Python against IEC 60364-5-52 conduit ampacity tables.
4. **No Panel Physical Footprint or Front Working Clearance (`>= 900 mm`) Verification:**
   - Unlike `arch.fixture` (which checks `size` and `clearance_front` via `V5`), `mep.elec.panel` is modeled as a dimensionless point with a facing direction and **zero** clearance check against door swings, fixtures, columns, or walls. Both `DB_MAIN` and `DB_CLINIC` 900 mm working zones had to be computed and collision-checked externally in Python.
5. **Narrow Wet-Zone Check (`M3` Missed All Clinic Basins/Sinks):**
   - `M3_wet_zone_clearance` only treats `shower | bath | bathtub | tub` fixtures as wet zones, ignoring `basin` (clinical scrub/handwash/lab/kitchen/mop sinks) and `toilet`. As a result, `M3` reported `across 0 wet fixture(s)` on a 16-room medical clinic with 10 basins/toilets! We had to run an independent Python clearance check against all 10 wet fixtures.
6. **No Automatic 100% Room Lighting Coverage Check:**
   - EDL checks that declared lights are switched, but does not verify that every architectural room has $\ge 1$ luminaire.
7. **Visual / Rendering Friction in `--annotated` SVG & Panel Schedule:**
   - The right-side SVG panel schedule tables **silently truncate circuit IDs beyond ~14 characters** (forcing us to rename circuits to $\le 12$ chars in Iteration 2).
   - `--annotated` renders dense overlapping text labels in smaller rooms (`ElecITRoom`, `MechRoom`, `PublicWC`, `StaffWC`, `Breakroom`) with no automatic label collision avoidance, and `MepWireRunIr` routes straight orthogonal L-segments across walls/rooms rather than along corridor cable trays.
   - `panel_schedule.csv` has no `Phase` column (because `mep.elec` is single-phase only) and no sub-main row.
