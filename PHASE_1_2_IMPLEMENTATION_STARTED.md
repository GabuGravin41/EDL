# Phase 1.2: Constraint Propagation Engine - Implementation Plan

**Status:** ✅ **STARTED**  
**Date:** 2026-11-26  
**Duration:** 3-4 weeks (concurrent development)  
**Team:** 1-2 Rust engineers  
**Objective:** Enable constraint specification in EDL and automatic verification

---

## 🎯 Scope

### Work Items (Weekly Breakdown)

#### Week 1: Constraint Syntax & Parsing (Parser + Lexer)
- [ ] Design constraint syntax (EBNF grammar)
- [ ] Extend lexer with constraint tokens
- [ ] Implement parser for constraint declarations
- [ ] Handle units (V, A, Ohm, PSI, GPM, mm, in, ft)
- [ ] Error recovery and diagnostics
- [ ] 20+ parser unit tests

**Deliverable:** Parser can read constraint declarations from EDL

**Files:**
- `src/syntax/constraints_ast.rs` ✅ Created (7.1 KB)
- `src/syntax/lexer.rs` — Extend with constraint tokens
- `src/syntax/parser.rs` — Add constraint parsing functions

**Example:**
```edl
constraint voltage_vcc: mandatory {
    target: net:VCC
    rules: {
        operating_voltage: 3.3V
        margin: 5%
        rated_voltage: >= 3.47V
    }
    standard: "IEC 60950-1"
}
```

---

#### Week 2: AST Extension & Compilation Integration
- [ ] Add `ConstraintDecl` to `ProjectAst`
- [ ] Implement entity reference resolution
- [ ] Create elaboration pass for constraint injection
- [ ] Link constraints to IR entities
- [ ] Handle hierarchical scoping
- [ ] 15+ AST integration tests

**Deliverable:** Constraints from EDL parsed → AST → ready for compilation

**Files:**
- `src/syntax/ast.rs` — Extend ProjectAst
- `src/syntax/elaborator.rs` — Constraint elaboration pass
- `src/core/ir.rs` — Add constraints to IR entities

**Integration:**
```
EDL File
  ↓
Lexer → Parser
  ↓
ConstraintDecl AST nodes
  ↓
Elaborator (resolve entity references)
  ↓
Canonical IR (with ConstraintContext per entity)
```

---

#### Week 3: Verification Pass & Propagation
- [ ] Implement constraint extraction pass
- [ ] Cross-entity constraint propagation
  - Voltage: net voltage → IC pins
  - Current: net current → trace derating
  - Thermal: component dissipation → heatsink requirement
  - Flow: demand → pipe sizing
- [ ] Run domain verifiers on constraints
- [ ] Generate violations
- [ ] 20+ propagation tests

**Deliverable:** Constraint violations detected and repair suggestions generated

**Files:**
- `src/core/elaborator.rs` — Propagation engine
- `src/domains/*/compile.rs` — Constraint extraction
- `src/core/constraints.rs` — Extend propagation logic

**Example Flow:**
```
net:VCC (voltage: 3.3V)
  ↓
Propagate to: IC1.operating_voltage = 3.3V
  ↓
Verify E1: IC1.rated_voltage >= 3.47V?
  ↓
Violation: component rated 3.3V, needs 3.47V
  ↓
Repair: "Replace with 3.3V variant"
```

---

#### Week 4: Output Formatting & Testing
- [ ] Implement JSON violation report
- [ ] Implement Markdown summary
- [ ] CLI command integration
- [ ] Performance optimization
- [ ] Comprehensive integration tests
- [ ] Documentation

**Deliverable:** Complete constraint verification pipeline end-to-end

**Files:**
- `src/core/reporter.rs` — Violation reporting
- `src/cli/commands.rs` — Extend with constraint command
- `src/domains/*/render.rs` — Constraint output formatting

**Output Examples:**

JSON:
```json
{
  "constraints": [
    {
      "id": "elec_e1_voltage_vcc",
      "entity": "net_vcc",
      "status": "violated",
      "violation": {
        "reason": "IC rated 3.3V but requires 3.47V"
      },
      "repairs": [...]
    }
  ]
}
```

Markdown:
```markdown
## Constraint Violations

### net_vcc (elec.e1_voltage_derating)
**Severity:** Mandatory  
**Violation:** Component rated 3.3V but requires 3.47V (5% margin)  
**Standard:** IEC 60950-1

**Repairs:**
- Replace with 3.3V-rated component
```

---

## 📋 Specific Tasks

### Parser Implementation
**Task 1.2.1: Extend Lexer**
- [ ] Add `CONSTRAINT` keyword token
- [ ] Add severity tokens: `MANDATORY`, `STRONG`, `WEAK`
- [ ] Add target keywords: `NET`, `COMPONENT`, `PIPE`, `FIXTURE`, `GROUP`
- [ ] Add operators: `IN`, `RANGE`
- [ ] Add unit recognition: `V`, `A`, `Ohm`, `PSI`, `GPM`, `mm`, `in`, `ft`
- [ ] Add tolerance syntax: `±5%`, `+10%`, `-5%`
- [ ] Error tokens for malformed constraints

