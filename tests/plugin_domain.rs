//! v0.9: prove that a brand-new domain (plumbing) plugs in through the public `DomainEngine` trait
//! with zero changes to the parser, lexer, IR schema or renderers.

use edl::core::ir::{CanonicalIr, EntityGeometryIr, EntityIr, VerificationStatus};
use edl::core::span::Diagnostic;
use edl::domains::registry::{
    push_evidence, DomainEngine, DomainRegistry, DomainRole, ExportFormat, RuleInfo, ViewTransform,
};
use edl::syntax::ast::ProjectAst;
use edl::{compile_file_with, core::span::Diagnostic as Diag};
use serde_json::json;
use std::collections::BTreeMap;

/// Overlay domain: `plumbing Name { source: (x, y)  max_run: 6m   feed F1: cold { fixture: Bath1_Shower } ... }`
struct PlumbingDomain;

impl PlumbingDomain {
    fn feeds(ir: &CanonicalIr) -> Vec<&EntityIr> {
        ir.entities.iter().filter(|e| e.kind == "plumbing.pipe").collect()
    }
}

impl DomainEngine for PlumbingDomain {
    fn id(&self) -> &'static str {
        "plumbing"
    }
    fn description(&self) -> &'static str {
        "Water supply feeds routed to fixtures (test plugin)"
    }
    fn role(&self) -> DomainRole {
        DomainRole::Overlay
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["plumbing"]
    }
    fn activates(&self, ast: &ProjectAst) -> bool {
        ast.extensions.iter().any(|b| b.keyword == "plumbing")
    }
    fn compile(&self, ast: &ProjectAst, ir: Option<CanonicalIr>) -> CanonicalIr {
        let mut ir = ir.unwrap();
        for block in ast.extensions.iter().filter(|b| b.keyword == "plumbing") {
            let tree = match block.parse_tree() {
                Ok(t) => t,
                Err(d) => {
                    ir.diagnostics.push(d);
                    ir.project.has_errors = true;
                    continue;
                }
            };
            let src = match tree.prop("source") {
                Some(edl::syntax::ext::ExtValue::Tuple(v)) if v.len() == 2 => {
                    (v[0].as_mm().unwrap(), v[1].as_mm().unwrap())
                }
                _ => {
                    ir.diagnostics.push(Diagnostic::error(
                        "E_PLUMB_SOURCE",
                        "plumbing",
                        "plumbing block needs `source: (x, y)`",
                        Some(tree.span.clone()),
                    ));
                    ir.project.has_errors = true;
                    continue;
                }
            };
            let max_run = tree.mm("max_run").unwrap_or(f64::INFINITY);
            let mut worst = 0.0f64;
            let mut over = Vec::new();
            for feed in tree.children_of("feed") {
                let fid = feed.name.clone().unwrap();
                let target = feed.ident("fixture").unwrap().to_string();
                // Reads solved base geometry from the IR — never from another domain's AST.
                let Some(fx) = ir.entities.iter().find(|e| e.id == target) else {
                    ir.diagnostics.push(Diagnostic::error(
                        "E_PLUMB_FIXTURE",
                        "plumbing",
                        format!("feed '{fid}': unknown fixture '{target}'"),
                        Some(feed.span.clone()),
                    ));
                    ir.project.has_errors = true;
                    continue;
                };
                let EntityGeometryIr::Fixture { body_rect, .. } = &fx.geometry else { continue };
                let c = body_rect.center();
                let elbow = (c.x, src.1);
                let len = (c.x - src.0).abs() + (c.y - src.1).abs();
                worst = worst.max(len);
                if len > max_run {
                    over.push(fid.clone());
                }
                ir.entities.push(EntityIr {
                    id: fid,
                    kind: "plumbing.pipe".into(),
                    frame: "Plan".into(),
                    span: Some(feed.span.clone()),
                    provenance: None,
                    props: BTreeMap::from([("run_mm".to_string(), json!(len.round()))]),
                    geometry: EntityGeometryIr::Custom {
                        shape: "polyline".into(),
                        data: json!({
                            "path": [[src.0, src.1], [elbow.0, elbow.1], [c.x, c.y]],
                            "bbox": [src.0.min(c.x), src.1.min(c.y), src.0.max(c.x), src.1.max(c.y)],
                            "subtype": feed.subtype,
                        }),
                    },
                });
            }
            push_evidence(
                &mut ir,
                "PL1_max_pipe_run",
                if over.is_empty() { VerificationStatus::ExactlyChecked } else { VerificationStatus::Violated },
                over.clone(),
                format!("worst feed run {:.0} mm (limit {:.0} mm)", worst, max_run),
            );
            if !over.is_empty() {
                ir.project.has_errors = true;
            }
        }
        ir
    }
    fn render_overlay(&self, ir: &CanonicalIr, view: &ViewTransform) -> Option<String> {
        let mut out = String::new();
        for e in Self::feeds(ir) {
            let EntityGeometryIr::Custom { data, .. } = &e.geometry else { continue };
            let pts: Vec<String> = data["path"]
                .as_array()?
                .iter()
                .map(|p| {
                    let (x, y) = view.map(p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
                    format!("{x:.1},{y:.1}")
                })
                .collect();
            out.push_str(&format!(
                "<polyline class=\"plumbing-pipe\" fill=\"none\" stroke=\"#0ea5e9\" stroke-width=\"2\" points=\"{}\"/>",
                pts.join(" ")
            ));
        }
        (!out.is_empty()).then_some(out)
    }
    fn exports(&self) -> Vec<ExportFormat> {
        vec![ExportFormat { name: "pipes", extension: "csv", description: "Pipe run list" }]
    }
    fn export(&self, format: &str, ir: &CanonicalIr) -> Option<String> {
        (format == "pipes").then(|| {
            let mut s = String::from("pipe,run_mm\n");
            for e in Self::feeds(ir) {
                s.push_str(&format!("{},{}\n", e.id, e.props["run_mm"]));
            }
            s
        })
    }
    fn rules(&self) -> Vec<RuleInfo> {
        vec![RuleInfo { id: "PL1_max_pipe_run", description: "Feeds within max run" }]
    }
}

