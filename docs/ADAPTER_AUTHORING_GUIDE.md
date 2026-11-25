# EDL Host-Tool Adapter & Domain Plugin Authoring Guide

EDL is built around two clean extension seams so open-source contributors never have to fork the compiler core:

1. **Host-Tool Adapters** — bridge EDL's `CanonicalIr` and `edl sync` into native visual CAD/EDA tools (**KiCad, AutoCAD, FreeCAD, Onshape, Revit, Fusion 360**).
2. **Domain Engine Plugins (`DomainEngine` trait)** — add brand-new engineering disciplines (**plumbing, HVAC, structural steel/RC, PCB constraints, mechanical assemblies**) with zero edits to the lexer, parser, or IR schema.

---

## Part 1 — Building a Bidirectional Host-Tool Adapter

A native host-tool adapter is intentionally **thin** (usually 200–400 lines in Python, C#, or Rust). It follows a 4-step contract:

```text
  .edl (source of truth)
     │
     ├─► 1. Compile (`edl build --json` or `edl export`)
     │       └─► Tag every emitted host entity with its stable `EDL_ID`
     │
     ├─► 2. Human edits visually in native tool (KiCad / AutoCAD / FreeCAD / Revit)
     │
     ├─► 3. Extract diff against `CanonicalIr` (match by `EDL_ID`, never by geometry guess!)
     │
     └─► 4. Emit surgical `.edl` patch (verified by re-compiling before writing!)
             └─► Unrepresentable edits become `report_only` / `unsupported` with a clear reason
```

### Rule #1: Always Embed Stable `EDL_ID` Metadata

Never rely on entity order, layer names, or display labels to match a host object back to EDL. Every exported object must carry its immutable `EntityIr.id`:

| Host Tool | Native Storage Mechanism for `EDL_ID` | Reference Implementation |
|---|---|---|
| **KiCad 9 (`.kicad_sch`)** | Hidden `(property "EDL_ID" "<id>" ... (hide yes))` on each placed `(symbol ...)` | [`src/domains/elec/kicad.rs`](../src/domains/elec/kicad.rs) |
| **AutoCAD / BricsCAD / LibreCAD (`.dxf`)** | Registered `APPID` (`EDL`) + `EDL_ANCHORS` `POINT` entity with XDATA `1001 EDL / 1000 <id> / 1000 <kind>` | [`src/domains/arch/dxf.rs`](../src/domains/arch/dxf.rs) |
| **FreeCAD (`.FCStd` / Python macro)** | Custom App property `obj.addProperty("App::PropertyString", "EDL_ID", "EDL")` on every `Part::Feature` / `Arch` object | *(Adapter pattern below)* |
| **Revit (C# Add-in / pyRevit)** | `ExtensibleStorage` schema (`EDL_ID`, `EDL_KIND`) or shared project parameter `EDL_ID` on `Wall`, `Room`, `FamilyInstance` | *(Adapter pattern below)* |
| **Onshape / Fusion 360** | FeatureScript `setAttribute(..., "EDL", { "id": ent.id })` / Fusion `attributes.add("EDL", "id", ent.id)` | *(Adapter pattern below)* |

### Rule #2: Respect Semantic Intent on Sync-Back

When a human moves or edits an object in a native GUI, classify the change honestly into three buckets (just like [`src/core/sync.rs`](../src/core/sync.rs)):

1. **`patch`** — has an exact, lossless EDL source representation:
   - Schematic component drag → `place <ID> at: (<x>mm, <y>mm)` inside `schematic { ... }`
   - Component value / footprint edit (non-module entity) → surgical line edit on `value:` / `footprint:`
   - Freestanding fixture move → surgical update of `at: (<x>mm, <y>mm)` relative to the host room's origin
2. **`report_only`** — valid visual drift, but the EDL entity is driven by a higher-level constraint:
   - Moving a constraint-solved `room` outline or a wall-anchored (`against: west`) fixture. Report the measured `(dx, dy)` drift to the human/AI rather than destroying the parametric constraint!
3. **`unsupported`** — cannot be safely patched on the expanded entity:
   - Editing a component owned by a `module` instance (`EntityIr.provenance` is `Some(...)`). Point the user/AI to the parent `use` parameter instead.

### Rule #3: Verify Before Writing

Never overwrite the user's `.edl` file blindly. Follow `apply_plan_to_file` in [`src/core/sync.rs`](../src/core/sync.rs):
1. Apply the minimal text patch in memory (preserving comments, formatting, and unrelated declarations).
2. Compile the patched source (resolving relative `import`s against the original file's directory).
3. **Reject and leave the file untouched** if the patch introduces syntax errors or *new* verification violations.

### Example: Minimal FreeCAD Python Adapter Blueprint

```python
import json, subprocess, FreeCAD, Part

# 1. Compile EDL -> Canonical IR JSON
ir = json.loads(subprocess.check_output(["edl", "build", "plan.edl", "--json"]))
doc = FreeCAD.newDocument(ir["project"]["name"])

# 2. Extrude exact wall solids from IR (2700 mm floor-to-ceiling)
for idx, cell in enumerate(ir["wall_solids"]):
    r = cell["rect"]
    box = Part.makeBox(r["max_x"] - r["min_x"], r["max_y"] - r["min_y"], 2700.0,
                       FreeCAD.Vector(r["min_x"], r["min_y"], 0.0))
    obj = doc.addObject("Part::Feature", f"Wall_{idx}")
    obj.Shape = box
    obj.addProperty("App::PropertyString", "EDL_ID", "EDL").EDL_ID = f"wall_{idx}"

# 3. Place fixtures with EDL_ID tags for round-trip tracking
for ent in ir["entities"]:
    g = ent["geometry"]
    if g["type"] == "fixture":
        r = g["body_rect"]
        obj = doc.addObject("Part::Feature", ent["id"])
        obj.Shape = Part.makeBox(r["max_x"] - r["min_x"], r["max_y"] - r["min_y"], 850.0,
                                 FreeCAD.Vector(r["min_x"], r["min_y"], 0.0))
        obj.addProperty("App::PropertyString", "EDL_ID", "EDL").EDL_ID = ent["id"]

doc.recompute()
```

### Live Two-Way Co-Editing Loop (`edl watch`)

For rapid human + AI collaboration without writing a native plugin first, run:

```bash
edl watch examples/mcu_board_3v3.edl --port 8765 --kicad out/live.kicad_sch
```

- Open `out/live.kicad_sch` in KiCad and `http://127.0.0.1:8765` in a browser.
- When the AI edits `mcu_board_3v3.edl` (directly or via `edl mcp`), `edl watch` recompiles, hot-reloads the browser SVG, and re-exports `out/live.kicad_sch`.
- When the human drags components or changes values in KiCad and hits **Save**, `edl watch` runs `edl sync --apply`, patches the `.edl` source cleanly, and refreshes the live preview.

---

## Part 2 — Adding a New Engineering Domain (`DomainEngine`)

You can add a complete new engineering domain (e.g., `plumbing`, `hvac`, `structural`, `pcb`) as an **Overlay** or **Base** plugin **without touching EDL's lexer, parser, or IR enum**.

### Step 1: Implement `DomainEngine`

See the working end-to-end reference test in [`tests/plugin_domain.rs`](../tests/plugin_domain.rs) (`PlumbingDomain`).

```rust
use edl::core::ir::{CanonicalIr, EntityGeometryIr, EntityIr, VerificationStatus};
use edl::domains::registry::{
    push_evidence, DomainEngine, DomainRole, ExportFormat, RuleInfo, ViewTransform,
};
use edl::syntax::ast::ProjectAst;

pub struct PlumbingDomain;

impl DomainEngine for PlumbingDomain {
    fn id(&self) -> &'static str { "plumbing" }
    fn description(&self) -> &'static str { "Domestic water supply routing & sizing" }
    fn role(&self) -> DomainRole { DomainRole::Overlay } // Layers on top of `arch`
    fn keywords(&self) -> &'static [&'static str] { &["plumbing"] }

    fn activates(&self, ast: &ProjectAst) -> bool {
        ast.extensions.iter().any(|b| b.keyword == "plumbing")
    }

    fn compile(&self, ast: &ProjectAst, ir: Option<CanonicalIr>) -> CanonicalIr {
        let mut ir = ir.expect("overlay requires solved base IR");
        for block in ast.extensions.iter().filter(|b| b.keyword == "plumbing") {
            let tree = block.parse_tree().unwrap(); // Generic typed AST with unit helpers!
            // 1. Read solved rooms/fixtures from `ir.entities` (never touch another domain's AST)
            // 2. Push `EntityIr` with `EntityGeometryIr::Custom { shape, data }`
            //    (include `"bbox": [min_x, min_y, max_x, max_y]` so `edl query` & `edl measure` work!)
            // 3. Record honest verification evidence with `push_evidence(&mut ir, ...)`
        }
        ir
    }

    fn render_overlay(&self, ir: &CanonicalIr, view: &ViewTransform) -> Option<String> {
        // Draw SVG elements using `view.map(x_mm, y_mm)` — automatically aligned with the floor plan!
        None
    }

    fn exports(&self) -> Vec<ExportFormat> {
        vec![ExportFormat { name: "pipes", extension: "csv", description: "Pipe run schedule" }]
    }

    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        (format == "pipes").then(|| "...".to_string())
    }

    fn rules(&self) -> Vec<RuleInfo> {
        vec![RuleInfo { id: "PL1_max_pipe_run", description: "Pipe runs within max length" }]
    }
}
```

### Step 2: Register and Compile

```rust
let mut registry = edl::domains::registry::DomainRegistry::builtin();
registry.register(Box::new(PlumbingDomain));

let ir = edl::compile_file_with(&registry, Path::new("apartment.edl"))?;
let svg = registry.render_svg(&ir, false);        // Base floor plan + mep.elec + plumbing overlay!
let csv = registry.export("pipes", &ir)?;         // Domain-specific export
```

### Generic Extension Block Grammar (`src/syntax/ext.rs`)

Any unknown top-level `<keyword> [<Name>] { ... }` inside a `project` automatically parses into `ExtNode` via `block.parse_tree()`:
- **Properties:** `key: value` where `value` can be a typed quantity (`9m`, `-50mm`), identifier or dotted ref (`Bath1.north`, `U1.OUT`), string (`"Main trunk"`), tuple `(6800mm, 300mm)`, or list `[A, B, C]`.
- **Nested child blocks:** `kind [Name] [: Subtype] { ... }` (e.g., `feed F_Shower: cold { fixture: Bath1_Shower }`).
- **Helpers:** `node.mm("max_run")`, `node.num("supply")`, `node.ident("fixture")`, `node.children_of("feed")`.
