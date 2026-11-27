# EDL-Bench — External-Grounded 2D Engineering Benchmark Suite

**Scope:** Strictly **2D engineering across EDL's 5 locked domains** (NO 3D mechanical CAD, NO PCB copper routing):
1. **Electronic Schematics (`elec`)** — Components, pins, nets, ERC, galvanic isolation, MNA/thermal derating
2. **Architectural Floor Plans (`arch`)** — Parametric geometry, structural grids, daylighting, access graphs
3. **Plumbing on Floor Plans (`mep.plumbing`)** — CW/HW/SAN/VENT routing, slopes, DFU capacity, exclusion zones
4. **Electrical Installations on Floor Plans (`mep.elec`)** — Lighting, sockets, switches, DB clearance, 3-phase balance, IEC 60364
5. **Building & Industrial Electrical Schematics (`elec.sld` & `elec.industrial`)** — 3-phase distribution, IEC 60909 `Ik3''`, N-1 contingency, motor interlocks

Every task evaluates **three orthogonal engineering axes**:
- **Geometric reasoning:** Spatial layout, clearances, sheet fit, zero overlaps, zero remaining DOF
- **Topological reasoning:** Pin-to-pin / room-to-room / pipe-to-fixture / circuit connectivity graphs
- **Engineering reasoning:** Real physical/code compliance (KiCad 9 ERC, UT Austin `verification_v2` datasheet topology, EN 806/12056, IEC 60364/60909)

---

## 4-Experiment Evaluation Matrix

| Experiment | Pipeline | What It Proves |
|---|---|---|
| **Exp 1** | Hand-Authored EDL → Compiler & Verifier | Whether EDL itself faithfully expresses and verifies industry-grade tasks |
| **Exp 2** | AI → Native Tools (Python + KiCad / DXF / Matplotlib, NO EDL) | Raw LLM + native tooling baseline (speed, code churn, silent failures, external grader pass rate) |
| **Exp 3** | AI → EDL (single-shot / open-loop) | Whether EDL's representation alone improves generation accuracy |
| **Exp 4** | AI → EDL → Verification → Repair Loop | End-to-end value of EDL's compiler + formal proofs + structured `RepairSuggestion`s |

---

## External Datasets & Licensing Audit (`benchmarks/external/`, gitignored)

| Resource | Domain Fit | Size | License | Commercial Caveat | Role in EDL-Bench |
|---|---|---|---|---|---|
| **[PCBSchemaGen_v2](https://github.com/HZou9/PCBSchemaGen_v2)** (UT Austin, NeurIPS 2026) | **Strong** (`elec`) | 227 tasks (`62` PCBBench + `165` OSE) | Data/KG: **MIT**; Verifier Code: **PolyForm Noncommercial 1.0.0** | Verifier code is non-commercial; benchmark specs/KG are MIT | **Live held-out external deterministic verifier** (`verification_v2` L1–L5) for Domain 1 A/B evals |
| **[ResPlan](https://github.com/m-agour/ResPlan)** | **Strong** (`arch` base) | 17,000 vector-graph plans | Data: **CC BY 4.0**; Code: **MIT** | Attribution required (commercial OK) | Real non-rectangular residential access-graph & metric area benchmarks + base envelopes for MEP overlays |
| **[MSD](https://github.com/caspervanengelenburg/msd)** (TU Delft, ECCV 2024) | **Strong** (`arch` complex) | 5,372 multi-unit plans (18.9K units) | Open academic (check Kaggle dataset terms) | Verify Kaggle terms before commercial redistribution | 4-Zone architectural grammar (`Private / Public / Wet-Service / Outdoor`) & multi-unit access graphs |
| **[CubiCasa5K](https://github.com/cubicasa/cubicasa5k)** | **Good** (`arch` + partial `mep.elec`) | 5,000 SVG floor plans (80+ categories) | **CC BY-NC 4.0** | **Non-commercial only** — do NOT ship derivatives in commercial builds | Reference architectural/fixture taxonomy & polygon representation study |
| **[PID2Graph](https://github.com/LatentSpaceIITB/pid_drawings)** & **[DEXPI](https://www.pi-research.org/research/software/dexpi-studio/)** | **Methodological** (`mep.plumbing`, `elec.sld`) | 36,561 GraphML P&IDs | Research | P&IDs are process schematics, NOT spatial building plumbing | Borrow typed graph-matching metrics (node/edge F1, semantic role recall) for `EDL-Bench` graders |

---

## Live External Suites

### 1. External Electronics Suite — UT Austin `PCBBench` (`pcbschemagen_ab/`)
- **6 Real Third-Party Hard/Medium Tasks:** `T46` (`CAN_ISOLATED_FULL`), `T49` (`BMS_5S_FULL_SYSTEM`), `T51` (`ISOLATED_DAQ_4CH`), `T52` (`IOT_SENSOR_NODE`), `T54` (`LI_ION_CHARGER_FULL`), `T62` (`MCU_SYNC_BUCK_SYSTEM`).
- **Held-Out External Grader:** [`verify_pcbschemagen.py`](verify_pcbschemagen.py) runs UT Austin's unmodified `verification_v2.verify_circuit` (`L1 → L4 → L1b → L2 → L3 → L5`, Reward `0.0 → 1.0`) + real `kicad-cli sch erc --severity-all` on BOTH Baseline and EDL `v3.6.0` schematics.

```bash
python3 benchmarks/edl_bench/verify_pcbschemagen.py \
  --suite pcbbench \
  --batch benchmarks/edl_bench/pcbschemagen_ab/batch_manifest.json \
  --out benchmarks/edl_bench/pcbschemagen_ab/results.json
```

### 2. External Floorplan + MEP Suite — `ResPlan` Real Topology (`floorplan_resplan/`)
- **3 Real Metric-Scale Residential Topology Specs** extracted directly from `ResPlan` (`RP_2BR_1`, `RP_3BR_0`, `RP_4BR_52`) with target room areas, `via_door` connectivity graphs, and shared-wall adjacencies.
- **Deterministic 3-Axis Grader:** [`floorplan_resplan/grade_floorplan.py`](floorplan_resplan/grade_floorplan.py).
