# Benchmark 3: Full Building Plumbing (CW/HW/SAN/VENT) Overlaid on the 15-Room Medical Clinic

## Objective
Overlay a complete, IPC-compliant **Domestic Cold Water (CW), Hot Water (HW), Sanitary Drainage (SAN), and Venting (VENT)** plumbing system onto your arm's **15-Room Medical Clinic Floor Plan (Benchmark 2)**.

## Mandatory Wet Fixture Schedule (All 11+ Wet Fixtures Must Be Served)
1. `Exam1` handwash basin (CW + HW + SAN + VENT)
2. `Exam2` handwash basin (CW + HW + SAN + VENT)
3. `Exam3` handwash basin (CW + HW + SAN + VENT)
4. `Exam4` handwash basin (CW + HW + SAN + VENT)
5. `Procedure` surgical scrub sink (CW + HW + SAN + VENT)
6. `Lab` sterilization/lab sink (CW + HW + SAN + VENT)
7. `Breakroom` kitchenette sink (CW + HW + SAN + VENT)
8. `PublicWC` accessible water closet (CW + SAN + VENT, **branch >= 100 mm / DN100**)
9. `PublicWC` lavatory basin (CW + HW + SAN + VENT)
10. `MechRoom` janitor mop/service sink (CW + HW + SAN + VENT)
11. `MechRoom` **Main Water Riser (`MAIN_CW`), Main Isolation Valve, and Electric Water Heater (`WH1`)** feeding the HW distribution manifold.
*(If your B2 plan also includes `StaffWC`, serve its WC + basin as well!)*

## Mandatory Non-Negotiables (Externally Graded!)
- **100% Wet Fixture Service:** Every wet fixture connected to required CW, HW (where hot water applies), SAN drain, and VENT stack.
- **IPC DFU & Pipe Sizing:**
  - WC drain branches **>= 100 mm** (DN100).
  - Basin/sink drain branches **>= 40 mm** (DN40/DN50).
  - Main sanitary collector trunk sized appropriately for total cumulative DFU (e.g., **>= 100–150 mm** at **1–2% slope**).
  - Cold/hot main headers >= 25–32 mm, fixture branches >= 15–20 mm.
- **Isolation Valves & Cleanouts:** Main shutoff valve + zone/fixture isolation valves + sanitary cleanouts at stack bases / directional changes.
- **Cross-Discipline Clash Prohibition:** **0 plumbing pipes, stacks, or wet equipment inside `ElecITRoom`**, and **0 pipes/risers clashing with structural columns**!
- **Deliverables:** Layered **DXF** (`clinic_plumbing.dxf`), high-res **PNG** (`clinic_plumbing.png`), **Pipe & Equipment Takeoff CSV** (`plumbing_schedule.csv`), and machine-readable `plumbing_manifest.json` matching the exact routed geometry.

## Required `plumbing_manifest.json` Schema
```json
{
  "fixtures": [{"id": "...", "room": "...", "kind": "wc|basin|sink|mop_sink|water_heater", "x": 0, "y": 0, "dfu": 4, "has_cw": true, "has_hw": false, "has_san": true, "has_vent": true}],
  "valves": [{"id": "...", "system": "CW|HW", "x": 0, "y": 0}],
  "cleanouts": [{"id": "...", "x": 0, "y": 0}],
  "pipes": [{"id": "...", "system": "CW|HW|SAN|VENT", "dn_mm": 100, "slope_pct": 2.0, "from_x": 0, "from_y": 0, "to_x": 5000, "to_y": 0, "dfu_load": 12}]
}
```
Plus source file(s) and `notes.md`.
