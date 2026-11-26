# Benchmark 5 (Hospital 11kV / 400V Power SLD) — Baseline (NO-EDL) Engineering Notes

## Benchmark Timing & Telemetry
- `start_epoch`: 1795710882
- `end_epoch`: 1795711428
- `elapsed_sec`: 546
- `iterations`: 3 (1 initial engineering verification script, 1 full SVG+DXF+manifest generation + ASCII encoding fix, 2 visual-inspection polish passes on `hospital_sld.png`)
- `tools_used`: `python3` (standard library `math`, `json`, `xml.sax.saxutils`), `inkscape` (SVG → 3360×2240 PNG export), manual DXF R12 (`AC1009`) serializer, visual PNG inspection

## Deliverables Produced
- `hospital_sld.dxf` — Layered AutoCAD R12 (`AC1009`) IEC 60617 Single-Line Diagram (8 semantic CAD layers: `BORDER_TITLE`, `MV_11KV`, `LV_NORMAL`, `LV_ESSENTIAL`, `FIRE_SAFETY`, `EQUIPMENT`, `INTERLOCKS`, `ANNOTATIONS`; 1,109 CAD entities on an A1-proportioned 840×560 mm sheet).
- `hospital_sld.svg` — Clean vector IEC 60617 Single-Line Diagram generated from the exact same coordinate & calculation engine as the DXF.
- `hospital_sld.png` — High-resolution (3360×2240 px, 192 DPI) raster render via Inkscape, visually verified for zero overlaps and clean legibility.
- `sld_manifest.json` — Complete IEC 60909 / IEC 60364 engineering data matching the required schema (3 LV sources, 4 buses including 11 kV RMU, 2 mandatory interlocks, 19 breakers, 10 sub-main feeders, plus contingency proofs).
- `generate_hospital_sld.py` — Unified calculation + verification + SVG/DXF generator script.

## Key Engineering Calculations & Design Decisions

### 1. IEC 60909-0:2016 Short-Circuit Proof
- **11 kV Utility Intake (`RMU_11KV`)**: $S_k'' = 250\text{ MVA} \Rightarrow I_k'' = 13.12\text{ kA}$. All 3 RMU VCBs (`Q_MV_IN`, `Q_MV_TX1`, `Q_MV_TX2`) rated `630 A / 25 kA` ($25.0 \ge 13.12\text{ kA}$).
- **400 V Main Buses (`MSB_A`, `MSB_B`)**: Single `1000 kVA` Dyn11 transformer ($u_k = 6.0\%, u_r = 1.1\%$) with IEC 60909-0:2016 voltage factor $c_{\max} = 1.05$, transformer impedance correction $K_T = 0.9634$, upstream utility impedance ($R_Q/X_Q = 0.10$), and 8 m `2000 A` Cu busduct gives:
  - **Single Transformer:** $I_k'' = 23.94\text{ kA}$ (peak $i_p = 52.91\text{ kA}$).
  - **Hypothetical Paralleled (Blocked by `IL_BUS_TIE`):** $I_k'' = 44.79\text{ kA}$.
  - All `MSB_A`/`MSB_B` ACBs (`Q_TX1`, `Q_TX2`, `Q_BT`, `Q_ATS_FEED`) and normal feeder MCCBs (`Q_F7`–`Q_F10`) rated `Icu = 50 kA` ($\ge 23.94\text{ kA}$, and even $\ge 44.79\text{ kA}$).
- **400 V Essential Bus (`EMSB`)**:
  - **Normal Utility Mode** (via `MSB_B` + 10 m `1600 A` Cu busduct + `ATS1`): $I_k'' = 23.34\text{ kA}$ (governing maximum).
  - **Standby Generator Mode** (`GEN1` `1000 kVA`, $X_d'' = 15\%$, $K_G = 0.9633$, 15 m `4×(3×240 mm²)` Cu XLPE): $I_k'' = 10.22\text{ kA}$.
  - `ATS1` ACBs (`Q_NORM`, `Q_GEN`) rated `1600 A / 50 kA`; all 6 essential MCCBs (`Q_F1`–`Q_F6`) rated `Icu = 36 kA` ($\ge 23.34\text{ kA}$).

