# Benchmark 5: Hospital 11kV / 400V Dual-Transformer + Standby Generator Power Single-Line Diagram (IEC 60909 / IEC 60364)

## Objective
Design an industry-scale **Hospital / Medical Center Medium-to-Low Voltage Power Single-Line Diagram (SLD)** and complete electrical protection engineering study (`11 kV / 400 V`, `50 Hz`, `2 × 1000 kVA` Transformers + `1000 kVA` Standby Diesel Generator + Interlocked Bus-Tie & ATS + 10 Outgoing Sub-Main Feeders).

## Mandatory Electrical Architecture
1. **Medium Voltage (11 kV) Intake:**
   - Utility 11 kV incoming feeder (`UTIL_11KV`, prospective fault level $S_k'' = 250\text{ MVA}$) into a 3-way 11 kV Ring Main Unit (`RMU_11KV`) with SF6/vacuum circuit breakers feeding two transformers.
2. **Dual Step-Down Transformers (N-1 Redundant):**
   - `TX1` & `TX2`: each **`1000 kVA`, `11 kV / 400 V`, Dyn11, $u_k = 6.0\%$**.
   - Main ACB incomers (`Q_TX1`, `Q_TX2`) into split Main LV Switchboard buses **`MSB_A`** and **`MSB_B`**, linked by an **interlocked Bus-Tie ACB (`Q_BT`)** (interlocked with `Q_TX1`/`Q_TX2` in 2-of-3 scheme so transformers are not paralleled unintentionally).
3. **Standby Power & Essential Bus:**
   - Standby Diesel Generator **`GEN1`** (`1000 kVA`, `400 V`, $X_d'' = 15\%$, 0.8 PF = 800 kW continuous).
   - **Interlocked Automatic Transfer Switch (`ATS1`)** feeding Essential Main Switchboard **`EMSB`** from either `MSB_A`/`MSB_B` (normal) or `GEN1` (emergency), with mandatory mechanical + electrical interlock preventing Utility–Generator paralleling!
4. **10 Outgoing Sub-Main Feeders (with exact loads, power factors, and real cable run lengths):**
   - **Essential Bus (`EMSB` — backed by `GEN1`):**
     1. `F1_MED_UPS`: Medical Procedure & ICU UPS — **150 kW**, $\cos\phi = 0.90$, length **35 m**
     2. `F2_FIRE_PUMP`: Fire & Life-Safety Pump — **75 kW**, $\cos\phi = 0.85$, length **55 m** (fire-rated cable)
     3. `F3_CLINIC_DB1`: Outpatient Clinic Sub-DB 1 — **120 kW**, $\cos\phi = 0.90$, length **45 m**
     4. `F4_CLINIC_DB2`: Outpatient Clinic Sub-DB 2 — **110 kW**, $\cos\phi = 0.90$, length **60 m**
     5. `F5_LAB_STERIL`: Sterilization & Lab Wing — **85 kW**, $\cos\phi = 0.88$, length **40 m**
     6. `F6_ELEVATORS`: Medical Elevators — **90 kW**, $\cos\phi = 0.85$, length **50 m**
     *(Total Essential = 630 kW <= 800 kW GEN1 capacity!)*
   - **Normal Buses (`MSB_A` / `MSB_B`):**
     7. `F7_IMAGING`: MRI / X-Ray Imaging Suite — **180 kW**, $\cos\phi = 0.90$, length **40 m** (on `MSB_A`)
     8. `F8_CHILLER1`: HVAC Chiller #1 — **220 kW**, $\cos\phi = 0.85$, length **65 m** (on `MSB_A`)
     9. `F9_CHILLER2`: HVAC Chiller #2 — **220 kW**, $\cos\phi = 0.85$, length **70 m** (on `MSB_B`)
     10. `F10_GEN_PWR`: Admin Lighting & General Power — **100 kW**, $\cos\phi = 0.90$, length **50 m** (on `MSB_B`)
     *(Demand factor 0.70 on Normal + Essential or N-1 shedding of 1 Chiller -> check exact transformer N-1 loading!)*

## Mandatory Non-Negotiables (Externally Graded!)
- **IEC 60909 Short-Circuit Proof ($I_k''$ & $I_{cu}$):** Compute prospective 3-phase short-circuit current $I_k''$ at `MSB_A`/`MSB_B` (from 1000 kVA, $u_k = 6\%$, plus utility $S_k'' = 250\text{ MVA}$ -> $I_k'' \approx 22\text{–}25\text{ kA}$ single transformer, $\approx 44\text{–}50\text{ kA}$ if paralleled). **Every LV breaker MUST have $I_{cu} \ge I_k''$!**
- **N-1 & Standby Contingency Proof:**
  - Single-transformer normal operation & N-1 essential/contingency loading within `1000 kVA` rating (with documented load-shedding if total peak exceeds 1000 kVA).
  - Generator `GEN1` (`1000 kVA` / `800 kW`) carries 100% of `EMSB` essential loads (`630 kW` / ~712 kVA) with margin.
- **Mandatory Interlocks:** ATS utility-vs-generator interlock AND 2-of-3 bus-tie interlock explicitly specified & verified.
- **Cable Sizing & Voltage Drop:** Every one of the 10 feeders must have a valid copper XLPE cable size ($\text{mm}^2$, parallel runs allowed for large loads), breaker rating $I_n$ satisfying $I_b \le I_n \le I_z$, and **$\Delta V \le 4.0\%$**!
- **Deliverables:** Valid IEC SLD **DXF** (`hospital_sld.dxf`), **SVG** (`hospital_sld.svg`), high-res **PNG** (`hospital_sld.png`), and `sld_manifest.json`.

## Required `sld_manifest.json` Schema
```json
{
  "sources": [{"id": "TX1", "kind": "transformer|generator", "kva": 1000, "voltage_v": 400, "uk_pct": 6.0}],
  "buses": [{"id": "MSB_A", "voltage_v": 400, "ik_ka": 23.5, "essential": false}],
  "interlocks": [{"id": "IL_ATS", "devices": ["Q_NORM", "Q_GEN"], "rule": "mutually_exclusive"}],
  "breakers": [{"id": "Q_TX1", "bus": "MSB_A", "in_a": 1600, "icu_ka": 50.0}],
  "feeders": [{"id": "F1_MED_UPS", "bus": "EMSB", "load_kw": 150, "pf": 0.90, "length_m": 35, "breaker_a": 320, "icu_ka": 36.0, "cable_runs": 1, "cable_mm2": 185, "iz_a": 380, "vdrop_pct": 1.1}]
}
```
Plus source file(s) and `notes.md`.
