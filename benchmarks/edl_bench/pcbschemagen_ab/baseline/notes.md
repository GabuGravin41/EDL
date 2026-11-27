# Baseline Arm (NO-EDL Native Python + KiCad 9) — PCBBench (`PCBSchemaGen_v2`)

## Timing
- **start_epoch**: `1795812132`
- **end_epoch**: `1795813438`
- **elapsed_sec**: `1306` (~21 min 46 sec)
- **Toolchain**: Python 3 + KiCad `9.0.9` (`kicad-cli`) + Inkscape `1.1.2`

## Per-Task Verification Summary

| Task | SubModuleName | Schematic | Components | Nets | ERC Errors | ERC Electrical/Routing Warnings | ERC `lib_symbol_issues` Warnings* | Python LOC |
|---|---|---|---|---|---|---|---|---|
| T46 | `CAN_ISOLATED_FULL` | `t46/t46_can_isolated.kicad_sch` | 26 | 21 | 0 | 0 | 3 | 259 |
| T49 | `BMS_5S_FULL_SYSTEM` | `t49/t49_bms_5s.kicad_sch` | 63 | 47 | 0 | 0 | 3 | 426 |
| T51 | `ISOLATED_DAQ_4CH` | `t51/t51_daq_4ch.kicad_sch` | 58 | 37 | 0 | 0 | 7 | 295 |
| T52 | `IOT_SENSOR_NODE` | `t52/t52_iot_node.kicad_sch` | 57 | 53 | 0 | 0 | 4 | 397 |
| T54 | `LI_ION_CHARGER_FULL` | `t54/t54_charger.kicad_sch` | 37 | 27 | 0 | 0 | 3 | 323 |
| T62 | `MCU_SYNC_BUCK_SYSTEM` | `t62/t62_mcu_sync_buck.kicad_sch` | 72 | 64 | 0 | 0 | 7 | 511 |
| **Shared** | `kicad_sch_builder.py` | — | — | — | — | — | — | **876** |
| **Total** | **6 / 6 Complete** | — | **313** | **249** | **0** | **0** | **27*** | **3,087** |

*\*Note on `lib_symbol_issues` warnings:* All 6 schematics embed the exact verbatim symbol definitions from `test.kicad_sym` inside `(lib_symbols ...)` with matching `(lib_id "test:<PartName>")` and include a project-local `sym-lib-table` pointing to `test.kicad_sym` plus a minimal `.kicad_pro`. Running `kicad-cli sym export svg` directly on the benchmark's 854-symbol `test.kicad_sym` returns `Unable to load library` (exit code 2), so KiCad 9 ERC emits one `lib_symbol_issues` warning per `test:*` IC instance when checking the external table, even though the embedded verbatim symbols load, netlist, ERC-verify, and plot to SVG/PNG cleanly. Every schematic has **0 ERC errors**, **0 electrical/routing warnings**, **0 missing required ICs**, and **0 missing InputNodes/OutputNodes** in the exported KiCad 9 netlist.

