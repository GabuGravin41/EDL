# EDL v3.6.0 Arm — UT Austin PCBBench (`PCBSchemaGen_v2`) Engineering Notes

## Timing
- `start_epoch`: `1795812134`
- `end_epoch`: `1795813146`
- `elapsed_sec`: `1012` (16m 52s across all 6 full schematics, including verification, KiCad 9 ERC, SVG/PNG rendering, and visual inspection + polish)

## Per-Task Verification Summary

| Task | Name | `.edl` LOC | Components | Nets | EDL Proofs Passed | EDL Errors / Warnings | KiCad 9 ERC Errors | KiCad 9 ERC Warnings | Notes |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---|
| 46 | `T46_CAN_ISOLATED_FULL` | 291 | 17 | 12 | 14 / 14 | 0 / 0 | 1* | 3† | 5kV galvanic barrier (E6+E15 proved 0 wire crossings), split 120R CAN termination + ESD TVS |
| 49 | `T49_BMS_5S_FULL_SYSTEM` | 394 | 60 | 45 | 12 / 12 | 0 / 0 | 4* | 3† | 5S RC cell filters, 5mR Kelvin shunt filter, BQ76200 VDDCP charge pump, common-drain 80V N-FETs, 3.3V REGOUT CAN |
| 51 | `T51_ISOLATED_DAQ_4CH` | 377 | 41 | 27 | 15 / 15 | 0 / 3‡ | 1* | 7† | 4x ADS7042 w/ 100R+1nF anti-aliasing RC filters, REF3033 precision 3.3V AVDD reference, 3x galvanic isolation proofs + E15 divider |
| 52 | `T52_IOT_SENSOR_NODE` | 409 | 51 | 24 | 11 / 11 | 0 / 0 | **0** | 4† | 12V->5.0V TPS54302 (UVLO EN divider + 75k/10.2k FB + C_FF) -> TLV1117-33 3.3V LDO -> STM32F103C8T6 + NTC `TEMP_ADC` + SN65HVD230 |
| 54 | `T54_LI_ION_CHARGER_FULL` | 307 | 35 | 23 | 12 / 12 | 0 / 0 | **0** | 3† | TPD4E05U06 USB ESD + BQ25895 (1uH SW, BTST, PMID, REGN, 103AT NTC, ILIM) + INA226 10mR high-side BAT shunt Kelvin sense + shared I2C |
| 62 | `T62_MCU_SYNC_BUCK_SYSTEM` | 550 | 74 | 40 | 12 / 12 | 0 / 0 | **0** | 7† | 200V->100V SiC sync buck (2x IMZA65R015M2H + 2x UCC5390E +18V/-4V split Ron/Roff gate drive + Kelvin source + low-side 5mR shunt w/ INA240A1 at Vcm=0V + 100k/2.7k VOUT divider + STM32 + CAN) |
| **Total** | **6 Tasks** | **2,328** | **278** | **171** | **76 / 76** | **0 / 3** | **6*** | **27†** | **0 real wiring, grid, overlap, power, or connectivity errors across all 278 components** |

\* **Hidden `NC` pin ERC footnote:** All 6 KiCad 9 ERC errors across T46/T49/T51 (1 on `NXE1S0303MC` pin 14 `NC`, 1 on `BQ76920` pin 11 `NC`, 3 on `BQ76200` pins 3/13/15 `NC`, 1 on T51 `NXE1S0303MC` pin 14 `NC`) originate solely from `(hide yes)` pins typed `passive` (instead of `no_connect`) inside the third-party `test.kicad_sym` symbol library. Zero real functional, power, ground, or signal pins have ERC errors.  
† **Library reload warning footnote:** All 27 KiCad 9 ERC warnings are `lib_symbol_issues` ("Symbol 'X' not found in symbol library 'test'") because KiCad 9 fails to reload the legacy 854-symbol external `test.kicad_sym` rescue file, even though every symbol is embedded verbatim in each `.kicad_sch` `lib_symbols` table and renders cleanly via `kicad-cli sch export svg`.  
‡ **EDL SCLK warning footnote:** On T51, EDL emitted 3 benign `W_ERC_MISSING_I2C_PULLUP` warnings on SPI clock nets `SCLK`, `SCLK_ISO_RAW`, `SCLK_ISO` due to substring matching `SCL` inside `SCLK`.