### 2. Normal Balance, N-1 Redundancy & Standby Generator Proof
- **ATS1 Normal Feed Selection (`MSB_B`):** Because `MSB_A` carries 400 kW of direct normal load (`F7_IMAGING` + `F8_CHILLER1`) vs. 320 kW on `MSB_B` (`F9_CHILLER2` + `F10_GEN_PWR`), tapping `ATS1` normal supply (`Q_ATS_FEED`) from `MSB_B` provides superior split-bus transformer load balance:
  - Normal `TX1` (DF = 0.70): `320.8 kVA` (32.1%).
  - Normal `TX2` (`MSB_B` + `EMSB`, DF = 0.70): `757.1 kVA` (75.7%); even at **100% EMSB + 70% MSB_B**, `TX2` is `970.7 kVA` ($\le 1000\text{ kVA}$).
- **N-1 Single-Transformer Contingency (`Q_BT` closed):**
  - Total connected plant load = `1350 kW / 1539.7 kVA` (at DF = 0.70: `1077.8 kVA > 1000 kVA`).
  - **Stage-1 Automatic BMS Shedding (`F8_CHILLER1`, 220 kW):** Remaining load at DF = 0.70 drops to **`896.9 kVA`** (`89.7%` of `1000 kVA` — PASS).
  - **Stage-2 Peak Override (`F8_CHILLER1` + `F9_CHILLER2` + `F10_GEN_PWR`):** Even at **100% peak coincidence**, remaining critical loads (`EMSB` + `F7_IMAGING`) equal **`912.1 kVA`** (`91.2%` — PASS).
- **Standby Generator (`GEN1`):** Carries 100% of `EMSB` essential loads (`630.0 kW / 712.2 kVA`) within `800 kW / 1000 kVA` continuous rating (`78.8%` kW / `71.2%` kVA — PASS).

### 3. IEC 60364-5-52 Cable Sizing & Voltage Drop
- All 10 feeders use Cu XLPE 90°C cables (Reference Method E perforated tray, conservative plantroom derating `k_derate = 0.88` for 35°C ambient × tray grouping) and 90°C operating conductor resistance (`R_90 = 1.28 × R_20`).
- `F2_FIRE_PUMP` upgraded to fire-rated `1×95 mm² FR-XLPE` with `200 A` MCCB (`Ib = 127.4 A`, `Iz = 262.2 A`, `ΔV = 0.76%`) to handle pump starting currents and life-safety integrity.
- Large 220 kW chillers (`F8_CHILLER1`, `F9_CHILLER2`) use parallel `2×150 mm²` runs (`Iz = 702.2 A >= In = 500 A >= Ib = 373.6 A`, `ΔV = 0.93%` and `1.00%`).
- Worst-case feeder voltage drop across all 10 circuits is **`1.00%`**, well below the `4.0%` limit.

## Honest Log of Challenges Encountered (Baseline / No-EDL Friction)
1. **No `ezdxf` or `svgwrite` installed in environment:** Had to write a custom dual-backend `DualCanvas` class from scratch in Python (~130 LOC) to emit both SVG and valid AutoCAD R12 (`AC1009`) DXF with proper `HEADER`, `TABLES` (`LTYPE`, `LAYER`, `STYLE`), and Y-flipped `ENTITIES`, plus manual ASCII sanitization for DXF R12 (caught a `UnicodeEncodeError` on `°` on first run).
2. **Manual coordinate layout & visual collision debugging:** Because SVG/DXF schematics have no constraint-based layout solver, absolute pixel coordinates had to be manually calculated for every symbol, busbar, interlock line, table, and label. Visual inspection of the first rendered PNG caught 6 label/line overlaps (`RMU_11KV` header vs `Q_MV_IN`, `IL_BUS_TIE` dashed line crossing `Q_TX1` text, `Q_TX2` text colliding with `Q_ATS_FEED` `NC` badge, `Q_BT` subtitle touching its moving contact, `Q_NORM` label touching its `NC` badge, and `MSB` header grazing `Q_BT`), requiring 2 manual coordinate-tweaking passes to eliminate all overlaps.
3. **Manual IEC 60617 symbol drawing:** Every electrical primitive (vertical/horizontal draw-out ACB with chevrons, Dyn11 transformer with delta/star windings and earth, synchronous generator, CT + 50/51 relay bubble, 3-phase conductor ticks, interlock diamonds) had to be hand-coded in raw line/circle/polygon primitives.
4. **Ensuring single-source-of-truth consistency:** Keeping `sld_manifest.json`, SVG labels, DXF text entities, and engineering tables synchronized required carefully building a unified Python generator script so recalculations propagate automatically to all outputs.
