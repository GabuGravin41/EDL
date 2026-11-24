# EDL Syntax Bake-Off — Empirical Scorecard

**Setup:** 6 blind, isolated LLM runs (`flash` and `pro` × 3 tracks), each given only `TASK_SPEC.md` and a 1-page cheat sheet.
- **Track A:** Block-Declarative EDL (`cheat_sheet_A.md`)
- **Track B:** Relational / Flat EDL (`cheat_sheet_B.md`)
- **Track C (Control):** Pure Python Builder (`cheat_sheet_C_python.md`)

---

## 1. Side-by-Side Results

| Metric | Track A — Block EDL (`flash` / `pro`) | Track B — Flat EDL (`flash` / `pro`) | Track C — Python Baseline (`flash` / `pro`) |
|---|---|---|---|
| **Task 1: First-Try Syntax Validity** | **100% / 100%** | **100% / 100%** | **100% / 100%** |
| **Task 1: Source Length (`Apartment_B`)** | **78 lines** (~1.4 KB) | **34 lines** (~1.3 KB) | **123 lines** (~4.2 KB) / **237 lines** (~7.5 KB) |
| **Task 1: Geometry & Verification Integrity** | Exact intent; zero manual wall/arc math | Exact intent; zero manual wall/arc math | **`flash` approximated door swings as bboxes** and skipped wall checks; **`pro` wrote 140 LOC of custom sweep-line math**; both manually computed all 7 wall rects |
| **Task 1: Derived `Bath` Constraints** | 4 clean `require` lines (order-independent) | 4 clean `require` lines (order-independent) | Order-dependent imperative assignments; fails on coupled constraints |
| **Task 2: Design Explanation (3 Qs)** | **3/3 / 3/3** exact | **3/3 / 3/3** exact | **3/3 / 3/3** exact (`pro` explicitly flagged hardcoded `4500` sync fragility) |
| **Task 3: Surgical Edit (Lines Touched)** | **2 lines / 2 lines** (0 shadow edits) | **2 lines / 2 lines** (0 shadow edits) | **3–4 lines / 3–4 lines** (had to hunt down hardcoded `seg_len` & `offset` copies) |
| **Task 4: Bug Diagnosis (incl. non-collision trap)** | **Exact / Exact** (`40mm`, `50mm`, `210mm` clear) | **Exact / Exact** (`40mm`, `50mm`, `210mm` clear) | **Exact / Exact** |
| **Task 4: Minimal Repair Discipline** | **Clean / Clean** (kept `hinge` & `swing`) | **Clean / Clean** (kept `hinge` & `swing`) | **`flash` accidentally flipped `swing` back to `"Kitchen"`**; `pro` clean |

---

## 2. Key Empirical Conclusions

### Finding 1 — The Core EDL Thesis Passed Decisively Against Python
1. **Zero DSL Cold-Start Failure:** Both `flash` and `pro` hit **100% first-try grammar accuracy** on both custom syntaxes from a single 1-page cheat sheet.
2. **3×–7× Less Code & Zero Verification Drift:** In Python, every LLM run invents its own ad-hoc geometry/verification logic (`flash` took a bounding-box shortcut; `pro` wrote 140 lines of sweep-line math) and manually computes wall coordinates. In EDL, exact wall derivation and exact analytical arc-collision checks live **once** in the deterministic compiler.
3. **Surgical Edit Safety:** Widening `Living` in EDL was a single-token edit (`4500mm → 5200mm`). In Python, it forced chasing hardcoded `seg_len` and `offset` shadows across unrelated openings—the exact silent-drift bug EDL is built to eliminate.

### Finding 2 — Track A (Block-Declarative) vs. Track B (Relational/Flat)
Both EDL syntaxes scored **100%** across all 4 tasks on both `flash` and `pro`. The real tradeoff is purely structural:
- **Track B** is ~56% fewer lines (**34 lines** vs. **78 lines**) and very snappy for tiny 1-floor sketches, *but* single-line `let D_Entry = door(...)` statements grow wide (~105+ chars) as properties are added, and flat top-level statements will need scoping containers anyway once projects have multiple floors, modules, or views.
- **Track A** scales cleanly to real projects (explicit `project`, `layout`, `verify`, `view` scopes; clear multi-line entity blocks ideal for span-based AI editing and future formatting/diffing), and is only ~10% more characters than Track B (~1.4 KB vs ~1.3 KB).

### Recommendation
**Lock Candidate A (Block-Declarative)** as the v0.1 syntax (keeping the concise `require ... == ...` constraint style and `shared_wall(A, B)` / `exterior(A, side)` selectors that achieved 100% first-try accuracy across both models).