**Files:** `src/syntax/lexer.rs`

---

**Task 1.2.2: Implement Parser**
- [ ] `parse_constraint_declaration()` — Top-level constraint rule
- [ ] `parse_constraint_target()` — Parse entity reference
- [ ] `parse_constraint_rules()` — Parse rules block
- [ ] `parse_constraint_rule()` — Single rule (param: op value)
- [ ] `parse_entity_reference()` — net:X, component:X, etc.
- [ ] `parse_operator()` — ==, !=, <, <=, >, >=, in
- [ ] `parse_value()` — Number, string, range, formula
- [ ] `parse_unit()` — Electrical, volume, pressure units
- [ ] Error recovery and diagnostics

**Files:** `src/syntax/parser.rs`

**Expected Parser Coverage:**
- Declarative constraints (Phase 1.2 focus)
- Tolerance specification (3.3V ±5%)
- Range constraints (height in [17in, 19in])
- Future: Parametric constraints (ipc_2152_thermal_derating)
- Future: Conditional constraints (if power > 1W then heatsink)

---

**Task 1.2.3: AST Extension**
- [ ] Add `ConstraintDecl` to `ProjectAst`
- [ ] Add `constraints: Vec<ConstraintDecl>` field
- [ ] Implement `From<ConstraintRule>` conversions
- [ ] Span tracking for error reporting
- [ ] Serialization support (serde)

**Files:** `src/syntax/ast.rs`

---

### Compilation Pass Implementation

**Task 1.2.4: Entity Resolution**
- [ ] Link constraint targets to IR entities
- [ ] Handle hierarchy: net:PowerModule/VOUT → specific net
- [ ] Handle groups: apply constraint to all members
- [ ] Error handling for unresolved references
- [ ] 10+ resolution tests

**Files:** `src/syntax/elaborator.rs`

---

**Task 1.2.5: Constraint Injection**
- [ ] Extract constraints from AST
- [ ] Create `ConstraintContext` for each entity
- [ ] Inject into IR entities
- [ ] Handle parametric constraints
- [ ] Merge duplicate constraints
- [ ] 10+ injection tests

**Files:** `src/core/elaborator.rs`

---

**Task 1.2.6: Cross-Entity Propagation**
- [ ] **Voltage propagation:** Net voltage → connected components
- [ ] **Current propagation:** Net current → trace derating
- [ ] **Thermal propagation:** Dissipation → heatsink requirement
- [ ] **Flow propagation:** Demand → pipe sizing
- [ ] **Parametric propagation:** Use formulas (IPC tables)
- [ ] 20+ propagation rules
- [ ] 15+ propagation tests

**Files:** `src/core/propagator.rs` (new)

---

**Task 1.2.7: Verification Integration**
- [ ] Run domain verifiers on constrained entities
- [ ] Collect violations
- [ ] Generate repair suggestions
- [ ] Prioritize by severity (mandatory first)
- [ ] 20+ verification tests

**Files:** `src/domains/*/compile.rs`

---

### Output Formatting

**Task 1.2.8: Violation Reporting**
- [ ] JSON format with full constraint details
- [ ] Markdown report with repair suggestions
- [ ] CLI integration (`edl verify --constraints`)
- [ ] Summary statistics (violations by domain/severity)
- [ ] 10+ format tests

**Files:** `src/core/reporter.rs` (new)

---

**Task 1.2.9: Documentation & Examples**
- [ ] API documentation
- [ ] Usage examples (electronics, plumbing, electrical)
- [ ] EDL file examples with constraints
- [ ] Troubleshooting guide
- [ ] Performance tuning guide

**Files:** `docs/CONSTRAINT_GUIDE.md` (new)

---

## 📊 Testing Strategy

### Unit Tests (by component)
- **Lexer:** 10 tests (token recognition)
- **Parser:** 20 tests (all constraint syntax variations)
- **AST:** 15 tests (node construction, serialization)
- **Entity Resolution:** 10 tests (reference linking)
- **Propagation:** 20 tests (each propagation rule)
- **Verification:** 20 tests (each domain verifier)
- **Reporting:** 10 tests (output formats)

**Total:** 105+ unit tests

### Integration Tests
- **End-to-End:** EDL file → parsed → constrained → violations
- **Multi-Domain:** Electronics + plumbing in one design
- **Large Design:** 1000+ entities, verify in <1 second
- **Error Recovery:** Malformed constraints, missing entities
- **Performance:** Incremental verification, caching

**Total:** 15+ integration tests

### Expected Pass Rate: 100%

---

## 🎯 Success Criteria

**Parser/Syntax:**
- ✓ Parses 100% of proposed constraint syntax
- ✓ Clear error messages for malformed constraints
- ✓ Backward compatible (non-breaking change)

