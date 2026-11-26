# Benchmark 5 (Hospital 11 kV / 400 V Power SLD) — EDL v3.2 Engineering Notes

## Benchmark Timing & Execution Metadata
- `start_epoch`: `1795710882`
- `end_epoch`: `1795711264`
- `elapsed_sec`: `382` (6m 22s)
- `iterations`: `2` (Iteration 1: initial build passed all 6 IEC checks on first compile, but visual PNG inspection revealed `MSB_A`/`MSB_B` header label clipping; Iteration 2: added standard 125 kvar PFC banks `PFC_A`/`PFC_B` to widen the 2-load main buses and raise power factor, resolving the label collision completely)
- `tools_used`: `./target/release/edl` (`domains`, `build`, `export --dxf`, `preview --annotated`), `inkscape` (`-d 150`), `python3` (manifest extraction & independent assertion verification)

## Verification Summary (`edl build hospital_sld.edl --markdown`)
All 6 EDL `elec.sld` verification proofs passed (`has_errors: false`):
- `S1_source_bus_and_n1_capacity` (`ExactlyChecked`): All 3 switchboards (`MSB_A`, `MSB_B`, `EMSB`) and 3 sources (`TX1`, `TX2`, `GEN1`) within capacity (`max normal Kz = 0.71`, `N-1 emergency Kz = 0.74`).
- `S2_breaker_cable_coordination` (`ExactlyChecked`): All 10 sub-main feeders + 2 PFC branches satisfy $I_b \le I_n \le I_z$ at 100% rated load (`demand = 1.0`).
- `S3_protection_selectivity` (`ExactlyChecked`): Passed (note: 0 sub-distribution feeders in flat 3-main-bus topology — see limitations below).
- `S4_short_circuit_breaking_capacity` (`ExactlyChecked`): Peak bus prospective $I_k'' = 24.44\text{ kA}$ (`MSB_A`/`MSB_B`); all main/tie/ATS/normal breakers rated `50 kA`, all essential MCCBs rated `36 kA` (safe for both utility-fed `24.4 kA` and generator-fed `10.0 kA`).
- `S5_cumulative_voltage_drop` (`ExactlyChecked`): Worst feeder voltage drop = `0.93%` (`F9_CHILLER2`, well below the `4.0%` spec limit).
- `S6_ats_coupler_interlock` (`FormallyChecked`): Formally verified across both `Q_BT` (`ats_2_of_3`) and `ATS1` (`mech_interlock`).

## Independent Engineering Contingency Check (Beyond What EDL Models)
Because EDL `elec.sld` does not transfer steady-state load across `coupler ATS1` from `EMSB` into `MSB_B`, we independently verified the true combined hospital contingencies:
1. **Connected Full Load (100%):** `MSB_A` = 400 kW (458 kVA), `MSB_B` = 320 kW (370 kVA), `EMSB` = 630 kW (712 kVA). Grand total connected = 1350 kW / 1540 kVA (740.5 kvar).
2. **Normal Operation (0.70 Hospital Demand Factor, `EMSB` fed from `MSB_B` via `ATS1`):**
   - `TX1` (`MSB_A`): 280 kW, 156.5 kvar (or 31.5 kvar w/ `PFC_A`) → 321 kVA (`Kz = 0.32`).
   - `TX2` (`MSB_B + EMSB`): 665 kW, 361.8 kvar (or 236.8 kvar w/ `PFC_B`) → 757 kVA without PFC / 706 kVA with PFC (`Kz = 0.71–0.76 ≤ 1.00`).
3. **N-1 Single-Transformer Contingency (`TX1` or `TX2` lost, `Q_BT` closed):**
   - At 0.70 demand without PFC or shedding: 945 kW / 1078 kVA (`Kz = 1.08` — slight overload).
   - **With `PFC_A` + `PFC_B` (250 kvar total):** 945 kW, 268.3 kvar → **982 kVA (`Kz = 0.98 ≤ 1.00`)** even without chiller shedding.
   - **With N-1 automatic shedding of 1 HVAC Chiller** (`F8_CHILLER1` or `F9_CHILLER2`): 791 kW / 897 kVA without PFC (`Kz = 0.90 ≤ 1.00`), or 823 kVA with PFC (`Kz = 0.82 ≤ 1.00`).
4. **Standby Diesel Generator Mode (Utility lost, `ATS1` transfers `EMSB` to `GEN1`):**
   - `GEN1` (1000 kVA / 800 kW continuous) carries 100% of `EMSB` essential loads = **630 kW / 712 kVA** (`78.8%` kW loading, `71.2%` kVA loading — comfortable margin).
