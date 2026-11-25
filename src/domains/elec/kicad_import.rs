//! Native KiCad 9 `.kicad_sch` -> Two-Layer Option-A EDL importer and round-trip netlist oracle.
//!
//! Converts any single-sheet or hierarchical multi-sheet KiCad 8/9 schematic into:
//!   1. `<stem>.kicad_sym` — extracted verbatim embedded `(lib_symbols ...)` library
//!   2. `<stem>.edl` — clean Two-Layer Option-A EDL:
//!        - `circuit { ... }`   : 100% exact semantic netlist (components, values, footprints, nets)
//!        - `schematic { ... }` : 100% exact visual/geometric layout (`place`, `wire`, `bus`,
//!                                `bus_entry`, `junction`, `power`, `label`, `global_label`,
//!                                `hierarchical_label`, `no_connect`, `text`, `sheet_block`)

use crate::core::sync::{parse_sexpr, Sx};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct KiCadImportReport {
    pub edl_path: PathBuf,
    pub sym_lib_path: PathBuf,
    pub sheet_count: usize,
    pub component_count: usize,
    pub net_count: usize,
    pub wire_count: usize,
    pub power_count: usize,
    pub bus_count: usize,
}

#[derive(Debug, Clone)]
pub struct NetlistCompareReport {
    pub orig_components: usize,
    pub rt_components: usize,
    pub orig_nets: usize,
    pub rt_nets: usize,
    pub missing_components: Vec<String>,
    pub extra_components: Vec<String>,
    pub missing_pin_pairs: Vec<String>,
    pub extra_pin_pairs: Vec<String>,
    pub exact_match: bool,
}

