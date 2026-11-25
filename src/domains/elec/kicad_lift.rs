//! Semantic KiCad -> AI-Native Intent EDL Lifter + `.edl.layout` Lockfile Splitter (EDL v3.0).
//!
//! Splits any real-world KiCad 8/9 schematic cleanly into:
//!   1. `<stem>.edl`        — SHORT, PURE AI-NATIVE INTENT (zero raw coordinates! Functional
//!                            groups, compact components/nets, `zone_local` + `symbol` routing,
//!                            lifted comments — typically 10x smaller than explicit dumps and
//!                            50–100x smaller than raw KiCad S-expressions!).
//!   2. `<stem>.edl.layout` — Machine-managed sidecar lockfile preserving exact human coordinates
//!                            (never fed into an LLM's context window).
//!   3. `<stem>.kicad_sym`  — Extracted verbatim symbol library.

use crate::core::sync::{parse_sexpr, Sx};
use crate::domains::elec::kicad_import::{export_and_parse_kicad_netlist, import_kicad_project};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct KiCadLiftReport {
    pub edl_path: PathBuf,
    pub layout_lock_path: PathBuf,
    pub sym_lib_path: PathBuf,
    pub orig_kicad_lines: usize,
    pub lockfile_lines: usize,
    pub intent_edl_lines: usize,
    pub component_count: usize,
    pub net_count: usize,
    pub group_count: usize,
}

