use clap::{Parser, Subcommand};
use edl::core::diff;
use edl::core::query::{inspect_summary, measure_ir, query_ir};
use edl::{
    build_bom, build_panel_schedule, compile_file, export_bom_csv, export_dxf, export_kicad_sch,
    export_panel_schedule_csv, export_spice_netlist, simulate_dc_and_rc,
};
use std::fs;
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "edl",
    version,
    about = "Engineering Description Language (EDL) v1.0 — multi-domain compiler, simulator, sync, plugin registry & MCP server"
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
        /// Optional specific sheet name to render (multi-sheet schematics)
        #[arg(long)]
        sheet: Option<String>,
        /// Render every sheet to `<output>/<sheet>.svg` (treats `-o` as a directory)
        #[arg(long)]
        all_sheets: bool,
        /// Optional comma-separated discipline layers for multi-discipline plans (e.g. `arch`, `arch,elec`, `arch,plumbing`, `all`)
        #[arg(long, value_delimiter = ',')]
        layers: Option<Vec<String>>,
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
        /// Output directory for a multi-sheet KiCad 9 hierarchical project
        #[arg(long)]
        kicad_dir: Option<PathBuf>,
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
    /// Import an existing KiCad 8/9 `.kicad_sch` project into EDL (use `--lift` for short AI-native intent + `.edl.layout` lockfile)
    ImportKicad {
        /// Path to the KiCad schematic (.kicad_sch; single-sheet or root hierarchical sheet)
        file: PathBuf,
        /// Output directory for generated `.edl`, optional `.edl.layout`, and extracted `.kicad_sym`
        #[arg(long)]
        out_dir: PathBuf,
        /// Lift into short, coordinate-free AI-native EDL v3.0 Intent + separate `.edl.layout` lockfile
        #[arg(long)]
        lift: bool,
        /// Automatically round-trip export and verify 100% pin-for-pin KiCad 9 netlist equivalence
        #[arg(long)]
        verify: bool,
    },
    /// Generate a grouped Bill of Materials (CSV or JSON) for an electronics project
    Bom {
        /// Path to .edl project file
        file: PathBuf,
        /// Emit CSV instead of JSON
        #[arg(long)]
        csv: bool,
    },
    /// Generate a Building Electrical DB Panel & Circuit Schedule (CSV or JSON)
    Schedule {
        /// Path to .edl project file
        file: PathBuf,
        /// Emit CSV instead of JSON
        #[arg(long)]
        csv: bool,
    },
    /// Generate a Building Plumbing Pipe & Equipment Takeoff Schedule (table or CSV)
    PlumbingSchedule {
        /// Path to .edl project file
        file: PathBuf,
        /// Emit CSV instead of formatted table
        #[arg(long)]
        csv: bool,
    },
    /// Bidirectional sync: turn edits made in KiCad / AutoCAD(DXF) back into minimal patches of the .edl source
    Sync {
        /// The .edl project that is the source of truth
        file: PathBuf,
        /// Edited KiCad schematic exported by EDL (moves, value and footprint edits)
        #[arg(long)]
        from_kicad: Option<PathBuf>,
        /// Edited DXF exported by EDL (fixture moves; semantic drift report)
        #[arg(long)]
        from_dxf: Option<PathBuf>,
        /// Write the patches into the .edl file (default is a verified dry run)
        #[arg(long)]
        apply: bool,
        /// Emit the structured patch plan as JSON
        #[arg(long)]
        json: bool,
    },
    /// Live co-editing bridge: serve a hot-reloading preview and (optionally) keep a KiCad file in two-way sync
    Watch {
        /// The .edl project to watch
        file: PathBuf,
        /// Local port for the live preview (default 8765)
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Keep this .kicad_sch in two-way sync (EDL edits re-export; KiCad saves sync back)
        #[arg(long)]
        kicad: Option<PathBuf>,
    },
    /// List the registered domain engines (role, owned keywords, export formats, verification rules)
    Domains {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Search or inspect native KiCad `.kicad_sym` symbol libraries (pins, roles, units, footprints)
    Symbols {
        /// Action: 'search' or 'inspect'
        action: String,
        /// Search query (e.g. "BMP280", "NPN", "LM358") or exact Library:Symbol (e.g. "Sensor_Pressure:BMP280")
        target: String,
        /// Max results for 'search'
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Optional extra .kicad_sym file or directory to search
        #[arg(long)]
        lib: Vec<PathBuf>,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Emit the formal JSON Schema for EDL Canonical IR (derived from Rust IR types)
    Schema,
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
                } else if ir.project.domain == "elec.industrial" {
                    println!(
                        "  Industrial Control: {} device(s) | {} control rung(s)",
                        ir.project.component_count, ir.project.net_count
                    );
                } else if ir.project.domain == "elec.sld" {
                    println!(
                        "  Building Power SLD: {} element(s) | {} switchboard bus(es)",
                        ir.project.component_count, ir.project.net_count
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
                    if !ir.mep_circuits.is_empty() {
                        let dev_count = ir
                            .entities
                            .iter()
                            .filter(|e| e.kind.starts_with("mep.elec."))
                            .count();
                        println!(
                            "  Building Electrical: {} device(s) | {} circuit(s) | {} wire run(s)",
                            dev_count,
                            ir.mep_circuits.len(),
                            ir.mep_wires.len()
                        );
                    }
                    let pl_models = edl::domains::mep_plumbing::render::extract_plumbing_models(&ir);
                    if !pl_models.is_empty() {
                        let nodes: usize = pl_models.iter().map(|m| m.nodes.len()).sum();
                        let pipes: usize = pl_models.iter().map(|m| m.pipes.len()).sum();
                        println!(
                            "  Building Plumbing  : {} node(s)/valve(s) | {} pipe run(s)",
                            nodes, pipes
                        );
                    }
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
            sheet,
            all_sheets,
            layers,
        } => {
            let ir = load_and_compile(&file);
            if all_sheets && ir.views.len() > 1 {
                let _ = fs::create_dir_all(&output);
                for v in &ir.views {
                    let svg = edl::render_svg_sheet(&ir, Some(&v.id), annotated);
                    let out_f = output.join(format!("{}.svg", v.id));
                    fs::write(&out_f, svg).unwrap_or_else(|e| {
                        eprintln!("error: failed to write '{}': {e}", out_f.display());
                        process::exit(2);
                    });
                    println!("Rendered sheet '{}' -> {}", v.id, out_f.display());
                }
            } else {
                let svg = if layers.is_some() && ir.project.domain != "electronics" {
                    edl::render_svg_with_layers(&ir, annotated, layers.as_deref())
                } else {
                    edl::render_svg_sheet(&ir, sheet.as_deref(), annotated)
                };
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
        }
        Commands::Export {
            file,
            dxf,
            kicad,
            kicad_dir,
            spice,
            bom,
        } => {
            let ir = load_and_compile(&file);
            if dxf.is_none() && kicad.is_none() && kicad_dir.is_none() && spice.is_none() && bom.is_none() {
                eprintln!("error: specify --dxf, --kicad, --kicad-dir, --spice, or --bom");
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
            if let Some(kdir) = kicad_dir {
                let _ = fs::create_dir_all(&kdir);
                let files = edl::export_kicad_project(&ir);
                for (fname, content) in &files {
                    let out_p = kdir.join(fname);
                    if let Some(parent) = out_p.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    fs::write(&out_p, content).unwrap_or_else(|e| {
                        eprintln!("Failed to write {}: {e}", out_p.display());
                        process::exit(1);
                    });
                }
                println!("Exported KiCad 9 project ({} file(s)) -> {}", files.len(), kdir.display());
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
        Commands::Schedule { file, csv } => {
            let ir = load_and_compile(&file);
            if csv {
                print!("{}", export_panel_schedule_csv(&ir));
            } else {
                let sched = build_panel_schedule(&ir);
                println!("{}", serde_json::to_string_pretty(&sched).unwrap());
            }
        }
        Commands::PlumbingSchedule { file, csv } => {
            let ir = load_and_compile(&file);
            print!("{}", edl::format_plumbing_schedule(&ir, csv));
        }
        Commands::Sync {
            file,
            from_kicad,
            from_dxf,
            apply,
            json,
        } => match edl::core::sync::run_sync(&file, from_kicad.as_deref(), from_dxf.as_deref(), apply) {
            Ok((plan, res)) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({"plan": plan, "result": res})).unwrap()
                    );
                } else {
                    print!("{}", edl::core::sync::format_plan(&plan));
                    println!("  => {}", res.message);
                }
                if !res.applied && plan.patch_count() > 0 {
                    process::exit(2);
                }
            }
            Err(e) => {
                eprintln!("sync error: {e}");
                process::exit(1);
            }
        },
        Commands::Watch { file, port, kicad } => {
            if let Err(e) = edl::watch::run_watch(&file, port, kicad.as_deref()) {
                eprintln!("watch error: {e}");
                process::exit(1);
            }
        }
        Commands::Domains { json } => {
            let info = edl::domains::registry::DomainRegistry::builtin().info();
            if json {
                println!("{}", serde_json::to_string_pretty(&info).unwrap());
            } else {
                for d in info {
                    println!("{} [{:?}] — {}", d.id, d.role, d.description);
                    for e in &d.exports {
                        println!("    export {:<9} .{:<10} {}", e.name, e.extension, e.description);
                    }
                    for r in &d.rules {
                        println!("    rule   {:<34} {}", r.id, r.description);
                    }
                }
            }
        }
        Commands::Symbols {
            action,
            target,
            limit,
            lib,
            json,
        } => match action.as_str() {
            "search" => {
                let hits = edl::domains::elec::kicad_sym::search_symbols(&target, &lib, limit);
                if json {
                    println!("{}", serde_json::to_string_pretty(&hits).unwrap());
                } else if hits.is_empty() {
                    println!("No symbols matching '{target}'.");
                } else {
                    for h in hits {
                        println!("{:<42} {}", h.lib_id, h.description);
                    }
                }
            }
            "inspect" => match edl::domains::elec::kicad_sym::load_symbol(&target, &lib) {
                Ok(sym) => {
                    let info = edl::domains::elec::kicad_sym::inspect_symbol_json(&sym);
                    println!("{}", serde_json::to_string_pretty(&info).unwrap());
                }
                Err(e) => {
                    eprintln!("symbol inspect error: {e}");
                    process::exit(1);
                }
            },
            other => {
                eprintln!("Unknown symbols action '{other}' (expected 'search' or 'inspect')");
                process::exit(1);
            }
        },
        Commands::ImportKicad {
            file,
            out_dir,
            lift,
            verify,
        } => {
            let edl_path = if lift {
                match edl::domains::elec::kicad_lift::lift_kicad_project(&file, &out_dir) {
                    Ok(rep) => {
                        let ratio = if rep.intent_edl_lines > 0 {
                            rep.orig_kicad_lines as f64 / rep.intent_edl_lines as f64
                        } else {
                            1.0
                        };
                        println!(
                            "Lifted KiCad project -> {} ({} lines, {:.1}x smaller than {} KiCad lines; lockfile: {} [{} lines], {} component(s), {} net(s), {} group(s))",
                            rep.edl_path.display(),
                            rep.intent_edl_lines,
                            ratio,
                            rep.orig_kicad_lines,
                            rep.layout_lock_path.display(),
                            rep.lockfile_lines,
                            rep.component_count,
                            rep.net_count,
                            rep.group_count
                        );
                        rep.edl_path
                    }
                    Err(e) => {
                        eprintln!("import-kicad --lift error: {e}");
                        process::exit(1);
                    }
                }
            } else {
                match edl::domains::elec::kicad_import::import_kicad_project(&file, &out_dir) {
                    Ok(rep) => {
                        println!(
                            "Imported KiCad project -> {} (+ {}, {} sheet(s), {} component(s), {} net(s), {} wire(s), {} power(s), {} bus(es))",
                            rep.edl_path.display(),
                            rep.sym_lib_path.display(),
                            rep.sheet_count,
                            rep.component_count,
                            rep.net_count,
                            rep.wire_count,
                            rep.power_count,
                            rep.bus_count
                        );
                        rep.edl_path
                    }
                    Err(e) => {
                        eprintln!("import-kicad error: {e}");
                        process::exit(1);
                    }
                }
            };
            if verify {
                let ir = load_and_compile(&edl_path);
                let rt_dir = out_dir.join("roundtrip_kicad");
                let _ = fs::create_dir_all(&rt_dir);
                let files = edl::export_kicad_project(&ir);
                let root_name = file
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("root.kicad_sch");
                for (fname, content) in &files {
                    let dest = rt_dir.join(fname);
                    if let Some(parent) = dest.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Err(e) = fs::write(&dest, content) {
                        eprintln!("Failed to write round-trip '{fname}': {e}");
                        process::exit(1);
                    }
                }
                let rt_root = if rt_dir.join(root_name).is_file() {
                    rt_dir.join(root_name)
                } else {
                    rt_dir.join(files.keys().next().unwrap())
                };
                match edl::domains::elec::kicad_import::compare_kicad_netlists(&file, &rt_root) {
                    Ok(cmp) => {
                        if cmp.exact_match {
                            println!(
                                "ROUND-TRIP NETLIST: 100% EXACT MATCH ({} components, {} multi-pin nets)",
                                cmp.orig_components, cmp.orig_nets
                            );
                        } else {
                            eprintln!(
                                "ROUND-TRIP NETLIST MISMATCH: orig=({} comps, {} nets) vs rt=({} comps, {} nets)",
                                cmp.orig_components, cmp.orig_nets, cmp.rt_components, cmp.rt_nets
                            );
                            if !cmp.missing_components.is_empty() {
                                eprintln!("  missing components: {:?}", cmp.missing_components);
                            }
                            if !cmp.extra_components.is_empty() {
                                eprintln!("  extra components: {:?}", cmp.extra_components);
                            }
                            for m in cmp.missing_pin_pairs.iter().take(5) {
                                eprintln!("  missing net: {m}");
                            }
                            for e in cmp.extra_pin_pairs.iter().take(5) {
                                eprintln!("  extra net:   {e}");
                            }
                            process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Netlist comparison failed: {e}");
                        process::exit(1);
                    }
                }
            }
        }
        Commands::Schema => {
            let schema = edl::core::ir::canonical_ir_schema();
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
        }
        Commands::Mcp => {
            if let Err(e) = edl::mcp::run_stdio_server() {
                eprintln!("MCP server error: {e}");
                process::exit(1);
            }
        }
    }
}