/// Import a KiCad 8/9 `.kicad_sch` (single-sheet or root hierarchical sheet) into Option-A EDL
/// inside `out_dir`, returning a summary report.
pub fn import_kicad_project(root_sch: &Path, out_dir: &Path) -> Result<KiCadImportReport, String> {
    if !root_sch.is_file() {
        return Err(format!("KiCad schematic not found: {}", root_sch.display()));
    }
    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create output dir '{}': {e}", out_dir.display()))?;

    let stem = root_sch
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported_project");
    let safe_stem = sanitize_ident(stem);
    let src_dir = root_sch.parent().unwrap_or_else(|| Path::new("."));

    // 1. Discover root + all child hierarchical sheets (preserving relative paths like `sch/Connectors.kicad_sch`!)
    //    AND record the original KiCad hierarchy UUID tree so reused sub-sheets map exact per-instance RefDes!
    let mut sheet_entries: Vec<(String, PathBuf)> = Vec::new(); // (rel_file, abs_path)
    let mut visited_files: BTreeSet<String> = BTreeSet::new();
    let mut queue: std::collections::VecDeque<(String, PathBuf)> = std::collections::VecDeque::new();
    let root_rel = root_sch
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("root.kicad_sch")
        .to_string();
    queue.push_back((root_rel.clone(), root_sch.to_path_buf()));

    while let Some((rel_file, sp)) = queue.pop_front() {
        if !visited_files.insert(rel_file.clone()) {
            continue;
        }
        if !sp.is_file() {
            continue;
        }
        let sp_dir = sp.parent().unwrap_or(src_dir);
        let text = fs::read_to_string(&sp)
            .map_err(|e| format!("Failed to read '{}': {e}", sp.display()))?;
        if let Ok(sx) = parse_sexpr(&text) {
            if let Some(items) = sx_list(&sx) {
                for item in items.iter().skip(1) {
                    if sx_head(item) == Some("sheet") {
                        if let Some(sf) = find_property(item, "Sheetfile") {
                            let child_abs = if sp_dir.join(&sf).is_file() {
                                sp_dir.join(&sf)
                            } else {
                                src_dir.join(&sf)
                            };
                            if child_abs.is_file() {
                                let child_rel = child_abs
                                    .strip_prefix(src_dir)
                                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                                    .unwrap_or_else(|_| sf.replace('\\', "/"));
                                queue.push_back((child_rel, child_abs));
                            }
                        }
                    }
                }
            }
        }
        sheet_entries.push((rel_file, sp));
    }

    // 2. Extract ALL embedded `(lib_symbols ...)` definitions across all sheets into `<safe_stem>.kicad_sym`
    let mut embedded_syms: BTreeMap<String, String> = BTreeMap::new();
    let mut power_lib_ids: BTreeSet<String> = BTreeSet::new();
    let mut parsed_sheets: Vec<(String, PathBuf, Sx)> = Vec::new();
    let mut sx_by_rel: BTreeMap<String, usize> = BTreeMap::new();

    for (rel_file, sp) in &sheet_entries {
        let text = fs::read_to_string(sp)
            .map_err(|e| format!("Failed to read '{}': {e}", sp.display()))?;
        for (sym_name, raw_sym, is_power) in extract_lib_symbols_raw(&text) {
            if is_power {
                power_lib_ids.insert(sym_name.clone());
            }
            embedded_syms.entry(sym_name).or_insert(raw_sym);
        }
        let sx = parse_sexpr(&text)
            .map_err(|e| format!("Failed to parse '{}': {e}", sp.display()))?;
        sx_by_rel.insert(rel_file.clone(), parsed_sheets.len());
        parsed_sheets.push((rel_file.clone(), sp.clone(), sx));
    }

    // Build original KiCad hierarchy map: rel_file -> Vec<(semantic_path, orig_uuid_path)>
    let mut orig_sheet_paths: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    if let Some((_, _, root_sx)) = parsed_sheets.first() {
        let orig_root_uuid = find_child_text(root_sx, "uuid").unwrap_or_default();
        fn walk_orig_hierarchy(
            cur_rel: &str,
            sem_prefix: &str,
            uuid_prefix: &str,
            parsed_sheets: &[(String, PathBuf, Sx)],
            sx_by_rel: &BTreeMap<String, usize>,
            out_map: &mut BTreeMap<String, Vec<(String, String)>>,
            depth: usize,
        ) {
            if depth > 12 {
                return;
            }
            out_map
                .entry(cur_rel.to_string())
                .or_default()
                .push((sem_prefix.to_string(), uuid_prefix.to_string()));
            let Some(&idx) = sx_by_rel.get(cur_rel) else {
                return;
            };
            let sx = &parsed_sheets[idx].2;
            let cur_dir = Path::new(cur_rel)
                .parent()
                .filter(|p| !p.as_os_str().is_empty());
            if let Some(items) = sx_list(sx) {
                for item in items.iter().skip(1) {
                    if sx_head(item) == Some("sheet") {
                        let sname = find_property(item, "Sheetname").unwrap_or_default();
                        let sfile = find_property(item, "Sheetfile").unwrap_or_default();
                        let suuid = find_child_text(item, "uuid").unwrap_or_default();
                        let norm = sfile.replace('\\', "/");
                        let child_rel = if sx_by_rel.contains_key(&norm) {
                            norm
                        } else if let Some(d) = cur_dir {
                            d.join(&norm).to_string_lossy().replace('\\', "/")
                        } else {
                            norm
                        };
                        let next_sem = if sem_prefix == "/" {
                            format!("/{sname}")
                        } else {
                            format!("{sem_prefix}/{sname}")
                        };
                        let next_uuid = format!("{uuid_prefix}/{suuid}");
                        walk_orig_hierarchy(
                            &child_rel,
                            &next_sem,
                            &next_uuid,
                            parsed_sheets,
                            sx_by_rel,
                            out_map,
                            depth + 1,
                        );
                    }
                }
            }
        }
        walk_orig_hierarchy(
            &root_rel,
            "/",
            &format!("/{orig_root_uuid}"),
            &parsed_sheets,
            &sx_by_rel,
            &mut orig_sheet_paths,
            0,
        );
    }

    let sym_lib_fname = format!("{safe_stem}.kicad_sym");
    let sym_lib_path = out_dir.join(&sym_lib_fname);
    {
        let mut sym_out = String::new();
        let _ = writeln!(sym_out, "(kicad_symbol_lib");
        let _ = writeln!(sym_out, "\t(version 20241209)");
        let _ = writeln!(sym_out, "\t(generator \"edl_import_kicad\")");
        let _ = writeln!(sym_out, "\t(generator_version \"2.1.0\")");
        for raw in embedded_syms.values() {
            for (li, line) in raw.lines().enumerate() {
                if li == 0 {
                    let _ = writeln!(sym_out, "\t{}", line.trim_start());
                } else {
                    let _ = writeln!(sym_out, "\t{line}");
                }
            }
        }
        let _ = writeln!(sym_out, ")");
        fs::write(&sym_lib_path, sym_out)
            .map_err(|e| format!("Failed to write '{}': {e}", sym_lib_path.display()))?;
    }

    // 3. Extract authoritative electrical netlist via `kicad-cli sch export netlist`
    let netlist = export_and_parse_kicad_netlist(root_sch)?;

    // 4. Parse all sheet visual geometry (placements, wires, buses, bus_entries, junctions, power symbols, labels, graphics, sheet_blocks)
    #[derive(Clone)]
    struct CompMeta {
        ref_id: String,
        lib_id: String,
        value: String,
        footprint: Option<String>,
        datasheet: Option<String>,
        dnp: bool,
        in_bom: bool,
        on_board: bool,
        props: Vec<(String, String)>,
        no_connects: BTreeSet<String>,
    }
    let mut comp_registry: BTreeMap<String, CompMeta> = BTreeMap::new();
    // Seed from netlist so every logical component is guaranteed present even on reused sheets
    for nc in &netlist.components {
        comp_registry.insert(
            nc.ref_id.clone(),
            CompMeta {
                ref_id: nc.ref_id.clone(),
                lib_id: nc.lib_id.clone(),
                value: nc.value.clone(),
                footprint: nc.footprint.clone(),
                datasheet: nc.datasheet.clone(),
                dnp: false,
                in_bom: true,
                on_board: true,
                props: nc.props.clone(),
                no_connects: BTreeSet::new(),
            },
        );
    }
    for (r, p) in &netlist.no_connect_pins {
        if let Some(cm) = comp_registry.get_mut(r) {
            cm.no_connects.insert(p.clone());
        }
    }

    let mut total_wires = 0usize;
    let mut total_powers = 0usize;
    let mut total_buses = 0usize;
    let mut schematic_blocks_edl = String::new();

    for (s_idx, (rel_file, sp, sx)) in parsed_sheets.iter().enumerate() {
        let items = sx_list(sx).ok_or_else(|| "Invalid schematic root".to_string())?;
        let fname = rel_file.as_str();
        let s_stem = sp
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Sheet");
        let sheet_ident = if s_idx == 0 {
            sanitize_ident(s_stem)
        } else {
            format!("Sheet_{s_idx}_{}", sanitize_ident(s_stem))
        };
        let sheet_orig_paths = orig_sheet_paths
            .get(rel_file)
            .cloned()
            .unwrap_or_default();

        let mut paper = "A4 landscape".to_string();
        let mut title: Option<String> = None;
        let mut rev: Option<String> = None;
        let mut company: Option<String> = None;

        let mut placements_edl = String::new();
        let mut powers_edl = String::new();
        let mut wires_edl = String::new();
        let mut buses_edl = String::new();
        let mut bus_entries_edl = String::new();
        let mut junctions_edl = String::new();
        let mut ncs_edl = String::new();
        let mut labels_edl = String::new();
        let mut text_boxes_edl = String::new();
        let mut graphics_edl = String::new();
        let mut sheets_edl = String::new();
        let mut bus_aliases_edl = String::new();

        for item in items.iter().skip(1) {
            match sx_head(item) {
                Some("paper") => {
                    if let Some(l) = sx_list(item) {
                        let sz = l.get(1).and_then(|v| v.as_text()).unwrap_or("A4");
                        let is_port = l.iter().skip(2).any(|v| v.as_text() == Some("portrait"));
                        paper = if is_port {
                            format!("{sz} portrait")
                        } else {
                            format!("{sz} landscape")
                        };
                    }
                }
                Some("title_block") => {
                    if let Some(l) = sx_list(item) {
                        for sub in l.iter().skip(1) {
                            match sx_head(sub) {
                                Some("title") => {
                                    title = sx_list(sub)
                                        .and_then(|v| v.get(1))
                                        .and_then(|v| v.as_text())
                                        .filter(|s| !s.is_empty())
                                        .map(|s| s.to_string());
                                }
                                Some("rev") => {
                                    rev = sx_list(sub)
                                        .and_then(|v| v.get(1))
                                        .and_then(|v| v.as_text())
                                        .filter(|s| !s.is_empty())
                                        .map(|s| s.to_string());
                                }
                                Some("company") => {
                                    company = sx_list(sub)
                                        .and_then(|v| v.get(1))
                                        .and_then(|v| v.as_text())
                                        .filter(|s| !s.is_empty())
                                        .map(|s| s.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Some("symbol") => {
                    let Some(lib_id) = find_child_text(item, "lib_name")
                        .or_else(|| find_child_text(item, "lib_id"))
                    else {
                        continue;
                    };
                    let (x, y, rot) = find_at_xyr(item).unwrap_or((0.0, 0.0, 0));
                    let mirror = find_child_text(item, "mirror");
                    let unit = find_child_text(item, "unit")
                        .and_then(|u| u.parse::<usize>().ok())
                        .unwrap_or(1);
                    let in_bom = find_child_text(item, "in_bom").map(|s| s != "no").unwrap_or(true);
                    let on_board = find_child_text(item, "on_board").map(|s| s != "no").unwrap_or(true);
                    let dnp = find_child_text(item, "dnp").map(|s| s == "yes").unwrap_or(false);

                    // Match symbol `(instances ...)` against THIS project's actual hierarchy UUID paths
                    // (immune to stale copy-pasted project refs AND captures every reused sub-sheet RefDes!).
                    let all_insts = collect_instance_refs(item);
                    let prop_ref = find_property(item, "Reference").unwrap_or_else(|| "U?".to_string());
                    let mut matched_instances: Vec<(String, String)> = Vec::new();
                    for (sem_path, orig_uuid_path) in &sheet_orig_paths {
                        if let Some(r) = all_insts
                            .iter()
                            .find(|(p, _)| p == orig_uuid_path)
                            .map(|(_, r)| r.clone())
                        {
                            matched_instances.push((sem_path.clone(), r));
                        }
                    }
                    let ref_id = matched_instances
                        .first()
                        .map(|(_, r)| r.clone())
                        .unwrap_or_else(|| prop_ref.clone());
                    let value = find_property(item, "Value").unwrap_or_default();
                    let footprint = find_property(item, "Footprint").filter(|s| !s.is_empty());
                    let datasheet = find_property(item, "Datasheet").filter(|s| !s.is_empty() && s != "~");
                    let ref_at = find_property_at(item, "Reference");
                    let val_at = find_property_at(item, "Value");

                    let is_power_sym = power_lib_ids.contains(&lib_id)
                        || ref_id.starts_with("#PWR")
                        || ref_id.starts_with("#FLG")
                        || lib_id.starts_with("power:");

                    let mir_s = mirror
                        .as_deref()
                        .map(|m| format!(" mirror: {m}"))
                        .unwrap_or_default();
                    let ori_s = if rot != 0 {
                        format!(" orient: {rot}")
                    } else {
                        String::new()
                    };

                    if is_power_sym {
                        total_powers += 1;
                        let pval_at_s = val_at
                            .map(|(vx, vy, vr)| format!(" val_at: ({vx:.4}mm, {vy:.4}mm, {vr})"))
                            .unwrap_or_default();
                        let pval_hide_s = if find_property_hide(item, "Value") {
                            " val_hide"
                        } else {
                            ""
                        };
                        let pval_just_s = find_property_justify(item, "Value")
                            .map(|j| format!(" val_justify: \"{}\"", esc_edl(&j)))
                            .unwrap_or_default();
                        let _ = writeln!(
                            powers_edl,
                            "        power \"{}\" \"{}\" ref: \"{}\" at: ({:.4}mm, {:.4}mm){ori_s}{mir_s}{pval_at_s}{pval_hide_s}{pval_just_s}",
                            esc_edl(&lib_id),
                            esc_edl(&value),
                            esc_edl(&ref_id),
                            x,
                            y
                        );
                    } else {
                        // Update ALL instance refs in `comp_registry` (so reused sub-sheet instances like C201 & C301 both get exact metadata!)
                        let mut refs_to_update: Vec<String> = matched_instances
                            .iter()
                            .map(|(_, r)| r.clone())
                            .collect();
                        if refs_to_update.is_empty() {
                            refs_to_update.push(ref_id.clone());
                        }
                        let extra_props = collect_extra_properties(item);
                        for rkey in &refs_to_update {
                            let entry = comp_registry.entry(rkey.clone()).or_insert_with(|| CompMeta {
                                ref_id: rkey.clone(),
                                lib_id: lib_id.clone(),
                                value: value.clone(),
                                footprint: footprint.clone(),
                                datasheet: datasheet.clone(),
                                dnp,
                                in_bom,
                                on_board,
                                props: Vec::new(),
                                no_connects: BTreeSet::new(),
                            });
                            entry.lib_id = lib_id.clone();
                            entry.dnp = dnp;
                            entry.in_bom = in_bom;
                            entry.on_board = on_board;
                            if entry.value.is_empty() {
                                entry.value = value.clone();
                            }
                            if entry.footprint.is_none() {
                                entry.footprint = footprint.clone();
                            }
                            if entry.datasheet.is_none() {
                                entry.datasheet = datasheet.clone();
                            }
                            for (pk, pv) in &extra_props {
                                if !entry.props.iter().any(|(k, _)| k == pk) {
                                    entry.props.push((pk.clone(), pv.clone()));
                                }
                            }
                        }

                        let unit_s = if unit > 1 {
                            format!(" unit: {unit}")
                        } else {
                            String::new()
                        };
                        let ref_at_s = ref_at
                            .map(|(rx, ry, rr)| format!(" ref_at: ({rx:.4}mm, {ry:.4}mm, {rr})"))
                            .unwrap_or_default();
                        let val_at_s = val_at
                            .map(|(vx, vy, vr)| format!(" val_at: ({vx:.4}mm, {vy:.4}mm, {vr})"))
                            .unwrap_or_default();
                        let inst_s = if matched_instances.len() > 1 {
                            let body = matched_instances
                                .iter()
                                .map(|(sp, r)| format!("\"{}\": \"{}\"", esc_edl(sp), esc_edl(r)))
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!(" instances: {{ {body} }}")
                        } else {
                            String::new()
                        };

                        // Capture non-default property visibility, font size, justification, and visible custom fields
                        let styled_fields = collect_styled_fields(item, x, y);
                        if styled_fields.is_empty() {
                            let _ = writeln!(
                                placements_edl,
                                "        place {}{unit_s} at: ({:.4}mm, {:.4}mm){ori_s}{mir_s}{ref_at_s}{val_at_s}{inst_s}",
                                fmt_ident_or_quoted(&ref_id),
                                x,
                                y
                            );
                        } else {
                            let _ = writeln!(
                                placements_edl,
                                "        place {}{unit_s} at: ({:.4}mm, {:.4}mm){ori_s}{mir_s}{ref_at_s}{val_at_s}{inst_s} {{",
                                fmt_ident_or_quoted(&ref_id),
                                x,
                                y
                            );
                            for fline in styled_fields {
                                let _ = writeln!(placements_edl, "            {fline}");
                            }
                            let _ = writeln!(placements_edl, "        }}");
                        }
                    }
                }
                Some("wire") => {
                    let pts = find_pts(item);
                    if pts.len() >= 2 {
                        total_wires += 1;
                        let chain = pts
                            .iter()
                            .map(|(x, y)| format!("({x:.4}mm, {y:.4}mm)"))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        let _ = writeln!(wires_edl, "        wire {chain}");
                    }
                }
                Some("bus") => {
                    let pts = find_pts(item);
                    if pts.len() >= 2 {
                        total_buses += 1;
                        let chain = pts
                            .iter()
                            .map(|(x, y)| format!("({x:.4}mm, {y:.4}mm)"))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        let _ = writeln!(buses_edl, "        bus {chain}");
                    }
                }
                Some("bus_entry") => {
                    if let (Some((x, y, _)), Some((dx, dy))) = (find_at_xyr(item), find_size_xy(item)) {
                        let _ = writeln!(
                            bus_entries_edl,
                            "        bus_entry at: ({x:.4}mm, {y:.4}mm) size: ({dx:.4}mm, {dy:.4}mm)"
                        );
                    }
                }
                Some("junction") => {
                    if let Some((x, y, _)) = find_at_xyr(item) {
                        let _ = writeln!(junctions_edl, "        junction at: ({x:.4}mm, {y:.4}mm)");
                    }
                }
                Some("no_connect") => {
                    if let Some((x, y, _)) = find_at_xyr(item) {
                        let _ = writeln!(ncs_edl, "        no_connect at: ({x:.4}mm, {y:.4}mm)");
                    }
                }
                Some(kind @ ("label" | "global_label" | "hierarchical_label" | "text")) => {
                    let txt = sx_list(item)
                        .and_then(|l| l.get(1))
                        .and_then(|v| v.as_text())
                        .unwrap_or("");
                    if let Some((x, y, rot)) = find_at_xyr(item) {
                        let sh = find_child_text(item, "shape")
                            .map(|s| format!(" shape: {s}"))
                            .unwrap_or_default();
                        let ori = if rot != 0 {
                            format!(" orient: {rot}")
                        } else {
                            String::new()
                        };
                        let just = find_justify(item)
                            .map(|j| format!(" justify: \"{}\"", esc_edl(&j)))
                            .unwrap_or_default();
                        let fsz = find_font_size(item)
                            .filter(|&sz| (sz - 1.27).abs() > 0.005)
                            .map(|sz| format!(" size: {sz:.4}mm"))
                            .unwrap_or_default();
                        let _ = writeln!(
                            labels_edl,
                            "        {kind} \"{}\"{sh} at: ({x:.4}mm, {y:.4}mm){ori}{just}{fsz}",
                            esc_edl(txt)
                        );
                    }
                }
                Some("text_box") => {
                    let txt = sx_list(item)
                        .and_then(|l| l.get(1))
                        .and_then(|v| v.as_text())
                        .unwrap_or("");
                    if let (Some((x, y, rot)), Some((w, h))) = (find_at_xyr(item), find_size_xy(item)) {
                        let ori = if rot != 0 {
                            format!(" orient: {rot}")
                        } else {
                            String::new()
                        };
                        let fsz = find_font_size(item)
                            .filter(|&sz| (sz - 1.27).abs() > 0.005)
                            .map(|sz| format!(" font: {sz:.4}mm"))
                            .unwrap_or_default();
                        let just = find_justify(item)
                            .map(|j| format!(" justify: \"{}\"", esc_edl(&j)))
                            .unwrap_or_default();
                        let (st, sw, sc) = find_stroke(item);
                        let fl = find_fill_color(item);
                        let st_s = st.map(|s| format!(" style: {s}")).unwrap_or_default();
                        let sw_s = sw.map(|v| format!(" width: {v:.4}mm")).unwrap_or_default();
                        let sc_s = sc.map(|c| format!(" color: \"{}\"", esc_edl(&c))).unwrap_or_default();
                        let fl_s = fl.map(|c| format!(" fill: \"{}\"", esc_edl(&c))).unwrap_or_default();
                        let _ = writeln!(
                            text_boxes_edl,
                            "        text_box \"{}\" at: ({x:.4}mm, {y:.4}mm) size: ({w:.4}mm, {h:.4}mm){ori}{fsz}{just}{st_s}{sw_s}{sc_s}{fl_s}",
                            esc_edl(txt)
                        );
                    }
                }
                Some("polyline") => {
                    let pts = find_pts(item);
                    if pts.len() >= 2 {
                        let chain = pts
                            .iter()
                            .map(|(x, y)| format!("({x:.4}mm, {y:.4}mm)"))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        let (st, sw, sc) = find_stroke(item);
                        let fl = find_fill_color(item);
                        let st_s = st.map(|s| format!(" style: {s}")).unwrap_or_default();
                        let sw_s = sw.map(|v| format!(" width: {v:.4}mm")).unwrap_or_default();
                        let sc_s = sc.map(|c| format!(" color: \"{}\"", esc_edl(&c))).unwrap_or_default();
                        let fl_s = fl.map(|c| format!(" fill: \"{}\"", esc_edl(&c))).unwrap_or_default();
                        let _ = writeln!(
                            graphics_edl,
                            "        graphic_line {chain}{st_s}{sw_s}{sc_s}{fl_s}"
                        );
                    }
                }
                Some("rectangle") => {
                    if let (Some((x0, y0)), Some((x1, y1))) = (find_xy_child(item, "start"), find_xy_child(item, "end")) {
                        let (st, sw, sc) = find_stroke(item);
                        let fl = find_fill_color(item);
                        let st_s = st.map(|s| format!(" style: {s}")).unwrap_or_default();
                        let sw_s = sw.map(|v| format!(" width: {v:.4}mm")).unwrap_or_default();
                        let sc_s = sc.map(|c| format!(" color: \"{}\"", esc_edl(&c))).unwrap_or_default();
                        let fl_s = fl.map(|c| format!(" fill: \"{}\"", esc_edl(&c))).unwrap_or_default();
                        let _ = writeln!(
                            graphics_edl,
                            "        graphic_rect ({x0:.4}mm, {y0:.4}mm) to ({x1:.4}mm, {y1:.4}mm){st_s}{sw_s}{sc_s}{fl_s}"
                        );
                    }
                }
                Some("bus_alias") => {
                    if let Some(l) = sx_list(item) {
                        let bname = l.get(1).and_then(|v| v.as_text()).unwrap_or("");
                        let mut members = Vec::new();
                        for sub in l.iter().skip(2) {
                            if sx_head(sub) == Some("members") {
                                if let Some(ml) = sx_list(sub) {
                                    for m in ml.iter().skip(1) {
                                        if let Some(ms) = m.as_text() {
                                            members.push(format!("\"{}\"", esc_edl(ms)));
                                        }
                                    }
                                }
                            }
                        }
                        if !bname.is_empty() && !members.is_empty() {
                            let _ = writeln!(
                                bus_aliases_edl,
                                "        bus_alias \"{}\": [{}]",
                                esc_edl(bname),
                                members.join(", ")
                            );
                        }
                    }
                }
                Some("sheet") => {
                    let sname = find_property(item, "Sheetname").unwrap_or_else(|| "SubSheet".to_string());
                    let sfile = find_property(item, "Sheetfile").unwrap_or_default();
                    let name_at_s = find_property_at(item, "Sheetname")
                        .map(|(nx, ny, nr)| format!(" name_at: ({nx:.4}mm, {ny:.4}mm, {nr})"))
                        .unwrap_or_default();
                    let file_at_s = find_property_at(item, "Sheetfile")
                        .map(|(fx, fy, fr)| format!(" file_at: ({fx:.4}mm, {fy:.4}mm, {fr})"))
                        .unwrap_or_default();
                    let file_just_s = find_property_justify(item, "Sheetfile")
                        .filter(|j| j != "left top")
                        .map(|j| format!(" file_justify: \"{}\"", esc_edl(&j)))
                        .unwrap_or_default();
                    let fill_s = find_fill_color(item)
                        .map(|c| format!(" fill: \"{}\"", esc_edl(&c)))
                        .unwrap_or_default();
                    if let (Some((x, y, _)), Some((w, h))) = (find_at_xyr(item), find_size_xy(item)) {
                        let mut pin_lines = String::new();
                        if let Some(l) = sx_list(item) {
                            for sub in l.iter().skip(1) {
                                if sx_head(sub) == Some("pin") {
                                    let sl = sx_list(sub).unwrap();
                                    let pn = sl.get(1).and_then(|v| v.as_text()).unwrap_or("");
                                    let ps = sl.get(2).and_then(|v| v.as_text()).unwrap_or("bidirectional");
                                    if let Some((px, py, prot)) = find_at_xyr(sub) {
                                        let po = if prot != 0 {
                                            format!(" orient: {prot}")
                                        } else {
                                            String::new()
                                        };
                                        let pj = find_justify(sub)
                                            .map(|j| format!(" justify: \"{}\"", esc_edl(&j)))
                                            .unwrap_or_default();
                                        let _ = writeln!(
                                            pin_lines,
                                            "            pin \"{}\" {ps} at: ({px:.4}mm, {py:.4}mm){po}{pj}",
                                            esc_edl(pn)
                                        );
                                    }
                                }
                            }
                        }
                        if pin_lines.is_empty() {
                            let _ = writeln!(
                                sheets_edl,
                                "        sheet_block \"{}\" file: \"{}\" at: ({x:.4}mm, {y:.4}mm) size: ({w:.4}mm, {h:.4}mm){name_at_s}{file_at_s}{file_just_s}{fill_s}",
                                esc_edl(&sname),
                                esc_edl(&sfile)
                            );
                        } else {
                            let _ = writeln!(
                                sheets_edl,
                                "        sheet_block \"{}\" file: \"{}\" at: ({x:.4}mm, {y:.4}mm) size: ({w:.4}mm, {h:.4}mm){name_at_s}{file_at_s}{file_just_s}{fill_s} {{",
                                esc_edl(&sname),
                                esc_edl(&sfile)
                            );
                            sheets_edl.push_str(&pin_lines);
                            let _ = writeln!(sheets_edl, "        }}");
                        }
                    }
                }
                _ => {}
            }
        }

        let _ = writeln!(schematic_blocks_edl, "    schematic {sheet_ident} {{");
        let _ = writeln!(schematic_blocks_edl, "        sheet: {paper}");
        let _ = writeln!(schematic_blocks_edl, "        file: \"{}\"", esc_edl(fname));
        if let Some(ref t) = title {
            let _ = writeln!(schematic_blocks_edl, "        title: \"{}\"", esc_edl(t));
        }
        if let Some(ref r) = rev {
            let _ = writeln!(schematic_blocks_edl, "        rev: \"{}\"", esc_edl(r));
        }
        if let Some(ref c) = company {
            let _ = writeln!(schematic_blocks_edl, "        author: \"{}\"", esc_edl(c));
        }
        let _ = writeln!(schematic_blocks_edl, "        routing {{ mode: explicit }}");
        if !placements_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Component placements");
            schematic_blocks_edl.push_str(&placements_edl);
        }
        if !powers_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Power & PWR_FLAG symbols");
            schematic_blocks_edl.push_str(&powers_edl);
        }
        if !wires_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Explicit wires");
            schematic_blocks_edl.push_str(&wires_edl);
        }
        if !bus_aliases_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Bus aliases");
            schematic_blocks_edl.push_str(&bus_aliases_edl);
        }
        if !buses_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Buses & entries");
            schematic_blocks_edl.push_str(&buses_edl);
            schematic_blocks_edl.push_str(&bus_entries_edl);
        }
        if !junctions_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Junctions");
            schematic_blocks_edl.push_str(&junctions_edl);
        }
        if !ncs_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Explicit no-connect markers");
            schematic_blocks_edl.push_str(&ncs_edl);
        }
        if !labels_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Net labels & annotations");
            schematic_blocks_edl.push_str(&labels_edl);
        }
        if !text_boxes_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Comment text boxes");
            schematic_blocks_edl.push_str(&text_boxes_edl);
        }
        if !graphics_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Graphical frames & lines");
            schematic_blocks_edl.push_str(&graphics_edl);
        }
        if !sheets_edl.is_empty() {
            let _ = writeln!(schematic_blocks_edl, "\n        // Hierarchical sheet blocks");
            schematic_blocks_edl.push_str(&sheets_edl);
        }
        let _ = writeln!(schematic_blocks_edl, "    }}\n");
    }

    // 5. Assemble complete Two-Layer Option-A `.edl` file
    let mut edl = String::new();
    let _ = writeln!(edl, "// Auto-imported from KiCad 9 schematic: {}", root_sch.display());
    let _ = writeln!(edl, "// Two-Layer Option-A EDL: exact semantic `circuit {{}}` netlist + exact geometric `schematic {{}}` layout.");
    let _ = writeln!(edl, "project \"{}\" {{", esc_edl(stem));
    let _ = writeln!(edl, "    units: mm");
    let _ = writeln!(edl, "    frame Sheet: Frame2");
    let _ = writeln!(edl, "    symbol_lib: \"{}\"\n", esc_edl(&sym_lib_fname));

    // Layer 1: Semantic circuit netlist
    let _ = writeln!(edl, "    circuit {safe_stem} {{");
    for cm in comp_registry.values() {
        let _ = writeln!(
            edl,
            "        component {}: \"{}\" {{",
            fmt_ident_or_quoted(&cm.ref_id),
            esc_edl(&cm.lib_id)
        );
        let _ = writeln!(edl, "            value: \"{}\"", esc_edl(&cm.value));
        if let Some(ref fp) = cm.footprint {
            let _ = writeln!(edl, "            footprint: \"{}\"", esc_edl(fp));
        }
        if let Some(ref ds) = cm.datasheet {
            let _ = writeln!(edl, "            datasheet: \"{}\"", esc_edl(ds));
        }
        if cm.dnp {
            let _ = writeln!(edl, "            dnp: true");
        }
        if !cm.in_bom {
            let _ = writeln!(edl, "            in_bom: false");
        }
        if !cm.on_board {
            let _ = writeln!(edl, "            on_board: false");
        }
        if !cm.no_connects.is_empty() {
            let ncs = cm
                .no_connects
                .iter()
                .map(|p| format!("\"{}\"", esc_edl(p)))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(edl, "            no_connect: [{ncs}]");
        }
        if !cm.props.is_empty() {
            let _ = writeln!(edl, "            props {{");
            for (pk, pv) in &cm.props {
                let _ = writeln!(edl, "                \"{}\": \"{}\"", esc_edl(pk), esc_edl(pv));
            }
            let _ = writeln!(edl, "            }}");
        }
        let _ = writeln!(edl, "        }}");
    }
    let _ = writeln!(edl);

    let mut emitted_nets = 0usize;
    for net in &netlist.nets {
        if net.nodes.len() < 2 {
            continue;
        }
        emitted_nets += 1;
        let connects = net
            .nodes
            .iter()
            .map(|(r, p)| format!("{}.{}", fmt_ident_or_quoted(r), fmt_pin_ref(p)))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            edl,
            "        net \"{}\" {{ connect: [{connects}] }}",
            esc_edl(&net.name)
        );
    }
    let _ = writeln!(edl, "    }}\n");

    // Layer 2: Exact geometric schematics
    edl.push_str(&schematic_blocks_edl);
    let _ = writeln!(edl, "}}");

    let edl_path = out_dir.join(format!("{safe_stem}.edl"));
    fs::write(&edl_path, edl)
        .map_err(|e| format!("Failed to write '{}': {e}", edl_path.display()))?;

    Ok(KiCadImportReport {
        edl_path,
        sym_lib_path,
        sheet_count: parsed_sheets.len(),
        component_count: comp_registry.len(),
        net_count: emitted_nets,
        wire_count: total_wires,
        power_count: total_powers,
        bus_count: total_buses,
    })
}

