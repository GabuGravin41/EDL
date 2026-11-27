# Benchmark 7 — Baseline Arm (Pure Python + KiCad 9, NO EDL)

## 1. Timing & Iteration Summary

| Metric | Value |
|---|---|
| `start_epoch` | `1795746335` |
| `end_epoch` | `1795748064` |
| `elapsed_sec` | `1729` (28m 49s) |
| `iterations` | `5` (Iter 1: initial build/load fail -> Iter 2: loaded with 233 ERC violations -> Iter 3: wire-splitter + sheet-pin fix -> 23 violations -> Iter 4: 0 errors / 0 warnings -> Iter 5: visual polish + re-verify 0/0) |
| KiCad CLI Version | `9.0.9` |
| Inkscape Version | `1.1.2` |
| Python LOC Written | ~1,300+ lines across `kicad_lib.py` (36 KB), `generate_plc.py` (107 KB), `verify_and_render.py` (6 KB) |

---

## 2. Final Verification Gate Results (ALL PASSED)

### Gate 1 — KiCad 9 ERC (`--exit-code-violations --severity-all`)
- Command: `kicad-cli sch erc --exit-code-violations --severity-all -o erc.rpt kicad/plc_controller.kicad_sch`
- Exit Code: **`0`**
- **Errors: `0` | Warnings: `0` | Total Violations: `0`** across all 5 hierarchical sheets (`/`, `/Sheet1_Power/`, `/Sheet2_MCU_Ethernet/`, `/Sheet3_Isolated_IO/`, `/Sheet4_Analog_Fieldbus/`).

### Gate 2 — Netlist & Galvanic Isolation Audit (`plc.net`)
- Command: `kicad-cli sch export netlist -o plc.net kicad/plc_controller.kicad_sch`
- **Total Real Components (excluding `#PWR`/`#FLG`): `243`** (Gate: `>= 150`)
  - Sheet 1 (`Sheet1_Power`): `55` unique components
  - Sheet 2 (`Sheet2_MCU_Ethernet`): `67` unique components
  - Sheet 3 (`Sheet3_Isolated_IO`): `63` unique components
  - Sheet 4 (`Sheet4_Analog_Fieldbus`): `58` unique components
- **Total Nets: `213` | Multi-Pin Nets (`>= 2` pins): `163`** (Gate: `>= 90`)
- **3 Galvanic Isolation Domains (`0` Shared Component Refs)**:
  - `GND`: `104` unique component refs
  - `ISO_IO_GND`: `17` unique component refs
  - `ISO_BUS_GND`: `25` unique component refs
  - `refs(GND) & refs(ISO_IO_GND)`: **`0`**
  - `refs(GND) & refs(ISO_BUS_GND)`: **`0`**
  - `refs(ISO_IO_GND) & refs(ISO_BUS_GND)`: **`0`**
- **All 18 Cross-Sheet Hierarchical Nets Verified End-to-End**:
  - Sheet 1 -> Sheet 2: `SYS_RST_N`, `PWR_GOOD`, `V24_SENSE`
  - Sheet 2 <-> Sheet 3: `DI1_MCU..DI4_MCU`, `DO1_MCU..DO4_MCU`
  - Sheet 2 <-> Sheet 4: `AIN1_ADC`, `AIN2_ADC`, `RS485_TX`, `RS485_RX`, `RS485_DE`, `CAN_TX`, `CAN_RX`

### Gate 3 — High-Resolution Visual Inspection (`3200px` PNGs via `read_file`)
All 5 sheets rendered via `kicad-cli sch export svg` -> `inkscape --export-type=png --export-width=3200` and visually inspected twice (Iter 4 -> Iter 5 polish):
- `root.png` (404 KB)
- `sheet1_power.png` (389 KB)
- `sheet2_mcu_eth.png` (648 KB)
- `sheet3_iso_io.png` (460 KB)
- `sheet4_analog_bus.png` (494 KB)

---

## 3. Brutally Honest Log of Multi-Sheet KiCad 9 + Raw Python Pain Points

Building a 243-component, 4-sheet hierarchical schematic directly in raw Python + KiCad 9 S-expressions without an HDL/EDL compiler exposed **9 severe, time-consuming failure modes** that required custom infrastructure and 5 full iterations to overcome:

1. **Silent/Opaque Schematic Parse Failures (`Failed to load schematic`, Iter 1)**:
   - `kicad-cli` gave ZERO line number or token diagnostic when loading a malformed `.kicad_sch` — just `Failed to load schematic` (exit code 3).
   - Had to isolate per-sheet loads to find the culprit: KiCad 9 forbids `(justify center)` (center alignment must omit the `justify` clause entirely), and multi-line strings need escaped `\\n`.

2. **KiCad 9 Wire-Splitting Requirement at Mid-Segment T-Junctions (Iter 2 -> 233 violations!)**:
   - The single biggest hidden trap in programmatic KiCad 9 schematic generation: emitting a single continuous bus wire `(x0, y) -> (x6, y)` with `(junction ...)` dots in the middle causes KiCad's connectivity solver to drop downstream taps!
   - Eeschema's GUI silently splits wires into segment-by-segment edges at every junction/pin/label. Without writing a dedicated geometric `_split_wires()` engine in `kicad_lib.py` that splits every horizontal/vertical wire at all interior pin, junction, label, and sheet-pin coordinates, 70+ pins appeared disconnected in both ERC and netlist export.