fn project_with_plumbing(max_run: &str) -> String {
    let base = std::fs::read_to_string("examples/apartment_2br_electrical.edl").unwrap();
    let end = base.rfind('}').unwrap();
    format!(
        "{}\n  plumbing Water {{\n    source: (6800mm, 300mm)\n    max_run: {max_run}\n    feed F_Shower: cold {{ fixture: Bath1_Shower }}\n    feed F_Vanity: hot {{ fixture: Bath1_Vanity }}\n  }}\n}}\n",
        &base[..end]
    )
}

/// Imports (`./lib/...`) need a base directory, so write a throw-away file next to the examples.
fn compile_in_examples(r: &DomainRegistry, name: &str, src: &str) -> Result<CanonicalIr, Diag> {
    let p = std::path::PathBuf::from(format!("examples/.tmp_plugin_{name}_{}.edl", std::process::id()));
    std::fs::write(&p, src).unwrap();
    let res = compile_file_with(r, &p);
    let _ = std::fs::remove_file(&p);
    res
}

fn registry() -> DomainRegistry {
    let mut r = DomainRegistry::builtin();
    r.register(Box::new(PlumbingDomain));
    r
}

#[test]
fn plugin_domain_layers_on_arch_and_mep_with_evidence_overlay_and_export() {
    let src = project_with_plumbing("9m");
    let ir = compile_in_examples(&registry(), "a", &src).unwrap();
    assert!(!ir.project.has_errors, "{:?} {:?}", ir.diagnostics, ir.evidence);
    // One shared IR: arch rooms + mep.elec devices + plumbing pipes.
    assert!(ir.entities.iter().any(|e| e.kind == "arch.room"));
    assert!(ir.entities.iter().any(|e| matches!(e.geometry, EntityGeometryIr::MepDevice { .. })));
    assert_eq!(ir.entities.iter().filter(|e| e.kind == "plumbing.pipe").count(), 2);
    let ev = ir.evidence.iter().find(|e| e.check_id == "PL1_max_pipe_run").unwrap();
    assert_eq!(ev.status, VerificationStatus::ExactlyChecked);
    assert!(ir.evidence.iter().filter(|e| e.check_id.starts_with('M')).count() == 6);

    // Generic tooling works on plugin entities (bbox via `Custom { data.bbox }`).
    let q = edl::core::query::query_ir(&ir, "entity", Some("F_Shower")).unwrap();
    assert!(q.to_string().contains("plumbing.pipe"));

    // Overlay is drawn through the host's coordinate mapping.
    let svg = registry().render_svg(&ir, false);
    assert!(svg.contains("id=\"overlay-plumbing\"") && svg.contains("plumbing-pipe"));
    assert!(svg.find("overlay-plumbing").unwrap() < svg.rfind("</svg>").unwrap());

    // Plugin export + registry introspection.
    assert!(registry().export("pipes", &ir).unwrap().starts_with("pipe,run_mm\nF_Shower,"));
    assert!(registry().export("dxf", &ir).unwrap().contains("EDL_ANCHORS"));
    let info = registry().info();
    assert_eq!(info.iter().map(|d| d.id).collect::<Vec<_>>(), ["arch", "elec", "mep.elec", "plumbing"]);
}