/// Compare two KiCad schematics by canonical electrical netlist (equivalence classes of connected `(ref, pin)` pairs),
/// independent of auto-generated anonymous net names like `Net-(C1-Pad1)`!
pub fn compare_kicad_netlists(
    orig_sch: &Path,
    roundtrip_sch: &Path,
) -> Result<NetlistCompareReport, String> {
    let orig = export_and_parse_kicad_netlist(orig_sch)?;
    let rt = export_and_parse_kicad_netlist(roundtrip_sch)?;

    let orig_comps: BTreeSet<String> = orig.components.iter().map(|c| c.ref_id.clone()).collect();
    let rt_comps: BTreeSet<String> = rt.components.iter().map(|c| c.ref_id.clone()).collect();

    let missing_components: Vec<String> = orig_comps.difference(&rt_comps).cloned().collect();
    let extra_components: Vec<String> = rt_comps.difference(&orig_comps).cloned().collect();

    // Canonical net connectivity = set of all unordered connected `(ref.pin, ref.pin)` pairs on multi-pin nets!
    let to_equiv_sets = |nl: &ParsedNetlist| -> BTreeSet<Vec<String>> {
        let mut sets = BTreeSet::new();
        for net in &nl.nets {
            if net.nodes.len() < 2 {
                continue;
            }
            let mut pins: Vec<String> = net
                .nodes
                .iter()
                .map(|(r, p)| format!("{r}.{p}"))
                .collect();
            pins.sort();
            pins.dedup();
            if pins.len() >= 2 {
                sets.insert(pins);
            }
        }
        sets
    };

    let orig_sets = to_equiv_sets(&orig);
    let rt_sets = to_equiv_sets(&rt);

    let missing_pin_pairs: Vec<String> = orig_sets
        .difference(&rt_sets)
        .map(|v| format!("[{}]", v.join(", ")))
        .collect();
    let extra_pin_pairs: Vec<String> = rt_sets
        .difference(&orig_sets)
        .map(|v| format!("[{}]", v.join(", ")))
        .collect();

    let exact_match = missing_components.is_empty()
        && extra_components.is_empty()
        && missing_pin_pairs.is_empty()
        && extra_pin_pairs.is_empty();

    Ok(NetlistCompareReport {
        orig_components: orig_comps.len(),
        rt_components: rt_comps.len(),
        orig_nets: orig_sets.len(),
        rt_nets: rt_sets.len(),
        missing_components,
        extra_components,
        missing_pin_pairs,
        extra_pin_pairs,
        exact_match,
    })
}

