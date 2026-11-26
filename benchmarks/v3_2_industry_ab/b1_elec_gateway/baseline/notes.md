# Baseline (NO-EDL) Engineering Notes — Benchmark 1: 24V Industrial IoT Edge Gateway & Isolated Relay Controller

## Run Metadata
- **start_epoch**: `1795710874`
- **end_epoch**: `1795711520`
- **elapsed_sec**: `646`
- **iterations**: `3` (Iter 1: initial full schematic generation + ERC/netlist; Iter 2: wire-splitting & label/rail fix -> 0 ERC errors, 1 warning; Iter 3: visual polish for rotated property text, Stage 4C/Stage 5 spacing, and LM75C standalone library symbol -> 0 ERC errors, 0 ERC warnings)
- **tools_used**: `python3` (stdlib `re`, `math`, `uuid`, `copy`, `json`), `/usr/share/kicad/symbols/*.kicad_sym`, `kicad-cli` (v9.0.9: `sch erc`, `sch export netlist`, `sch export svg`), `inkscape` (v1.1.2: 150 DPI SVG-to-PNG rasterization)
- **erc_errors**: `0`
- **erc_warnings**: `0`

## Verified Design Metrics
- **Sheet**: Single A2 (`594 × 420 mm`), all 5 stages boxed cleanly inside `[20.32, 568.96] × [20.32, 396.24]` and strictly clear of the A2 title block (`X > 460, Y > 356`).
- **Real components in exported netlist**: `60` (requirement: `>= 45`)
- **Multi-pin nets (`>= 2` pins) in exported netlist**: `39` (requirement: `>= 25`)
- **Suspicious 1-pin nets**: `0`
- **Overlapping component bodies**: `0` (verified both mathematically by bounding-box checker and visually in `gateway.png`)
- **Border / title-block violations**: `0`
- **Voltage domains**: 4 distinct domains (`+24V`, `+5V`, `+3V3`, and galvanically isolated `ISO_24V` / `ISO_GND` across `PC817` optocoupler).

## Honest Log of Difficulties & Bugs Encountered
1. **Manual KiCad 9 S-expression symbol extraction & `extends` inheritance flattening**:
   - Standard KiCad 9 library symbols like `LM2576S-ADJ`, `AMS1117-3.3`, `STM32F103C8Tx`, `SP3485EN`, and `LM75B` use `(extends "...")` inheritance in `/usr/share/kicad/symbols/*.kicad_sym`. Writing a custom S-expression parser, inheritance flattener, and sub-symbol renamer (`Parent_0_1` -> `Child_0_1`) was required just to embed valid symbols into `(lib_symbols ...)`.
   - Even with flattening, `Sensor_Temperature:LM75B` triggered a KiCad 9 `lib_symbol_mismatch` warning when compared against the installed library; switching to the pin-identical standalone `Sensor_Temperature:LM75C` (`ext=None`) resolved the final warning cleanly.
2. **KiCad 9 wire T-junction / unbroken bus connectivity bug (Iteration 1 -> 70 ERC errors, 17 warnings)**:
   - Initially drawing unbroken horizontal/vertical bus wires (e.g. `+3V3`, `+5V`, `+24V`, `GND`, `U_MCU` VDD rail) and dropping `(junction ...)` dots at interior intersections caused KiCad 9's connectivity solver to drop downstream pins/segments after the first junction!
   - Fix: Implemented a `split_wires_at_points()` pass in `generate_gateway.py` that splits every orthogonal wire segment at every pin, label, and wire endpoint lying on its interior, deduplicates segments, and places junctions at `>= 3`-way vertices. That single fix eliminated all 70 ERC errors in one step.
3. **Coordinate & wiring bugs caught on Iteration 1**:
   - Duplicate local + global label on `NRST` triggered `same_local_global_label` warning (removed redundant local label).
   - `ISO_24V` rail started at `J_ISO_PWR` pin 1 (`x = 350.52`), missing `R_OPTO_PU` slightly to its left (`x = 340.36`); extended the `ISO_24V` bus left to `340.36`.
4. **KiCad 9 rotated symbol property text orientation (caught by visual inspection of Iteration 2 PNG)**:
   - On symbols rotated `90°` or `270°` (`F1`, `L_EMI`, `D_TVS`, `D_CATCH`, `L_BUCK`, `D_PWR`, `R_DIV1`, `R_AAF`, `D_CLAMP`, `R_OPTO`, `R_GATE`, `D_FLY`), emitting `(at x y 0)` for `Reference`/`Value` properties caused KiCad's SVG renderer to rotate the text vertically (`rotate(-90)`), causing adjacent Ref/Val strings to collide.
   - Fix: Emitted property angle `90` when parent symbol rotation is in `(90, 270)` so all component text renders cleanly horizontal, and adjusted label offsets on `U_MCU`, `J_ISO_PWR`, `R_OPTO_PU`, `Q_RLY`, `K1`, and Stage 4C (`ADC_AIN`) so zero labels collide with wires, bodies, or section boxes.
5. **Custom reference designator annotation warning on netlist export**:
   - `kicad-cli sch export netlist` prints a benign CLI message `Warning: schematic has annotation errors` because the benchmark specification uses descriptive non-numeric references (`U_BUCK`, `U_LDO`, `U_MCU`, `D_REV`, `D_TVS`, `L_EMI`, `R_TERM`, `J_PWR`, etc.), which KiCad's legacy annotator flags as lacking a trailing numeric suffix while still exporting all 60 components and 39 nets cleanly and passing `kicad-cli sch erc` with 0 errors and 0 warnings.
6. **Manual coordinate placement effort**:
   - Without an automated schematic placer/router, every component's `(x, y, rot, mirror)`, wire route, label offset, and stage boundary had to be hand-calculated on the `2.54 mm` grid (~750 lines of Python), plus a custom geometric overlap/border validator and visual inspection pass.
