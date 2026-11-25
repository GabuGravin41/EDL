use crate::core::diff::diff_ir;
use crate::core::ir::CanonicalIr;
use crate::core::query::{inspect_summary, measure_ir, query_ir};
use crate::domains::elec::kicad_erc::run_and_ingest_kicad_erc;
use crate::{
    build_bom, build_panel_schedule, compile_file, compile_source, export_bom_csv, export_dxf,
    export_kicad_sch, export_panel_schedule_csv, export_spice_netlist,
    simulate_dc_and_rc,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::Path;

/// Run a standards-compliant Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio.
/// Supports both newline-delimited JSON-RPC (standard MCP stdio) and `Content-Length:` headers.
pub fn run_stdio_server() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = io::BufWriter::new(stdout.lock());

    while let Some(msg) = read_jsonrpc_message(&mut reader)? {
        if let Some(resp) = handle_jsonrpc_request(&msg) {
            let line = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
            writeln!(writer, "{line}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// Handle a single MCP JSON-RPC 2.0 request (public so unit tests can invoke it directly!).
pub fn handle_jsonrpc_request(req: &Value) -> Option<Value> {
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = req.get("id").cloned();

    // JSON-RPC notifications have no `id` and expect no response
    if id.is_none() && method.starts_with("notifications/") {
        return None;
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "edl",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": mcp_tool_schemas() })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match call_mcp_tool(tool_name, &args) {
                Ok(payload) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&payload).unwrap_or_default()
                    }],
                    "isError": false
                })),
                Err(err_msg) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": err_msg
                    }],
                    "isError": true
                })),
            }
        }
        other => Err((-32601, format!("Method not found: {other}"))),
    };

    let req_id = id.unwrap_or(Value::Null);
    match result {
        Ok(val) => Some(json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "result": val
        })),
        Err((code, message)) => Some(json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "error": { "code": code, "message": message }
        })),
    }
}