/// Lift a KiCad 8/9 schematic into short, coordinate-free AI-native `.edl` + sidecar `.edl.layout` lockfile.
pub fn lift_kicad_project(root_sch: &Path, out_dir: &Path) -> Result<KiCadLiftReport, String> {
    if !root_sch.is_file() {
        return Err(format!("KiCad schematic not found: {}", root_sch.display()));
    }
    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create output dir '{}': {e}", out_dir.display()))?;

    let orig_kicad_lines = fs::read_to_string(root_sch)
        .map(|s| s.lines().count())
        .unwrap_or(0);

    // 1. Generate the exact `.kicad_sym` library + machine-managed `.edl.layout` lockfile
    let raw_rep = import_kicad_project(root_sch, out_dir)?;
    let lock_text = fs::read_to_string(&raw_rep.edl_path)
        .map_err(|e| format!("Failed to read generated lockfile: {e}"))?;
    let lockfile_lines = lock_text.lines().count();

    let stem = root_sch
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let safe_stem = sanitize_ident(stem);
    let sym_lib_fname = raw_rep
        .sym_lib_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("symbols.kicad_sym")
        .to_string();
    let lock_fname = format!("{safe_stem}.edl.layout");
    let layout_lock_path = out_dir.join(&lock_fname);
    fs::write(&layout_lock_path, &lock_text)
        .map_err(|e| format!("Failed to write lockfile '{}': {e}", layout_lock_path.display()))?;

    // Extract exact resolved lib_ids & notes from the parsed explicit EDL + root KiCad schematic
    let mut exact_lib_ids: BTreeMap<String, String> = BTreeMap::new();
    let mut comp_flags: BTreeMap<String, (bool, bool, bool)> = BTreeMap::new(); // (dnp, in_bom, on_board)
    {
        if let Ok(parsed_lock) = crate::syntax::parser::parse_project(&lock_fname, &lock_text) {
            for c in &parsed_lock.elec.circuits {
                for comp in &c.components {
                    exact_lib_ids.insert(comp.id.clone(), comp.category.clone());
                    comp_flags.insert(comp.id.clone(), (comp.dnp, comp.in_bom, comp.on_board));
                }
            }
        }
    }
    let lifted_notes = extract_sheet_notes(root_sch);

    // 2. Parse authoritative electrical netlist
    let nl = export_and_parse_kicad_netlist(root_sch)?;
    let mut nc_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (r, p) in &nl.no_connect_pins {
        nc_map.entry(r.clone()).or_default().insert(p.clone());
    }

    // 3. Semantic Graph Clustering (zero coordinates!)
    let mut pin_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut comp_nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut net_comps: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for net in &nl.nets {
        if net.nodes.len() < 2 {
            continue;
        }
        let mut seen = BTreeSet::new();
        for (r, _p) in &net.nodes {
            *pin_counts.entry(r.clone()).or_insert(0) += 1;
            if seen.insert(r.clone()) {
                comp_nets.entry(r.clone()).or_default().push(net.name.clone());
                net_comps.entry(net.name.clone()).or_default().push(r.clone());
            }
        }
    }

    let is_gnd_net = |n: &str| {
        let u = n.to_ascii_uppercase();
        u == "GND" || u.contains("GND") || u == "VSS" || u == "EARTH" || u == "CHASSIS"
    };
    let is_pwr_net = |n: &str| {
        let u = n.to_ascii_uppercase();
        u.starts_with('+')
            || u.starts_with('-')
            || u.starts_with("VCC")
            || u.starts_with("VDD")
            || u.starts_with("VBUS")
            || u.starts_with("VIN")
            || u.starts_with("VOUT")
            || u.starts_with("V3V3")
            || u.starts_with("V5V")
            || u.starts_with("3V3")
            || u.starts_with("5V")
    };

    // Classify components into Anchors vs Passives vs Mechanical/Logos
    let is_mech = |r: &str| {
        let u = r.to_ascii_uppercase();
        u.starts_with("LOGO")
            || u.starts_with("MH")
            || u.starts_with("FTG")
            || u.starts_with("FID")
            || u.starts_with("TP")
            || u.starts_with("H") && u[1..].chars().all(|c| c.is_ascii_digit())
    };
    let is_passive_ref = |r: &str| {
        let u = r.to_ascii_uppercase();
        let first = u.chars().next().unwrap_or('?');
        let rest_num = u[1..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);
        (matches!(first, 'R' | 'C' | 'L' | 'D' | 'F') && rest_num) || u.starts_with("FB")
    };

    let mut anchors: Vec<String> = Vec::new();
    let mut mechanicals: Vec<String> = Vec::new();
    let mut passives: Vec<String> = Vec::new();

    for c in &nl.components {
        let pc = pin_counts.get(&c.ref_id).copied().unwrap_or(0);
        if is_mech(&c.ref_id) || pc == 0 {
            mechanicals.push(c.ref_id.clone());
        } else if !is_passive_ref(&c.ref_id) || pc >= 3 {
            anchors.push(c.ref_id.clone());
        } else {
            passives.push(c.ref_id.clone());
        }
    }

    // Build signal-only adjacency between components for 2-hop passive -> anchor affinity
    let mut sig_adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (nname, members) in &net_comps {
        if is_gnd_net(nname) || is_pwr_net(nname) || members.len() > 16 {
            continue;
        }
        for a in members {
            for b in members {
                if a != b {
                    sig_adj.entry(a.clone()).or_default().push(b.clone());
                }
            }
        }
    }

    let anchor_set: BTreeSet<String> = anchors.iter().cloned().collect();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut group_titles: BTreeMap<String, String> = BTreeMap::new();

    // Group anchors into readable functional buckets:
    // - Major ICs (U*, IC*, >= 8 pins): own group each (or `Core_ICs` if many small ICs)
    // - Connectors (J*, P*, CN*, USB*): `Connectors` (or split into chunks of 4)
    // - Power / Active (Q*, VR*, T*): `Power_Stage`
    let mut anchor_to_group: BTreeMap<String, String> = BTreeMap::new();
    let mut ic_anchors: Vec<String> = Vec::new();
    let mut conn_anchors: Vec<String> = Vec::new();
    let mut other_anchors: Vec<String> = Vec::new();

    for a in &anchors {
        let u = a.to_ascii_uppercase();
        let pc = pin_counts.get(a).copied().unwrap_or(0);
        if u.starts_with('J') || u.starts_with("CN") || u.starts_with("USB") || u.starts_with('P') && u[1..].chars().all(|c| c.is_ascii_digit()) {
            conn_anchors.push(a.clone());
        } else if u.starts_with('U') || pc >= 8 {
            ic_anchors.push(a.clone());
        } else {
            other_anchors.push(a.clone());
        }
    }

    // Sort ICs by pin count descending so primary MCU/Hub IC comes first
    ic_anchors.sort_by(|a, b| {
        pin_counts
            .get(b)
            .copied()
            .unwrap_or(0)
            .cmp(&pin_counts.get(a).copied().unwrap_or(0))
            .then_with(|| a.cmp(b))
    });

    for (i, ic) in ic_anchors.iter().enumerate() {
        let gname = if ic_anchors.len() <= 4 {
            format!("IC_{}", sanitize_ident(ic))
        } else {
            format!("IC_Block_{}", (i / 3) + 1)
        };
        let val = nl
            .components
            .iter()
            .find(|c| &c.ref_id == ic)
            .map(|c| c.value.as_str())
            .unwrap_or(ic);
        group_titles
            .entry(gname.clone())
            .or_insert_with(|| format!("{ic} ({val})"));
        groups.entry(gname.clone()).or_default().push(ic.clone());
        anchor_to_group.insert(ic.clone(), gname);
    }

    for (i, cn) in conn_anchors.iter().enumerate() {
        let gname = if conn_anchors.len() <= 5 {
            "Connectors".to_string()
        } else {
            format!("Connectors_{}", (i / 4) + 1)
        };
        group_titles
            .entry(gname.clone())
            .or_insert_with(|| "I/O Connectors".to_string());
        groups.entry(gname.clone()).or_default().push(cn.clone());
        anchor_to_group.insert(cn.clone(), gname);
    }

    for oa in &other_anchors {
        let gname = "Active_Support".to_string();
        group_titles
            .entry(gname.clone())
            .or_insert_with(|| "Active & Support Devices".to_string());
        groups.entry(gname.clone()).or_default().push(oa.clone());
        anchor_to_group.insert(oa.clone(), gname);
    }

    // Attach each passive to its nearest signal-connected anchor (2-hop BFS) or to `Power_Decoupling`!
    let mut decoupling_caps: Vec<String> = Vec::new();
    for p in &passives {
        let pnets = comp_nets.get(p).cloned().unwrap_or_default();
        let all_pwr_gnd = !pnets.is_empty() && pnets.iter().all(|n| is_gnd_net(n) || is_pwr_net(n));
        if all_pwr_gnd && p.to_ascii_uppercase().starts_with('C') {
            decoupling_caps.push(p.clone());
            continue;
        }
        // BFS up to depth 2 to find closest anchor
        let mut found_anchor: Option<String> = None;
        let mut q: VecDeque<(String, usize)> = VecDeque::new();
        let mut vis: BTreeSet<String> = BTreeSet::new();
        q.push_back((p.clone(), 0));
        vis.insert(p.clone());
        while let Some((cur, depth)) = q.pop_front() {
            if depth > 0 && anchor_set.contains(&cur) {
                found_anchor = Some(cur);
                break;
            }
            if depth >= 2 {
                continue;
            }
            if let Some(nbrs) = sig_adj.get(&cur) {
                for n in nbrs {
                    if vis.insert(n.clone()) {
                        q.push_back((n.clone(), depth + 1));
                    }
                }
            }
        }
        let gname = found_anchor
            .and_then(|a| anchor_to_group.get(&a).cloned())
            .unwrap_or_else(|| "Passives".to_string());
        group_titles
            .entry(gname.clone())
            .or_insert_with(|| "Support Passives".to_string());
        groups.entry(gname).or_default().push(p.clone());
    }

    if !decoupling_caps.is_empty() {
        group_titles.insert(
            "Power_Decoupling".to_string(),
            "Power Rail Decoupling".to_string(),
        );
        groups.insert("Power_Decoupling".to_string(), decoupling_caps);
    }
    if !mechanicals.is_empty() {
        group_titles.insert(
            "Mechanical".to_string(),
            "Mechanical / Fiducials / Logos".to_string(),
        );
        groups.insert("Mechanical".to_string(), mechanicals);
    }

    // 4. Emit compact, pure AI-native Intent `.edl` (ZERO raw coordinates!)
    let mut edl = String::new();
    let _ = writeln!(edl, "// Lifted to AI-Native EDL v3.0 Intent from: {}", root_sch.display());
    let _ = writeln!(edl, "// Zero raw coordinates — compiler computes layout & routing deterministically.");
    let _ = writeln!(edl, "// Machine coordinate lockfile saved separately to: {lock_fname}");
    let _ = writeln!(edl, "project \"{}\" {{", esc_edl(stem));
    let _ = writeln!(edl, "    units: mm");
    let _ = writeln!(edl, "    frame Sheet: Frame2");
    let _ = writeln!(edl, "    symbol_lib: \"{}\"", esc_edl(&sym_lib_fname));
    let _ = writeln!(edl, "    layout_lock: \"{}\"\n", esc_edl(&lock_fname));

    let _ = writeln!(edl, "    circuit {safe_stem} {{");
    for c in &nl.components {
        let lib_id = exact_lib_ids
            .get(&c.ref_id)
            .cloned()
            .unwrap_or_else(|| c.lib_id.clone());
        let (dnp, in_bom, on_board) = comp_flags
            .get(&c.ref_id)
            .copied()
            .unwrap_or((false, true, true));
        let mut attrs = format!("value: \"{}\"", esc_edl(&c.value));
        if let Some(ref fp) = c.footprint {
            let _ = write!(attrs, " footprint: \"{}\"", esc_edl(fp));
        }
        if dnp {
            attrs.push_str(" dnp: true");
        }
        if !in_bom {
            attrs.push_str(" in_bom: false");
        }
        if !on_board {
            attrs.push_str(" on_board: false");
        }
        if let Some(ncs) = nc_map.get(&c.ref_id) {
            if !ncs.is_empty() {
                let nc_list = ncs
                    .iter()
                    .map(|p| format!("\"{}\"", esc_edl(p)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = write!(attrs, " no_connect: [{nc_list}]");
            }
        }
        let _ = writeln!(
            edl,
            "        component {}: \"{}\" {{ {attrs} }}",
            fmt_ident_or_quoted(&c.ref_id),
            esc_edl(&lib_id)
        );
    }
    let _ = writeln!(edl);

    let mut emitted_nets = 0usize;
    for net in &nl.nets {
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

    // Layer 2: Pure Intent Schematic (functional zones + 2D sheet-aware skyline packing, ZERO coordinates!)
    let target_sheet = if nl.components.len() <= 28 {
        "A3 landscape"
    } else if nl.components.len() <= 110 {
        "A2 landscape"
    } else {
        "A1 landscape"
    };
    let _ = writeln!(edl, "    schematic Main {{");
    let _ = writeln!(edl, "        sheet: {target_sheet}");
    let _ = writeln!(edl, "        title: \"{} (EDL v3.0 Intent)\"", esc_edl(stem));
    let _ = writeln!(edl, "        routing {{ default: zone_local  power_nets: symbol  ground_nets: symbol }}\n");

    let mut ordered_gnames: Vec<String> = Vec::new();
    for (gname, members) in &groups {
        if members.is_empty() {
            continue;
        }
        ordered_gnames.push(gname.clone());
        let title = group_titles
            .get(gname)
            .cloned()
            .unwrap_or_else(|| gname.clone());
        let _ = writeln!(edl, "        group {gname} {{");
        let _ = writeln!(edl, "            title: \"{}\"", esc_edl(&title));
        // Format into compact 2D hstack rows (6 per row for dense decoupling banks, 5 for general blocks)
        let row_cap = if gname == "Power_Decoupling" { 6 } else { 5 };
        if members.len() <= row_cap {
            let ms = members
                .iter()
                .map(|m| fmt_ident_or_quoted(m))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(edl, "            hstack {{ {ms} }}");
        } else {
            let _ = writeln!(edl, "            vstack {{");
            for chunk in members.chunks(row_cap) {
                let ms = chunk
                    .iter()
                    .map(|m| fmt_ident_or_quoted(m))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(edl, "                hstack {{ {ms} }}");
            }
            let _ = writeln!(edl, "            }}");
        }
        let _ = writeln!(edl, "        }}");
    }

    for note in &lifted_notes {
        let _ = writeln!(edl, "        note \"{}\"", esc_edl(note));
    }

    if !ordered_gnames.is_empty() {
        let _ = writeln!(edl, "\n        layout {{");
        let _ = writeln!(edl, "            pack: skyline");
        let _ = writeln!(edl, "        }}");
    }

    let _ = writeln!(edl, "    }}");
    let _ = writeln!(edl, "}}");

    let intent_edl_lines = edl.lines().count();
    let edl_path = out_dir.join(format!("{safe_stem}.edl"));
    fs::write(&edl_path, &edl)
        .map_err(|e| format!("Failed to write intent EDL '{}': {e}", edl_path.display()))?;

    Ok(KiCadLiftReport {
        edl_path,
        layout_lock_path,
        sym_lib_path: raw_rep.sym_lib_path,
        orig_kicad_lines,
        lockfile_lines,
        intent_edl_lines,
        component_count: nl.components.len(),
        net_count: emitted_nets,
        group_count: ordered_gnames.len(),
    })
}

fn extract_sheet_notes(root_sch: &Path) -> Vec<String> {
    let mut notes = Vec::new();
    let Ok(text) = fs::read_to_string(root_sch) else {
        return notes;
    };
    let Ok(sx) = parse_sexpr(&text) else {
        return notes;
    };
    let Sx::List(items) = sx else {
        return notes;
    };
    for item in items.iter().skip(1) {
        if let Sx::List(l) = item {
            if let Some(Sx::Atom(h)) = l.first() {
                if h == "text_box" {
                    if let Some(Sx::Str(t)) = l.get(1) {
                        let cleaned = t.trim().replace('\n', " ");
                        if !cleaned.is_empty() {
                            notes.push(cleaned);
                        }
                    }
                }
            }
        }
    }
    notes
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
            "project" | "circuit" | "schematic" | "component" | "net" | "pin" | "place" | "wire" | "bus" | "power" | "label" | "text" | "use" | "module" | "verify" | "note" | "array" | "decouple" | "pullup" | "pulldown" | "divider" | "series" | "rc_filter"
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
