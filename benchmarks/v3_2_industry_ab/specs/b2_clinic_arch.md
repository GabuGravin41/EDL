# Benchmark 2: 15-Room Outpatient Specialist Medical Clinic Floor Plan (Architecture + Structural Grid)

## Objective
Design an industry-scale **Outpatient Specialist Medical Clinic (~240–290 m², 15 rooms)** with a non-trivial stepped/L-shaped footprint, structural steel/concrete column grid, doors, windows, and clinical fixtures.

## Mandatory Program & Room Schedule (15 Rooms)
1. **Vestibule** (`Vestibule`): main entrance airlock, >= 6.0 m², exterior entry door (>= 1200 mm).
2. **Reception & Waiting Lounge** (`Waiting`): >= 28.0 m², exterior glazing/windows, reception counter fixture.
3. **Public Accessible WC** (`PublicWC`): >= 4.5 m², accessible toilet + handwash basin, >= 900 mm door.
4. **Staff Breakroom / Kitchenette** (`Breakroom`): >= 12.0 m², exterior window, kitchenette sink & counter.
5. **Central Clinical Corridor** (`Corridor`): clear width >= 1800 mm, connecting Public Wing, Clinical Wing, and Service Wing cleanly.
6. **Exam Room 1** (`Exam1`): >= 12.0 m², exterior window, exam table + handwash basin + desk.
7. **Exam Room 2** (`Exam2`): >= 12.0 m², exterior window, exam table + handwash basin + desk.
8. **Exam Room 3** (`Exam3`): >= 12.0 m², exterior window, exam table + handwash basin + desk.
9. **Exam Room 4** (`Exam4`): >= 12.0 m², exterior window, exam table + handwash basin + desk.
10. **Minor Procedure Room** (`Procedure`): >= 18.0 m², exterior window, wide door (>= 1100 mm), procedure table + scrub sink.
11. **Nurse Station** (`NurseStation`): >= 10.0 m², central oversight of Corridor & Exam Rooms, workstation counter.
12. **Lab & Sterilization** (`Lab`): >= 10.0 m², lab bench + sterilization sink.
13. **Pharmacy & Clean Storage** (`Pharmacy`): >= 8.0 m², shelving fixture.
14. **Electrical & IT Server Room** (`ElecITRoom`): >= 6.0 m², dedicated lockable door from corridor, **dry zone (no wet fixtures!)**.
15. **Mechanical & Janitor Room** (`MechRoom`): >= 7.0 m², mop sink + water heater space.
*(Optional bonus 16th room: `StaffWC` >= 3.5 m² with toilet + basin — include if helpful, minimum 15 rooms required!)*

## Mandatory Non-Negotiables (Externally Graded!)
- **Structural Column Grid:** Regular grid of structural columns (e.g., 300×300 mm or 350×350 mm on 4.5–6.0 m bays); **0 columns blocking any door swing, opening cutout, or narrow corridor passage**.
- **Watertight Geometry:** **0 overlapping room polygons** (`shapely` intersection > 1 mm²), exterior walls clearly thicker than interior partitions (e.g., 250–300 mm exterior vs 120–150 mm interior).
- **100% Graph Reachability:** Every single room reachable from the exterior via `Vestibule` and interior doors (`networkx` BFS).
- **Door Jamb & Swing Clearance:** Every door has >= 100 mm jamb margin from perpendicular walls; **0 door-swing-to-door-swing or door-swing-to-fixture collisions**.
- **Daylighting:** `Waiting`, `Breakroom`, `Exam1`–`Exam4`, and `Procedure` all have exterior windows on exterior walls.
- **Professional Layered DXF + High-Res PNG:** Export a valid multi-layer `.dxf` (walls, doors/swings, windows, columns, fixtures, room labels/dimensions) AND a crisp architectural `.png` rendering.

## Required Output Files (in your assigned output directory)
1. `clinic_arch.dxf` — valid AutoCAD/CAD layered DXF floor plan.
2. `clinic_arch.png` — high-resolution rendered floor plan image (and `.svg` if produced).
3. `arch_manifest.json` — machine-readable geometry summary matching the exact DXF coordinates (mm):
   ```json
   {
     "rooms": [{"id": "...", "min_x": 0, "min_y": 0, "max_x": 4000, "max_y": 3500, "area_m2": 14.0, "habitable": true}],
     "doors": [{"id": "...", "room_a": "...", "room_b": "...", "width_mm": 900, "hinge_x": 0, "hinge_y": 0, "swing_bbox": [0,0,900,900]}],
     "windows": [{"id": "...", "room": "...", "width_mm": 1500, "x": 0, "y": 0}],
     "columns": [{"id": "...", "min_x": 0, "min_y": 0, "max_x": 300, "max_y": 300}],
     "fixtures": [{"id": "...", "room": "...", "kind": "...", "min_x": 0, "min_y": 0, "max_x": 600, "max_y": 500}]
   }
   ```
4. Source file(s) (`clinic_arch.edl` for EDL arm, or Python scripts for Baseline arm) + `notes.md`.
