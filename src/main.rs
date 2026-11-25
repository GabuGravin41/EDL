use clap::{Parser, Subcommand};
use edl::core::diff;
use edl::core::query::{inspect_summary, measure_ir, query_ir};
use edl::{
    build_bom, compile_file, export_bom_csv, export_dxf, export_kicad_sch, export_spice_netlist,
    render_svg, simulate_dc_and_rc,
};
use std::fs;
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "edl",
    version,
    about = "Engineering Description Language (EDL) v0.6 compiler, simulator & MCP server"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse, elaborate modules/imports, type-check, solve/route, validate, and emit Canonical IR
    Build {
        /// Path to .edl project file
        file: PathBuf,
        /// Emit machine-readable JSON Canonical IR
        #[arg(long)]
        json: bool,
        /// Also invoke kicad-cli sch erc and ingest target-side ERC into the evidence ledger
        #[arg(long)]
        kicad_erc: bool,
    },
    /// Render an architectural or schematic SVG preview (with optional annotations)
    Preview {
        /// Path to .edl project file
        file: PathBuf,
        /// Output SVG file path
        #[arg(short, long)]
        output: PathBuf,
        /// Show entity/pin annotations even when all checks pass
        #[arg(long)]
        annotated: bool,
    },
    /// Export to a native CAD/EDA format (DXF, KiCad .kicad_sch, SPICE .cir, or BOM .csv)
    Export {
        /// Path to .edl project file
        file: PathBuf,
        /// Output DXF file path (architecture)
        #[arg(long)]
        dxf: Option<PathBuf>,
        /// Output KiCad schematic (.kicad_sch) file path (electronics)
        #[arg(long)]
        kicad: Option<PathBuf>,
        /// Output SPICE3 netlist (.cir) file path (electronics)
        #[arg(long)]
        spice: Option<PathBuf>,
        /// Output CSV Bill of Materials (.csv) file path (electronics)
        #[arg(long)]
        bom: Option<PathBuf>,
    },
    /// Compute a semantic diff between two .edl revisions by stable entity ID
    Diff {
        /// Baseline .edl file
        before: PathBuf,
        /// Modified .edl file
        after: PathBuf,
        /// Emit machine-readable JSON diff
        #[arg(long)]
        json: bool,
    },
    /// Emit a compact AI-friendly structural inspection summary (bounds, provenance, nets, evidence)
    Inspect {
        /// Path to .edl project file
        file: PathBuf,
        /// Also invoke kicad-cli sch erc for electronics projects
        #[arg(long)]
        kicad_erc: bool,
    },
    /// Run a targeted semantic query (summary | entity | net | room | module | violations)
    Query {
        /// Path to .edl project file
        file: PathBuf,
        /// Query kind: summary, entity, net, room, module, or violations
        kind: String,
        /// Optional target entity ID, net name, room ID, or module name
        target: Option<String>,
        /// Also invoke kicad-cli sch erc before querying (electronics)
        #[arg(long)]
        kicad_erc: bool,
    },
    /// Measure an entity, net route wire length, or pairwise clear gap / distance / shared wall
    Measure {
        /// Path to .edl project file
        file: PathBuf,
        /// First entity ID or schematic net name
        target_a: String,
        /// Optional second entity ID for pairwise measurements
        target_b: Option<String>,
    },
    /// Run built-in exact MNA DC operating-point + analytical RC cutoff simulation (electronics)
    Simulate {
        /// Path to .edl project file
        file: PathBuf,
        /// Emit machine-readable JSON simulation report
        #[arg(long)]
        json: bool,
    },
    /// Generate a grouped Bill of Materials (CSV or JSON) for an electronics project
    Bom {
        /// Path to .edl project file
        file: PathBuf,
        /// Emit CSV instead of JSON
        #[arg(long)]
        csv: bool,
    },
    /// Start the Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio
    Mcp,
}

