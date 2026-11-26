# Benchmark 1: 24V Industrial IoT Edge Gateway & Isolated Relay Controller (KiCad 9 Schematic)

## Objective
Design an industry-scale, production-ready **KiCad 9 electronic schematic** (`.kicad_sch`) for a **24V Industrial IoT Edge Gateway & Isolated Relay Controller** (~45–60 components across 5 functional stages and 4 voltage domains).

## Mandatory Functional Requirements
1. **Stage 1 — 24V Industrial Power Input, Surge Protection & 24V→5V Buck Stage:**
   - 24V DC input screw terminal (`J_PWR`, 2-pin: `24V_IN`, `GND`).
   - Input protection: resettable fuse (`F1`), reverse-polarity Schottky diode (`D_REV`), 28V TVS surge diode (`D_TVS`), Pi EMI filter (`L_EMI` inductor + input bulk/decoupling capacitors rated **>= 35V**).
   - Step-down DC-DC buck regulator (`U_BUCK`, 24V → 5V, 1A+) with switching inductor (`L_BUCK`), catch/freewheeling diode (or synchronous buck), feedback voltage divider resistors (`R_FB1`, `R_FB2`), and output bulk/ceramic capacitors rated **>= 10V**.
2. **Stage 2 — 5V→3.3V Low-Noise LDO & Power Indicator:**
   - 5V → 3.3V LDO linear regulator (`U_LDO`) with input & output decoupling capacitors rated **>= 10V**.
   - 3.3V power-on indicator LED (`D_PWR`, Green) with current-limiting resistor (`R_PWR`) sized for **2 mA – 10 mA** and power rating derated **<= 80%** (e.g. >= 125 mW).
3. **Stage 3 — MCU Core, Clock, Reset & Debug:**
   - 3.3V microcontroller (`U_MCU`, e.g., STM32F103 / STM32F4 / RP2040 / ESP32-class real KiCad library symbol or complete multi-pin symbol) with **>= 4 local decoupling capacitors** (`100nF` + `10uF`).
   - External crystal oscillator (`Y1`, e.g., 8 MHz or 16 MHz) with 2 load capacitors (`C_XTAL1`, `C_XTAL2`).
   - External RC reset network (`R_RST` 10k pull-up + `C_RST` 100nF + tactile reset button `SW_RST`).
   - SWD debug header (`J_SWD`: 3V3, SWDIO, SWCLK, NRST, GND) and UART console header (`J_UART`: TX, RX, GND).
4. **Stage 4 — Dual Industrial Peripherals & 0–10V Analog Front-End:**
   - **Shared I2C Bus (`I2C_SDA`, `I2C_SCL`)**: 2 I2C peripheral ICs (e.g., Temp/Humidity sensor + EEPROM or ADC), each with local decoupling capacitor, plus **mandatory 4.7kΩ pull-up resistors** on both `I2C_SDA` and `I2C_SCL` to `3.3V`.
   - **RS-485 Industrial Transceiver (`U_485`)**: connected to MCU UART TX/RX/DE, with **120Ω bus termination resistor** (`R_TERM`) across `RS485_A` and `RS485_B`, local decoupling cap, and 3-pin screw terminal (`J_RS485`: A, B, GND).
   - **0–10V Industrial Analog Input (`J_AIN`)**: scaled via precision voltage divider (`0–10V → 0–3.0V`, e.g., `23.2kΩ / 10kΩ` or `24kΩ / 10kΩ`) + RC anti-aliasing filter (`R_AAF` + `C_AAF`, $f_c \approx 1\text{–}3\text{kHz}$) + protection clamping diode (`D_CLAMP`) feeding MCU ADC input.
5. **Stage 5 — Galvanically Isolated 24V Relay / Solenoid Driver Output:**
   - Optocoupler (`U_OPTO`) driven by MCU GPIO through a current-limiting LED resistor (`R_OPTO`), providing **galvanic isolation** between the 3.3V logic domain and the 24V relay coil driver side (`ISO_24V` / `ISO_GND`).
   - Low-side N-MOSFET or NPN transistor (`Q_RLY`) with gate/base resistor and pull-down resistor driving a **24V relay coil (`K1`)**.
   - **Mandatory flyback/freewheeling diode (`D_FLY`)** wired antiparallel across the relay coil (`K1`) to clamp inductive kickback.
   - Isolated output terminal block (`J_OUT`) connected to relay contacts (COM, NO, NC) and isolated 24V power input (`J_ISO_PWR`).

## Mandatory Non-Negotiables (Externally Graded!)
- Must open and pass **`kicad-cli sch erc`** with **0 ERC errors**.
- Must export a valid netlist via **`kicad-cli sch export netlist`** with **>= 45 real components** and **>= 25 multi-pin nets**, with zero disconnected required pins, zero shorts between power rails and GND, and all 5 functional stages genuinely connected.
- **0 components/pins/wires outside the declared KiCad sheet border** and **0 collisions with the title block**.
- **0 overlapping component bodies** on the schematic.
- Readable, clean engineering schematic layout rendered to PNG via `kicad-cli sch export svg` + `inkscape`.

## Required Output Files (in your assigned output directory)
1. Primary KiCad 9 schematic file: `gateway.kicad_sch` (plus any sub-sheets if multi-sheet, and `.kicad_pro` if needed).
2. High-resolution rendered PNG: `gateway.png` (and per-sheet PNGs if multi-sheet).
3. Source file(s) used to generate it (`gateway.edl` for EDL arm, or Python script(s) for Baseline arm).
4. Brief `notes.md` logging: total iterations, tools used, wall-clock notes, and any friction/errors encountered.
