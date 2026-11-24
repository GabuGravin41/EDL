# EDL v0.4 Language & Compiler Reference

EDL v0.4 builds on the v0.3 Architecture (`arch`) + Electronics (`elec`) foundation with:
1. **Clean Core / Syntax / Domain Separation** inside the Rust compiler (`src/core/`, `src/syntax/`, `src/domains/{arch,elec}/`).
2. **Cross-Domain Composition & Reuse**: file `import "..."`, parameterized `module Name(p1 = def1, ...) { ... }`, electronics `bind PORT -> Comp.Pin` (with automatic internal-net merging), and `use Inst = Name(arg: val)` instantiation across **both** Architecture and Electronics, preserving `provenance: { module, instance }` on every elaborated Canonical IR entity.
3. **Layered Schematic Control (v0.3.2+)**: per-schematic `routing { default | power_nets | ground_nets | net <Name>: direct | bus | rail | symbol | label_stub }`, per-pin `side: north|south|east|west`, and explicit `place <Comp> at: (x, y)` overrides.

---

## 1. Compiler Architecture (`src/`)

```text
src/
├── core/               # Domain-neutral kernel
│   ├── span.rs         # Source spans & structured diagnostics
│   ├── units.rs        # Typed SI units & dimensional analysis (length, V, A, ohm, F, Hz, W)
│   ├── geom.rs         # Analytical 2D geometry (Point2, Rect2, Segment2, Arc2, QuarterDisk2, Side)
│   ├── ir.rs           # Canonical IR (entities with ModuleProvenanceIr, relations, evidence)
│   └── diff.rs         # Semantic diff by stable entity ID & geometry
├── syntax/             # Unified frontend & elaboration
│   ├── lexer.rs        # Tokenizer (quantities, strings, arrows ->, operators)
│   ├── ast.rs          # Core + file-level AST (ImportDecl, ModuleDecl, UseDecl, ProjectAst)
│   ├── parser.rs       # Recursive-descent parser with module parameter substitution
│   └── elaborate.rs    # Recursive import loader + arch/elec module expansion + net merging
└── domains/            # Self-contained engineering domains
    ├── arch/           # ast, solver (RREF), compile (CSG walls + 6 rules), svg, dxf
    └── elec/           # ast, compile (5-rule ERC + multi-style router), svg, kicad (KiCad 9)
```

---

## 2. File Imports (`import`)

Library `.edl` files declare reusable `module` blocks (and may themselves `import` other libraries; diamond and cyclic imports are safely deduplicated via canonical paths).

```edl
import "./lib/elec_blocks.edl"
import "./lib/arch_units.edl"
```

Paths resolve relative to the importing `.edl` file. Local `module` declarations in the project file can extend or override imported modules.

---

## 3. Parameterized Modules (`module`, `bind`, `use`)

### Electronics Modules (`elec`)

Define reusable subcircuits with optional default parameters and explicit external ports (`bind PORT -> Comp.Pin`):

```edl
module DecoupledLDO(
  vin_min = 4.5V,
  vin_max = 12V,
  vout = 3.3V,
  part = "AMS1117-3.3",
  cap = 10uF
) {
  component U: regulator {
    value: part
    pin IN: power_in (vin_min..vin_max) side: west
    pin OUT: power_out (vout) side: east
    pin GND: ground (0V) side: south
  }
  component C_IN: capacitor  { value: cap  pin POS: passive side: north  pin NEG: ground (0V) side: south }
  component C_OUT: capacitor { value: cap  pin POS: passive side: north  pin NEG: ground (0V) side: south }

  net _IN  { connect: [U.IN, C_IN.POS] }
  net _OUT { connect: [U.OUT, C_OUT.POS] }
  net _GND { connect: [U.GND, C_IN.NEG, C_OUT.NEG] }

  bind IN  -> U.IN
  bind OUT -> U.OUT
  bind GND -> U.GND
}
```

Instantiate inside any `circuit` block and connect to its ports:

```edl
use Reg3V3 = DecoupledLDO(vout: 3.3V, part: "AMS1117-3.3", cap: 10uF)

net VBUS { connect: [J_USB.VBUS, Reg3V3.IN] }
net V3V3 { connect: [Reg3V3.OUT, U_MCU.VDD] }
net GND  { connect: [J_USB.GND, Reg3V3.GND, U_MCU.GND] }
```

**Elaboration semantics:**
- Components are deterministically named `<Inst>_<Comp>` (e.g. `Reg3V3_U`, `Reg3V3_C_IN`) and tagged with `provenance: { module: "DecoupledLDO", instance: "Reg3V3" }` in Canonical IR.
- **Internal net absorption:** If `bind PORT -> Comp.Pin` targets a pin belonging to an internal module net (such as `_GND`), connecting a parent net to `Inst.PORT` merges all pins of that internal net into the parent net and drops the redundant internal net.
- **Purely internal nets** (e.g. an R-to-LED midpoint `net _MID { connect: [R.P2, D.A] }`) stay scoped as `<Inst>__MID` and are routed and ERC-checked normally.
- **Schematic group expansion:** Writing `group Regulator { members: [Reg3V3] }` automatically expands `Reg3V3` to all components instantiated by `Reg3V3` (or you can target individual sub-components via `Reg3V3.U` / `place Reg3V3.U at: (x, y)`).

### Architecture Modules (`arch`)

Define reusable rooms or room clusters with their windows, doors, internal layouts, and fixtures:

```edl
module StandardBedroom(w = 3400mm, d = 3200mm, bed_w = 1600mm, win_w = 1400mm) {
  room Bed {
    role: habitable
    clear_size: (w, d)
  }
  opening Win: window {
    host: exterior(Bed, north)
    width: win_w
    align: center
  }
  fixture BedUnit: bed {
    room: Bed
    size: (bed_w, 2000mm)
    against: west
    offset: 150mm from end
    clearance_front: 500mm
  }
}
```

Instantiate inside `project`:

```edl
use Bed1 = StandardBedroom(w: 3600mm, d: 3400mm, bed_w: 1600mm, win_w: 1600mm)
use Bed2 = StandardBedroom(w: 3200mm, d: 3400mm, bed_w: 1200mm, win_w: 1400mm)
```

**Elaboration semantics:**
- **Single-room modules** bind the room directly to the instance name (`Bed1`, `Bed2`), so parent `layout` constraints and `opening` hosts simply reference `Bed1.west`, `shared_wall(Living, Bed1)`, etc.
- **Multi-room modules** bind sub-rooms to `<Inst>_<SubRoom>`, accessible in parent layouts and openings via dot notation `Inst.SubRoom.west` and `shared_wall(Hall, Inst.SubRoom)`.
- Internal openings and fixtures are named `<Inst>_<Id>` and tagged with `provenance: { module, instance }` in Canonical IR.

---

## 4. Reference Examples

- `examples/lib/elec_blocks.edl` + `examples/dual_rail_modular.edl` — Modular 5V → 3.3V MCU + dual LED board (0 EDL ERC errors, **0 KiCad 9 ERC errors**).
- `examples/lib/arch_units.edl` + `examples/duplex_modular.edl` — Modular 5-room 2BR apartment composed from `StandardBath` + two parameterized `StandardBedroom` instances (0 DOF, 7/7 architectural checks `EXACTLY_CHECKED`).
- `examples/mcu_board_3v3_symbols.edl` — Layered schematic routing (`direct` + `symbol` + `label_stub` + `side:` overrides).