**Compilation:**
- ✓ All parsed constraints → IR entities
- ✓ No missed constraint targets
- ✓ Verification runs in <100ms for 100-entity designs

**Propagation:**
- ✓ Voltage propagates correctly to IC pins
- ✓ Current propagates to traces
- ✓ Hierarchical constraints apply to sub-entities

**Output:**
- ✓ All violations reported
- ✓ Repair suggestions are actionable
- ✓ AI can apply auto-fixable repairs

---

## 🧪 Example Test Case

**Test:** `test_constraint_propagation_voltage`

```rust
#[test]
fn test_constraint_propagation_voltage() {
    // Parse EDL with constraint
    let edl = r#"
        constraint voltage_vcc: mandatory {
            target: net:VCC
            rules: {
                operating_voltage: 3.3V
                margin: 5%
                rated_voltage: >= 3.47V
            }
            standard: "IEC 60950-1"
        }
    "#;
    
    let ast = parse_edl(edl);
    let ir = compile(ast);
    
    // Verify constraint was injected
    let net_vcc = ir.get_net("VCC").unwrap();
    assert!(!net_vcc.constraints.is_empty());
    
    // Verify constraint was propagated to IC pins
    let ic1 = ir.get_component("IC1").unwrap();
    assert_eq!(ic1.operating_voltage_v, Some(3.3));
    
    // Verify violation was detected
    let violations = ic1.violations.iter()
        .filter(|v| v.constraint_id == "e1_voltage_derating")
        .collect::<Vec<_>>();
    assert!(!violations.is_empty());
    
    // Verify repair suggestion was generated
    let repair = &violations[0].repair_suggestions[0];
    assert!(repair.description.contains("3.3V"));
}
```

---

## 📈 Effort Breakdown

| Task | Hours | Week | Complexity |
|------|-------|------|------------|
| 1.2.1: Extend Lexer | 8 | 1 | Low |
| 1.2.2: Implement Parser | 20 | 1 | Medium |
| 1.2.3: AST Extension | 12 | 1 | Low |
| 1.2.4: Entity Resolution | 12 | 2 | Medium |
| 1.2.5: Constraint Injection | 16 | 2 | Medium |
| 1.2.6: Propagation Engine | 24 | 3 | High |
| 1.2.7: Verification Integration | 12 | 3 | Medium |
| 1.2.8: Violation Reporting | 16 | 4 | Medium |
| 1.2.9: Documentation | 12 | 4 | Low |
| Testing + Debugging | 32 | Throughout | Medium |
| **Total** | **164 hours** | **4 weeks** | — |

---

## 🚀 Parallelization Opportunities

- Week 1: Lexer/Parser can be done in parallel with Phase 1.3 (electrical verifier)
- Week 2: AST extension independent of Phase 1.3
- Week 3: Propagation rules can be developed by domain experts in parallel
- Week 4: Output formatting independent of other work

**Recommendation:** Assign 1-2 engineers full-time, with 1 domain expert part-time for propagation rules.

---

## 🔄 Dependencies

### Internal Dependencies
- Phase 1.1 (Abstract Constraint Framework) ✅ **COMPLETE**
- Domain verifiers (E1-E4, P1-P5) ✅ **COMPLETE**

### External Dependencies
- None (pure Rust implementation)

### Blocking Issues
- None identified

---

## 🎓 Key Concepts for Phase 1.2

**Constraint Syntax:** Declarative (annotation-style in EDL)
**Entity Resolution:** Reference nets, components, pipes, fixtures by name
**Propagation:** Automatic spread of constraint values across hierarchy
**Verification:** Run domain verifiers on constrained entities
**Repair Suggestions:** Auto-generated based on violations

---

## 📞 Decisions Needed

1. **Tolerance Syntax:** Use ±5% or ±(5%)? (recommend ±5%)
2. **Range Syntax:** Use [min, max] or in min..max? (recommend [min, max])
3. **Parametric Formulas:** Hardcoded or extensible? (recommend extensible plugin system)
4. **Performance Target:** <100ms, <500ms, or <1s for 1000-entity design? (recommend <100ms)

---

## 📅 Timeline

- **Week 1:** Lexer + Parser (ready for Monday end-of-day)
- **Week 2:** AST + Compilation (ready for Friday)
- **Week 3:** Propagation engine (ready for Friday)
- **Week 4:** Output + Testing (ready for Friday)
- **Week 5:** Buffer for integration and bug fixes

---

## ✅ Sign-Off

**Phase 1.2 Ready to Start:** ✅ Yes
- Phase 1.1 foundation is solid
- Syntax is well-specified
- Parser strategy is clear
- Testing strategy is defined

**Next Step:** Assign engineer and begin Week 1 work

---

**Version:** 1.0  
**Status:** ✅ **Ready for Implementation**  
**Start Date:** TBD  
**Estimated Completion:** 4-5 weeks from start  
