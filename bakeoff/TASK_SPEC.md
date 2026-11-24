# EDL Syntax Bake-Off — Task Specification (Identical Across All Tracks)

You will complete **4 engineering tasks** using ONLY the notation and conventions shown in your assigned cheat sheet.

---

## Task 1 — Generate a New Floor Plan (`Apartment_B`)
Create a complete, dimensionally exact 2D floor plan with the following requirements:
- **Units:** `mm`, right-handed 2D frame `FloorPlan` (`+X = east`, `+Y = north`).
- **Wall convention:** `clear_interior` measurements; `exterior_wall = 200mm`, `interior_wall = 120mm`.
- **Rooms (4 rooms):**
  1. `Hall`: clear `1500mm` wide × `3600mm` deep, `southwest` corner anchored at `FloorPlan.origin (0mm, 0mm)`.
  2. `Living`: clear `4200mm` wide × `3600mm` deep, east of `Hall` (separated by `120mm` interior partition), south-aligned with `Hall`.
  3. `Bedroom`: clear `3400mm` wide × `3000mm` deep, north of `Living` (separated by `120mm` interior partition), east-aligned with `Living`.
  4. `Bath`: north of `Hall` (separated by `120mm` interior partition), west-aligned with `Hall`, east face separated from `Bedroom`'s west face by a `120mm` interior partition, north-aligned with `Bedroom`. (**Important:** Do NOT hardcode `Bath`'s width or depth if your notation supports deriving/constraining them—let the constraints determine them! If your notation requires explicit coordinates/sizes, compute them accurately and show the exact numbers.)
- **Openings (5 openings):**
  1. `D_Entry` (door, `900mm` wide): on south exterior wall of `Hall`, centered, hinge at west (`start`), swings into `Hall`.
  2. `D_Living` (door, `850mm` wide): on shared wall between `Hall` and `Living`, `500mm` from south end, hinge at south (`start`), swings into `Living`.
  3. `D_Bath` (door, `750mm` wide): on shared wall between `Hall` and `Bath`, centered, hinge at west (`start`), swings into `Bath`.
  4. `D_Bed` (door, `800mm` wide): on shared wall between `Living` and `Bedroom`, `300mm` from west end of their shared wall, hinge at west (`start`), swings into `Bedroom`.
  5. `W_Living` (window, `2000mm` wide): on east exterior wall of `Living`, centered.
- **Verification checks to declare:**
  - No room overlaps
  - All openings maintain `>= 100mm` jamb margin on both sides
  - No door swing collisions
- **View:** `PlanView` on `A3 landscape` at `1:50` scale.

---

## Task 2 — Explain the Worked Example (`Studio_A`)
Using the `Studio_A` worked example from your cheat sheet, answer concisely:
1. What are the exact solved clear dimensions (`width × depth` in mm) and `(min_x, min_y, max_x, max_y)` bounds of `Bath`?
2. What is the overall exterior footprint (`width × depth` in mm, including `200mm` exterior walls)?
3. What happens geometrically if `Living`'s clear width increases from `4500mm` to `5000mm`? Which rooms/walls/openings move, and which stay unchanged?

---

## Task 3 — Surgical Modification of `Studio_A`
Produce the updated `Studio_A` source with ONLY these two changes (leave everything else untouched):
1. Increase `Living` clear width from `4500mm` to `5200mm`.
2. Widen `W_Living` from `1800mm` to `2200mm`.

---

## Task 4 — Diagnose & Repair a Broken `Studio_A`
Suppose someone modifies `Studio_A` so that:
- `D_Bath` has `offset: 40mm from start` (instead of `300mm`) and `width: 750mm`, AND
- `D_Kitchen` is changed to `offset: 1550mm from start` (on the `2400mm` shared wall), `width: 800mm`, `hinge: end` (north end of opening, at `y = 2350mm`), `swing: into Living`.

Identify **every** violation with exact millimeter numbers (check both jamb margins `>= 100mm` and door-swing arc overlap in `Living`), and provide the minimal repaired code for `D_Bath` and `D_Kitchen`.

---

## Output Format Required
Return a clean markdown response with 4 sections:
- `## Task 1` (single fenced code block with the complete `Apartment_B` source)
- `## Task 2` (concise numbered answers 1, 2, 3)
- `## Task 3` (single fenced code block with the updated `Studio_A` source)
- `## Task 4` (brief exact diagnosis + minimal repaired snippet)
