# Benchmark 4: Full Building Electrical Installation (IEC 60364) Overlaid on the 15-Room Medical Clinic

## Objective
Overlay a complete, IEC 60364-compliant **Building Electrical Installation** (Distribution Boards, Lighting, Switches, Power Sockets, Dedicated Medical & Heavy-Load Circuits, Routed Conduits/Cables, 3-Phase Load Balancing, and Voltage-Drop Proofs) onto your arm's **15-Room Medical Clinic Floor Plan (Benchmark 2)**.

## Mandatory Electrical Scope
1. **Distribution Boards (Panels):**
   - `DB_MAIN` (Main 400V/230V 3-Phase Distribution Board) inside `ElecITRoom` with **>= 900 mm unobstructed front working clearance**.
   - `DB_CLINIC` (Clinical Sub-Distribution Board, fed via 3-phase sub-main from `DB_MAIN`) near `NurseStation` / `Corridor` with **>= 900 mm clear working zone** (never blocked by door swings, fixtures, or plumbing!).
2. **Room-by-Room Device Coverage (All 15 Rooms):**
   - **Lighting luminaires** in every single room (scaled to room area — e.g., >= 4 fixtures in `Waiting`, >= 3 in `Corridor`, >= 2 in `Procedure`, >= 1–2 in each `Exam1`–`Exam4`, `Lab`, `Breakroom`, `NurseStation`, etc.) with wall switches on the door strike side.
   - **Emergency / Exit Lighting** along `Corridor`, `Vestibule`, `Waiting`, and `Procedure`.
   - **General & Medical Power Sockets** across all clinical/admin rooms (`Exam1`–`Exam4`, `Procedure`, `NurseStation`, `Lab`, `Waiting`, `Breakroom`, `Pharmacy`, `ElecITRoom`).
   - **Dedicated High-Load / Special Circuits:**
     - `Procedure` medical RCD-protected circuit (30 mA Type A RCD).
     - `Lab` Autoclave / Sterilizer dedicated 20A/25A circuit.
     - `MechRoom` Electric Water Heater dedicated 20A/25A circuit.
     - `ElecITRoom` Server Rack dedicated 16A/20A UPS-backed circuit.
     - `Breakroom` Kitchenette Appliances dedicated 20A circuit.

## Mandatory Non-Negotiables (Externally Graded!)
- **100% Room Coverage & Circuit Assignment:** Every room has lighting; every electrical device is assigned to a valid circuit & DB panel.
- **IEC 60364 Coordination ($I_b \le I_n \le I_z$):** For EVERY circuit, design current $I_b \le$ breaker rating $I_n \le$ cable ampacity $I_z$ (copper PVC/XLPE in conduit).
- **Real Routed Voltage Drop ($\Delta V\%$):** Computed along actual routed cable lengths in meters from DB to furthest device:
  - Lighting circuits: **$\Delta V \le 3.0\%$**
  - Power / Sub-main circuits: **$\Delta V \le 5.0\%$**
- **3-Phase Load Balancing:** Total connected load distributed across `L1`, `L2`, `L3` with **phase imbalance $\le 15\%$** ($\frac{\max - \min}{\text{avg}} \le 0.15$).
- **Cross-Discipline Clearance:** **0 DB panels or working zones overlapping door swings, columns, or wet plumbing risers**.
- **Deliverables:** Layered **DXF** (`clinic_electrical.dxf`), high-res **PNG** (`clinic_electrical.png`), **Panel & Circuit Schedule CSV** (`panel_schedule.csv`), and machine-readable `electrical_manifest.json`.

## Required `electrical_manifest.json` Schema
```json
{
  "panels": [{"id": "DB_MAIN", "room": "ElecITRoom", "x": 0, "y": 0, "width_mm": 600, "depth_mm": 250, "clearance_bbox": [0,0,900,1200]}],
  "devices": [{"id": "...", "room": "...", "kind": "light|switch|socket|emergency_light|dedicated_load", "circuit": "C1", "load_w": 36, "x": 0, "y": 0}],
  "circuits": [{"id": "C1", "panel": "DB_MAIN", "kind": "lighting|power|dedicated|submain", "phase": "L1|L2|L3|3P", "breaker_a": 10, "rcd_ma": 30, "cable_mm2": 1.5, "load_w": 450, "route_len_m": 24.5, "vdrop_pct": 1.4}],
  "wires": [{"circuit": "C1", "from_x": 0, "from_y": 0, "to_x": 3000, "to_y": 0}]
}
```
Plus source file(s) and `notes.md`.
