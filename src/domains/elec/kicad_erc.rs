use crate::core::ir::{CanonicalIr, EvidenceRecord, VerificationStatus};
use crate::core::span::Diagnostic;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct KicadErcReport {
    #[serde(default)]
    sheets: Vec<KicadErcSheet>,
}

#[derive(Debug, Deserialize)]
struct KicadErcSheet {
    #[serde(default)]
    violations: Vec<KicadErcViolation>,
}

#[derive(Debug, Deserialize)]
struct KicadErcViolation {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    items: Vec<KicadErcItem>,
}

#[derive(Debug, Deserialize)]
struct KicadErcItem {
    #[serde(default)]
    description: String,
}

/// Export `ir` to a temporary `.kicad_sch`, run real `kicad-cli sch erc --severity-error --format json`,
/// and ingest the external verification results straight into `ir.evidence` and `ir.diagnostics`.
pub fn run_and_ingest_kicad_erc(ir: &mut CanonicalIr) {
    if ir.project.domain != "electronics" {
        return;
    }

    let stamp = std::process::id();
    let safe_name: String = ir
        .project
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let work_dir = std::env::temp_dir().join(format!("edl_erc_{safe_name}_{stamp}"));
    let _ = std::fs::remove_dir_all(&work_dir);
    let _ = std::fs::create_dir_all(&work_dir);
    let sch_path = work_dir.join(format!("{safe_name}.kicad_sch"));
    let rpt_path = work_dir.join(format!("{safe_name}_erc.json"));

    let kicad_files = crate::domains::elec::kicad::export_kicad_project(ir);
    let mut write_err: Option<std::io::Error> = None;
    for (fname, content) in &kicad_files {
        if let Err(e) = std::fs::write(work_dir.join(fname), content) {
            write_err = Some(e);
            break;
        }
    }
    if let Some(e) = write_err {
        ir.evidence.push(EvidenceRecord {
            check_id: "K1_kicad_erc_external".to_string(),
            status: VerificationStatus::Unverified,
            method: "external_kicad_cli_9_erc_json".to_string(),
            tolerance_mm: None,
            subjects: Vec::new(),
            details: format!("Failed to write temporary schematic for KiCad ERC: {e}"),
        });
        return;
    }

    let output = Command::new("kicad-cli")
        .args([
            "sch",
            "erc",
            "--severity-error",
            "--format",
            "json",
            "-o",
            &rpt_path.display().to_string(),
            &sch_path.display().to_string(),
        ])
        .output();

    let _ = std::fs::remove_file(&sch_path);

    let out = match output {
        Ok(o) => o,
        Err(e) => {
            let _ = std::fs::remove_file(&rpt_path);
            ir.evidence.push(EvidenceRecord {
                check_id: "K1_kicad_erc_external".to_string(),
                status: VerificationStatus::Unverified,
                method: "external_kicad_cli_9_erc_json".to_string(),
                tolerance_mm: None,
                subjects: Vec::new(),
                details: format!("kicad-cli unavailable ({e}); external KiCad ERC skipped"),
            });
            return;
        }
    };

    let json_text = std::fs::read_to_string(&rpt_path).unwrap_or_default();
    let _ = std::fs::remove_file(&rpt_path);

    if json_text.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        ir.evidence.push(EvidenceRecord {
            check_id: "K1_kicad_erc_external".to_string(),
            status: VerificationStatus::Unverified,
            method: "external_kicad_cli_9_erc_json".to_string(),
            tolerance_mm: None,
            subjects: Vec::new(),
            details: format!("kicad-cli produced no ERC report: {}", stderr.trim()),
        });
        return;
    }

    match serde_json::from_str::<KicadErcReport>(&json_text) {
        Ok(report) => {
            let mut violations = Vec::new();
            for sheet in report.sheets {
                for v in sheet.violations {
                    if v.severity.eq_ignore_ascii_case("error") || v.severity.is_empty() {
                        violations.push(v);
                    }
                }
            }
            if violations.is_empty() {
                ir.evidence.push(EvidenceRecord {
                    check_id: "K1_kicad_erc_external".to_string(),
                    status: VerificationStatus::ExactlyChecked,
                    method: "external_kicad_cli_9_erc_json".to_string(),
                    tolerance_mm: None,
                    subjects: vec![ir.project.name.clone()],
                    details: "KiCad 9 CLI ERC passed with 0 error-level violations".to_string(),
                });
            } else {
                let count = violations.len();
                for v in violations {
                    let details = v
                        .items
                        .iter()
                        .map(|i| i.description.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    ir.diagnostics.push(
                        Diagnostic::error(
                            "E_KICAD_ERC",
                            "external_erc",
                            format!("[KiCad ERC {}] {}: {}", v.kind, v.description, details),
                            None,
                        )
                        .with_suggestion(
                            "Inspect exported .kicad_sch wire/pin geometry or net routing style",
                        ),
                    );
                }
                ir.project.has_errors = true;
                ir.evidence.push(EvidenceRecord {
                    check_id: "K1_kicad_erc_external".to_string(),
                    status: VerificationStatus::Violated,
                    method: "external_kicad_cli_9_erc_json".to_string(),
                    tolerance_mm: None,
                    subjects: vec![ir.project.name.clone()],
                    details: format!("KiCad 9 CLI ERC reported {count} error-level violation(s)"),
                });
            }
        }
        Err(e) => {
            ir.evidence.push(EvidenceRecord {
                check_id: "K1_kicad_erc_external".to_string(),
                status: VerificationStatus::Unverified,
                method: "external_kicad_cli_9_erc_json".to_string(),
                tolerance_mm: None,
                subjects: Vec::new(),
                details: format!("Failed to parse KiCad ERC JSON: {e}"),
            });
        }
    }
}
