# Benchmark 1 (EDL v3.4.0 Arm): 24V Industrial IoT Edge Gateway & Isolated Relay Controller

## Timing & Run Metrics
- **Tooling:** `./target/release/edl` (`edl 3.4.0`), `kicad-cli` (KiCad 9.0.9), `inkscape` (`--export-width=3508`)
- **`start_epoch`:** `1795733901`
- **`end_epoch`:** `1795734981`
- **`elapsed_sec`:** `1080` (18m 00s)
- **`iterations`:** `6` (initial synthesis + 5 targeted verification & visual-polish iterations)
- **Blind Run Compliance:** Clean blind engineering run — `benchmarks/` was never read or inspected.

## Architecture & Component Summary (72 Real Components, 43 Multi-Pin Nets)

| Stage | Zone (`A2 landscape`) | Key Components | Electrical Verification Highlights |
| :--- | :--- | :--- | :--- |
| **Stage 1** — 24V Input, Surge/EMI & 24V→5V Buck | `(17.78, 17.78) .. (370.84, 152.40)` (blue dashed) | `J_PWR`, `F1` (1.5A/60V), `D_REV` (SS36), `D_TVS` (SMBJ28A), Pi EMI (`C_IN1`/`C_IN2` 100µF/100nF 50V, `L_EMI` 10µH, `C_IN3`/`C_IN4`), `U_BUCK` (`LM2596S-ADJ`), `R_EN`, `D_BUCK` (SS34), `L_BUCK` (33µH), `C_5V1`/`C_5V2` (220µF/100nF 16V), `R_FB1`/`R_FB2`/`C_FF` (3.09k/1k/10nF) | `Vout = 1.23V × (1 + 3.09k/1k) = 5.03V`; continuous horizontal power spine; single shared horizontal `GND` rail; all caps >= 1.25× voltage derated |
| **Stage 2** — 5V→3.3V LDO & Power Indicators | `(381.00, 17.78) .. (548.64, 152.40)` (blue dashed) | `C_LDO_IN1`/`C_LDO_IN2` (10µF/100nF 16V), `U_LDO` (`AMS1117-3.3`), `C_LDO_OUT1`/`C_LDO_OUT2` (22µF/100nF 16V), `R_5V`/`D_5V` (1k/Yellow), `R_PWR`/`D_PWR` (330Ω/Green) | `I_LED(3V3) = 3.9 mA` (`5.1 mW` << `100 mW` 80% thermal limit); `I_LED(5V) = 3.0 mA` (`9.0 mW`); shared horizontal `GND` trunk |
| **Stage 3** — STM32 MCU Core, HSE, Reset, SWD & UART | `(17.78, 162.56) .. (246.38, 386.08)` (blue dashed) | `U_MCU` (`STM32G030C8T6`, LQFP-48), 4-cap horizontal decoupling bank (`C_MCU1` 10µF + `C_MCU2..4` 100nF), `Y1` (8MHz) + `C_XTAL1`/`C_XTAL2` (18pF), `R_RST`/`C_RST`/`SW_RST`, `J_SWD` (5-pin), `J_UART` (3-pin), `R_STAT`/`D_STAT` | Real KiCad 9 root symbol `STM32G030C_6-8_Tx`; shared horizontal `+3V3` and `GND` decoupling rails; crisp red-outlined global labels for cross-stage buses |
| **Stage 4** — I2C Sensor/EEPROM, RS-485 & 0–10V AFE | `(256.54, 162.56) .. (548.64, 274.32)` (blue dashed) | `R_SCL`/`R_SDA` (4.7kΩ), `U_SENS` (`LM75C`, `C_SENS`, `R_SENS_ADDR`), `U_EEPROM` (`24LC256`, `C_EEP`, `R_EEP_ADDR`), `U_485` (`LTC2850IS8`, `C_485`, `R_TERM` 120Ω, `R_485_PU`/`R_485_PD`, `J_RS485`), `J_AIN`, `R_DIV1`/`R_DIV2` (23.2k/10k), `R_AAF`/`C_AAF` (1k/100nF), `D_CLAMP` (`BAT54`) | 0–10V scales to `3.01V` (`10V × 10k / 33.2k`); anti-aliasing `fc = 1/(2π·1k·100nF) = 1.59 kHz`; Schottky clamp to `+3V3`; 120Ω RS-485 termination + fail-safe bias |
| **Stage 5** — Galvanically Isolated 24V Relay Driver | `(256.54, 284.48) .. (462.28, 386.08)` (red dashed, clear of title block) | `R_OPTO` (330Ω), `U_OPTO` (`PC817`) straddling vertical red dashed `ISOLATION BARRIER` (`X = 309.88mm`), `J_ISO_PWR`, `C_ISO` (100nF/50V), `R_GATE`/`R_PD` (10k/10k), `Q_RLY` (`2N7002`), `D_FLY` (`1N4148W`), `K1` (`G5LE-1-DC24` SPDT), `J_OUT` (NC/NO/COM) | `I_OPTO = 6.4 mA`; MOSFET `V_GS = 12V` (`14.4 mW` per 10k resistor); `0` shared references between `GND` (45 refs) and `ISO_GND` (4 refs); zero wires crossing isolation barrier |