// =========================================================================
// Internal KiCad netlist & S-expression parsing helpers
// =========================================================================

pub(crate) struct NetlistComp {
    pub(crate) ref_id: String,
    pub(crate) lib_id: String,
    pub(crate) value: String,
    pub(crate) footprint: Option<String>,
    pub(crate) datasheet: Option<String>,
    pub(crate) props: Vec<(String, String)>,
}

pub(crate) struct NetlistNet {
    pub(crate) name: String,
    pub(crate) nodes: Vec<(String, String)>, // (ref, pin_number)
}

pub(crate) struct ParsedNetlist {
    pub(crate) components: Vec<NetlistComp>,
    pub(crate) nets: Vec<NetlistNet>,
    pub(crate) no_connect_pins: Vec<(String, String)>,
}

pub(crate) fn export_and_parse_kicad_netlist(sch_path: &Path) -> Result<ParsedNetlist, String> {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let seq = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!(
        "edl_nl_{}_{}_{}.net",
        std::process::id(),
        seq,
        sch_path.file_stem().and_then(|s| s.to_str()).unwrap_or("sch")
    ));
    let output = Command::new("kicad-cli")
        .args([
            "sch",
            "export",
            "netlist",
            "--format",
            "kicadsexpr",
            "-o",
            tmp.to_str().unwrap_or("/tmp/edl_nl.net"),
            sch_path.to_str().ok_or("Invalid schematic path")?,
        ])
        .output()
        .map_err(|e| format!("Failed to invoke `kicad-cli`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`kicad-cli sch export netlist` failed on '{}': {}",
            sch_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = fs::read_to_string(&tmp)
        .map_err(|e| format!("Failed to read netlist '{}': {e}", tmp.display()))?;
    let _ = fs::remove_file(&tmp);

    let sx = parse_sexpr(&text)?;
    let root = sx_list(&sx).ok_or("Invalid netlist S-expression")?;

    let mut components = Vec::new();
    let mut nets = Vec::new();
    let mut no_connect_pins = Vec::new();

    for sec in root.iter().skip(1) {
        match sx_head(sec) {
            Some("components") => {
                if let Some(clist) = sx_list(sec) {
                    for comp in clist.iter().skip(1) {
                        if sx_head(comp) != Some("comp") {
                            continue;
                        }
                        let ref_id = find_child_text(comp, "ref").unwrap_or_default();
                        if ref_id.is_empty() {
                            continue;
                        }
                        let value = find_child_text(comp, "value").unwrap_or_default();
                        let footprint = find_child_text(comp, "footprint").filter(|s| !s.is_empty());
                        let datasheet = find_child_text(comp, "datasheet").filter(|s| !s.is_empty() && s != "~");
                        let lib_id = sx_list(comp)
                            .and_then(|l| l.iter().find(|c| sx_head(c) == Some("libsource")))
                            .map(|ls| {
                                let lib = find_child_text(ls, "lib").unwrap_or_default();
                                let part = find_child_text(ls, "part").unwrap_or_default();
                                if lib.is_empty() {
                                    part
                                } else {
                                    format!("{lib}:{part}")
                                }
                            })
                            .unwrap_or_default();
                        let mut props = Vec::new();
                        if let Some(cl) = sx_list(comp) {
                            for sub in cl.iter().skip(1) {
                                if sx_head(sub) == Some("property") {
                                    let pk = find_child_text(sub, "name").unwrap_or_default();
                                    let pv = find_child_text(sub, "value").unwrap_or_default();
                                    if !pk.is_empty()
                                        && !pv.is_empty()
                                        && !matches!(
                                            pk.as_str(),
                                            "Sheetname"
                                                | "Sheetfile"
                                                | "ki_fp_filters"
                                                | "ki_keywords"
                                                | "ki_description"
                                                | "EDL_ID"
                                        )
                                    {
                                        props.push((pk, pv));
                                    }
                                }
                            }
                        }
                        components.push(NetlistComp {
                            ref_id,
                            lib_id,
                            value,
                            footprint,
                            datasheet,
                            props,
                        });
                    }
                }
            }
            Some("nets") => {
                if let Some(nlist) = sx_list(sec) {
                    for net in nlist.iter().skip(1) {
                        if sx_head(net) != Some("net") {
                            continue;
                        }
                        let name = find_child_text(net, "name").unwrap_or_default();
                        let mut nodes = Vec::new();
                        if let Some(nl) = sx_list(net) {
                            for sub in nl.iter().skip(1) {
                                if sx_head(sub) == Some("node") {
                                    let r = find_child_text(sub, "ref").unwrap_or_default();
                                    let p = find_child_text(sub, "pin").unwrap_or_default();
                                    let pty = find_child_text(sub, "pintype").unwrap_or_default();
                                    if !r.is_empty() && !p.is_empty() {
                                        if pty.contains("no_connect") {
                                            no_connect_pins.push((r.clone(), p.clone()));
                                        }
                                        nodes.push((r, p));
                                    }
                                }
                            }
                        }
                        nets.push(NetlistNet { name, nodes });
                    }
                }
            }
            _ => {}
        }
    }

    Ok(ParsedNetlist {
        components,
        nets,
        no_connect_pins,
    })
}

/// Extract every top-level `(symbol "<name>" ...)` inside `(lib_symbols ...)` verbatim with balanced parens.
fn extract_lib_symbols_raw(sch_text: &str) -> Vec<(String, String, bool)> {
    let Some(ls_start) = sch_text.find("(lib_symbols") else {
        return Vec::new();
    };
    let Some(ls_block) = extract_balanced(sch_text, ls_start) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let bytes = ls_block.as_bytes();
    let mut i = "(lib_symbols".len();
    let mut depth = 1i32;
    let mut in_str = false;
    let mut esc = false;

    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = true;
            i += 1;
            continue;
        }
        if b == b'(' {
            if depth == 1 && ls_block[i..].starts_with("(symbol \"") {
                if let Some(raw) = extract_balanced(ls_block, i) {
                    let name = extract_first_quoted(&raw["(symbol ".len()..]).unwrap_or_default();
                    let is_power = raw.contains("(power)");
                    if !name.is_empty() {
                        out.push((name, raw.to_string(), is_power));
                    }
                    i += raw.len();
                    continue;
                }
            }
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth <= 0 {
                break;
            }
        }
        i += 1;
    }
    out
}

