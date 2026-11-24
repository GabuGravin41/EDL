# 01 — Focused Prior-Art & Library Lessons for EDL v0.1

A concise, practical synthesis of what to **borrow**, **avoid**, and **differentiate** from existing tools before writing EDL's first compiler.

---

## 1. Languages & Modeling Systems

| System | Core Strength to Borrow | Boundary / What NOT to Copy |
|---|---|---|
| **Penrose** (CMU) | Clean 3-way split: **Domain** (types/relations), **Substance** (semantic objects), **Style** (visual presentation + layout constraints). Best conceptual precedent for EDL's semantic-vs-presentation boundary. | Optimization-only diagramming; no physical units, CAD tolerance semantics, or engineering interchange adapters. |
| **Onshape FeatureScript** | Strong unit-typed math (`ValueWithUnits`), coordinate systems (`CoordSystem`), and **semantic geometric queries** (`qCreatedBy`, `qCoPlanar`) instead of brittle integer edge indices. | Imperative script tied tightly to Onshape's proprietary Parasolid feature tree; not cross-tool or declarative. |
| **Zoo (KittyCAD) KCL** | Modern Rust-based parametric CAD language designed with AI authoring in mind; compact pipe/feature syntax and clean unit handling. | Primarily procedural 3D sketch/extrude CAD—does not model cross-domain semantic graphs (rooms/walls, electrical nets, verification ledgers). |
| **atopile** (`.ato`) | Compact declarative syntax for engineering intent (`interface`, `signal`, `~` tolerances, explicit SI quantities, KiCad compilation). Great reference for how small, readable declarative engineering files look. | Electronics-only; layout is still mostly delegated to KiCad rather than solved via a separate presentation layer. |
| **JITX / tscircuit** | Shows the power of typed interfaces, automated ERC/DRC, and reusable component modules in code-first hardware design. | Host-language imperative freedom (Stanza / TypeScript+React) makes static inspection, deterministic patching, and strict declarative guarantees harder. |
| **SysML v2 / KerML** | Formal semantic metamodel + textual notation + explicit **requirements/verification** objects + REST/JSON model query API. | Far too heavyweight and bureaucratic for fast geometric authoring by LLMs. Borrow the *concept* of a queryable semantic model + verification cases, not the ceremony. |
| **Modelica** | Acausal physical connectors (`flow`/`potential` variables) and unit-checked equations. Future gold standard for Tier 5 simulation hooks. | Not a geometric/spatial layout language. Defer equation-based simulation until Tier 5. |

### EDL's Non-Duplicative Wedge
None of the systems above combine all five of these in one small, AI-operable stack:
1. **Domain-neutral typed semantic graph** (units + coordinate frames + ports/relations),
2. **Space/relation-first declarative 2D geometry** with exact analytical primitives,
3. **Stratified deterministic resolution** (DAG → linear layout → later specialized solvers),
4. **Evidence-bearing verification ledger** (explicit status + tolerance + provenance), and
5. **Agent-native compiler loop** (structured JSON IR/diagnostics with source spans + annotated SVG visual self-check + native CAD export).

---

## 2. Candidate Libraries & Tooling (v0.1 Scope)

### If building directly in Rust
| Need | Top Candidate | License | Notes for v0.1 |
|---|---|---|---|
| **Parser & Diagnostics** | `winnow` or `chumsky` + `ariadne`/`miette` | MIT/Apache-2.0 | Fast to build; great span-aware error reporting. |
| **Linear Layout Solver** | `kasuari` (maintained Cassowary fork in Rust) | MIT/Apache-2.0 | Ideal for Tier 2 room alignment, derived spans, and soft preferences. Wrap with our own DOF/conflict diagnostics. |
| **2D Analytic Primitives** | Custom tiny `geometry2d` module (`Point2`, `Vec2`, `Segment`, `Circle`, `Arc`, `Rect`, `Polygon`) | Owned | Keeps exact analytical arcs (e.g., door swings!) authoritative—never polygonized in the IR. |
| **2D Offsets / Contours (if needed)** | `cavalier_contours` (lines + bulge arcs) or `geo` (polygons only) | MIT/Apache-2.0 | Only pull in if/when arbitrary non-rectangular wall joins or offsets are needed. For orthogonal v0.1 floor plans, exact segment/rect/arc math is tiny (~300 LOC) and 100% deterministic. |
| **SVG / DXF Export** | Custom lightweight writers (or `dxf` crate) | MIT/Apache-2.0 | SVG and basic DXF (`LINE`, `ARC`, `CIRCLE`, `LWPOLYLINE`, `TEXT`, layers) are simple enough that owning thin writers avoids impedance mismatches. |

### Deferred (Post-v0.1, Recorded Now to Prevent Dead Ends)
- **Nonlinear 2D Sketch Solver (Tier 3):** Evaluate FreeCAD's **PlaneGCS** (`LGPL-2.1-or-later`) before SolveSpace (`GPLv3`).
- **Schematic Graph Layout / Routing (Tier 4):** Evaluate **ELK (Eclipse Layout Kernel)** (`EPL-2.0`).
- **3D B-Rep / STEP (Phase 4+):** **Open CASCADE (OCCT)** (`LGPL-2.1` with exception) behind a batched FFI boundary.

---

## 3. Concrete Takeaways Locked for v0.1

1. **Space-first, acyclic floor-plan ontology:** Declare rooms with clear interior dimensions and spatial relations; let the deterministic engine derive the shared wall graph, exterior envelope, and clean L/T wall junctions. (Eliminates manual wall-thickness arithmetic and circular `Room ↔ Wall` references.)
2. **Exact analytical door swings from day one:** Store door swing arcs as true `(center, radius, start_angle, sweep_angle)` analytical arcs, so both geometric clearance checks and SVG/DXF outputs are exact.
3. **Minimal CLI surface (4 commands):**
   - `edl build <file.edl> --json` (parse, type-check, solve, validate, emit IR + spans + measurements + evidence)
   - `edl preview <file.edl> -o <out.svg>` (annotated SVG with room labels, dimensions, wall hatches, door swings, and red violation overlays)
   - `edl export <file.edl> --dxf <out.dxf>` (layered DXF with walls, doors, windows, room boundaries, and annotations)
   - `edl diff <a.edl> <b.edl> --json` (semantic diff across entity IDs, solved coordinates, and verification statuses)