fn load_and_compile(path: &PathBuf) -> edl::core::ir::CanonicalIr {
    match compile_file(path) {
        Ok(ir) => ir,
        Err(diag) => {
            eprintln!("{}", serde_json::to_string_pretty(&diag).unwrap());
            process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build {
            file,
            json,
            kicad_erc,
        } => {
            let mut ir = load_and_compile(&file);
            if kicad_erc {
                edl::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&ir).unwrap());
            } else {
                println!("EDL Project: {} [{}]", ir.project.name, ir.project.domain);
                if ir.project.domain == "electronics" {
                    println!(
                        "  Components: {} | Nets: {} | Routes: {}",
                        ir.project.component_count,
                        ir.project.net_count,
                        ir.schematic_routes.len()
                    );
                } else {
                    if let Some([w, d]) = ir.project.exterior_footprint_mm {
                        println!(
                            "  Exterior footprint : {:.0} mm x {:.0} mm ({:.2} m x {:.2} m)",
                            w,
                            d,
                            w / 1000.0,
                            d / 1000.0
                        );
                    }
                    println!(
                        "  Rooms: {} | Openings: {} | Fixtures: {} | Clear area: {:.2} m² | Remaining DOF: {}",
                        ir.project.room_count,
                        ir.project.opening_count,
                        ir.project.fixture_count,
                        ir.project.total_clear_area_m2,
                        ir.project.remaining_dof.len()
                    );
                }
                println!("  Evidence:");
                for ev in &ir.evidence {
                    println!("    [{:?}] {} — {}", ev.status, ev.check_id, ev.details);
                }
                if !ir.diagnostics.is_empty() {
                    println!("  Diagnostics:");
                    for d in &ir.diagnostics {
                        println!(
                            "    [{:?}] {} ({}) — {}",
                            d.severity, d.code, d.category, d.message
                        );
                        if let Some(ref sug) = d.suggestion {
                            println!("      hint: {sug}");
                        }
                    }
                }
            }
            if ir.project.has_errors {
                process::exit(1);
            }
        }
        Commands::Preview {
            file,
            output,
            annotated,
        } => {
            let ir = load_and_compile(&file);
            let svg = render_svg(&ir, annotated);
            if let Some(parent) = output.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            fs::write(&output, svg).unwrap_or_else(|e| {
                eprintln!("error: failed to write '{}': {e}", output.display());
                process::exit(2);
            });
            println!(
                "Rendered SVG preview -> {} (errors: {})",
                output.display(),
                ir.project.has_errors
            );
        }
        Commands::Export {
            file,
            dxf,
            kicad,
            spice,
            bom,
        } => {
            let ir = load_and_compile(&file);
            if dxf.is_none() && kicad.is_none() && spice.is_none() && bom.is_none() {
                eprintln!("error: specify --dxf, --kicad, --spice, or --bom");
                process::exit(2);
            }
            if let Some(dxf_path) = dxf {
                if ir.project.has_errors {
                    eprintln!("warning: exporting DXF despite verification error(s)");
                }
                let content = export_dxf(&ir);
                if let Some(parent) = dxf_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                fs::write(&dxf_path, content).unwrap_or_else(|e| {
                    eprintln!("error: failed to write '{}': {e}", dxf_path.display());
                    process::exit(2);
                });
                println!("Exported layered DXF -> {}", dxf_path.display());
            }
            if let Some(kicad_path) = kicad {
                if ir.project.has_errors {
                    eprintln!("warning: exporting KiCad schematic despite ERC error(s)");
                }
                let content = export_kicad_sch(&ir);
                if let Some(parent) = kicad_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                fs::write(&kicad_path, content).unwrap_or_else(|e| {
                    eprintln!("error: failed to write '{}': {e}", kicad_path.display());
                    process::exit(2);
                });
                println!("Exported KiCad schematic -> {}", kicad_path.display());
            }
            if let Some(spice_path) = spice {
                let content = export_spice_netlist(&ir);
                if let Some(parent) = spice_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                fs::write(&spice_path, content).unwrap_or_else(|e| {
                    eprintln!("error: failed to write '{}': {e}", spice_path.display());
                    process::exit(2);
                });
                println!("Exported SPICE3 netlist -> {}", spice_path.display());
            }
            if let Some(bom_path) = bom {
                let content = export_bom_csv(&ir);
                if let Some(parent) = bom_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                fs::write(&bom_path, content).unwrap_or_else(|e| {
                    eprintln!("error: failed to write '{}': {e}", bom_path.display());
                    process::exit(2);
                });
                println!("Exported CSV BOM -> {}", bom_path.display());
            }
        }
        Commands::Diff {
            before,
            after,
            json,
        } => {
            let ir_a = load_and_compile(&before);
            let ir_b = load_and_compile(&after);
            let d = diff::diff_ir(&ir_a, &ir_b);
            if json {
                println!("{}", serde_json::to_string_pretty(&d).unwrap());
            } else {
                println!("Semantic Diff: {} -> {}", d.project_a, d.project_b);
                let fmt_fp = |fp: Option<[f64; 2]>| -> String {
                    fp.map(|[w, h]| format!("{w:.0}×{h:.0} mm"))
                        .unwrap_or_else(|| "none".to_string())
                };
                println!(
                    "  Footprint : {} -> {}",
                    fmt_fp(d.footprint_before_mm),
                    fmt_fp(d.footprint_after_mm)
                );
                println!(
                    "  Clear area: {:.2} m² -> {:.2} m²",
                    d.area_before_m2, d.area_after_m2
                );
                println!("  Unchanged : {}", d.unchanged_entities.join(", "));
                for ch in &d.changed_entities {
                    println!("  [{}] {} ({}) — {}", ch.change, ch.id, ch.kind, ch.details);
                }
                for vc in &d.verification_changes {
                    println!("  [verify] {}: {:?} -> {:?}", vc.check_id, vc.before, vc.after);
                }
            }
        }
        Commands::Inspect { file, kicad_erc } => {
            let mut ir = load_and_compile(&file);
            if kicad_erc {
                edl::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
            }
            let summary = inspect_summary(&ir);
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        }
        Commands::Query {
            file,
            kind,
            target,
            kicad_erc,
        } => {
            let mut ir = load_and_compile(&file);
            if kicad_erc {
                edl::domains::elec::kicad_erc::run_and_ingest_kicad_erc(&mut ir);
            }
            match query_ir(&ir, &kind, target.as_deref()) {
                Ok(val) => println!("{}", serde_json::to_string_pretty(&val).unwrap()),
                Err(msg) => {
                    eprintln!("error: {msg}");
                    process::exit(1);
                }
            }
        }
        Commands::Measure {
            file,
            target_a,
            target_b,
        } => {
            let ir = load_and_compile(&file);
            match measure_ir(&ir, &target_a, target_b.as_deref()) {
                Ok(val) => println!("{}", serde_json::to_string_pretty(&val).unwrap()),
                Err(msg) => {
                    eprintln!("error: {msg}");
                    process::exit(1);
                }
            }
        }
        Commands::Simulate { file, json } => {
            let ir = load_and_compile(&file);
            let sim = simulate_dc_and_rc(&ir);
            if json {
                println!("{}", serde_json::to_string_pretty(&sim).unwrap());
            } else {
                println!(
                    "EDL MNA Simulation: {} (converged: {}, {} iter)",
                    sim.project, sim.converged, sim.iterations
                );
                println!("  Node DC Voltages:");
                for (net, v) in &sim.node_voltages_v {
                    println!("    {:<16} = {:>8.3} V", net, v);
                }
                if !sim.branches.is_empty() {
                    println!("  Branch Currents & Power:");
                    for b in &sim.branches {
                        println!(
                            "    {:<18} ({:<9} {:<10}) | dV={:>6.3}V | I={:>7.3} mA | P={:>7.3} mW [{}]",
                            b.component, b.category, b.value, b.v_drop_v, b.current_ma, b.power_mw, b.state
                        );
                    }
                }
                if !sim.rc_filters.is_empty() {
                    println!("  Analytical RC Filters (-3dB cutoff):");
                    for f in &sim.rc_filters {
                        println!(
                            "    node {:<12} ({} + {}) -> fc = {:.2} Hz",
                            f.node, f.resistor, f.capacitor, f.cutoff_hz
                        );
                    }
                }
            }
        }
        Commands::Bom { file, csv } => {
            let ir = load_and_compile(&file);
            if csv {
                print!("{}", export_bom_csv(&ir));
            } else {
                let bom = build_bom(&ir);
                println!("{}", serde_json::to_string_pretty(&bom).unwrap());
            }
        }
        Commands::Mcp => {
            if let Err(e) = edl::mcp::run_stdio_server() {
                eprintln!("MCP server error: {e}");
                process::exit(1);
            }
        }
    }
}
