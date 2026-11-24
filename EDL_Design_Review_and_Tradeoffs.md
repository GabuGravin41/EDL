# EDL Design Review, Reality Check & Architectural Tradeoffs

**Reviewed Documents:**
- `EDL_Proposal_v0.1.md` (Concept Proposal & Research Plan)
- `EDL_RFC_001_Initial_Language_Design.md` (Initial Language Design RFC)

**Purpose:** Give an honest, technically grounded assessment of what EDL is trying to build, how hard each piece actually is in practice, the pros and cons of the major architectural forks, and where ChatGPT's current drafts are strong vs. where hidden engineering traps live—so we can compare approaches and lock in the right design.

---

## 1. Overall Sentiment

**Verdict: Strongly positive on the core thesis, cautious on the current execution scope.**

You have landed on a real, high-value problem. Today, LLMs are great at writing software because programming languages express **intent, structure, types, and invariants**, while compilers and layout engines handle exact low-level math. In engineering (CAD, BIM, EDA), LLMs are forced to emit raw coordinates, vertices, or fragile imperative scripts where intent, topology, geometry, and visual layout are tangled together.

What makes these two drafts noticeably better than most "AI CAD" concepts:
- **Strong architectural instincts:** Separating *Semantics*, *Geometry/Topology*, *Construction DAG*, *Constraints*, and *Presentation/Layout* is the right foundation.
- **High epistemic discipline:** The layered verification model and explicit status vocabulary (`proven` / `satisfied_within_tolerance` / `simulated` / `assumed` / `unverified` / `violated`) avoid the dangerous "if it compiles, it's physically safe" illusion.
- **Pragmatic build-vs-reuse instinct:** Refusing to rewrite a 3D B-rep kernel or FEA solver from scratch saves years of wasted effort.
- **Good evolution between docs:** `RFC 001` already fixes several flaws from `Proposal v0.1`—it splits linear vs. nonlinear solvers (§9.4), distinguishes `require` / `prefer` / `minimize` (§6.3), introduces frame-typed points `Point2<Frame>` (§5.2), drops bidirectional round-tripping from the compiler pipeline (§7), and replaces 10 waterfall spec docs with concrete acceptance tests (§12).

**Where my skepticism kicks in:**
ChatGPT's drafts exhibit a classic "clean architecture document" trap: **everything sounds harmonious on paper because hard algorithmic collisions are hidden inside polite requirements.** When you actually start coding `RFC 001 §11–12`, several requirements collide head-on (for example: demanding analytic arcs + 2D fillets/offsets while proposing Rust `geo`, or bundling nonlinear sketch solving + soft layout optimization + incremental caching + schematic auto-routing into the early milestones).

If you build this as currently scoped in `RFC 001`, you risk spending 6–12 months fighting solver/kernel plumbing before you ever test whether an AI can actually write and edit EDL better than Python or KiCad S-expressions.

---

## 2. What Are We Actually Building? (Stripped of Buzzwords)

Before locking in a stack, it helps to state plainly what EDL *is* and what it *isn't*.

EDL is **not one thing**—it is **four distinct layers** stacked together:

1. **An Intent & Type Language (`.edl`):** A declarative, unit-safe, frame-safe way to say *"what exists, how pieces relate, and what must hold."*
2. **A Deterministic Resolution Engine (IR + Solvers + Geometry):** Takes partial/relative descriptions and computes exact numbers, shapes, and topologies—or loudly explains why it can't (under-constrained, over-constrained, invalid).
3. **A Verification & Evidence Ledger:** Runs deterministic checks (units, geometry validity, ERC/DRC) and external checks (target tool validators, solvers) and attaches provenance to every claim.
4. **Target Adapters (Compilers/Exporters):** Translates the resolved IR into native engineering artifacts (SVG, DXF, KiCad `.kicad_sch`, STEP, IFC) while reporting any lossy mappings.

**The single most important question to lock down:**
> Is EDL primarily an **AI-first intermediate authoring layer** (optimizing for LLM generation, inspection, and surgical editing with tight compiler feedback), OR a **universal cross-discipline engineering language** (trying to unify architecture, electronics, and 3D mechanical CAD from day one)?

Trying to optimize for *both* simultaneously on day one is where scope explodes.

---

## 3. Honest Difficulty Map: How Easy or Hard Is Each Piece?

This table separates what is genuinely straightforward engineering from what is a multi-year rabbit hole.