## Verification Gate Results (ALL PASSED)
1. **`./target/release/edl build gateway.edl`** — Exit code `0`, `0` errors, `0` warnings; **14 / 14 rules `[ExactlyChecked]`**:
   - `E1_all_required_pins_connected`: 72/72 components connected
   - `E2_no_power_ground_short`: 0 shorts across 43 nets
   - `E3_no_multiple_power_drivers`: 0 conflicts across 43 nets
   - `E4_voltage_compatibility`: all 4 rated `power_in` pins within limits
   - `E5_no_single_pin_nets`: all 43 nets connect >= 2 pins
   - `E6_galvanic_isolation_GND_ISO_GND`: 0 conductive paths between `GND` and `ISO_GND`
   - `E7_ic_decoupling`: all 5 active IC supply rails decoupled
   - `E8_i2c_pullups`: both `I2C_SCL` and `I2C_SDA` pulled up with 4.7kΩ
   - `E9_inductive_flyback`: relay coil `K1` clamped by `D_FLY`
   - `E10_bom_manufacturability`: all 15 rated caps >= 1.25× rail voltage
   - `E11_sheet_frame_fit_Main`: 72 components & 5 zones inside `A2 landscape` (594×420mm) clear of title block (`98% W × 99% H` utilization)
   - `E12_no_component_overlap_Main`: 0 component body overlaps
   - `E13_voltage_derating`: all 64 voltage-rated components within 80% DC voltage derating
   - `E14_power_derating`: all 30 power-rated components within 80% thermal power derating
2. **`./target/release/edl export gateway.edl --kicad gateway.kicad_sch`** — Clean export with `(paper "A2")`.
3. **`kicad-cli sch erc --exit-code-violations --severity-all`** — Exit code `0`, **`0` Errors, `0` Warnings** (`0` `lib_symbol_mismatch`, `0` `footprint_link_issues`, `0` electrical/pin warnings).
4. **`kicad-cli sch export netlist`** — Confirmed **`72` real components** (`>= 55`), **`43` multi-pin nets** (`>= 30`), and **`0` shared refs** between `GND` (45 refs) and `ISO_GND` (4 refs).
5. **Visual Inspection (`gateway.png`, 3508×2480 px)** — Verified via `read_file`: clean 2×2/2×3 A2 grid clear of title block, unbroken horizontal power spine in Stage 1, shared horizontal GND/power rails across Stages 1–4, zero overlapping labels or symbols, red dashed Stage 5 with crisp vertical red dashed `ISOLATION BARRIER` and zero barrier-crossing wires.

## Iteration Log
- **Iter 1:** Initial 70-component 5-stage synthesis; caught `E11` minor A2 overflow (0.4mm), `E13` IC `voltage_rating` (3.6V vs 80% margin), `Diode_SMD:D_SOT-23_ANK` missing footprint, and vertical `+3V3` drop crossing `R_SCL` `I2C_SCL` stub.
- **Iter 2:** Fixed `D_CLAMP` footprint (`Diode_SMD:D_SOD-323`), `U_485` abs-max rating (`7V`), staggered `R_SCL`/`R_SDA` off `C_SENS` X-axis -> achieved **0 KiCad ERC errors & 0 warnings**; visual inspection of `gateway.png` flagged Stage 2 LED label overlap, Stage 4 `U_EEPROM`/`U_485` label proximity and west GND clutter, and `J_ISO_PWR` `mirror: y` routing `ISO_GND` left across the isolation barrier.
- **Iter 3:** Added `R_SENS_ADDR` & `R_EEP_ADDR` (72 components, 43 nets) eliminating west GND clutter on `LM75C`/`24LC256`, switched `J_PWR -> F1` to direct `IN_24V` unbroken wire, separated `U_EEPROM` and `U_485` global labels, made `D_STAT` horizontal.
- **Iter 4:** Switched `J_PWR`/`J_AIN`/`J_ISO_PWR` from `mirror: y` to `orient: 180` (pins cleanly facing east), made Stage 2 `D_5V`/`D_PWR` horizontal (merging Stage 2's 4 LDO caps onto ONE horizontal GND rail!), shifted `C_EEP` clear of `R_SCL`/`R_SDA` -> **0 KiCad ERC errors & 0 warnings**.
- **Iter 5:** Routed `ISO_GND` via `label_stub` (preventing ground-trunk routing from crossing the barrier or dipping under Stage 5) and trimmed Stage 2/4 right edge (`548.64mm`) and Stage 3/5 bottom edge (`386.08mm`) -> **ALL 14/14 EDL rules `[ExactlyChecked]`** + **`0` KiCad ERC errors/warnings**.
- **Iter 6:** Nudged `C_ISO` +5.08mm right (`345.44mm`) for extra label breathing room next to `J_ISO_PWR`, re-verified all gates, and visually inspected final `gateway.png`.
