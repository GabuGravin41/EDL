# Benchmark 1 — EDL v3.3.0 Engineering Notes

## Metrics
- **start_epoch:** `1795729888`
- **end_epoch:** `1795730320`
- **elapsed_sec:** `432` (7 min 12 sec)
- **iterations:** `6`
- **edl_errors:** `0` (all 14 verification rules E1–E14 `[ExactlyChecked]`)
- **kicad_erc_errors:** `0`
- **kicad_erc_warnings:** `0` (`kicad-cli sch erc --exit-code-violations --severity-all` exited `0`)
- **sheet:** `A2 landscape` (`594×420 mm`, verified `(paper "A2")` in `.kicad_sch`, `91% W × 82% H` utilization, 0 overlaps, 0 title-block collisions)
- **netlist:** `60` real components, `37` multi-pin nets (0 shared refs between `GND` and `ISO_GND`)

## Architecture Summary
1. **Stage 1 (24V Input, Surge/EMI & Buck):** `J_PWR` → `F1` (1.5A polyfuse) → `D_REV` (SS34) → `D_TVS` (SMBJ28A 28V TVS) → Pi EMI filter (`C_IN1` 100µF/50V, `C_IN2` 100nF/50V, `L_EMI` 10µH, `C_IN3` 100µF/50V) → `U_BUCK` (`LM2596S-ADJ`) + `L_BUCK` (33µH) + `D_BUCK` (SS34) + `R_FB1`/`R_FB2` (3.09kΩ/1kΩ → 5.03V) + `C_5V1`/`C_5V2` (220µF/100nF, 16V).
2. **Stage 2 (5V→3.3V LDO & Power Indicator):** `U_LDO` (`AMS1117-3.3`) with `C_LDO_IN1` (10µF/16V), `C_LDO_OUT1` (22µF/16V), `C_LDO_OUT2` (100nF/16V), plus `R_PWR` (330Ω, 125mW → 3.6 mA, 3.4% thermal load) and green `D_PWR`.
3. **Stage 3 (MCU Core, Clock, Reset & Debug):** `U_MCU` (`STM32G030K8Tx`, LQFP-32) with 4 local decoupling caps (`C_MCU1` 10µF + `C_MCU2..4` 100nF), 8 MHz crystal `Y1` + `C_XTAL1`/`C_XTAL2` (18pF), RC reset (`R_RST` 10kΩ + `C_RST` 100nF + `SW_RST`), 5-pin `J_SWD` and 3-pin `J_UART`.
4. **Stage 4 (I2C, RS-485 & 0–10V Analog Front-End):**
   - Shared `I2C_SDA`/`I2C_SCL` with 4.7kΩ pull-ups (`R_SDA`, `R_SCL`), `U_SENS` (`HDC1080` + `C_SENS`), `U_EEPROM` (`24LC256` + `C_EEPROM`).
   - 3.3V RS-485 transceiver `U_485` (`LTC2850xS8`, SOIC-8, + `C_485`) with 120Ω termination `R_TERM` and 3-pin `J_RS485`.
   - 0–10V Analog Input `J_AIN` → divider `R_DIV1`/`R_DIV2` (23.2kΩ/10kΩ → 3.01V) → RC anti-aliasing filter `R_AAF`/`C_AAF` (10kΩ/6.8nF, $f_c \approx 1.4\text{ kHz}$) → `D_CLAMP` (`BAT54` to `+3V3`) → `ADC_IN0`.
5. **Stage 5 (Galvanically Isolated 24V Relay Driver):** `R_OPTO` (330Ω) → `U_OPTO` (`PC817`) → 12V gate divider `R_GATE`/`R_PD` (10kΩ/10kΩ) → `Q_RLY` (`2N7002`) → `K1` (`G5LE-1-DC24`) with antiparallel flyback `D_FLY` (`1N4148W`), `J_ISO_PWR` + `C_ISO` (100nF/50V), and `J_OUT` (COM/NO/NC).