5. **Exact IEC 60909 Short-Circuit Check (`Sk'' = 250 MVA` vs EDL's hardcoded `500 MVA`):**
   - Transformer impedance: $Z_T = 0.06 \times 400^2 / 10^6 = 9.60\text{ m}\Omega$.
   - Exact utility impedance at 250 MVA: $Z_Q = 400^2 / 250\times 10^6 = 0.64\text{ m}\Omega \Rightarrow Z_{tot} = 10.24\text{ m}\Omega \Rightarrow I_k'' = \frac{1.05 \times 400}{\sqrt{3} \times 10.24\text{ m}\Omega} = \mathbf{23.68\text{ kA}}$ (single TX) / **45.9 kA** (paralleled).
   - EDL computed `24.44 kA` because `compile.rs` hardcodes $Z_Q = 0.32\text{ m}\Omega$ (`~500 MVA`), which is +3.2% more conservative. All breakers (`36 kA` on `EMSB`, `50 kA` on `MSB_A`/`MSB_B`) exceed both values.

---

## Brutally Honest EDL Assessment

### Where EDL Helped
1. **Zero-boilerplate IEC 60909 / IEC 60364 verification loop:** Writing ~110 lines of declarative `.edl` immediately computed $I_b$, $I_z$ (with parallel cable 0.92 grouping derating), $\Delta U\%$, busbar $P/Q/S/\cos\phi$, prospective $I_k''$, $I_b \le I_n \le I_z$, $I_{cu} \ge I_k''$, and `S6` interlock proof in <1 second.
2. **Simultaneous multi-format CAD output from one source of truth:** One `.edl` file produced layered AutoCAD R12 DXF (`SLD_BUSBAR`, `SLD_FEEDER`, `SLD_SYMBOLS`, `SLD_INTERLOCK`, `SLD_TEXT`, `SLD_METRICS`, 5016 lines), clean annotated IEC SVG, and machine-readable JSON Canonical IR (`build --json`) with zero manual drawing coordinates or CAD scripting.
3. **Instant visual professionalism:** IEC double-circle transformers, CT rings, breaker X-marks/blades, 3-phase cable ticks, color-coded UPS/motor/lighting/PFC load symbols, interlock badges, and the bottom-right N-1 summary block came completely free.

### Where EDL Fought Us, Lacked Features, or Slowed Us Down
1. **No MV (11 kV) Bus / RMU / Utility Feeder Primitives (Major Gap):**
   - `elec.sld` cannot model `UTIL_11KV` (`Sk'' = 250 MVA`) or a 3-way `RMU_11KV` switchgear bus above the step-down transformers.
   - Worse, `compile.rs:560` **hardcodes** upstream MV grid impedance to `0.00032 Ω` (`~500 MVA`), ignoring the actual utility short-circuit level (`250 MVA`). There is no `sk_mva` parameter on `source: transformer`. We could only document `UTIL_11KV` and `RMU_11KV` in the drawing title and source comments.
2. **No Native Dual-Source ATS / Essential Bus Transfer Topology (Major Gap):**
   - A real hospital `EMSB` is fed by **two** mutually exclusive sources: normal utility feed (`Q_NORM` from `MSB_A`/`MSB_B`) OR standby generator (`Q_GEN` from `GEN1`) via `ATS1`.
   - In EDL `elec.sld`, a `sub` bus can only have ONE parent `feeder`, and `render.rs` silently **drops/ignores** any `source` attached to a `sub` bus!
   - Conversely, promoting `EMSB` to a `main` bus linked via `coupler ATS1` solves rendering and `S6` formal interlock verification, **but** `compile.rs` never propagates `EMSB`'s load or utility fault current across a `coupler` into `MSB_B`. As a result:
     - EDL's `S1` N-1 calculation (`Q_BT`) only sums `MSB_A + MSB_B` (missing `EMSB`'s 630 kW normal-mode contribution!).
     - EDL's `EMSB` short-circuit `ik3_ka` reports only `9.97 kA` (generator-fed `Xd'' = 15%`), NOT the worst-case normal utility-fed fault level (`~24.4 kA`) that actually governs `EMSB` breaker `Icu` selection!
     - Because `Q_GEN` had to be `standby: false` to get `EMSB` capacity-checked by `S1`, EDL didn't label it `ACB (STBY)`.
3. **Single `demand` Factor Conflates Branch Sizing with System Diversity / N-1 Load Shedding:**
   - In `compile.rs:448`, `ld.demand` scales **both** the individual branch current $I_b$ / voltage drop $\Delta U\%$ AND the bus/N-1 aggregate load. In real IEC 60364 engineering, each outgoing feeder's cable & breaker must be sized for 100% feeder load (`demand = 1.0`), while main switchboards and N-1 contingencies apply bus-level diversity (e.g., 0.70) and load-shedding priority flags (e.g., shed 1 Chiller on N-1). EDL lacks both `bus_diversity` and `shed_on_n1: true`.
4. **Renderer Layout Bugs on Narrow Main Buses & Fixed Canvas Height:**
   - In `render.rs:75`, a `main` bus with only 2 direct loads clamps to `width = 340 px`, while its left bus title (`MSB_A (400V TN-S | 1600A)`) + right metrics string (`S=458kVA ... Ik3''=24.4kA`) require `~415 px`. On Iteration 1, the right white pill rectangle literally **clipped and overwrote** the left bus title (`MSB_A (400V TN-!`). We had to add `PFC_A` and `PFC_B` (bringing each bus to 3 branches = 444 px) to eliminate the text collision without modifying `src/`.
   - Even after widening, `render.rs` hardcodes `total_h = 965 px` (leaving ~320 px of awkward empty whitespace when there is no second `sub` bus tier), draws all `coupler` blades visually open regardless of `normally_open: false`, and prints raw `mech_interlock` instead of a formatted ATS label.
5. **No Fire-Rated Cable Semantics or Selectivity Across Main-Bus Breakers:**
   - Fire-rated cable (`F2_FIRE_PUMP`) is purely an unchecked text string; `S3_protection_selectivity` only checks `feeder -> load` pairs (so direct loads on `main` buses got 0 selectivity checks against their `1600 A` incomers!).
6. **Undocumented Grammar Required Source Inspection:**
   - `edl schema` did not expose the `power_sld` AST keywords, so verifying exact supported keys (`xd_pct`, `mode: mech_interlock`, `parallel_runs`, `tier: main`) required reading `src/domains/sld/compile.rs` and `render.rs` in addition to `examples/building_power_sld_2x1600kva.edl`.
