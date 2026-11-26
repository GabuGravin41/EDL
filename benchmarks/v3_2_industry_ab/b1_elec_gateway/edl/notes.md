# Benchmark 1 (EDL v3.2 Arm) — Engineering Notes

## Metrics
- **start_epoch**: `1795710879`
- **end_epoch**: `1795711417`
- **elapsed_sec**: `538` (~8 min 58 sec)
- **iterations**: `4` (Iter 1: initial full 5-stage build; Iter 2: fixed `VSSA` ground role, power-symbol stub collisions on `J_PWR`/`U_SENS`, and A1 3-row layout; Iter 3: swapped `RESET`/`XTAL` sub-rows to resolve silent `MCU_NRST`↔`XTAL_OUT` router merge; Iter 4: moved `R_BOOT0` to a 3rd sub-row to resolve silent `MCU_NRST`↔`MCU_BOOT0` router merge → exact 39/39 netlist match)
- **tools_used**:
  - `./target/release/edl` (`domains`, `symbols search`, `symbols inspect`, `build`, `export --kicad`, `query`)
  - `kicad-cli` (`sch erc --severity-all`, `sch export netlist`, `sch export svg`)
  - `inkscape` (`-o gateway.png -d 150`, rendered 4967×3508 px)
  - `python3` (ERC JSON analysis, KiCad netlist 39-for-39 net/pin verification, symbol JSON summary)