#[test]
fn plugin_rule_failures_and_diagnostics_flow_through_the_same_ledger() {
    let ir = compile_in_examples(&registry(), "b", &project_with_plumbing("2m")).unwrap();
    assert!(ir.project.has_errors);
    let ev = ir.evidence.iter().find(|e| e.check_id == "PL1_max_pipe_run").unwrap();
    assert_eq!(ev.status, VerificationStatus::Violated);
    assert!(!ev.subjects.is_empty() && ev.subjects.len() <= 2);

    // Malformed plugin syntax yields a spanned diagnostic, not a panic.
    let bad = project_with_plumbing("9m").replace("source: (6800mm, 300mm)", "src: 3");
    let ir = compile_in_examples(&registry(), "c", &bad).unwrap();
    assert!(ir.diagnostics.iter().any(|d| d.code == "E_PLUMB_SOURCE" && d.span.is_some()));
}

#[test]
fn unclaimed_extension_keywords_are_reported_with_guidance_by_the_builtin_registry() {
    // Same source, built-in registry only: `plumbing` is not claimed by anyone.
    let ir = compile_in_examples(&DomainRegistry::builtin(), "d", &project_with_plumbing("9m")).unwrap();
    assert!(ir.project.has_errors);
    let d = ir.diagnostics.iter().find(|d| d.code == "E_UNKNOWN_DOMAIN_BLOCK").unwrap();
    assert!(d.message.contains("plumbing"));
    assert!(d.suggestion.as_ref().unwrap().contains("building_electrical"));
    // The rest of the project still compiled and verified.
    assert!(ir.evidence.iter().filter(|e| e.check_id.starts_with('M')).count() == 6);
}

#[test]
fn generic_block_grammar_parses_nested_blocks_lists_tuples_and_dotted_refs() {
    let src = r#"project "P" {
      units: mm
      hvac Main {
        supply: 1200
        duct D1: round {
          size: (300mm, 200mm)
          route: [A.out, B.in, C.in]
          label: "Main trunk"
          offset: -50mm
        }
      }
    }"#;
    let ast = edl::syntax::parser::parse_project("t.edl", src).unwrap();
    let blk = &ast.extensions[0];
    assert_eq!((blk.keyword.as_str(), blk.name().as_deref()), ("hvac", Some("Main")));
    let t = blk.parse_tree().unwrap();
    assert_eq!(t.num("supply"), Some(1200.0));
    let d = t.children_of("duct").next().unwrap();
    assert_eq!((d.name.as_deref(), d.subtype.as_deref()), (Some("D1"), Some("round")));
    assert_eq!(d.mm("offset"), Some(-50.0));
    assert_eq!(d.ident("label"), Some("Main trunk"));
    match d.prop("route").unwrap() {
        edl::syntax::ext::ExtValue::List(v) => assert_eq!(v[1].as_ident(), Some("B.in")),
        o => panic!("{o:?}"),
    }
}