fn mcp_tool_schemas() -> Value {
    json!([
        {
            "name": "edl_compile",
            "description": "Compile and verify an EDL project from a file path or inline EDL source. Returns a compact structural summary, verification evidence ledger, diagnostics, and machine-readable repair plans. Optionally runs real KiCad 9 CLI ERC for electronics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Path to .edl project file" },
                    "source": { "type": "string", "description": "Inline .edl source code (alternative to file_path)" },
                    "kicad_erc": { "type": "boolean", "description": "Also run kicad-cli sch erc and ingest results (electronics)" }
                }
            }
        },
        {
            "name": "edl_query",
            "description": "Run a targeted semantic query against an EDL project without dumping the entire IR. Supported kinds: summary, entity, net, room, module, violations.",
            "inputSchema": {
                "type": "object",
                "required": ["kind"],
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" },
                    "kind": { "type": "string", "enum": ["summary", "entity", "net", "room", "module", "violations"] },
                    "target": { "type": "string", "description": "Entity ID, net name, room ID, or module name" }
                }
            }
        },
        {
            "name": "edl_measure",
            "description": "Compute exact analytical geometry measurements: single entity bounds/area, net routed wire length (mm), or pairwise center distance, clear gap, shared wall length, and shared nets between two entities.",
            "inputSchema": {
                "type": "object",
                "required": ["target_a"],
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" },
                    "target_a": { "type": "string" },
                    "target_b": { "type": "string" }
                }
            }
        },
        {
            "name": "edl_render_svg",
            "description": "Render an architectural floor plan or electronics schematic SVG preview to disk.",
            "inputSchema": {
                "type": "object",
                "required": ["output_path"],
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" },
                    "output_path": { "type": "string" },
                    "annotated": { "type": "boolean" }
                }
            }
        },
        {
            "name": "edl_export",
            "description": "Export an EDL project to AutoCAD DXF ('dxf'), KiCad 9 schematic ('kicad'), SPICE3 netlist ('spice'), or CSV BOM ('bom'), optionally running real KiCad 9 ERC.",
            "inputSchema": {
                "type": "object",
                "required": ["format", "output_path"],
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" },
                    "format": { "type": "string", "enum": ["dxf", "kicad", "spice", "bom"] },
                    "output_path": { "type": "string" },
                    "kicad_erc": { "type": "boolean" }
                }
            }
        },
        {
            "name": "edl_simulate",
            "description": "Run built-in exact MNA DC operating-point + analytical RC filter cutoff simulation on an EDL electronics project. Returns exact net voltages (V), resistor/LED/diode currents (mA) and power (mW), and RC -3dB cutoffs (Hz).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" }
                }
            }
        },
        {
            "name": "edl_bom",
            "description": "Generate a grouped Bill of Materials (BOM) by (category, value, footprint) with quantities, designators, and module provenance.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" }
                }
            }
        },
        {
            "name": "edl_schedule",
            "description": "Generate a Building Electrical (mep.elec) DB Panel & Circuit Schedule with breaker ratings, RCDs, cable gauges, 3D run lengths, loads, and voltage drops.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" },
                    "csv": { "type": "boolean" }
                }
            }
        },
        {
            "name": "edl_plumbing_schedule",
            "description": "Generate a Building Plumbing (mep.plumbing) pipe & equipment takeoff schedule (CWS/HWS/DWV lengths, diameters, flow velocities, gravity slopes, and valves).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" },
                    "source": { "type": "string" },
                    "csv": { "type": "boolean" }
                }
            }
        },
        {
            "name": "edl_sync",
            "description": "Bidirectional sync: diff an edited host-tool file (KiCad .kicad_sch or DXF exported by EDL) against the .edl source of truth and produce a minimal structured patch plan (component moves -> `place`, value/footprint edits, freestanding fixture moves -> `at:`). With apply=true the patches are written to the .edl file only after the patched source re-compiles without new errors. Unrepresentable edits are returned as report_only/unsupported with a reason.",
            "inputSchema": {
                "type": "object",
                "required": ["file_path"],
                "properties": {
                    "file_path": { "type": "string", "description": "The .edl project (source of truth)" },
                    "from_kicad": { "type": "string", "description": "Path to the edited .kicad_sch" },
                    "from_dxf": { "type": "string", "description": "Path to the edited .dxf" },
                    "apply": { "type": "boolean", "description": "Write patches into the .edl file (default false = verified dry run)" }
                }
            }
        },
        {
            "name": "edl_symbol_search",
            "description": "Search installed KiCad 9 `.kicad_sym` libraries (~20,000+ real components) or project-local `.kicad_sym` files by part number, category, or keyword. Use this BEFORE writing a schematic component so you pick real library symbols instead of guessing pinouts.",
            "inputSchema": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string", "description": "Search query (e.g. 'BMP280', 'LM358', 'BC547', 'STM32')" },
                    "limit": { "type": "integer", "description": "Maximum results (default 15)" },
                    "extra_libs": { "type": "array", "items": { "type": "string" }, "description": "Optional project-local .kicad_sym file or directory paths" }
                }
            }
        },
        {
            "name": "edl_symbol_inspect",
            "description": "Inspect a KiCad `.kicad_sym` symbol by exact 'Library:Symbol' ID (e.g. 'Sensor_Pressure:BMP280', 'Transistor_BJT:BC547', 'Amplifier_Operational:LM358'). Returns exact pin names, pin numbers, electrical roles, unit count, and default footprint (with `extends` inheritance resolved).",
            "inputSchema": {
                "type": "object",
                "required": ["lib_id"],
                "properties": {
                    "lib_id": { "type": "string", "description": "Exact 'Library:Symbol' reference" },
                    "extra_libs": { "type": "array", "items": { "type": "string" } }
                }
            }
        },
        {
            "name": "edl_domains",
            "description": "List the registered EDL domain engines (arch, elec, mep.elec, plus any plugins): role (base/overlay), owned syntax keywords, export formats and the verification rules each domain checks. Use this to discover what can be designed and verified before writing EDL.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "edl_diff",
            "description": "Compute a semantic diff by stable entity ID between two EDL revisions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "before_path": { "type": "string" },
                    "before_source": { "type": "string" },
                    "after_path": { "type": "string" },
                    "after_source": { "type": "string" }
                }
            }
        }
    ])
}