## Iteration Log & Honest Experience with EDL v3.3.0
- **Iter 1:** First build compiled cleanly and passed all 13 electrical/derating/overlap rules (`E1–E10`, `E12–E14`) on the very first run, flagging only `E11_sheet_frame_fit_Main` (`121% W × 73% H` overflow on A2).
- **Iter 2:** Restructured zones into tighter 2D `vstack`/`hstack` subzones → `edl build` passed all 14 checks (`98% W × 76% H`), exported `.kicad_sch`, and ran `kicad-cli sch erc` which revealed 3 real issues:
  1. `J_PWR` had `orient: 180`, placing `+24V` (Pin 1) below `GND` (Pin 2) by 2.54 mm, causing EDL's upward `+24V` power stub and downward `GND` stub to collide (`pin_to_pin` error + `multiple_net_names` warning). Removing `orient: 180` resolved this completely.
  2. `Interface_UART:MAX3485` triggered KiCad's `lib_symbol_mismatch` warning because `MAX3485` uses `(extends "LTC2850xS8")` in KiCad 9's library; switching to the root symbol `Interface_UART:LTC2850xS8` (identical pinout and 3.3V RS-485 spec) eliminated the warning.
  3. Intermediate nets `V24_FUSED` and `V24_PROT` were auto-classified as power rails due to the `V24_` prefix and routed with non-existent `power:V24_*` symbols (`lib_symbol_issues` warnings); explicitly routing them as `label_stub` fixed all 7 warnings.
- **Iter 3–4:** Fixed the 3 ERC issues and slightly tightened Stage 1 & Stage 3 → **0 EDL errors, 0 KiCad ERC errors, 0 KiCad ERC warnings**. However, visual inspection of `gateway.png` caught that `98% W` utilization caused EDL's KiCad exporter to bump `(paper "A2")` to `(paper "A1")`.
- **Iter 5–6:** Routed `V24_FUSED`/`V24_PROT` as `label_stub` (removing long perimeter loops above Stage 1), stacked `NRST RC & BUTTON` 2-wide, and set `group_gap: 10mm`, `row_gap: 10mm`, `member_gap: 8mm` → utilization dropped to `91% W × 82% H`, KiCad emitted genuine `(paper "A2")`, ERC remained **0 errors / 0 warnings**, and visual inspection of `gateway.png` confirmed a crisp, readable, production-clean A2 landscape schematic.

### Strengths & Friction Points
- **What worked exceptionally well:** Zero raw `(x, y)` coordinates; fast `edl symbols search/inspect`; instant built-in E1–E14 verification catching galvanic isolation, inductive flyback, I2C pull-ups, IC decoupling, and 80% voltage/power derating before KiCad even runs; verbatim KiCad 9 `power.kicad_sym` rails + automatic `PWR_FLAG`s yielding **0 ERC errors AND 0 ERC warnings**.
- **Honest friction points encountered:**
  1. Adjacent pins on a rotated 2-pin connector (`J_PWR` with `orient: 180`) where the bottom pin gets an upward power symbol and the top pin gets a downward GND symbol caused an undetected stub short in EDL's own router that only `kicad-cli sch erc` caught.
  2. KiCad library symbols using `(extends ...)` (like `Interface_UART:MAX3485`) emit a `lib_symbol_mismatch` warning on export unless the root symbol is chosen.
  3. Prefix-based power-net auto-classification (`V24_*`) automatically attempted `power:V24_PROT` symbols until overridden in `routing { ... }`.
  4. `E11` reported A2 fit at `98–99% W` while the KiCad exporter silently promoted `(paper "A2")` to `(paper "A1")` until width utilization was brought down to `~91% W`.
  5. Minor cosmetic note: KiCad's netlist exporter printed `Warning: schematic has annotation errors` (harmless, 0 ERC violations) and `pack: skyline` sorts zones by height (so Stage 3 appeared top-left rather than Stage 1).