fn extract_balanced(s: &str, start: usize) -> Option<&str> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (off, ch) in s[start..].char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
        } else if ch == '"' {
            in_str = true;
        } else if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(&s[start..start + off + 1]);
            }
        }
    }
    None
}

fn extract_first_quoted(s: &str) -> Option<String> {
    let rest = s.strip_prefix('"')?;
    let mut out = String::new();
    let mut esc = false;
    for ch in rest.chars() {
        if esc {
            out.push(ch);
            esc = false;
        } else if ch == '\\' {
            esc = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

trait SxExt {
    fn as_text(&self) -> Option<&str>;
    fn as_f64(&self) -> Option<f64>;
}

impl SxExt for Sx {
    fn as_text(&self) -> Option<&str> {
        match self {
            Sx::Atom(s) | Sx::Str(s) => Some(s.as_str()),
            Sx::List(_) => None,
        }
    }
    fn as_f64(&self) -> Option<f64> {
        self.as_text()?.parse::<f64>().ok()
    }
}

fn sx_list(sx: &Sx) -> Option<&[Sx]> {
    match sx {
        Sx::List(v) => Some(v.as_slice()),
        _ => None,
    }
}

fn sx_head(sx: &Sx) -> Option<&str> {
    sx_list(sx)?.first()?.as_text()
}

fn find_child_text(sx: &Sx, head: &str) -> Option<String> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some(head) {
            return sx_list(c)?.get(1)?.as_text().map(|s| s.to_string());
        }
    }
    None
}

fn find_property(sx: &Sx, prop_name: &str) -> Option<String> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("property") {
            let l = sx_list(c)?;
            if l.get(1)?.as_text() == Some(prop_name) {
                return l.get(2)?.as_text().map(|s| s.to_string());
            }
        }
    }
    None
}

