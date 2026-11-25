//! Native KiCad 9 `.kicad_sym` S-expression library reader, search index, and geometry transformer.
//!
//! Lets EDL load any of the ~20,000+ open-source KiCad symbols (or user/EasyEDA-exported `.kicad_sym` files)
//! directly by `"Library:Symbol"` reference — with full `extends` inheritance, multi-unit extraction,
//! exact pin coordinates/electrical roles, 90°/180°/270° rotation + X/Y mirroring, and vector graphics
//! shared between EDL's SVG preview and KiCad 9 `.kicad_sch` export.

use crate::core::geom::{Point2, Rect2, Side};
use crate::core::ir::PortRole;
use crate::core::sync::{parse_sexpr, Sx};
use crate::domains::elec::symbols::{SymPrim, SymbolGraphic};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Optional mirroring in symbol local space (matches KiCad `(mirror x)` / `(mirror y)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymMirror {
    X,
    Y,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiCadLibPin {
    pub name: String,
    pub number: String,
    pub etype: String,
    pub role: PortRole,
    pub unit: usize,
    /// Tip coordinate in KiCad symbol space (mm, Y-up).
    pub x: f64,
    pub y: f64,
    /// KiCad pin orientation in degrees (0=East, 90=North, 180=West, 270=South — points from tip into body).
    pub angle: f64,
    pub length: f64,
    pub hide: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KiCadLibPrim {
    Rectangle {
        unit: usize,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke_mm: f64,
        fill: String, // "none" | "outline" | "background"
    },
    Polyline {
        unit: usize,
        pts: Vec<[f64; 2]>,
        stroke_mm: f64,
        fill: String,
    },
    Circle {
        unit: usize,
        cx: f64,
        cy: f64,
        radius: f64,
        stroke_mm: f64,
        fill: String,
    },
    Arc {
        unit: usize,
        start: [f64; 2],
        mid: [f64; 2],
        end: [f64; 2],
        stroke_mm: f64,
        fill: String,
    },
    Text {
        unit: usize,
        text: String,
        x: f64,
        y: f64,
        angle: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiCadLibSymbol {
    pub lib_id: String,
    pub library: String,
    pub name: String,
    pub reference_prefix: String,
    pub default_value: String,
    pub default_footprint: Option<String>,
    pub description: String,
    pub keywords: String,
    pub unit_count: usize,
    pub hide_pin_names: bool,
    pub hide_pin_numbers: bool,
    pub pins: Vec<KiCadLibPin>,
    pub primitives: Vec<KiCadLibPrim>,
    /// Optional verbatim KiCad `(symbol ...)` S-expression for 100% lossless round-trip export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_sexpr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSearchHit {
    pub lib_id: String,
    pub library: String,
    pub name: String,
    pub description: String,
    pub keywords: String,
}

/// Default directories searched for `.kicad_sym` libraries.
pub fn default_symbol_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(env_dir) = std::env::var("EDL_KICAD_SYM_DIR") {
        for part in env_dir.split(':') {
            let p = PathBuf::from(part);
            if p.is_dir() {
                dirs.push(p);
            }
        }
    }
    for std_dir in [
        "/usr/share/kicad/symbols",
        "/usr/local/share/kicad/symbols",
        "/Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols",
    ] {
        let p = PathBuf::from(std_dir);
        if p.is_dir() && !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    dirs
}

/// Collect all available `.kicad_sym` library files (extra project libs first, then system dirs).
pub fn list_symbol_library_files(extra_libs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for p in extra_libs {
        if p.is_file() {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                if seen.insert(stem.to_string()) {
                    out.push((stem.to_string(), p.clone()));
                }
            }
        } else if p.is_dir() {
            collect_dir_libs(p, &mut seen, &mut out);
        }
    }
    for dir in default_symbol_dirs() {
        collect_dir_libs(&dir, &mut seen, &mut out);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect_dir_libs(
    dir: &Path,
    seen: &mut std::collections::BTreeSet<String>,
    out: &mut Vec<(String, PathBuf)>,
) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("kicad_sym") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if seen.insert(stem.to_string()) {
                    out.push((stem.to_string(), path));
                }
            }
        }
    }
}

/// Fast text-level search across all `.kicad_sym` libraries (searches name, library, description, keywords).
pub fn search_symbols(query: &str, extra_libs: &[PathBuf], limit: usize) -> Vec<SymbolSearchHit> {
    let q_terms: Vec<String> = query
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if q_terms.is_empty() {
        return Vec::new();
    }

    let mut exact_hits = Vec::new();
    let mut partial_hits = Vec::new();
    let q_full = query.trim().to_lowercase();

    for (lib_name, path) in list_symbol_library_files(extra_libs) {
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let lib_lower = lib_name.to_lowercase();
        let mut cur_name: Option<String> = None;
        let mut cur_desc = String::new();
        let mut cur_kw = String::new();

        let flush = |name: Option<String>,
                     desc: &str,
                     kw: &str,
                     exact: &mut Vec<SymbolSearchHit>,
                     partial: &mut Vec<SymbolSearchHit>| {
            let Some(sym_name) = name else { return };
            let lib_id = format!("{lib_name}:{sym_name}");
            let hay = format!(
                "{} {} {} {}",
                lib_lower,
                sym_name.to_lowercase(),
                desc.to_lowercase(),
                kw.to_lowercase()
            );
            if q_terms.iter().all(|t| hay.contains(t.as_str())) {
                let hit = SymbolSearchHit {
                    lib_id,
                    library: lib_name.clone(),
                    name: sym_name.clone(),
                    description: desc.to_string(),
                    keywords: kw.to_string(),
                };
                let nl = sym_name.to_lowercase();
                if nl == q_full || hit.lib_id.to_lowercase() == q_full {
                    exact.push(hit);
                } else {
                    partial.push(hit);
                }
            }
        };

        for line in content.lines() {
            // Top-level symbols in KiCad 9 `.kicad_sym` are indented by 1 tab or 2 spaces (subsymbols by 2+ tabs).
            let trimmed = line.trim_start();
            let indent_tabs = line.bytes().take_while(|&b| b == b'\t').count();
            let indent_spaces = line.bytes().take_while(|&b| b == b' ').count();
            let is_top_sym = trimmed.starts_with("(symbol \"")
                && (indent_tabs == 1 || (indent_tabs == 0 && indent_spaces <= 2));

            if is_top_sym {
                flush(
                    cur_name.take(),
                    &cur_desc,
                    &cur_kw,
                    &mut exact_hits,
                    &mut partial_hits,
                );
                cur_desc.clear();
                cur_kw.clear();
                if let Some(s) = extract_quoted_after(trimmed, "(symbol \"") {
                    cur_name = Some(s);
                }
            } else if cur_name.is_some() {
                if let Some(d) = extract_quoted_after(trimmed, "(property \"Description\" \"") {
                    cur_desc = d;
                } else if let Some(k) = extract_quoted_after(trimmed, "(property \"ki_keywords\" \"") {
                    cur_kw = k;
                }
            }
        }
        flush(
            cur_name.take(),
            &cur_desc,
            &cur_kw,
            &mut exact_hits,
            &mut partial_hits,
        );
    }

    exact_hits.sort_by(|a, b| a.lib_id.cmp(&b.lib_id));
    partial_hits.sort_by(|a, b| a.name.len().cmp(&b.name.len()).then(a.lib_id.cmp(&b.lib_id)));
    exact_hits.extend(partial_hits);
    exact_hits.truncate(limit);
    exact_hits
}

fn extract_quoted_after(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?;
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

/// Load a single symbol by `"Library:Symbol"` (or bare project-embedded `"Symbol"`) reference,
/// resolving project-local `.kicad_sym` bundles first, then system KiCad libraries.
pub fn load_symbol(lib_ref: &str, extra_libs: &[PathBuf]) -> Result<KiCadLibSymbol, String> {
    // 1. Check explicitly declared project `.kicad_sym` files for an exact full-name match
    //    (supports single bundled `<project>.kicad_sym` extracted from a KiCad schematic's `lib_symbols`,
    //    including both `"Lib:Part"` and bare `"Logo_2"` symbols!).
    for p in extra_libs {
        if p.is_file() {
            if let Ok(content) = fs::read_to_string(p) {
                if let Some(raw_sym) = extract_top_symbol_sexpr(&content, lib_ref) {
                    let sx = parse_sexpr(&raw_sym)?;
                    let (lib_name, sym_name) = lib_ref
                        .split_once(':')
                        .unwrap_or(("", lib_ref));
                    let mut sym = resolve_symbol_sx(lib_name, &content, sym_name, &sx, 0)?;
                    sym.lib_id = lib_ref.to_string();
                    sym.raw_sexpr = Some(raw_sym);
                    return Ok(sym);
                }
            }
        }
    }

    let (lib_name, sym_name) = lib_ref
        .split_once(':')
        .ok_or_else(|| format!("Invalid KiCad symbol reference '{lib_ref}' (expected 'Library:Symbol' or embedded symbol in `symbol_lib`)"))?;

    let libs = list_symbol_library_files(extra_libs);
    let path = libs
        .iter()
        .find(|(name, _)| name == lib_name)
        .map(|(_, p)| p.clone())
        .ok_or_else(|| {
            format!(
                "KiCad symbol library '{lib_name}.kicad_sym' not found in {} search path(s)",
                libs.len()
            )
        })?;

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read '{}': {e}", path.display()))?;
    load_symbol_from_lib_str(lib_name, &content, sym_name)
}

/// Extract the raw verbatim `(symbol "<lib_ref>" ...)` S-expression from any project `extra_libs`
/// or system `.kicad_sym` library (used by lossless Option-A `.kicad_sch` export).
pub fn load_raw_symbol_sexpr(lib_ref: &str, extra_libs: &[PathBuf]) -> Option<String> {
    for p in extra_libs {
        if p.is_file() {
            if let Ok(content) = fs::read_to_string(p) {
                if let Some(raw) = extract_top_symbol_sexpr(&content, lib_ref) {
                    return Some(raw);
                }
            }
        }
    }
    if let Some((lib_name, sym_name)) = lib_ref.split_once(':') {
        let libs = list_symbol_library_files(extra_libs);
        if let Some((_, path)) = libs.iter().find(|(name, _)| name == lib_name) {
            if let Ok(content) = fs::read_to_string(path) {
                if let Some(raw) = extract_top_symbol_sexpr(&content, sym_name) {
                    // Qualify top-level symbol header `(symbol "Sym"` -> `(symbol "Lib:Sym"` for `.kicad_sch`
                    let old_hdr = format!("(symbol \"{sym_name}\"");
                    let new_hdr = format!("(symbol \"{lib_ref}\"");
                    return Some(raw.replacen(&old_hdr, &new_hdr, 1));
                }
            }
        }
    }
    None
}

/// Parse a symbol directly from a `.kicad_sym` file's text (public for unit testing without disk deps).
pub fn load_symbol_from_lib_str(
    lib_name: &str,
    content: &str,
    sym_name: &str,
) -> Result<KiCadLibSymbol, String> {
    // Extract raw S-expression for `sym_name` (and follow `extends` chain without parsing unrelated symbols!).
    let raw_sym = extract_top_symbol_sexpr(content, sym_name)
        .ok_or_else(|| format!("Symbol '{sym_name}' not found in library '{lib_name}'"))?;
    let sx = parse_sexpr(&raw_sym)?;

    let mut sym = resolve_symbol_sx(lib_name, content, sym_name, &sx, 0)?;
    if sym.raw_sexpr.is_none() {
        let old_hdr = format!("(symbol \"{sym_name}\"");
        let new_hdr = format!("(symbol \"{lib_name}:{sym_name}\"");
        sym.raw_sexpr = Some(raw_sym.replacen(&old_hdr, &new_hdr, 1));
    }
    Ok(sym)
}

fn resolve_symbol_sx(
    lib_name: &str,
    content: &str,
    sym_name: &str,
    sx: &Sx,
    depth: usize,
) -> Result<KiCadLibSymbol, String> {
    if depth > 8 {
        return Err(format!("Circular `extends` chain while loading '{lib_name}:{sym_name}'"));
    }
    let items = sx_list(sx).ok_or_else(|| "Invalid symbol S-expression".to_string())?;

    // Check for `(extends "BaseSymbol")`
    let extends_name = items.iter().skip(1).find_map(|c| {
        if sx_head(c) == Some("extends") {
            sx_list(c)?.get(1)?.as_text().map(|s| s.to_string())
        } else {
            None
        }
    });

    let mut base = if let Some(parent_name) = extends_name {
        let parent_raw = extract_top_symbol_sexpr(content, &parent_name).ok_or_else(|| {
            format!(
                "Base symbol '{parent_name}' (extended by '{sym_name}') not found in '{lib_name}'"
            )
        })?;
        let parent_sx = parse_sexpr(&parent_raw)?;
        resolve_symbol_sx(lib_name, content, &parent_name, &parent_sx, depth + 1)?
    } else {
        KiCadLibSymbol {
            lib_id: format!("{lib_name}:{sym_name}"),
            library: lib_name.to_string(),
            name: sym_name.to_string(),
            reference_prefix: "U".to_string(),
            default_value: sym_name.to_string(),
            default_footprint: None,
            description: String::new(),
            keywords: String::new(),
            unit_count: 1,
            hide_pin_names: false,
            hide_pin_numbers: false,
            pins: Vec::new(),
            primitives: Vec::new(),
            raw_sexpr: None,
        }
    };

    base.lib_id = format!("{lib_name}:{sym_name}");
    base.library = lib_name.to_string();
    base.name = sym_name.to_string();

    for child in items.iter().skip(1) {
        match sx_head(child) {
            Some("pin_names") => {
                if has_hide_yes(child) {
                    base.hide_pin_names = true;
                }
            }
            Some("pin_numbers") => {
                if has_hide_yes(child) {
                    base.hide_pin_numbers = true;
                }
            }
            Some("property") => {
                let l = sx_list(child).unwrap();
                let key = l.get(1).and_then(|v| v.as_text()).unwrap_or("");
                let val = l.get(2).and_then(|v| v.as_text()).unwrap_or("");
                match key {
                    "Reference" if !val.is_empty() => base.reference_prefix = val.to_string(),
                    "Value" if !val.is_empty() => base.default_value = val.to_string(),
                    "Footprint" if !val.is_empty() => base.default_footprint = Some(val.to_string()),
                    "Description" => base.description = val.to_string(),
                    "ki_keywords" => base.keywords = val.to_string(),
                    _ => {}
                }
            }
            Some("symbol") => {
                // Sub-symbol `<Name>_<unit>_<style>` (only take style 0 or 1 — standard IEEE/IEC body!)
                let l = sx_list(child).unwrap();
                let sub_name = l.get(1).and_then(|v| v.as_text()).unwrap_or("");
                let (unit, style) = parse_subsymbol_unit_style(sub_name);
                if style > 1 {
                    continue;
                }
                if unit > base.unit_count {
                    base.unit_count = unit;
                }
                for item in l.iter().skip(2) {
                    match sx_head(item) {
                        Some("pin") => {
                            if let Some(pin) = parse_pin_sx(item, unit) {
                                base.pins.push(pin);
                            }
                        }
                        Some("rectangle") => {
                            if let Some(prim) = parse_rect_sx(item, unit) {
                                base.primitives.push(prim);
                            }
                        }
                        Some("polyline") => {
                            if let Some(prim) = parse_polyline_sx(item, unit) {
                                base.primitives.push(prim);
                            }
                        }
                        Some("circle") => {
                            if let Some(prim) = parse_circle_sx(item, unit) {
                                base.primitives.push(prim);
                            }
                        }
                        Some("arc") => {
                            if let Some(prim) = parse_arc_sx(item, unit) {
                                base.primitives.push(prim);
                            }
                        }
                        Some("text") => {
                            if let Some(prim) = parse_text_sx(item, unit) {
                                base.primitives.push(prim);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(base)
}

/// Fast balanced-paren extractor for a single top-level `(symbol "<name>" ...)` inside a `.kicad_sym`.
fn extract_top_symbol_sexpr(content: &str, sym_name: &str) -> Option<String> {
    let needle = format!("(symbol \"{sym_name}\"");
    let mut search_from = 0usize;
    while let Some(rel) = content[search_from..].find(&needle) {
        let start = search_from + rel;
        // Verify this is a top-level symbol (preceded on its line only by <= 1 tab or <= 2 spaces).
        let line_start = content[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prefix = &content[line_start..start];
        let is_top = prefix.chars().all(|c| c == '\t' || c == ' ')
            && (prefix == "\t" || prefix.len() <= 2);
        // Also ensure next char after needle is whitespace or ')' or newline (not a longer symbol prefix)
        let after_idx = start + needle.len();
        let next_ok = content[after_idx..]
            .chars()
            .next()
            .map(|c| c.is_whitespace() || c == ')')
            .unwrap_or(true);
        if is_top && next_ok {
            let mut depth = 0i32;
            let mut in_str = false;
            let mut esc = false;
            for (off, ch) in content[start..].char_indices() {
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
                        return Some(content[start..start + off + 1].to_string());
                    }
                }
            }
        }
        search_from = start + needle.len();
    }
    None
}

fn parse_subsymbol_unit_style(sub_name: &str) -> (usize, usize) {
    let parts: Vec<&str> = sub_name.rsplitn(3, '_').collect();
    if parts.len() >= 2 {
        let style = parts[0].parse::<usize>().unwrap_or(1);
        let unit = parts[1].parse::<usize>().unwrap_or(1);
        (unit, style)
    } else {
        (1, 1)
    }
}

fn parse_pin_sx(sx: &Sx, unit: usize) -> Option<KiCadLibPin> {
    let l = sx_list(sx)?;
    let etype = l.get(1)?.as_text()?.to_string();
    let (mut x, mut y, mut angle, mut length) = (0.0, 0.0, 0.0, 2.54);
    let mut name = String::new();
    let mut number = String::new();
    let mut hide = false;

    for it in l.iter().skip(2) {
        if it.as_text() == Some("hide") {
            hide = true;
            continue;
        }
        match sx_head(it) {
            Some("at") => {
                let a = sx_list(it)?;
                x = a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                y = a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
                angle = a.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
            }
            Some("length") => {
                length = sx_list(it)?.get(1).and_then(|v| v.as_f64()).unwrap_or(2.54);
            }
            Some("name") => {
                name = sx_list(it)?.get(1).and_then(|v| v.as_text()).unwrap_or("").to_string();
            }
            Some("number") => {
                number = sx_list(it)?.get(1).and_then(|v| v.as_text()).unwrap_or("").to_string();
            }
            Some("hide") => {
                if sx_list(it)?.get(1).and_then(|v| v.as_text()) == Some("yes") {
                    hide = true;
                }
            }
            _ => {}
        }
    }

    let clean_name = if name == "~" || name.is_empty() {
        number.clone()
    } else {
        // Strip KiCad overbar syntax `~{RST}` -> `RST` for clean EDL identifiers while preserving pin number
        name.replace("~{", "").replace('}', "")
    };

    let role = map_kicad_etype(&etype, &clean_name);

    Some(KiCadLibPin {
        name: clean_name,
        number,
        etype,
        role,
        unit,
        x,
        y,
        angle,
        length,
        hide,
    })
}

fn map_kicad_etype(etype: &str, name: &str) -> PortRole {
    let nl = name.to_uppercase();
    match etype {
        "power_in" => {
            if nl == "GND" || nl.starts_with("GND") || nl == "VSS" || nl == "AGND" || nl == "PGND" || nl == "DGND" {
                PortRole::Ground
            } else {
                PortRole::PowerIn
            }
        }
        "power_out" => PortRole::PowerOut,
        "input" => PortRole::Input,
        "output" | "tri_state" | "open_collector" | "open_emitter" => PortRole::Output,
        "bidirectional" => PortRole::Bidir,
        "no_connect" => PortRole::Nc,
        _ => PortRole::Passive,
    }
}

fn parse_stroke_fill(items: &[Sx]) -> (f64, String) {
    let mut stroke = 0.254;
    let mut fill = "none".to_string();
    for it in items {
        match sx_head(it) {
            Some("stroke") => {
                if let Some(sl) = sx_list(it) {
                    for s in sl.iter().skip(1) {
                        if sx_head(s) == Some("width") {
                            let w = sx_list(s).and_then(|wl| wl.get(1)).and_then(|v| v.as_f64()).unwrap_or(0.254);
                            stroke = if w <= 0.001 { 0.254 } else { w };
                        }
                    }
                }
            }
            Some("fill") => {
                if let Some(fl) = sx_list(it) {
                    for f in fl.iter().skip(1) {
                        if sx_head(f) == Some("type") {
                            if let Some(t) = sx_list(f).and_then(|tl| tl.get(1)).and_then(|v| v.as_text()) {
                                fill = t.to_string();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (stroke, fill)
}

fn parse_rect_sx(sx: &Sx, unit: usize) -> Option<KiCadLibPrim> {
    let l = sx_list(sx)?;
    let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for it in l.iter().skip(1) {
        match sx_head(it) {
            Some("start") => {
                let a = sx_list(it)?;
                x1 = a.get(1)?.as_f64()?;
                y1 = a.get(2)?.as_f64()?;
            }
            Some("end") => {
                let a = sx_list(it)?;
                x2 = a.get(1)?.as_f64()?;
                y2 = a.get(2)?.as_f64()?;
            }
            _ => {}
        }
    }
    let (stroke_mm, fill) = parse_stroke_fill(&l[1..]);
    Some(KiCadLibPrim::Rectangle {
        unit,
        x1,
        y1,
        x2,
        y2,
        stroke_mm,
        fill,
    })
}

fn parse_polyline_sx(sx: &Sx, unit: usize) -> Option<KiCadLibPrim> {
    let l = sx_list(sx)?;
    let mut pts = Vec::new();
    for it in l.iter().skip(1) {
        if sx_head(it) == Some("pts") {
            for p in sx_list(it)?.iter().skip(1) {
                if sx_head(p) == Some("xy") {
                    let pl = sx_list(p)?;
                    if let (Some(x), Some(y)) = (pl.get(1).and_then(|v| v.as_f64()), pl.get(2).and_then(|v| v.as_f64())) {
                        pts.push([x, y]);
                    }
                }
            }
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let (stroke_mm, fill) = parse_stroke_fill(&l[1..]);
    Some(KiCadLibPrim::Polyline {
        unit,
        pts,
        stroke_mm,
        fill,
    })
}

fn parse_circle_sx(sx: &Sx, unit: usize) -> Option<KiCadLibPrim> {
    let l = sx_list(sx)?;
    let (mut cx, mut cy, mut radius) = (0.0, 0.0, 0.0);
    for it in l.iter().skip(1) {
        match sx_head(it) {
            Some("center") => {
                let a = sx_list(it)?;
                cx = a.get(1)?.as_f64()?;
                cy = a.get(2)?.as_f64()?;
            }
            Some("radius") => {
                radius = sx_list(it)?.get(1)?.as_f64()?;
            }
            _ => {}
        }
    }
    let (stroke_mm, fill) = parse_stroke_fill(&l[1..]);
    Some(KiCadLibPrim::Circle {
        unit,
        cx,
        cy,
        radius,
        stroke_mm,
        fill,
    })
}

fn parse_arc_sx(sx: &Sx, unit: usize) -> Option<KiCadLibPrim> {
    let l = sx_list(sx)?;
    let (mut start, mut mid, mut end) = ([0.0, 0.0], [0.0, 0.0], [0.0, 0.0]);
    for it in l.iter().skip(1) {
        match sx_head(it) {
            Some("start") => {
                let a = sx_list(it)?;
                start = [a.get(1)?.as_f64()?, a.get(2)?.as_f64()?];
            }
            Some("mid") => {
                let a = sx_list(it)?;
                mid = [a.get(1)?.as_f64()?, a.get(2)?.as_f64()?];
            }
            Some("end") => {
                let a = sx_list(it)?;
                end = [a.get(1)?.as_f64()?, a.get(2)?.as_f64()?];
            }
            _ => {}
        }
    }
    let (stroke_mm, fill) = parse_stroke_fill(&l[1..]);
    Some(KiCadLibPrim::Arc {
        unit,
        start,
        mid,
        end,
        stroke_mm,
        fill,
    })
}

fn parse_text_sx(sx: &Sx, unit: usize) -> Option<KiCadLibPrim> {
    let l = sx_list(sx)?;
    let text = l.get(1)?.as_text()?.to_string();
    let (mut x, mut y, mut angle) = (0.0, 0.0, 0.0);
    for it in l.iter().skip(2) {
        if sx_head(it) == Some("at") {
            let a = sx_list(it)?;
            x = a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            y = a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
            angle = a.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
        }
    }
    Some(KiCadLibPrim::Text {
        unit,
        text,
        x,
        y,
        angle,
    })
}

fn has_hide_yes(sx: &Sx) -> bool {
    let Some(l) = sx_list(sx) else { return false };
    l.iter().skip(1).any(|c| {
        c.as_text() == Some("hide")
            || (sx_head(c) == Some("hide")
                && sx_list(c).and_then(|h| h.get(1)).and_then(|v| v.as_text()) == Some("yes"))
    })
}

// ---------------------------------------------------------------------------
// Exact 2D transforms (KiCad symbol Y-up -> sheet Y-down)
// ---------------------------------------------------------------------------

/// Transform a point `(lx, ly)` in KiCad Y-up symbol space by `mirror` then CCW `orient_deg`
/// into transformed Y-up coordinates.
pub fn transform_local_yup(lx: f64, ly: f64, orient_deg: i32, mirror: Option<SymMirror>) -> (f64, f64) {
    let (mx, my) = match mirror {
        Some(SymMirror::X) => (lx, -ly),
        Some(SymMirror::Y) => (-lx, ly),
        None => (lx, ly),
    };
    let rot = ((orient_deg % 360) + 360) % 360;
    match rot {
        90 => (-my, mx),
        180 => (-mx, -my),
        270 => (my, -mx),
        _ => (mx, my),
    }
}

/// Convert a KiCad Y-up local point into EDL schematic world mm (Y-down) around symbol center `(cx, cy)`.
pub fn local_to_world(
    cx: f64,
    cy: f64,
    lx: f64,
    ly: f64,
    orient_deg: i32,
    mirror: Option<SymMirror>,
) -> Point2 {
    let (rx, ry) = transform_local_yup(lx, ly, orient_deg, mirror);
    Point2::new(cx + rx, cy - ry)
}

/// Compute the EDL `Side` on which a pin sits (direction pointing OUTWARD from body to tip)
/// given the pin's KiCad `angle` (which points INWARD from tip into body!).
pub fn pin_outward_side(angle_deg: f64, orient_deg: i32, mirror: Option<SymMirror>) -> Side {
    let rad = angle_deg.to_radians();
    // Inward vector in symbol Y-up space
    let (ix, iy) = (rad.cos(), rad.sin());
    let (rx, ry) = transform_local_yup(ix, iy, orient_deg, mirror);
    // Outward vector is (-rx, -ry) in Y-up!
    let (ox, oy) = (-rx, -ry);
    if ox.abs() >= oy.abs() {
        if ox >= 0.0 {
            Side::East
        } else {
            Side::West
        }
    } else if oy >= 0.0 {
        Side::North
    } else {
        Side::South
    }
}

/// Compute the tight body bounding box (excluding pin tips) of a KiCad symbol unit in transformed Y-down local offsets.
pub fn symbol_unit_body_bounds(
    sym: &KiCadLibSymbol,
    unit: usize,
    orient_deg: i32,
    mirror: Option<SymMirror>,
) -> Rect2 {
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let include = |b: &mut [f64; 4], lx: f64, ly: f64| {
        let p = local_to_world(0.0, 0.0, lx, ly, orient_deg, mirror);
        b[0] = b[0].min(p.x);
        b[1] = b[1].min(p.y);
        b[2] = b[2].max(p.x);
        b[3] = b[3].max(p.y);
    };

    for prim in &sym.primitives {
        match prim {
            KiCadLibPrim::Rectangle { unit: u, x1, y1, x2, y2, .. } if *u == 0 || *u == unit => {
                include(&mut b, *x1, *y1);
                include(&mut b, *x2, *y2);
            }
            KiCadLibPrim::Polyline { unit: u, pts, .. } if *u == 0 || *u == unit => {
                for p in pts {
                    include(&mut b, p[0], p[1]);
                }
            }
            KiCadLibPrim::Circle { unit: u, cx, cy, radius, .. } if *u == 0 || *u == unit => {
                include(&mut b, cx - radius, cy - radius);
                include(&mut b, cx + radius, cy + radius);
            }
            KiCadLibPrim::Arc { unit: u, start, mid, end, .. } if *u == 0 || *u == unit => {
                include(&mut b, start[0], start[1]);
                include(&mut b, mid[0], mid[1]);
                include(&mut b, end[0], end[1]);
            }
            _ => {}
        }
    }

    // Fallback: include pin body attachment points if symbol had no body primitives
    if !b[0].is_finite() {
        for p in sym.pins.iter().filter(|p| p.unit == 0 || p.unit == unit) {
            let rad = p.angle.to_radians();
            include(&mut b, p.x + p.length * rad.cos(), p.y + p.length * rad.sin());
        }
    }

    if !b[0].is_finite() {
        Rect2::new(-5.08, -5.08, 5.08, 5.08)
    } else {
        Rect2::new(b[0], b[1], b[2], b[3])
    }
}

/// Render a loaded KiCad symbol unit into world-mm `SymbolGraphic` primitives for EDL's SVG preview.
pub fn build_kicad_symbol_graphic(
    sym: &KiCadLibSymbol,
    unit: usize,
    cx: f64,
    cy: f64,
    orient_deg: i32,
    mirror: Option<SymMirror>,
) -> SymbolGraphic {
    let mut primitives = Vec::new();
    for prim in &sym.primitives {
        match prim {
            KiCadLibPrim::Rectangle {
                unit: u,
                x1,
                y1,
                x2,
                y2,
                stroke_mm,
                fill,
            } if *u == 0 || *u == unit => {
                let p1 = local_to_world(cx, cy, *x1, *y1, orient_deg, mirror);
                let p2 = local_to_world(cx, cy, *x2, *y2, orient_deg, mirror);
                primitives.push(SymPrim::Rect {
                    rect: Rect2::new(p1.x.min(p2.x), p1.y.min(p2.y), p1.x.max(p2.x), p1.y.max(p2.y)),
                    filled: fill != "none",
                    stroke_mm: *stroke_mm,
                });
            }
            KiCadLibPrim::Polyline {
                unit: u,
                pts,
                stroke_mm,
                fill,
            } if *u == 0 || *u == unit => {
                let wpts: Vec<Point2> = pts
                    .iter()
                    .map(|p| local_to_world(cx, cy, p[0], p[1], orient_deg, mirror))
                    .collect();
                primitives.push(SymPrim::Polyline {
                    pts: wpts,
                    filled: fill != "none",
                    stroke_mm: *stroke_mm,
                });
            }
            KiCadLibPrim::Circle {
                unit: u,
                cx: lcx,
                cy: lcy,
                radius,
                stroke_mm,
                fill,
            } if *u == 0 || *u == unit => {
                let center = local_to_world(cx, cy, *lcx, *lcy, orient_deg, mirror);
                primitives.push(SymPrim::Circle {
                    center,
                    radius: *radius,
                    filled: fill != "none",
                    stroke_mm: *stroke_mm,
                });
            }
            KiCadLibPrim::Arc {
                unit: u,
                start,
                mid,
                end,
                stroke_mm,
                fill,
            } if *u == 0 || *u == unit => {
                let p0 = local_to_world(cx, cy, start[0], start[1], orient_deg, mirror);
                let pm = local_to_world(cx, cy, mid[0], mid[1], orient_deg, mirror);
                let p1 = local_to_world(cx, cy, end[0], end[1], orient_deg, mirror);
                let wpts = sample_three_point_arc(p0, pm, p1, 14);
                primitives.push(SymPrim::Polyline {
                    pts: wpts,
                    filled: fill != "none",
                    stroke_mm: *stroke_mm,
                });
            }
            KiCadLibPrim::Text {
                unit: u,
                text,
                x,
                y,
                ..
            } if *u == 0 || *u == unit => {
                let pos = local_to_world(cx, cy, *x, *y, orient_deg, mirror);
                primitives.push(SymPrim::Mark {
                    pos,
                    text: text.clone(),
                });
            }
            _ => {}
        }
    }

    let all_numeric_names = !sym.pins.is_empty()
        && sym.pins.iter().all(|p| p.name == p.number || p.name.len() <= 1);
    SymbolGraphic {
        primitives,
        hide_pin_names: sym.hide_pin_names || (sym.pins.len() <= 3 && all_numeric_names),
    }
}

/// Sample a circular arc passing through `p0 -> pm -> p1` into a smooth polyline.
fn sample_three_point_arc(p0: Point2, pm: Point2, p1: Point2, steps: usize) -> Vec<Point2> {
    let d = 2.0 * (p0.x * (pm.y - p1.y) + pm.x * (p1.y - p0.y) + p1.x * (p0.y - pm.y));
    if d.abs() < 1e-6 {
        return vec![p0, pm, p1];
    }
    let a2 = p0.x * p0.x + p0.y * p0.y;
    let b2 = pm.x * pm.x + pm.y * pm.y;
    let c2 = p1.x * p1.x + p1.y * p1.y;
    let ux = (a2 * (pm.y - p1.y) + b2 * (p1.y - p0.y) + c2 * (p0.y - pm.y)) / d;
    let uy = (a2 * (p1.x - pm.x) + b2 * (p0.x - p1.x) + c2 * (pm.x - p0.x)) / d;
    let r = ((p0.x - ux).powi(2) + (p0.y - uy).powi(2)).sqrt();

    let a0 = (p0.y - uy).atan2(p0.x - ux);
    let am = (pm.y - uy).atan2(pm.x - ux);
    let a1 = (p1.y - uy).atan2(p1.x - ux);

    let norm = |a: f64| {
        let mut t = a % std::f64::consts::TAU;
        if t < 0.0 {
            t += std::f64::consts::TAU;
        }
        t
    };
    let ccw_m = norm(am - a0);
    let ccw_1 = norm(a1 - a0);
    let sweep = if ccw_m <= ccw_1 {
        ccw_1
    } else {
        ccw_1 - std::f64::consts::TAU
    };

    (0..=steps)
        .map(|i| {
            let ang = a0 + sweep * (i as f64 / steps as f64);
            Point2::new(ux + r * ang.cos(), uy + r * ang.sin())
        })
        .collect()
}

/// Produce a compact JSON-serializable inspection summary for `edl symbols inspect` & MCP `edl_symbol_inspect`.
pub fn inspect_symbol_json(sym: &KiCadLibSymbol) -> serde_json::Value {
    let mut by_unit: BTreeMap<usize, Vec<serde_json::Value>> = BTreeMap::new();
    for p in &sym.pins {
        by_unit.entry(p.unit).or_default().push(serde_json::json!({
            "name": p.name,
            "number": p.number,
            "etype": p.etype,
            "edl_role": p.role,
            "side_at_rot0": pin_outward_side(p.angle, 0, None),
            "at_mm": [p.x, p.y],
        }));
    }
    serde_json::json!({
        "lib_id": sym.lib_id,
        "library": sym.library,
        "name": sym.name,
        "reference_prefix": sym.reference_prefix,
        "default_value": sym.default_value,
        "default_footprint": sym.default_footprint,
        "description": sym.description,
        "keywords": sym.keywords,
        "unit_count": sym.unit_count,
        "pin_count": sym.pins.len(),
        "units": by_unit,
    })
}

// Local helpers on Sx (matching src/core/sync.rs)
fn sx_list(sx: &Sx) -> Option<&Vec<Sx>> {
    match sx {
        Sx::List(v) => Some(v),
        _ => None,
    }
}
fn sx_head(sx: &Sx) -> Option<&str> {
    match sx_list(sx)?.first()? {
        Sx::Atom(a) => Some(a.as_str()),
        _ => None,
    }
}

trait SxExt {
    fn as_text(&self) -> Option<&str>;
    fn as_f64(&self) -> Option<f64>;
}
impl SxExt for Sx {
    fn as_text(&self) -> Option<&str> {
        match self {
            Sx::Atom(s) | Sx::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    fn as_f64(&self) -> Option<f64> {
        self.as_text()?.parse::<f64>().ok()
    }
}