| Subsystem | Actual Difficulty | Why (Real-World Engineering Reality) |
|---|---|---|
| **Lexer, Parser, AST & Diagnostics** (Rust) | **Easy** (1–3 weeks) | Well-understood tooling (`winnow`, `chumsky`, `miette`/`ariadne` for crisp errors). Don't overthink parser benchmarks early. |
| **Dimensional & Frame Type System** (`mm`, `deg`, `Point2<World>`) | **Easy–Medium** (2–4 weeks) | Textbook type checking with dimensional exponents (SI base units) and phantom/tagged coordinate frames. High ROI for catching LLM hallucinations. |
| **Constructive Parametric DAG** (derived values, transforms, simple features) | **Easy–Medium** (2–4 weeks) | Pure functional/DAG evaluation is fast, 100% deterministic, and easy to debug. |
| **Linear Layout Constraints** (alignment, adjacency, spacing, margins) | **Easy–Medium** (2–4 weeks) | Solved well by Cassowary (`kasuari` / `cassowary-rs`) or flexbox/grid-style solvers. Great for floor-plan boxes and coarse schematic blocks. |
| **SVG & Basic DXF Export** | **Easy–Medium** (1–3 weeks) | SVG is trivial for 2D analytical curves (`M/L/A/C`). Basic DXF (`LINE`, `ARC`, `CIRCLE`, `LWPOLYLINE`, layers, dimensions) is very doable. |
| **Incremental Recomputation** (Salsa-style caching) | **Medium** (3–6 weeks) | Rust has mature patterns (`salsa`), *but* for 2D prototypes (<1,000 entities), full rebuild takes <5 ms anyway. Premature for v0.1. |
| **KiCad Schematic Adapter** (real `.kicad_sch` output) | **Medium–Hard** (4–8 weeks) | KiCad's S-expr format is clean, *but* real schematics require symbol library lookup, exact pin offsets/orientations, grid snapping (1.27 mm / 50 mil), junction dots, wire endpoints matching pin coordinates, and running `kicad-cli sch erc` to verify. |
| **2D Analytic Geometry + Booleans + Offsets + Fillets** | **Hard** (months if custom) | **Critical catch in RFC 001 §9.2:** Rust `geo` and `Clipper2` are *polygonal* (straight segments only)—they do **not** natively support analytical arcs, circles, or exact curve-curve fillets/booleans! Preserving true arcs requires either polyline-with-bulge libraries (like `cavalier_contours` in Rust), CGAL (GPL/commercial traps), or OCCT (C++ FFI). |
| **General Nonlinear 2D Geometric Constraint Solver** (tangency, angles, multi-branch, DOF + conflict diagnosis) | **Very Hard** (6–18+ months if built; tricky even if wrapped) | Writing a robust sketch solver with symbolic/numeric rank analysis, branch selection, and *good conflict diagnostics* is a specialist domain. Wrapping **SolveSpace (`slvs`)** brings a **GPLv3** license; wrapping **FreeCAD's `PlaneGCS`** (LGPLv2.1—unmentioned in ChatGPT's docs!) is license-friendlier, though still C++ FFI. |
| **Readable Orthogonal Schematic Auto-Layout & Auto-Routing** | **Very Hard** (research-adjacent) | Avoiding symbol/text/wire overlaps while producing *human-readable* schematics (minimizing crossings, grouping signal flow left-to-right, placing power/GND flags cleanly) is NP-hard and deeply aesthetic. Even Altium/KiCad don't fully solve this. You will likely want to wrap/study **ELK (Eclipse Layout Kernel)** or start with constrained/grid-based routing. |
| **3D Persistent Topological Naming / Semantic Selectors** (`edges(blank, matching = top_perimeter)`) | **Research-Grade Hard** | Known in CAD as the **Topological Naming Problem**. FreeCAD struggled with it for a decade. Works nicely for simple queries (`top_perimeter` of an extrusion), gets brutal once Booleans, splits, and fillets merge/destroy faces. |
| **Bidirectional Round-Tripping** (Edit in KiCad/AutoCAD → sync back to `.edl`) | **Tarpit / Near-Impossible** | The bidirectional view-update problem. If a user drags a wire or wall in a native app, mapping that back to declarative `require` / `prefer` constraints is ambiguous. Avoid for v1–v2. |
| **Making LLMs Reliably Author & Edit a Brand-New DSL** | **Medium–Hard** (empirical, untested) | LLMs have zero pre-training on `.edl`. Without a tiny, ultra-regular grammar + structured compiler feedback loop (MCP/CLI inspect + error repair), LLMs will hallucinate syntax or fall back to raw coordinates. |