## Electrical Highlights by Task
- **T46 (`CAN_ISOLATED_FULL`)**: Pure galvanic isolation between primary (`VDD`/`GND`) and secondary (`VDD_ISO`/`GND_ISO`) domains; `NXE1S0303MC` with `4.7uH` + `10uF`/`100nF` EMI input filter and `10uF`/`100nF` + `3.3k` minimum load; `ISO7741` Ch A (`INA`→`OUTA`) TXD forward and Ch D (`OUTD`→`IND`) RXD reverse with `33R` damping and unused inputs pulled down via `10k`; `SN65HVD230` with `10k` slope-control `RS`, ISO 11898-2 split `60R`/`60R`/`4.7nF` termination via `VREF`, and `PESD1CAN` TVS protection.
- **T49 (`BMS_5S_FULL_SYSTEM`)**: `BQ76920` 5S AFE with 5S `1k`/`1uF` differential RC cell filters (`VC5..VC0`), `1uF` `CAP1`, `10k` NTC on `TS1`, `2mR` low-side shunt between `CELL0` and `PACK-` with `100R`/`100nF` Kelvin filter (`SRP`/`SRN`) and `0R` Kelvin star-ground tie (`CELL0`→`GND`); `BQ76200` with `470nF` `VDDCP`-to-`BAT` charge pump cap driving common-drain back-to-back high-side NFETs (`Q1` CHG, `Q2` DSG) with `100R` gate resistors and `10M` G-S bleed resistors; `SN65HVD230` powered by `BQ76920.REGOUT` (`+3V3`) with split termination and TVS ESD.
- **T51 (`ISOLATED_DAQ_4CH`)**: Pure galvanic isolation across `NXE1S0303MC` + `ISO7741` (SPI forward `SCLK`/`CS`/`MOSI`, reverse `MISO`); `REF3033` precision 3.3V reference powering `AVDD` of all four `ADS7042` SAR ADCs; each ADC has `100R`/`1nF` anti-aliasing kickback filter on `AIN1..AIN4`, single-ended `AINM`→`GND_ISO`, local `AVDD`/`DVDD` decoupling, and series SPI damping.
- **T52 (`IOT_SENSOR_NODE`)**: Cascaded `VIN12` (12V) → `TPS54302` (`5.01V` buck: `100k||75pF` / `13.5k` FB divider, `100k`/`22k` UVLO EN divider = `2.16V > 1.30V`, `100nF` BOOT, `10uH`, `SMBJ15A` input TVS) → `TLV1117-33` (`3.3V` LDO with both `OUTPUT_1` & `OUTPUT_2` tied) → `STM32F103C8T6` (8 MHz HSE + 32.768 kHz LSE crystals, `NRST` RC, `BOOT0`/`BOOT1` `10k` pull-downs, ferrite-filtered `VDDA`, SWD header, `PA0` NTC divider `TEMP_ADC`, `PA11`/`PA12` CAN1) + `SN65HVD230` CAN bus.
- **T54 (`LI_ION_CHARGER_FULL`)**: `TPD4E05U06` flow-through ESD clamping on USB `D+`/`D-` + `SMBJ5.0A` TVS on `VBUS`; `BQ25895` 5A buck charger with `1uH` inductor, `47nF` `BTST` cap, `2x10uF`+`100nF` `PMID`, `4.7uF` `REGN`, `130R` `ILIM` (`3.0A`), JEITA NTC (`5.23k`/`30.1k`/`10k`), `STAT` LED, and `~CE` enabled; `INA226` (addr `0x40`, shared `I2C` with `BQ25895`) monitoring `10mR` battery shunt (`BAT_CHG`→`BAT`) via `10R`/`100nF` Kelvin RC filter.
- **T62 (`MCU_SYNC_BUCK_SYSTEM`)**: 200V→100V digital synchronous buck with `STM32F103C8T6` (`TIM1_CH1`/`CH1N` complementary PWM, external `PWM` cmd input, `ADC` sensing for `VOUT`/`VIN`/`IOUT`, CAN telemetry), two `UCC5390E` isolated gate drivers (high-side powered by `600V` `US1J` bootstrap diode + `4.7R` + `1uF`/`100nF` referenced to `HS_KS`), separate turn-on (`10R`) and fast turn-off (`3.3R` + `B340A` Schottky) gate paths + `10k` G-KS resistors, two `IMZA65R015M2H` 650V 15mΩ SiC MOSFETs with true Kelvin Source (`KS`) returns, `100uH` + HV DC-link/output filter caps, `INA240A1` monitoring `5mR` low-side shunt (`CM ≈ 0V`, strictly within `-4V..+80V` spec) with `Vs/2` midpoint reference (`REF1=+3V3`, `REF2=GND`), and `SN65HVD230` CAN transceiver.

## Candid Engineering Pain Points (Native Python + KiCad 9)
1. **KiCad 9 Unsplit-Wire Connectivity Trap**: In KiCad 9, emitting one continuous wire segment with multiple interior junctions causes KiCad's ERC/netlister to disconnect everything past the first interior junction. Building an automated geometric wire-splitter and junction-degree pass in `kicad_sch_builder.py` was mandatory to eliminate phantom `pin_not_connected` errors.
2. **Coordinate-Transform Order (`rot` vs `mirror`)**: KiCad applies symbol rotation CCW in Y-up symbol coordinates *before* applying `mirror` (where `mirror y` flips X and `mirror x` flips Y), then converts Y-up to schematic Y-down. Reversing that order silently shifted `Q1` pins by 5.08 mm on T49 until caught by ERC.
3. **Heterogeneous Symbol Origins & Stacked Pins in `test.kicad_sym`**: Some symbols (`ISO7741`, `BQ25895`, `STM32F103C8T6`) are centered at `(0, 0)`, while others (`TPS54302`, `TLV1117-33`, `UCC5390E`) place pin 1 at `(0, 0)` and extend asymmetrically to `+38.1 mm`. Several symbols (`BQ25895`, `TPD4E05U06`, `INA240A1`) use stacked pins at identical coordinates, and some merged symbols use legacy KiCad 6/7 `hide` tokens. Parsing the exact symbol S-expressions programmatically was essential.
4. **Manual 2D Coordinate Bookkeeping & Pin Grazing**: Manually placing ~313 components and hundreds of orthogonal wire segments across 3,087 lines of Python is tedious and brittle — a single un-jogged orthogonal turn grazed `ISO7741.OUTE` on T46, and a 2.54 mm downward GND stub collided with adjacent `STM32.PA13` (`SWDIO`) on T52, requiring iterative ERC + visual inspection cycles.
5. **External `test.kicad_sym` Load Failure in KiCad 9**: Because KiCad 9 fails to parse the merged 854-symbol external `test.kicad_sym` directly, every `test:*` IC requires verbatim S-expression embedding into `(lib_symbols ...)` and triggers unavoidable `lib_symbol_issues` warnings when `sym-lib-table` points to `test.kicad_sym`.
6. **Visual Inspection Verification**: Visual PNG inspection of `t46.png`, `t52.png`, and `t62.png` caught overlapping passive text labels and narrow section-header collisions early, leading to orientation-aware Reference/Value placement in `kicad_sch_builder.py` across all 6 schematics.