3. **Unintended Shorts From Manual Coordinate-Space Routing (Iter 3 -> 4 fatal rail shorts!)**:
   - Without an automated netlist-driven router or collision detector, manually computing `(x, y)` coordinates across 243 symbols caused **6 silent shorts** that only KiCad ERC caught:
     - `J5` rotated 180° flipped Pin 2 (`y=78.74`) ABOVE Pin 1 (`y=81.28`), causing the vertical drop wire to short `ISO_IO_24V` directly to `ISO_IO_GND`!
     - `SW1` (`SW_Push`, horizontal by default) rotated 90° placed Pin 2 on TOP and Pin 1 on BOTTOM, so routing `MCU_NRST` to Pin 1 ran straight through Pin 2 (`GND`)!
     - `Y2` (`Crystal`, horizontal by default) rotated 90° flipped Pin 1/Pin 2 vertically, causing `C38`'s wire to run through `C38`'s body into `GND`.
     - `PHY_VDDCR` vertical bus wire ran straight through `C43`'s vertical body at `x=373.38`, shorting `PHY_VDDCR` (`#FLG301`) to `GND` (`#FLG201`).
     - `PS1` (`TMR-0511`) pin 7 drop wire ran down `x=327.66` right over `C18` pin 1's wire, shorting `+Vout` to `-Vout`.
     - `U6` `VREF+` and `J4` LED pin 10 routed vertically by 2.54mm into adjacent `MCU_BOOT0` and `ETH_LED2_K` stubs.
   - Every single one of these is a direct consequence of manually tracking KiCad's Y-up symbol space vs Y-down schematic space and pin-side polarity under 90°/180°/270° rotations and `mirror y`.

4. **Hierarchical Sheet Pin Angle Inversion (`top=90`, `bottom=270`, Iter 2)**:
   - On the root sheet, KiCad 9 defines a hierarchical `(sheet ...)` pin with `angle=270` as snapping to the **bottom** edge and `angle=90` to the **top** edge — the exact opposite of intuition for pin orientation. Getting this backwards caused KiCad to teleport 15 sheet pins to the opposite edges of Sheets 2 and 3, breaking all 15 inter-sheet wires (`30x pin_not_connected` + `15x wire_dangling`).

5. **KiCad 9 Symbol `extends` Flattening & `lib_symbol_mismatch` Fragility (Iter 2 -> Iter 4)**:
   - KiCad 9 `.kicad_sym` libraries heavily use single- and multi-level `(extends "Base")` inheritance (e.g. `MAX485E -> MAX481E -> LTC2850xS8`, `LM75B -> LM75C`, `STM32F407VETx -> STM32F407V_E-G_Tx`).
   - KiCad schematics require flattened definitions in `(lib_symbols ...)` with renamed unit sub-symbols.
   - `Device:Crystal_GND24` used KiCad 9's `(property private "KLC_S3.3" ...)` syntax — two private properties collided when keyed naively on `sub[1]`, triggering `lib_symbol_mismatch`.
   - Even after fixing `private`, `LM75B` and `MAX485E` still triggered `lib_symbol_mismatch` due to subtle internal property-position inheritance differences between derived and base symbols; switching to base symbols (`Sensor_Temperature:LM75C`, `Interface_UART:LTC2850xS8` + local 3.3V isolated LDO `U27`) eliminated every mismatch warning.

6. **Manual Cross-Sheet Reference Designator Bookkeeping (Iter 2)**:
   - With 243 parts hand-numbered across 4 Python builder functions, `C54` and `C55` were accidentally allocated in both Sheet 3 and Sheet 4, causing `Warning: schematic has annotation errors` during netlist export until audited and renumbered (`C66`/`C67`).

7. **Strict KiCad 9 Footprint Library Existence Checks (Iter 2)**:
   - Under `--severity-all`, KiCad 9 ERC validates every `(property "Footprint" ...)` string against `/usr/share/kicad/footprints/*.pretty`. Legacy/guessed footprint names (`TerminalBlock_bornier-*`, `TMR-xxxx`, `SOIC-8_5.23x5.23mm`) generated 11 `footprint_link_issues` warnings that had to be glob-searched and replaced with exact KiCad 9 footprint names.

8. **ERC Power-Pin Semantics Across Passive Filters & Internal Regulators**:
   - Real hardware constructs like `LM2596S-ADJ` (output through passive `L2` to `+5V`), `FB1` (`+5V -> 5V_ANA`), `FB2`/`FB3` (`TMR-0511` secondary EMI filter to `ISO_BUS_5V`/`ISO_BUS_GND`), and `LAN8720A`'s `VDDCR` (internal 1.2V regulator output typed as `power_in` in KiCad's library!) all require explicit `PWR_FLAG` placement plus custom local power library (`plc_power.kicad_sym` + `sym-lib-table`) to avoid `power_pin_not_driven` errors while keeping 3 distinct ground nets (`GND`, `ISO_IO_GND`, `ISO_BUS_GND`).

9. **Visual Label Collisions Only Catchable via High-Res PNG Inspection (Iter 4 -> Iter 5)**:
   - Even after hitting 0 ERC errors / 0 warnings, rendering 3200px PNGs revealed multiple visual collisions that required an entire extra layout pass:
     - Root sheet legend box intersected the `V24_SENSE` wire and overlapped its own title.
     - 7× 33Ω RMII series resistors (`R22..R28`) on 2.54mm pitch stacked text vertically until staggered in X.
     - `C63` and `C64` decoupling caps collided with `LTC2850IS8` and `MCP2551` signal stubs until moved left of the stubs.
     - `R31..R34` Ethernet termination labels, `Q1..Q4` MOSFET labels, and `PC817` reference designators sitting on top of the red dashed isolation barrier line all required manual per-component coordinate offsets.
   - Writing ~1,300 lines of brittle absolute `(x, y)` coordinate math in Python works, but any mid-design topology edit ripples through dozens of hardcoded coordinates.