---

## 4. Major Architectural Decisions: Pros, Cons & Recommendations

Before writing code, there are **6 foundational forks** we should explicitly decide rather than drift into.

### Decision 1: Standalone `.edl` Language vs. Embedded DSL (Python/TypeScript) vs. Hybrid IR

| Approach | Pros | Cons |
|---|---|---|
| **A. Standalone `.edl` DSL** *(ChatGPT's choice)* | Clean declarative semantics; no Turing-complete escape hatches; deterministic; easy to analyze statically; clean separation of constraints/layers; compact token count for LLMs. | Cold-start problem for LLMs (zero pre-training data); have to build parser, formatter, LSP, error messages, package manager, docs from scratch. |
| **B. Embedded DSL in Python or TypeScript** (like CadQuery, build123d, tscircuit) | LLMs already write fluent Python/TS; instant loops, math, ecosystem, IDEs, packages; fastest path to working demos. | LLMs sneak in arbitrary imperative logic, raw floats bypassing units, and unanalyzable control flow; hard to inspect or surgically patch an AST; conflates execution order with declarative intent. |
| **C. Hybrid: Canonical Typed IR (JSON/S-expr/Rust structs) + Minimal Declarative Surface Syntax** *(Recommended)* | Lets you test the *semantics, solvers, adapters, and AI loop* immediately via structured IR/API while iterating freely on surface syntax without rewriting the engine. Allows both `.edl` text authoring and structured tool-call patching by AI agents. | Slightly more initial discipline required to keep IR clean and decoupled from syntax sugar. |

> **Recommendation:** Choose **C (IR-first + minimal `.edl` syntax)**. Build the typed IR and engine cleanly first; treat the `.edl` surface syntax as a thin, swappable frontend until LLM benchmarks prove which grammar works best.

---

### Decision 2: Two Domains at Once (Floor Plan + Schematic) vs. Single Wedge First

| Approach | Pros | Cons |
|---|---|---|
| **A. Two Domains Simultaneously** *(ChatGPT's choice)* | Forces the core IR to stay genuinely cross-domain; catches domain leakage early (e.g., hardcoding architectural assumptions into core). | Floor plans (metric 2D geometry) and schematics (topological graphs + orthogonal auto-routing) require **completely different solvers**. Doubles engine complexity before proving either domain is useful. |
| **B. Single Wedge Domain First, Cross-Domain IR Discipline** | Much faster to reach a genuinely impressive, real-world-useful result (e.g., an AI generating real, ERC-clean KiCad schematics *or* real, wall-thickness-aware architectural plans with DXF output). Real users/benchmarks immediately validate value. | Risk of accidentally baking domain assumptions into the core IR if you aren't disciplined. |

> **Recommendation:** Keep the **IR design** tested conceptually against both domains (on paper / in tests), **but build and ship ONE domain end-to-end first** (or keep Schematic Layout strictly grid/template-driven in v0.1 rather than attempting general auto-routing). Decide which domain is your primary wedge!

---

### Decision 3: Constraint Model — General Nonlinear Solver vs. Stratified Semantics

`RFC 001` hints at splitting Cassowary (linear) and nonlinear solvers, which is good, *but* the proposed syntax (`constraints { require ... prefer ... minimize ... }`) mixes all three levels into one flat block.

| Approach | Pros | Cons |
|---|---|---|
| **A. Unified Declarative Constraint Block** | Looks elegant and uniform in small examples. | Hides an algorithmic cliff between linear-time deterministic math, nonlinear multi-root geometry, and NP-hard heuristic layout. Very hard to guarantee determinism or clear error messages. |
| **B. Explicitly Stratified Resolution (3 Tiers)** *(Recommended)* | **Tier 1: Constructive/Parametric DAG** (100% deterministic, zero ambiguity).<br>**Tier 2: Linear Layout / Alignment Constraints** (Cassowary—fast, prioritized, clear conflicts).<br>**Tier 3: Local Sketch Solvers / Specialized Routers** (explicitly scoped blocks with seed/branch policy & residual reports). | Slightly more explicit structure in the language. |

> **Recommendation:** Choose **B (Stratified)**. Most floor plans and schematics can actually be solved cleanly by **Tier 1 + Tier 2** without even needing a nonlinear 2D sketch solver in v0.1! Add Tier 3 (nonlinear sketch solver like PlaneGCS) only when you genuinely need tangencies, arbitrary angles, and mechanical sketches.

---

### Decision 4: Implementation Stack — Rust Day 1 vs. Rapid Prototype then Rust

| Approach | Pros | Cons |
|---|---|---|
| **A. Rust from Day 1** *(ChatGPT's choice)* | Great fit for a real compiler/IR (enums, pattern matching, strong types, native speed, zero runtime deps, WASM compilation for browser previews later). | Slower iteration while the semantics are still fluid; weaker 2D analytic CAD/curve library ecosystem than C++/Python (`geo` has no arcs!); C++ FFI to OCCT/PlaneGCS is painful early on. |
| **B. Python/TypeScript Prototype for 3–4 Weeks → Rust Core** | Ultra-fast iteration on syntax, IR, LLM eval harness, `ezdxf`, KiCad generation, and solver experiments (easy bindings to Z3, scipy, shapely, KiCad, IfcOpenShell). | Throwaway prototype code (temptation to keep building on the prototype). |

> **Recommendation:** If you are comfortable in Rust, **Rust from Day 1 is totally viable** *if* you keep v0.1 scope tight (no C++ FFI in v0.1—use pure-Rust crates like `cavalier_contours` for 2D line+arc polylines/offsets, `kasuari` for linear constraints, and native SVG/DXF/KiCad S-expr writers). Just don't touch C++ FFI (OCCT/CGAL/PlaneGCS) until the language semantics and LLM loop are proven.

---

### Decision 5: What Makes EDL Truly "AI-Native"?

This is the **biggest blind spot** across both of ChatGPT's documents. Both docs talk extensively about *data representation*, and almost nothing about the **AI authoring loop**.

| Approach | Pros | Cons |
|---|---|---|
| **A. File-Centric Compiler Only** *(Current drafts)* | Familiar traditional compiler workflow (`edl build file.edl`). | Forces an AI agent to regenerate whole files, guess why a solver failed, and parse human prose error messages. |
| **B. Protocol & Loop-First ("Compiler as an Agent Tool")** *(Recommended)* | From v0.1, expose:<br>1. **Structured JSON Diagnostics** (exact entity IDs, conflicting constraint pairs, remaining DOF list, suggested fixes).<br>2. **IR Query Commands** (`edl query clearances`, `edl query unconnected-pins`, `edl inspect LivingRoom`).<br>3. **Visual Self-Check Output** (instant annotated SVG render with bounding boxes, labels, and violated constraints highlighted in red so multimodal LLMs can inspect their own output!).<br>4. **Surgical Patching** (edit one entity/constraint by stable ID without rewriting the whole file). | Requires designing CLI/JSON/MCP interfaces alongside the compiler from the start. |

> **Recommendation:** **B is the actual moat.** An LLM + EDL compiler + structured DOF/conflict feedback + annotated SVG render loop will beat raw CAD generation by an order of magnitude—even with a tiny v0.1 feature set.

---

### Decision 6: One-Way Compilation vs. Round-Tripping

- **Recommendation:** Lock in **one-way compilation (`.edl` = single source of truth) + target-side verification** (e.g., export `.kicad_sch` → run `kicad-cli sch erc --format json` → ingest results into EDL's verification ledger). Explicitly list bidirectional editing as a **non-goal** until v2+.

---

## 5. Technical Gaps & Blind Spots in ChatGPT's Two Drafts

To compare directly with ChatGPT's ideas, here are the specific gaps worth calling out:

1. **The Rust `geo` vs. Analytic Curves contradiction (`RFC 001 §9.2` & `§12`):**
   `RFC 001` says to evaluate Rust `geo` *and* demands exact analytic arcs/circles + 2D fillets/offsets in Prototype 1. Rust `geo` is strictly polygonal (straight line segments)—it has no arc primitive. If you want pure-Rust 2D analytic lines + circular arcs + offsets + booleans, look at **`cavalier_contours`** (MIT/Apache, polyline + bulge arcs) or **`kurbo`** (2D curves/Beziers/arcs, though limited booleans).
2. **Missing `PlaneGCS` (FreeCAD's sketch solver):**
   Both docs flag SolveSpace's **GPLv3** license as a risk, yet miss that **FreeCAD's `PlaneGCS`** is **LGPLv2.1** and is specifically built for 2D parametric CAD sketch solving with DOF and redundant/conflicting constraint detection.
3. **Missing crucial modern prior art:**
   Neither doc studies the closest living peers to EDL:
   - **Penrose** (CMU): Separates *Domain* (types/relations), *Substance* (objects/semantics), and *Style* (visual layout/constraints)—almost identical to EDL's mental model.
   - **JITX**, **atopile**, **tscircuit**: Code-first electronics languages solving typed ports, constraints, verification, and schematic/PCB generation.
   - **Zoo (KittyCAD) KCL**: A modern Rust-based parametric CAD language built specifically with AI-assisted CAD in mind.
   - **SysML v2** & **Modelica**: Textual engineering languages with typed interfaces, units, requirements, verification cases, and physical equations.
   - **ELK (Eclipse Layout Kernel)**: The industry-standard open-source hierarchical & orthogonal graph layout engine (directly relevant to schematic layout).
4. **Oversimplified domain examples hide real modeling friction:**
   - **Floor plan (`shared_boundary: 0m`):** Real rooms don't share zero-thickness edges; walls have structural + finish thickness, and dimensions are specified clear-interior vs. centerline vs. exterior-face. Testing a 3-room plan *with wall thickness and a door-swing clearance* will immediately expose whether your entity/geometry split actually works.
   - **Schematic (`Example B` in `Proposal v0.1`):** `J2.ground` was accidentally left unconnected in `net GND`—a great unintentional reminder of how valuable Layer 3 ERC checks are!
5. **Premature optimization & doc bloat:**
   `Proposal v0.1` plans 10 spec documents before code and benchmarks for 100,000 declarations. `RFC 001` improves this, but still packs 2D fillets, offsets, nonlinear solving, soft preferences, and incremental caching into the first prototype. **Validate the AI authoring hypothesis first on 50 entities before optimizing for 100,000.**

---

## 6. Recommended Path to Lock In (A Pragmatic v0.1 Plan)

If I were guiding this project from today to a locked-in architecture with minimum wasted effort:

### Step 1: Lock 3 Strategic Decisions (1–2 days of discussion)
1. **Pick the first wedge domain** to prove real-world usefulness first:
   - *Option A:* **2D Architectural / Spatial Plans** (rooms, walls with thickness, openings, clearances, dimensions → SVG + DXF).
   - *Option B:* **Electronic Schematics** (typed components/ports, nets, ERC rules, grouped layout → SVG + KiCad `.kicad_sch` validated by `kicad-cli`).
   - *Option C:* **2D Mechanical Parts / Sketches** (analytic profiles, holes, fillets, constraints → SVG + DXF, clean ramp to 3D/OCCT).
   *(Keep the core IR domain-neutral, but only build one domain package + adapter to production quality first.)*
2. **Pick the constraint tier for v0.1:** Start with **Tier 1 (Constructive DAG) + Tier 2 (Linear constraints via Cassowary)** before pulling in nonlinear C++ sketch solvers.
3. **Commit to the AI feedback loop:** Design the CLI/JSON diagnostics, IR query tool, and annotated SVG self-check view as first-class v0.1 features.

### Step 2: 1-Week Prior-Art & Syntax Bake-Off (Before Coding the Compiler)
- Spend 2–3 days inspecting **Penrose**, **Zoo KCL**, **atopile/JITX**, and **SysML v2** to borrow their best ideas and avoid their known traps.
- Hand-write **2 realistic benchmark designs** (e.g., a 3-room apartment with wall thicknesses + doors + clearance checks, and a 6-component buck-regulator schematic with ERC rules) in **2 competing syntaxes** (e.g., block-declarative like `RFC 001` vs. a tighter expression/relational syntax).
- Run an actual **LLM test**: give 3 different LLMs (without special fine-tuning) a 1-page grammar cheat sheet and ask them to *generate*, *explain*, and *modify* both benchmarks. Measure syntax errors, semantic mistakes, and edit reliability.

### Step 3: Build the Thin Vertical Slice (3–5 weeks)
Build the smallest end-to-end executable compiler in Rust:
- Lexer/parser + rich error diagnostics
- Dimensional + coordinate-frame type checker
- Typed canonical IR (serialized cleanly as JSON for inspection)
- Tier 1 DAG + Tier 2 Cassowary linear solver with explicit under-/over-constrained reports
- Pure-Rust 2D analytic primitives (points, segments, circles, arcs, rectangles, polygons; `cavalier_contours` if offsets/booleans are needed)
- Verification ledger (Layer 1 types + Layer 2 geometry validity + Layer 3 basic domain rules)
- Annotated SVG renderer + 1 real adapter (DXF or KiCad)

At the end of those 4–6 weeks, you won't be guessing about EDL anymore—you will have empirical proof of whether LLMs actually engineer dramatically better with EDL than without it, and clear data on which hard problems (nonlinear solving, auto-routing, 3D OCCT) are worth investing in next.