## Visual Inspection Highlights
- Visually inspected rendered 2000px PNGs for `t52.png`, `t62.png`, and `t49.png` via `read_file`:
  - Auto-merged shunt capacitor banks (`C_5V_1..3`, `C_3V3_1..2`, `C_MCU_BULK + C_MCU1..4`, `C_BUS1..3`, `C_OUT1..3`) rendered cleanly on unified power/ground buses.
  - Red dashed isolation barriers (`divider`) cleanly bisected `U_DRV_HS`/`U_DRV_LS` (T62) and `U1`/`U3` (T46, T51) with 0 wire crossings verified by E15.
  - Caught and fixed minor symbol orientations during visual inspection (`D_VIN`, `D_PWR_LED`, `D_CANH`, `D_CANL` on T52; `D_VS_CLAMP` polarity flip `orient: 270` and extra spacing around `C_VEE2_HS/LS` and `J_HV_OUT` on T62).

## Candid Engineering Feedback on EDL v3.6.0
1. **What worked exceptionally well:**
   - Native `symbol_lib ".../test.kicad_sym"` loading + automatic `edl_power.kicad_sym` / `sym-lib-table` / `.kicad_pro` emission made multi-domain isolated designs (`VDD`/`GND`, `VDD_ISO`/`GND_ISO`, `VREF_3V3`, `VCC2_HS`/`SW_NODE`/`VEE2_HS`, `VCC2_LS`/`KS_LS`/`VEE2_LS`) effortless.
   - `E1`–`E16` static verification caught real mistakes immediately: missing `isolated: true` tags, one real footprint name typo (`NXE1SxxxxMC` -> `NXExSxxxxMC` with auto-suggested exact KiCad 9 footprint!), A3/A2 sheet margin fit, I2C pull-up completeness, and voltage/power derating.
   - `T52` (51 components) and `T62` (74 components) compiled cleanly with 0 EDL errors and 0 KiCad ERC errors on the first run.
2. **Concrete improvement opportunities for EDL v3.6.x:**
   - **Hidden `passive` `NC` pins in third-party `.kicad_sym` libraries:** EDL currently ignores `(hide yes)` pins when emitting KiCad `(no_connect ...)` flags, causing KiCad 9 ERC `pin_not_connected` errors on third-party symbols whose hidden `NC` pins were mistakenly authored as electrical type `passive`. Emitting `(no_connect ...)` for hidden `NC` pins declared `pin <n>: nc` would eliminate those remaining ERC errors completely.
   - **I2C net name matching (`W_ERC_MISSING_I2C_PULLUP`):** Matching `SCL` as a substring falsely flags SPI `SCLK` nets; token-boundary matching (`SCL` != `SCLK`) would prevent false-positive warnings on SPI designs.
   - **E14 worst-case resistor power check on high-Z Kelvin sense filters:** A 10R RC filter resistor (`R_INP`) between a `power_out` node (`BAT_CHG`) and a high-impedance `input` pin (`INA226.VIN+`) was checked as worst-case `V_rail^2 / R` (1.76W instead of ~1nW). Accounting for high-Z `input` termination on the far side of Kelvin/RC filter resistors would make E14 even more accurate without losing real overload detection.
   - **Stacked duplicate-name pins:** EDL deduplicates same-name stacked pins (e.g. `BQ25895` pins 15/16 `SYS`, 13/14 `BAT`, 19/20 `SW`, 17/18/25 `PGND`) cleanly when referenced by name, which produces 0 KiCad ERC errors, though referencing both pin numbers simultaneously causes a double-driver error on `power_out` pins; documenting or deduplicating numeric aliases in netlists would make this seamless.