fn compile_from_args(args: &Value, path_key: &str, src_key: &str) -> Result<CanonicalIr, String> {
    if let Some(p) = args.get(path_key).and_then(|v| v.as_str()) {
        compile_file(Path::new(p))
            .map_err(|d| serde_json::to_string_pretty(&d).unwrap_or_else(|_| d.message))
    } else if let Some(s) = args.get(src_key).and_then(|v| v.as_str()) {
        compile_source("<mcp-inline>.edl", s)
            .map_err(|d| serde_json::to_string_pretty(&d).unwrap_or_else(|_| d.message))
    } else {
        Err(format!("Provide either '{path_key}' or '{src_key}'"))
    }
}

fn call_mcp_tool(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "edl_compile" => {
            let mut ir = compile_from_args(args, "file_path", "source")?;
            if args.get("kicad_erc").and_then(|v| v.as_bool()).unwrap_or(false) {
                run_and_ingest_kicad_erc(&mut ir);
            }
            let summary = inspect_summary(&ir);
            let violations = query_ir(&ir, "violations", None)?;
            Ok(json!({
                "summary": summary,
                "violations": violations,
            }))
        }
        "edl_query" => {
            let ir = compile_from_args(args, "file_path", "source")?;
            let kind = args
                .get("kind")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'kind' parameter".to_string())?;
            let target = args.get("target").and_then(|v| v.as_str());
            query_ir(&ir, kind, target)
        }
        "edl_measure" => {
            let ir = compile_from_args(args, "file_path", "source")?;
            let a = args
                .get("target_a")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'target_a' parameter".to_string())?;
            let b = args.get("target_b").and_then(|v| v.as_str());
            measure_ir(&ir, a, b)
        }
        "edl_render_svg" => {
            let ir = compile_from_args(args, "file_path", "source")?;
            let out_path = args
                .get("output_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'output_path' parameter".to_string())?;
            let annotated = args.get("annotated").and_then(|v| v.as_bool()).unwrap_or(false);
            let layers: Option<Vec<String>> = args
                .get("layers")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
            let svg = crate::render_svg_with_layers(&ir, annotated, layers.as_deref());
            let p = Path::new(out_path);
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            std::fs::write(p, &svg).map_err(|e| e.to_string())?;
            Ok(json!({
                "output_path": out_path,
                "bytes": svg.len(),
                "has_errors": ir.project.has_errors,
            }))
        }
        "edl_export" => {
            let mut ir = compile_from_args(args, "file_path", "source")?;
            let fmt = args
                .get("format")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'format' parameter".to_string())?;
            let out_path = args
                .get("output_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'output_path' parameter".to_string())?;
            let content = match fmt {
                "dxf" => export_dxf(&ir),
                "kicad" => export_kicad_sch(&ir),
                "spice" => export_spice_netlist(&ir),
                "bom" => export_bom_csv(&ir),
                other => return Err(format!("Unknown export format '{other}' (expected 'dxf', 'kicad', 'spice', or 'bom')")),
            };
            let p = Path::new(out_path);
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            std::fs::write(p, &content).map_err(|e| e.to_string())?;
            if fmt == "kicad" && args.get("kicad_erc").and_then(|v| v.as_bool()).unwrap_or(false) {
                run_and_ingest_kicad_erc(&mut ir);
            }
            Ok(json!({
                "format": fmt,
                "output_path": out_path,
                "bytes": content.len(),
                "has_errors": ir.project.has_errors,
                "evidence": ir.evidence,
            }))
        }
        "edl_simulate" => {
            let ir = compile_from_args(args, "file_path", "source")?;
            let sim = simulate_dc_and_rc(&ir);
            serde_json::to_value(&sim).map_err(|e| e.to_string())
        }
        "edl_bom" => {
            let ir = compile_from_args(args, "file_path", "source")?;
            let bom = build_bom(&ir);
            serde_json::to_value(&bom).map_err(|e| e.to_string())
        }
        "edl_schedule" => {
            let ir = compile_from_args(args, "file_path", "source")?;
            if args.get("csv").and_then(|v| v.as_bool()).unwrap_or(false) {
                Ok(json!({ "csv": export_panel_schedule_csv(&ir) }))
            } else {
                let sched = build_panel_schedule(&ir);
                serde_json::to_value(&sched).map_err(|e| e.to_string())
            }
        }
        "edl_plumbing_schedule" => {
            let ir = compile_from_args(args, "file_path", "source")?;
            let csv = args.get("csv").and_then(|v| v.as_bool()).unwrap_or(false);
            let models = crate::domains::mep_plumbing::render::extract_plumbing_models(&ir);
            Ok(json!({
                "schedule": crate::format_plumbing_schedule(&ir, csv),
                "models": models,
            }))
        }
        "edl_sync" => {
            let file = args
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'file_path' parameter".to_string())?;
            let k = args.get("from_kicad").and_then(|v| v.as_str()).map(Path::new);
            let d = args.get("from_dxf").and_then(|v| v.as_str()).map(Path::new);
            let apply = args.get("apply").and_then(|v| v.as_bool()).unwrap_or(false);
            let (plan, result) = crate::core::sync::run_sync(Path::new(file), k, d, apply)?;
            Ok(json!({ "plan": plan, "result": result }))
        }
        "edl_symbol_search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'query' parameter".to_string())?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let extra: Vec<std::path::PathBuf> = args
                .get("extra_libs")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).map(std::path::PathBuf::from).collect())
                .unwrap_or_default();
            let hits = crate::domains::elec::kicad_sym::search_symbols(query, &extra, limit);
            serde_json::to_value(&hits).map_err(|e| e.to_string())
        }
        "edl_symbol_inspect" => {
            let lib_id = args
                .get("lib_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required 'lib_id' parameter".to_string())?;
            let extra: Vec<std::path::PathBuf> = args
                .get("extra_libs")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).map(std::path::PathBuf::from).collect())
                .unwrap_or_default();
            let sym = crate::domains::elec::kicad_sym::load_symbol(lib_id, &extra)?;
            Ok(crate::domains::elec::kicad_sym::inspect_symbol_json(&sym))
        }
        "edl_domains" => {
            serde_json::to_value(crate::domains::registry::DomainRegistry::builtin().info()).map_err(|e| e.to_string())
        }
        "edl_diff" => {
            let before = compile_from_args(args, "before_path", "before_source")?;
            let after = compile_from_args(args, "after_path", "after_source")?;
            let d = diff_ir(&before, &after);
            serde_json::to_value(&d).map_err(|e| e.to_string())
        }
        other => Err(format!("Unknown MCP tool '{other}'")),
    }
}

fn read_jsonrpc_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Support LSP-style Content-Length framing if a client uses it
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            let len: usize = rest.trim().parse().unwrap_or(0);
            // Drain blank separator line(s)
            loop {
                let mut hdr = String::new();
                if reader.read_line(&mut hdr)? == 0 || hdr.trim().is_empty() {
                    break;
                }
            }
            let mut buf = vec![0u8; len];
            reader.read_exact(&mut buf)?;
            let val: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);
            return Ok(Some(val));
        }
        // Standard MCP stdio newline-delimited JSON-RPC
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            return Ok(Some(val));
        }
    }
}