fn collect_extra_properties(sx: &Sx) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(l) = sx_list(sx) else { return out };
    for c in l.iter().skip(1) {
        if sx_head(c) == Some("property") {
            let Some(pl) = sx_list(c) else { continue };
            let key = pl.get(1).and_then(|v| v.as_text()).unwrap_or("");
            let val = pl.get(2).and_then(|v| v.as_text()).unwrap_or("");
            if !key.is_empty()
                && !val.is_empty()
                && !matches!(
                    key,
                    "Reference"
                        | "Value"
                        | "Footprint"
                        | "Datasheet"
                        | "Description"
                        | "ki_fp_filters"
                        | "ki_keywords"
                        | "ki_description"
                        | "EDL_ID"
                )
            {
                out.push((key.to_string(), val.to_string()));
            }
        }
    }
    out
}

fn find_property_at(sx: &Sx, prop_name: &str) -> Option<(f64, f64, i32)> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("property") {
            let l = sx_list(c)?;
            if l.get(1)?.as_text() == Some(prop_name) {
                return find_at_xyr(c);
            }
        }
    }
    None
}

fn collect_instance_refs(sx: &Sx) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(items) = sx_list(sx) else { return out };
    for c in items.iter().skip(1) {
        if sx_head(c) == Some("instances") {
            if let Some(cl) = sx_list(c) {
                for proj in cl.iter().skip(1) {
                    if sx_head(proj) == Some("project") {
                        if let Some(pl) = sx_list(proj) {
                            for p in pl.iter().skip(2) {
                                if sx_head(p) == Some("path") {
                                    let pstr = sx_list(p)
                                        .and_then(|l| l.get(1))
                                        .and_then(|v| v.as_text())
                                        .unwrap_or("");
                                    if let Some(r) = find_child_text(p, "reference") {
                                        if !pstr.is_empty() && !r.is_empty() {
                                            out.push((pstr.to_string(), r));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn find_at_xyr(sx: &Sx) -> Option<(f64, f64, i32)> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("at") {
            let l = sx_list(c)?;
            let x = l.get(1)?.as_f64()?;
            let y = l.get(2)?.as_f64()?;
            let rot = l.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0).round() as i32;
            return Some((x, y, rot.rem_euclid(360)));
        }
    }
    None
}

fn find_size_xy(sx: &Sx) -> Option<(f64, f64)> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("size") {
            let l = sx_list(c)?;
            return Some((l.get(1)?.as_f64()?, l.get(2)?.as_f64()?));
        }
    }
    None
}

fn find_xy_child(sx: &Sx, head: &str) -> Option<(f64, f64)> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some(head) {
            let l = sx_list(c)?;
            return Some((l.get(1)?.as_f64()?, l.get(2)?.as_f64()?));
        }
    }
    None
}

fn find_pts(sx: &Sx) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let Some(l) = sx_list(sx) else { return pts };
    for c in l.iter().skip(1) {
        if sx_head(c) == Some("pts") {
            if let Some(pl) = sx_list(c) {
                for p in pl.iter().skip(1) {
                    if sx_head(p) == Some("xy") {
                        if let Some(xyl) = sx_list(p) {
                            if let (Some(x), Some(y)) =
                                (xyl.get(1).and_then(|v| v.as_f64()), xyl.get(2).and_then(|v| v.as_f64()))
                            {
                                pts.push((x, y));
                            }
                        }
                    }
                }
            }
        }
    }
    pts
}

fn find_justify(sx: &Sx) -> Option<String> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("effects") {
            for e in sx_list(c)?.iter().skip(1) {
                if sx_head(e) == Some("justify") {
                    let parts: Vec<&str> = sx_list(e)?
                        .iter()
                        .skip(1)
                        .filter_map(|v| v.as_text())
                        .collect();
                    if !parts.is_empty() {
                        return Some(parts.join(" "));
                    }
                }
            }
        }
    }
    None
}