- **edl_errors**: `0` (final build; 2 in Iter 1: `E_ERC_POWER_GROUND_SHORT` on `GND` due to `U_MCU.VSSA`, and `E_SHEET_FRAME_OVERFLOW` on A1 landscape before moving Stage 5 to row 3)
- **kicad_erc_errors**: `0` (final build; 2 `pin_to_pin` errors + 1 `multiple_net_names` warning in Iter 1; final build has 0 errors and 0 non-library warnings, with 131 library-symbol warnings from EDL's embedded `EDL:*` / custom `power:*` symbols)

## Final Design Summary
- **Components**: `61` real components across 5 functional stages and 4 voltage domains (`24V`, `5V`, `3.3V`, and galvanically isolated `ISO_24V` / `ISO_GND`)
- **Multi-pin nets**: `39` (100% 1-to-1 match between EDL IR and exported KiCad netlist)
- **Sheet fit & overlaps**: `A1 landscape` (841×594 mm), `83% W × 71% H`, `0` component overlaps, `0` elements outside border, `0` title-block collisions
- **Deliverables**:
  - `gateway.edl`
  - `gateway.kicad_sch`
  - `gateway.png`
  - `notes.md`

---

## Brutally Honest Engineering Log

### Where EDL v3.2 Genuinely Helped
1. **Zero raw global coordinates (`zone`/`subzone`/`hstack`/`vstack`/`row`)**: Laying out 61 components with multi-pin real KiCad symbols (including a 48-pin `STM32F103C8Tx`, `LM2596S-ADJ`, `SP3485EN`, `SHT30-DIS`, `24LC256`, `PC817`, `Relay_SPDT`) into 5 cleanly framed, titled engineering zones took minutes rather than fighting manual `(x, y)` geometry math.
2. **Native KiCad symbol discovery (`edl symbols search` / `inspect`)**: Querying real KiCad 9 `.kicad_sym` pin names, numbers, electrical types, and rotation-0 sides in JSON directly from the CLI saved huge guesswork and prevented wrong pin-name typos on `STM32F103C8Tx` and `Relay:Relay_SPDT`.
3. **Automatic `E11` sheet-frame fit & `E12` overlap verification**: On Iteration 1, EDL immediately told us our 2-row layout overflowed A1 width by `+95.0mm` (`D_FLY, K1, J_OUT` outside border) and confirmed `0` body overlaps across all 61 parts—fast, actionable feedback before even opening an image.
4. **Built-in galvanic isolation (`E6`), IC decoupling (`E7`), I2C pull-ups (`E8`), and derating (`E13`/`E14`)**: Declaring `isolated: true` on `U_OPTO` (`Isolator:PC817`) and `require galvanic_isolation(GND, ISO_GND)` formally verified zero conductive paths between the primary logic domain and the isolated 24V relay domain.
5. **Automatic `no_connect` flags and duplicate power pin handling**: Listing unused STM32 pins in `no_connect: [...]` and binding `U_MCU.VDD` once automatically connected all three `VDD` pins (#24, #36, #48) and flagged all 24 unused GPIOs cleanly with 0 KiCad `pin_not_connected` errors.

### Where EDL v3.2 Fought Me, Had Bugs, or Slowed Me Down
1. **[CRITICAL BUG] Silent Manhattan wire-to-stub shorting in `zone_local` routing (missed by BOTH EDL and KiCad ERC!)**:
   - In Iteration 2, EDL's KiCad router routed `XTAL_OUT` (`C_XTAL2.1 → U_MCU.PD1`) horizontally straight through the `MCU_NRST` label stub at `U_MCU.NRST` (`y=185.42mm`), **silently shorting `XTAL_OUT` to `MCU_NRST`** into an 8-pin net!
   - In Iteration 3, after swapping the `RESET` and `XTAL` rows, the exact same thing happened to `MCU_BOOT0` (`R_BOOT0.1 → U_MCU.BOOT0`), silently shorting `MCU_BOOT0` into `MCU_NRST`!
   - **Why this is dangerous**: Neither EDL (`0` errors) nor `kicad-cli sch erc` (`0` errors, because `NRST`, `PD1`, `BOOT0` are passive/input/bidirectional pins!) flagged either short. It was ONLY caught because we programmatically audited all 39 EDL nets against the exported KiCad netlist. EDL's schematic router **must** perform wire-to-pin / wire-to-stub clearance checks (or a post-export KiCad-netlist equivalence assertion).
2. **[CRITICAL BUG] Adjacent power-symbol stub collisions on rotated/multi-rail symbols**:
   - Because `routing { power_nets: symbol }` treats any net starting with `V` as a vertical power symbol, `V24_RAW` (upward stub from `J_PWR.Pin_1` when `orient: 180` put `Pin_1` below `Pin_2`) collided head-on with the downward `GND` stub from `J_PWR.Pin_2`, shorting 24V input straight to GND in the exported schematic!
   - The exact same collision happened on `U_SENS` (`SHT30-DIS`), where west-side `ADDR` (`GND`, downward stub) sat 2.54mm above `RESET` (`V3V3`, upward stub), shorting the entire `V3V3` rail to `GND`!
   - Workarounds required renaming intermediate 24V protection nets (`IN24_RAW`/`IN24_FUSED`/`IN24_PROT`) so they wouldn't trigger `V*` power-symbol classification, and adding `R_SENS_RST` (10k pull-up) on `U_SENS.RESET` (good EE practice anyway, but forced by a router/stub-placement bug).
3. **[BUG] `VSSA` misclassified as `power_in` instead of `ground`**:
   - EDL's KiCad symbol importer classified `STM32F103C8Tx.VSS` as `ground`, but classified `VSSA` (Analog Ground) as `power_in`. Connecting `U_MCU.VSSA` to `GND` triggered a false-positive `E2_no_power_ground_short` error until we manually added `pin VSSA: ground` on `U_MCU`.
4. **[GAP] Verification rules `E4`, `E9`, and `E10` silently checked `0` items on real KiCad library symbols**:
   - Even with `LM2596S-ADJ`, `AMS1117-3.3`, `2N7002` + `Relay_SPDT` + `1N4007` flyback diode, and 19 voltage-rated caps / 15 power-rated resistors:
     - `E4_voltage_compatibility` reported `All 0 rated power_in pin connection(s)` (native KiCad `.kicad_sym` pins don't carry voltage range metadata unless manually overridden).
     - `E9_inductive_flyback` reported `All 0 transistor-switched inductive load(s)` (didn't recognize `Relay:Relay_SPDT` as an inductive load or `Transistor_FET:2N7002` as a low-side switch!).
     - `E10_bom_manufacturability` reported `0 rated cap(s) >= 1.25x rail V, 0 resistor(s)` (whereas `E13`/`E14` DID check the 19 voltage-rated and 15 power-rated parts — confusing duplication/divergence between E10 vs E13/E14).
5. **[FRICTION] KiCad library warnings & annotation warnings on exported `.kicad_sch`**:
   - EDL embeds every component under a synthetic `EDL:<Ref>` library ID and non-standard power rails under `power:<NetName>`, producing `131` KiCad ERC warnings (`90` `lib_symbol_issues` + `41` `lib_symbol_mismatch`). Even though ERC **errors** are `0`, real KiCad engineers expect clean symbol library references (`Device:R`, `power:+3V3`, etc.).
   - Also `kicad-cli sch export netlist` prints `Warning: schematic has annotation errors` when designators follow human-readable spec names without trailing digits (e.g. `U_BUCK`, `U_MCU`, `D_FLY`).
6. **[FRICTION] No built-in `edl validate` subcommand & opaque power-net naming heuristic**:
   - The heuristic that treats any `V*` net as a KiCad `power:` symbol isn't explicit in the DSL—an explicit `power: true` property on nets (rather than name-prefix guessing) would prevent `V24_RAW`/`V24_FUSED` from unexpectedly turning into power symbols.