fn find_font_size(sx: &Sx) -> Option<f64> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("effects") {
            for e in sx_list(c)?.iter().skip(1) {
                if sx_head(e) == Some("font") {
                    for f in sx_list(e)?.iter().skip(1) {
                        if sx_head(f) == Some("size") {
                            let l = sx_list(f)?;
                            return l.get(2).and_then(|v| v.as_f64()).or_else(|| l.get(1).and_then(|v| v.as_f64()));
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_hide(sx: &Sx) -> bool {
    let Some(items) = sx_list(sx) else { return false };
    for c in items.iter().skip(1) {
        if sx_head(c) == Some("effects") {
            if let Some(el) = sx_list(c) {
                for e in el.iter().skip(1) {
                    // KiCad 7/8 legacy bare atom: `(effects ... hide)`
                    if e.as_text() == Some("hide") {
                        return true;
                    }
                    // KiCad 9 explicit form: `(effects ... (hide yes))`
                    if sx_head(e) == Some("hide") {
                        let arg = sx_list(e).and_then(|l| l.get(1)).and_then(|v| v.as_text());
                        if arg != Some("no") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn find_property_node<'a>(sx: &'a Sx, prop_name: &str) -> Option<&'a Sx> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("property") {
            let l = sx_list(c)?;
            if l.get(1)?.as_text() == Some(prop_name) {
                return Some(c);
            }
        }
    }
    None
}

fn find_property_hide(sx: &Sx, prop_name: &str) -> bool {
    find_property_node(sx, prop_name).map(find_hide).unwrap_or(false)
}

fn find_property_justify(sx: &Sx, prop_name: &str) -> Option<String> {
    find_property_node(sx, prop_name).and_then(find_justify)
}

fn find_color_rgba(sx: &Sx) -> Option<String> {
    for c in sx_list(sx)?.iter().skip(1) {
        if sx_head(c) == Some("color") {
            let l = sx_list(c)?;
            if l.len() >= 5 {
                let a = l.get(4).and_then(|v| v.as_f64()).unwrap_or(0.0);
                if a > 0.0001 {
                    let parts: Vec<&str> = l.iter().skip(1).take(4).filter_map(|v| v.as_text()).collect();
                    if parts.len() == 4 {
                        return Some(parts.join(" "));
                    }
                }
            }
        }
    }
    None
}

fn find_stroke(sx: &Sx) -> (Option<String>, Option<f64>, Option<String>) {
    let Some(items) = sx_list(sx) else {
        return (None, None, None);
    };
    for c in items.iter().skip(1) {
        if sx_head(c) == Some("stroke") {
            let st = find_child_text(c, "type").filter(|t| t != "default");
            let sw = find_child_text(c, "width")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|&w| w > 0.0001);
            let sc = find_color_rgba(c);
            return (st, sw, sc);
        }
    }
    (None, None, None)
}

fn find_fill_color(sx: &Sx) -> Option<String> {
    let items = sx_list(sx)?;
    for c in items.iter().skip(1) {
        if sx_head(c) == Some("fill") {
            let ftype = find_child_text(c, "type").unwrap_or_default();
            if ftype == "none" {
                return None;
            }
            if let Some(rgba) = find_color_rgba(c) {
                return Some(rgba);
            }
        }
    }
    None
}

/// Collect EDL `field ...` lines for any symbol property that has non-default visibility, font size,
/// justification, or is a visible secondary/custom property (e.g. visible `Footprint`, `Description`, `Val`).
fn collect_styled_fields(sx: &Sx, sym_x: f64, sym_y: f64) -> Vec<String> {
    let mut out = Vec::new();
    let Some(items) = sx_list(sx) else { return out };
    for c in items.iter().skip(1) {
        if sx_head(c) != Some("property") {
            continue;
        }
        let Some(pl) = sx_list(c) else { continue };
        let name = pl.get(1).and_then(|v| v.as_text()).unwrap_or("");
        let val = pl.get(2).and_then(|v| v.as_text()).unwrap_or("");
        if name.is_empty()
            || matches!(
                name,
                "ki_fp_filters" | "ki_keywords" | "ki_description" | "EDL_ID"
            )
        {
            continue;
        }
        let hide = find_hide(c);
        let fsz = find_font_size(c).filter(|&sz| (sz - 1.27).abs() > 0.005);
        let just = find_justify(c);
        let (fx, fy, frot) = find_at_xyr(c).unwrap_or((sym_x, sym_y, 0));

        let is_primary = matches!(name, "Reference" | "Value");
        let needs_emit = if is_primary {
            hide || fsz.is_some() || just.is_some()
        } else {
            !hide && !val.is_empty()
        };
        if !needs_emit {
            continue;
        }
        let mut line = format!("field \"{}\"", esc_edl(name));
        if !is_primary {
            let _ = write!(line, " \"{}\"", esc_edl(val));
        }
        if hide {
            line.push_str(" hide");
        }
        let _ = write!(line, " at: ({fx:.4}mm, {fy:.4}mm, {frot})");
        if let Some(sz) = fsz {
            let _ = write!(line, " size: {sz:.4}mm");
        }
        if let Some(ref j) = just {
            let _ = write!(line, " justify: \"{}\"", esc_edl(j));
        }
        out.push(line);
    }
    out
}

fn sanitize_ident(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, 'P');
    }
    out
}

fn is_bare_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(
            s,
            "project" | "circuit" | "schematic" | "component" | "net" | "pin" | "place" | "wire" | "bus" | "power" | "label" | "text" | "use" | "module" | "verify"
        )
}

fn fmt_ident_or_quoted(s: &str) -> String {
    if is_bare_ident(s) {
        s.to_string()
    } else {
        format!("\"{}\"", esc_edl(s))
    }
}

fn fmt_pin_ref(s: &str) -> String {
    if is_bare_ident(s) || (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit())) {
        s.to_string()
    } else {
        format!("\"{}\"", esc_edl(s))
    }
}

fn esc_edl(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}
