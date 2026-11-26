use crate::core::geom::Side;
use crate::core::ir::*;
use crate::domains::elec::ast::PinRole;
use crate::domains::elec::symbols::{build_symbol_graphic, SymPrim};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// Export a valid KiCad 8/9 S-expression schematic (`.kicad_sch`) directly from Canonical IR.
/// - All coordinates snapped to KiCad's 2.54mm (100 mil) grid
/// - Embeds clean per-component `lib_symbols` with exact pin names, numbers, electrical types, and coordinates
/// - Emits orthogonal `wire` segments, `junction` dots, and net `label`s
/// - Verified against `kicad-cli sch erc` and `kicad-cli sch export svg`
pub fn export_kicad_sch(ir: &CanonicalIr) -> String {
    let first_sheet = if ir.views.len() > 1 {
        ir.views.first().map(|v| v.id.as_str())
    } else {
        None
    };
    export_kicad_sheet(ir, first_sheet, None, 1)
}

/// Export a complete KiCad 9 schematic project (1 file for single-sheet, or root hierarchical sheet +
/// per-sheet `.kicad_sch` sub-sheets for multi-sheet projects). Key = filename, Value = file contents.
pub fn export_kicad_project(ir: &CanonicalIr) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let root_file = format!("{}.kicad_sch", sanitize_filename(&ir.project.name));
    if !ir.explicit_sheets.is_empty() {
        // Build recursive KiCad hierarchy map: sheet_file -> Vec<(semantic_path, uuid_path)>
        // so single-use AND reused multi-level child sheets emit exact `(path "/root_uuid/sheet_uuid/...")`!
        let mut by_file: BTreeMap<String, &ExplicitSchematicSheetIr> = BTreeMap::new();
        for (idx, es) in ir.explicit_sheets.iter().enumerate() {
            let fname = es.sheet_file.clone().unwrap_or_else(|| {
                if idx == 0 {
                    root_file.clone()
                } else {
                    format!("{}.kicad_sch", sanitize_filename(&es.sheet_id))
                }
            });
            by_file.insert(fname, es);
        }

        let root_es = &ir.explicit_sheets[0];
        let root_fname = root_es
            .sheet_file
            .clone()
            .unwrap_or_else(|| root_file.clone());
        let root_uuid = det_uuid("root", &root_es.sheet_id);

        let mut sheet_inst_paths: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        fn walk_hierarchy(
            cur_file: &str,
            sem_prefix: &str,
            uuid_prefix: &str,
            by_file: &BTreeMap<String, &ExplicitSchematicSheetIr>,
            out_map: &mut BTreeMap<String, Vec<(String, String)>>,
            depth: usize,
        ) {
            if depth > 12 {
                return;
            }
            out_map
                .entry(cur_file.to_string())
                .or_default()
                .push((sem_prefix.to_string(), uuid_prefix.to_string()));
            let Some(es) = by_file.get(cur_file) else {
                return;
            };
            let cur_dir = std::path::Path::new(cur_file)
                .parent()
                .filter(|p| !p.as_os_str().is_empty());
            for (bi, sb) in es.sheet_blocks.iter().enumerate() {
                let su = det_uuid("exp_sheet", &format!("{}_{bi}_{}", es.sheet_id, sb.name));
                let child_file = if by_file.contains_key(&sb.file) {
                    sb.file.clone()
                } else if let Some(dir) = cur_dir {
                    dir.join(&sb.file).to_string_lossy().replace('\\', "/")
                } else {
                    sb.file.clone()
                };
                let next_sem = if sem_prefix == "/" {
                    format!("/{}", sb.name)
                } else {
                    format!("{sem_prefix}/{}", sb.name)
                };
                let next_uuid = format!("{uuid_prefix}/{su}");
                walk_hierarchy(&child_file, &next_sem, &next_uuid, by_file, out_map, depth + 1);
            }
        }
        walk_hierarchy(
            &root_fname,
            "/",
            &format!("/{root_uuid}"),
            &by_file,
            &mut sheet_inst_paths,
            0,
        );

        for (idx, es) in ir.explicit_sheets.iter().enumerate() {
            let fname = es.sheet_file.clone().unwrap_or_else(|| {
                if idx == 0 {
                    root_file.clone()
                } else {
                    format!("{}.kicad_sch", sanitize_filename(&es.sheet_id))
                }
            });
            let paths = sheet_inst_paths.get(&fname).cloned().unwrap_or_else(|| {
                let u = det_uuid("root", &es.sheet_id);
                vec![("/".to_string(), format!("/{u}"))]
            });
            files.insert(fname, export_explicit_kicad_sheet(ir, es, &paths, idx + 1));
        }
        return files;
    }
    if ir.views.len() <= 1 {
        files.insert(root_file, export_kicad_sch(ir));
        return files;
    }

    let root_uuid = det_uuid("root", &ir.project.name);
    let mut root = String::new();
    let _ = writeln!(root, "(kicad_sch");
    let _ = writeln!(root, "  (version 20231120)");
    let _ = writeln!(root, "  (generator \"edl\")");
    let _ = writeln!(root, "  (generator_version \"2.0.0\")");
    let _ = writeln!(root, "  (uuid \"{root_uuid}\")");
    let _ = writeln!(root, "  (paper \"A4\")");
    let _ = writeln!(root, "  (title_block");
    let _ = writeln!(root, "    (title \"{} - Root Sheet\")", escape_sexpr(&ir.project.name));
    let _ = writeln!(root, "    (comment 1 \"Generated by EDL v2.0.0 ({} sheets)\")", ir.views.len());
    let _ = writeln!(root, "  )");
    let _ = writeln!(root, "  (lib_symbols)");

    for (idx, view) in ir.views.iter().enumerate() {
        let sheet_uuid = det_uuid("sheet_box", &view.id);
        let fname = format!("{}.kicad_sch", sanitize_filename(&view.id));
        let col = idx % 3;
        let row = idx / 3;
        let sx = 35.56 + (col as f64) * 76.20;
        let sy = 35.56 + (row as f64) * 45.72;
        let _ = writeln!(root, "  (sheet");
        let _ = writeln!(root, "    (at {sx:.2} {sy:.2})");
        let _ = writeln!(root, "    (size 55.88 30.48)");
        let _ = writeln!(root, "    (fields_autoplaced yes)");
        let _ = writeln!(root, "    (stroke (width 0.1524) (type solid))");
        let _ = writeln!(root, "    (fill (color 0 0 0 0.0000))");
        let _ = writeln!(root, "    (uuid \"{sheet_uuid}\")");
        let _ = writeln!(
            root,
            "    (property \"Sheetname\" \"{}\" (at {sx:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (justify left bottom)))",
            escape_sexpr(&view.id),
            sy - 1.27
        );
        let _ = writeln!(
            root,
            "    (property \"Sheetfile\" \"{}\" (at {sx:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (justify left top)))",
            escape_sexpr(&fname),
            sy + 31.75
        );
        let _ = writeln!(root, "    (instances");
        let _ = writeln!(root, "      (project \"{}\"", escape_sexpr(&ir.project.name));
        let _ = writeln!(root, "        (path \"/{root_uuid}\" (page \"{}\"))", idx + 2);
        let _ = writeln!(root, "      )");
        let _ = writeln!(root, "    )");
        let _ = writeln!(root, "  )");

        let hier_path = format!("/{root_uuid}/{sheet_uuid}");
        files.insert(
            fname,
            export_kicad_sheet(ir, Some(&view.id), Some(&hier_path), idx + 2),
        );
    }

    let _ = writeln!(root, "  (sheet_instances");
    let _ = writeln!(root, "    (path \"/\" (page \"1\"))");
    let _ = writeln!(root, "  )");
    let _ = writeln!(root, ")");
    files.insert(root_file, root);
    files
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

pub fn export_kicad_sheet(
    ir: &CanonicalIr,
    sheet_name: Option<&str>,
    hier_path: Option<&str>,
    page_num: usize,
) -> String {
    // Option-A Explicit Geometric Mode: emit 100% lossless KiCad 9 schematic preserving
    // verbatim `lib_symbols`, exact coordinates/rotations/mirrors, multi-unit ICs, buses & wires!
    if let Some(es) = match sheet_name {
        Some(sn) => ir.explicit_sheets.iter().find(|s| s.sheet_id == sn),
        None => ir.explicit_sheets.first(),
    } {
        let root_uuid = det_uuid("root", &es.sheet_id);
        let p = hier_path
            .map(|h| h.to_string())
            .unwrap_or_else(|| format!("/{root_uuid}"));
        return export_explicit_kicad_sheet(ir, es, &[("/".to_string(), p)], page_num);
    }

    let mut out = String::new();
    let multi_sheet = ir.views.len() > 1;
    let sheet_key = sheet_name.unwrap_or(&ir.project.name);
    let root_uuid = det_uuid("root", sheet_key);
    let inst_path = hier_path
        .map(|p| p.to_string())
        .unwrap_or_else(|| format!("/{root_uuid}"));
    let sheet_entities: Vec<&EntityIr> = ir
        .entities
        .iter()
        .filter(|e| match sheet_name {
            Some(sn) if multi_sheet => e.props.get("sheet").and_then(|v| v.as_str()) == Some(sn),
            _ => true,
        })
        .collect();
    let sheet_routes: Vec<&SchematicRouteIr> = ir
        .schematic_routes
        .iter()
        .filter(|r| match sheet_name {
            Some(sn) if multi_sheet => r.sheet.as_deref() == Some(sn),
            _ => true,
        })
        .collect();
    let sheet_groups: Vec<&SchematicGroupIr> = ir
        .schematic_groups
        .iter()
        .filter(|g| match sheet_name {
            Some(sn) if multi_sheet => g.sheet.as_deref() == Some(sn),
            _ => true,
        })
        .collect();

    // Auto-select paper size (A4 -> A3 -> A2 -> A1) including zone frames so nothing ever overflows
    // KiCad's sheet border or collides with its bottom-right title block.
    let (mut max_x, mut max_y) = (228.6_f64, 152.4_f64);
    for ent in &sheet_entities {
        if let EntityGeometryIr::Component { body_rect, .. } = &ent.geometry {
            max_x = max_x.max(body_rect.max_x + 25.0);
            max_y = max_y.max(body_rect.max_y + 25.0);
        }
    }
    for r in &sheet_routes {
        for seg in &r.segments {
            max_x = max_x.max(seg.p0.x.max(seg.p1.x) + 18.0);
            max_y = max_y.max(seg.p0.y.max(seg.p1.y) + 18.0);
        }
        for term in &r.terminals {
            max_x = max_x.max(term.pos.x + 18.0);
            max_y = max_y.max(term.pos.y + 18.0);
        }
    }
    for g in &sheet_groups {
        max_x = max_x.max(g.bbox.max_x + 15.0);
        max_y = max_y.max(g.bbox.max_y + 15.0);
    }
    // Check paper capacities with margin for KiCad border (10.16mm) + title block clearance (never draw past frame!)
    let declared_sheet = sheet_name
        .and_then(|sn| ir.views.iter().find(|v| v.id == sn))
        .or_else(|| ir.views.first())
        .map(|v| v.sheet.to_lowercase())
        .unwrap_or_default();
    let min_paper_rank: u8 = if declared_sheet.contains("a0") {
        4
    } else if declared_sheet.contains("a1") {
        3
    } else if declared_sheet.contains("a2") {
        2
    } else if declared_sheet.contains("a3") {
        1
    } else {
        0
    };
    let fit_rank: u8 = if max_x > 820.0 || max_y > 574.0 {
        4
    } else if max_x > 574.0 || max_y > 400.0 {
        3
    } else if max_x > 400.0 || max_y > 277.0 {
        2
    } else if max_x > 277.0 || max_y > 190.0 {
        1
    } else {
        0
    };
    let paper = match min_paper_rank.max(fit_rank) {
        4 => "A0",
        3 => "A1",
        2 => "A2",
        1 => "A3",
        _ => "A4",
    };

    let sheet_title = sheet_name
        .and_then(|sn| ir.views.iter().find(|v| v.id == sn))
        .and_then(|v| v.title.as_deref())
        .unwrap_or(&ir.project.name);

    let _ = writeln!(out, "(kicad_sch");
    let _ = writeln!(out, "  (version 20231120)");
    let _ = writeln!(out, "  (generator \"edl\")");
    let _ = writeln!(out, "  (generator_version \"2.0.0\")");
    let _ = writeln!(out, "  (uuid \"{root_uuid}\")");
    let _ = writeln!(out, "  (paper \"{paper}\")");
    let _ = writeln!(out, "  (title_block");
    let _ = writeln!(out, "    (title \"{}\")", escape_sexpr(sheet_title));
    let _ = writeln!(out, "    (comment 1 \"Generated by EDL v2.0.0\")");
    let _ = writeln!(out, "  )");

    // Identify all nets using native KiCad `(power)` symbols anywhere in the schematic project
    // (because KiCad power symbols carry a `power_in` pin and must be driven by a `power_out` pin for 0-error ERC).
    let mut power_symbol_nets: BTreeSet<String> = BTreeSet::new();
    for r in &ir.schematic_routes {
        for t in &r.terminals {
            if t.kind == "gnd_symbol" || t.kind == "power_symbol" {
                power_symbol_nets.insert(t.label.clone());
            }
        }
    }

    // Identify:
    // (a) the first Ground (or Passive fallback) pin per ground net so KiCad 9 ERC sees GND as driven, and
    // (b) the first Passive pin on any series-propagated / power-symbol net that feeds a PowerIn pin
    //     without a direct PowerOut pin, eliminating false `power_pin_not_driven` errors!
    let mut gnd_driver_pins: BTreeSet<String> = BTreeSet::new();
    for rel in &ir.relations {
        if rel.is_ground {
            let mut promoted = false;
            let mut first_passive: Option<String> = None;
            for full_pin in &rel.connects {
                if let Some((cid, pid)) = full_pin.split_once('.') {
                    if let Some(ent) = ir.entities.iter().find(|e| e.id == cid) {
                        if let EntityGeometryIr::Component { pins, .. } = &ent.geometry {
                            if let Some(p) = pins
                                .iter()
                                .find(|p| p.name == pid || p.pin_code.as_deref() == Some(pid))
                            {
                                if p.role == PinRole::Ground {
                                    gnd_driver_pins.insert(format!("{cid}.{}", p.name));
                                    promoted = true;
                                    break;
                                } else if p.role == PinRole::Passive && first_passive.is_none() {
                                    first_passive = Some(format!("{cid}.{}", p.name));
                                }
                            }
                        }
                    }
                }
            }
            if !promoted {
                if let Some(fp) = first_passive {
                    gnd_driver_pins.insert(fp);
                }
            }
        } else {
            let mut has_direct_pwr_out = false;
            let mut has_pwr_in = power_symbol_nets.contains(&rel.id);
            let mut first_passive: Option<String> = None;
            for full_pin in &rel.connects {
                if let Some((cid, pid)) = full_pin.split_once('.') {
                    if let Some(ent) = ir.entities.iter().find(|e| e.id == cid) {
                        if let EntityGeometryIr::Component { pins, .. } = &ent.geometry {
                            if let Some(p) = pins
                                .iter()
                                .find(|p| p.name == pid || p.pin_code.as_deref() == Some(pid))
                            {
                                match p.role {
                                    PinRole::PowerOut => has_direct_pwr_out = true,
                                    PinRole::PowerIn => has_pwr_in = true,
                                    PinRole::Passive if first_passive.is_none() => {
                                        first_passive = Some(format!("{cid}.{}", p.name));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            if has_pwr_in && !has_direct_pwr_out {
                if let Some(fp) = first_passive {
                    gnd_driver_pins.insert(fp);
                }
            }
        }
    }

    // Collect unique native KiCad `(power)` symbols needed on THIS sheet: net_name -> is_gnd
    let mut sheet_power_syms: BTreeMap<String, bool> = BTreeMap::new();
    for r in &sheet_routes {
        for t in &r.terminals {
            if t.kind == "gnd_symbol" {
                sheet_power_syms.insert(t.label.clone(), true);
            } else if t.kind == "power_symbol" {
                sheet_power_syms.entry(t.label.clone()).or_insert(false);
            }
        }
    }

    // 1. Embedded lib_symbols (components + native KiCad `power:*` symbols)
    let _ = writeln!(out, "  (lib_symbols");
    for (pnet, &is_gnd) in &sheet_power_syms {
        let esc = escape_sexpr(pnet);
        // KiCad 9 symbol names cannot contain '/' (hierarchical net paths); sanitize symbol ID
        // while keeping the exact electrical net path on the pin name and Value property!
        let sym_id_raw = if pnet.contains('/') {
            pnet.trim_start_matches('/').replace('/', "_")
        } else {
            pnet.clone()
        };
        let sym_id = escape_sexpr(&sym_id_raw);
        let _ = writeln!(out, "    (symbol \"power:{sym_id}\"");
        let _ = writeln!(out, "      (power)");
        let _ = writeln!(out, "      (pin_numbers hide)");
        let _ = writeln!(out, "      (pin_names (offset 0) hide)");
        let _ = writeln!(out, "      (exclude_from_sim no)");
        let _ = writeln!(out, "      (in_bom no)");
        let _ = writeln!(out, "      (on_board no)");
        if is_gnd {
            let _ = writeln!(
                out,
                "      (property \"Reference\" \"#PWR\" (at 0 -6.35 0) (effects (font (size 1.27 1.27)) (hide yes)))"
            );
            let _ = writeln!(
                out,
                "      (property \"Value\" \"{esc}\" (at 0 -3.81 0) (effects (font (size 1.27 1.27))))"
            );
        } else {
            let _ = writeln!(
                out,
                "      (property \"Reference\" \"#PWR\" (at 0 -3.81 0) (effects (font (size 1.27 1.27)) (hide yes)))"
            );
            let _ = writeln!(
                out,
                "      (property \"Value\" \"{esc}\" (at 0 3.556 0) (effects (font (size 1.27 1.27))))"
            );
        }
        let _ = writeln!(
            out,
            "      (property \"Footprint\" \"\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))"
        );
        let _ = writeln!(
            out,
            "      (property \"Datasheet\" \"\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))"
        );
        let _ = writeln!(out, "      (symbol \"{sym_id}_0_1\"");
        if is_gnd {
            // Official KiCad 9 GND downward triangle graphic
            let _ = writeln!(
                out,
                "        (polyline (pts (xy 0 0) (xy 0 -1.27) (xy 1.27 -1.27) (xy 0 -2.54) (xy -1.27 -1.27) (xy 0 -1.27)) (stroke (width 0) (type default)) (fill (type none)))"
            );
        } else {
            // Official KiCad 9 upward power supply arrow graphic
            let _ = writeln!(
                out,
                "        (polyline (pts (xy -0.762 1.27) (xy 0 2.54)) (stroke (width 0) (type default)) (fill (type none)))"
            );
            let _ = writeln!(
                out,
                "        (polyline (pts (xy 0 2.54) (xy 0.762 1.27)) (stroke (width 0) (type default)) (fill (type none)))"
            );
            let _ = writeln!(
                out,
                "        (polyline (pts (xy 0 0) (xy 0 2.54)) (stroke (width 0) (type default)) (fill (type none)))"
            );
        }
        let _ = writeln!(out, "      )");
        let _ = writeln!(out, "      (symbol \"{sym_id}_1_1\"");
        let pin_rot = if is_gnd { 270 } else { 90 };
        let _ = writeln!(
            out,
            "        (pin power_in line (at 0 0 {pin_rot}) (length 0) (name \"{esc}\" (effects (font (size 1.27 1.27)))) (number \"1\" (effects (font (size 1.27 1.27)))))"
        );
        let _ = writeln!(out, "      )");
        let _ = writeln!(out, "    )");
    }
    for ent in &sheet_entities {
        let EntityGeometryIr::Component {
            category,
            value,
            hide_pin_names,
            symbol_prims,
            body_rect,
            pins,
            ..
        } = &ent.geometry
        else {
            continue;
        };

        let sym_name = format!("EDL:{}", ent.id);
        let cx = snap254(body_rect.center().x);
        let cy = snap254(body_rect.center().y);
        // KiCad lib_symbol uses Y-up relative to symbol center
        let ly_bot = -(body_rect.max_y - cy);
        let ly_top = -(body_rect.min_y - cy);

        let graphic = if let Some(ref prims) = symbol_prims {
            crate::domains::elec::symbols::SymbolGraphic {
                primitives: prims.clone(),
                hide_pin_names: *hide_pin_names,
            }
        } else {
            build_symbol_graphic(category, *body_rect, pins)
        };

        let _ = writeln!(out, "    (symbol \"{sym_name}\"");
        if graphic.hide_pin_names {
            let _ = writeln!(out, "      (pin_names (offset 1.016) hide)");
        } else {
            let _ = writeln!(out, "      (pin_names (offset 1.016))");
        }
        let _ = writeln!(out, "      (exclude_from_sim no)");
        let _ = writeln!(out, "      (in_bom yes)");
        let _ = writeln!(out, "      (on_board yes)");
        let _ = writeln!(
            out,
            "      (property \"Reference\" \"{}\" (at 0 {:.2} 0) (effects (font (size 1.27 1.27))))",
            prefix_of(&ent.id),
            ly_top + 2.54
        );
        let _ = writeln!(
            out,
            "      (property \"Value\" \"{}\" (at 0 {:.2} 0) (effects (font (size 1.27 1.27))))",
            escape_sexpr(value),
            ly_bot - 2.54
        );
        let _ = writeln!(
            out,
            "      (property \"Footprint\" \"\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))"
        );
        let _ = writeln!(
            out,
            "      (property \"Datasheet\" \"\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))"
        );
        let _ = writeln!(
            out,
            "      (property \"Description\" \"{}\" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            escape_sexpr(category)
        );

        // Symbol graphics sub-unit (real IEEE/IEC primitives in Y-up symbol-local mm)
        let _ = writeln!(out, "      (symbol \"{}_0_1\"", ent.id);
        for prim in &graphic.primitives {
            match prim {
                SymPrim::Rect { rect, filled, stroke_mm } => {
                    let fill_ty = if *filled { "background" } else { "none" };
                    let _ = writeln!(
                        out,
                        "        (rectangle (start {:.2} {:.2}) (end {:.2} {:.2}) (stroke (width {:.3}) (type default)) (fill (type {fill_ty})))",
                        rect.min_x - cx,
                        -(rect.min_y - cy),
                        rect.max_x - cx,
                        -(rect.max_y - cy),
                        stroke_mm
                    );
                }
                SymPrim::Polyline { pts, filled, stroke_mm } => {
                    let fill_ty = if *filled { "background" } else { "none" };
                    let pts_s = pts
                        .iter()
                        .map(|p| format!("(xy {:.2} {:.2})", p.x - cx, -(p.y - cy)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let _ = writeln!(
                        out,
                        "        (polyline (pts {pts_s}) (stroke (width {:.3}) (type default)) (fill (type {fill_ty})))",
                        stroke_mm
                    );
                }
                SymPrim::Circle { center, radius, filled, stroke_mm } => {
                    let fill_ty = if *filled { "background" } else { "none" };
                    let _ = writeln!(
                        out,
                        "        (circle (center {:.2} {:.2}) (radius {:.2}) (stroke (width {:.3}) (type default)) (fill (type {fill_ty})))",
                        center.x - cx,
                        -(center.y - cy),
                        radius,
                        stroke_mm
                    );
                }
                SymPrim::Mark { pos, text } => {
                    let _ = writeln!(
                        out,
                        "        (text \"{}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27))))",
                        escape_sexpr(text),
                        pos.x - cx,
                        -(pos.y - cy)
                    );
                }
            }
        }
        let _ = writeln!(out, "      )");

        // Symbol pins sub-unit
        let _ = writeln!(out, "      (symbol \"{}_1_1\"", ent.id);
        let mut gnd_driven_names: BTreeSet<String> = BTreeSet::new();
        for p in pins {
            let code = p.pin_code.clone().unwrap_or_else(|| p.number.to_string());
            let full_key = format!("{}.{}", ent.id, p.name);
            let alt_key = format!("{}.{}#{}", ent.id, p.name, code);
            let is_drv = !gnd_driven_names.contains(&p.name)
                && (gnd_driver_pins.contains(&full_key) || gnd_driver_pins.contains(&alt_key));
            if is_drv {
                gnd_driven_names.insert(p.name.clone());
            }
            let etype = kicad_pin_etype(p.role, is_drv);
            let px = p.tip_point.x - cx;
            let py = -(p.tip_point.y - cy);
            let len = p.tip_point.dist(p.body_point);
            let rot = match p.side {
                Side::West => 0,
                Side::East => 180,
                Side::North => 270,
                Side::South => 90,
            };
            let _ = writeln!(
                out,
                "        (pin {etype} line (at {:.2} {:.2} {rot}) (length {:.2}) (name \"{}\" (effects (font (size 1.016 1.016)))) (number \"{}\" (effects (font (size 1.016 1.016)))))",
                px,
                py,
                len,
                escape_sexpr(&p.name),
                escape_sexpr(&code)
            );
        }
        let _ = writeln!(out, "      )");
        let _ = writeln!(out, "    )");
    }
    let _ = writeln!(out, "  )");

    // 1b. Native KiCad Zone Frames, Dashed Subzones, Bold Blue Section Titles & Yellow Design Notes
    for (g_idx, grp) in sheet_groups.iter().enumerate() {
        let stroke_ty = if grp.border.as_deref() == Some("dashed") {
            "dash"
        } else {
            "solid"
        };
        let gu = det_uuid("zone_rect", &format!("{sheet_key}_{g_idx}_{}", grp.name));
        let _ = writeln!(
            out,
            "  (rectangle (start {:.2} {:.2}) (end {:.2} {:.2}) (stroke (width 0.30) (type {stroke_ty}) (color 37 99 235 1)) (fill (type none)) (uuid \"{gu}\"))",
            grp.bbox.min_x, grp.bbox.min_y, grp.bbox.max_x, grp.bbox.max_y
        );
        let label_text = grp.title.as_deref().unwrap_or(&grp.name);
        let tu = det_uuid("zone_title", &format!("{sheet_key}_{g_idx}_{}", grp.name));
        let _ = writeln!(
            out,
            "  (text \"{}\" (exclude_from_sim no) (at {:.2} {:.2} 0) (effects (font (size 1.905 1.905) (bold yes) (color 29 78 216 1)) (justify left top)) (uuid \"{tu}\"))",
            escape_sexpr(label_text),
            grp.bbox.min_x + 2.54,
            grp.bbox.min_y + 1.50
        );

        for (sz_idx, sz) in grp.subzones.iter().enumerate() {
            let szu = det_uuid("subzone_rect", &format!("{sheet_key}_{g_idx}_{sz_idx}_{}", sz.title));
            let _ = writeln!(
                out,
                "  (rectangle (start {:.2} {:.2}) (end {:.2} {:.2}) (stroke (width 0.20) (type dash) (color 59 130 246 0.85)) (fill (type none)) (uuid \"{szu}\"))",
                sz.bbox.min_x, sz.bbox.min_y, sz.bbox.max_x, sz.bbox.max_y
            );
            let sztu = det_uuid("subzone_title", &format!("{sheet_key}_{g_idx}_{sz_idx}_{}", sz.title));
            let _ = writeln!(
                out,
                "  (text \"{}\" (exclude_from_sim no) (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27) (bold yes) (color 37 99 235 1)) (justify bottom)) (uuid \"{sztu}\"))",
                escape_sexpr(&sz.title),
                sz.bbox.center().x,
                sz.bbox.max_y - 1.20
            );
        }

        if !grp.notes.is_empty() {
            let note_h = (grp.notes.len() as f64) * 3.8 + 2.54;
            let ny0 = grp.bbox.max_y - note_h - 1.5;
            let ny1 = grp.bbox.max_y - 1.5;
            let nx0 = grp.bbox.min_x + 2.54;
            let nx1 = (grp.bbox.max_x - 2.54).max(nx0 + 20.0);
            let nbu = det_uuid("zone_note_box", &format!("{sheet_key}_{g_idx}"));
            let _ = writeln!(
                out,
                "  (rectangle (start {:.2} {:.2}) (end {:.2} {:.2}) (stroke (width 0.15) (type solid) (color 202 138 4 1)) (fill (type color) (color 254 249 195 0.90)) (uuid \"{nbu}\"))",
                nx0, ny0, nx1, ny1
            );
            let joined = grp.notes.join("\\n");
            let ntu = det_uuid("zone_note_txt", &format!("{sheet_key}_{g_idx}"));
            let _ = writeln!(
                out,
                "  (text \"{}\" (exclude_from_sim no) (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27) (color 113 63 18 1)) (justify left top)) (uuid \"{ntu}\"))",
                escape_sexpr(&joined),
                nx0 + 1.8,
                ny0 + 1.2
            );
        }
    }

    // 2. Placed symbol instances
    for ent in &sheet_entities {
        let EntityGeometryIr::Component {
            value,
            footprint,
            body_rect,
            pins,
            ..
        } = &ent.geometry
        else {
            continue;
        };

        let sym_name = format!("EDL:{}", ent.id);
        let cx = snap254(body_rect.center().x);
        let cy = snap254(body_rect.center().y);
        let u = det_uuid("sym", &ent.id);

        let _ = writeln!(out, "  (symbol");
        let _ = writeln!(out, "    (lib_id \"{sym_name}\")");
        let _ = writeln!(out, "    (at {:.2} {:.2} 0)", cx, cy);
        let _ = writeln!(out, "    (unit 1)");
        let _ = writeln!(out, "    (exclude_from_sim no)");
        let _ = writeln!(out, "    (in_bom yes)");
        let _ = writeln!(out, "    (on_board yes)");
        let _ = writeln!(out, "    (dnp no)");
        let _ = writeln!(out, "    (uuid \"{u}\")");
        // KiCad-native RefDes/Value placement:
        // - Vertical narrow 2-pin passives (Device:R/C/D/LED with North/South pins): place RefDes + Value
        //   cleanly stacked to the RIGHT of the symbol body (zero overlap with vertical wires & power symbols!)
        // - All other symbols: RefDes above top edge, Value below bottom edge, clearing North/South pins.
        let is_vertical_2pin = pins.len() == 2
            && pins.iter().all(|p| matches!(p.side, Side::North | Side::South))
            && body_rect.width() <= 8.5;
        let (ref_x, ref_y, ref_just, val_x, val_y, val_just) = if is_vertical_2pin {
            (
                body_rect.max_x + 1.4,
                cy - 1.27,
                " (justify left)",
                body_rect.max_x + 1.4,
                cy + 1.27,
                " (justify left)",
            )
        } else {
            let max_north_x = pins
                .iter()
                .filter(|p| p.side == Side::North)
                .map(|p| p.body_point.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let max_south_x = pins
                .iter()
                .filter(|p| p.side == Side::South)
                .map(|p| p.body_point.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let (rx, rj) = if max_north_x.is_finite() {
                (max_north_x + 2.54, " (justify left)")
            } else {
                (cx, "")
            };
            let (vx, vj) = if max_south_x.is_finite() {
                (max_south_x + 2.54, " (justify left)")
            } else {
                (cx, "")
            };
            (rx, body_rect.min_y - 2.0, rj, vx, body_rect.max_y + 2.0, vj)
        };
        let _ = writeln!(
            out,
            "    (property \"Reference\" \"{}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)){ref_just}))",
            escape_sexpr(&ent.id),
            ref_x,
            ref_y
        );
        let _ = writeln!(
            out,
            "    (property \"Value\" \"{}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)){val_just}))",
            escape_sexpr(value),
            val_x,
            val_y
        );
        let _ = writeln!(
            out,
            "    (property \"Footprint\" \"{}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            escape_sexpr(footprint.as_deref().unwrap_or("")),
            cx,
            cy
        );
        let _ = writeln!(
            out,
            "    (property \"Datasheet\" \"\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            cx, cy
        );
        // Stable EDL identity: survives re-annotation / Reference edits in KiCad, used by `edl sync`.
        let _ = writeln!(
            out,
            "    (property \"EDL_ID\" \"{}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            escape_sexpr(&ent.id),
            cx,
            cy
        );
        for p in pins {
            let code = p.pin_code.clone().unwrap_or_else(|| p.number.to_string());
            let pu = det_uuid("pin", &format!("{}.{}", ent.id, code));
            let _ = writeln!(out, "    (pin \"{}\" (uuid \"{pu}\"))", escape_sexpr(&code));
        }
        let _ = writeln!(out, "    (instances");
        let _ = writeln!(out, "      (project \"{}\"", escape_sexpr(&ir.project.name));
        let _ = writeln!(
            out,
            "        (path \"{inst_path}\" (reference \"{}\") (unit 1))",
            escape_sexpr(&ent.id)
        );
        let _ = writeln!(out, "      )");
        let _ = writeln!(out, "    )");
        let _ = writeln!(out, "  )");
    }

    // 2b. Explicit No-Connect markers (prevents KiCad ERC pin_not_connected errors!)
    for ent in &sheet_entities {
        if let EntityGeometryIr::Component { pins, .. } = &ent.geometry {
            for p in pins.iter().filter(|p| p.no_connect) {
                let code = p.pin_code.clone().unwrap_or_else(|| p.number.to_string());
                let ncu = det_uuid("nc", &format!("{}.{}", ent.id, code));
                let _ = writeln!(
                    out,
                    "  (no_connect (at {:.2} {:.2}) (uuid \"{ncu}\"))",
                    p.tip_point.x, p.tip_point.y
                );
            }
        }
    }

    // 3. Orthogonal wires, junctions, native KiCad `(power)` symbols, and net labels
    let mut pwr_idx = 0usize;
    for (r_idx, route) in sheet_routes.iter().enumerate() {
        for (s_idx, seg) in route.segments.iter().enumerate() {
            let wu = det_uuid("wire", &format!("{r_idx}_{s_idx}"));
            let _ = writeln!(
                out,
                "  (wire (pts (xy {:.2} {:.2}) (xy {:.2} {:.2})) (stroke (width 0) (type default)) (uuid \"{wu}\"))",
                seg.p0.x, seg.p0.y, seg.p1.x, seg.p1.y
            );
        }
        for (j_idx, jpt) in route.junctions.iter().enumerate() {
            let ju = det_uuid("junc", &format!("{r_idx}_{j_idx}"));
            let _ = writeln!(
                out,
                "  (junction (at {:.2} {:.2}) (diameter 0) (color 0 0 0 0) (uuid \"{ju}\"))",
                jpt.x, jpt.y
            );
        }
        if let Some(lp) = route.label_pos {
            let lu = det_uuid("lbl", &route.net_id);
            let _ = writeln!(
                out,
                "  (label \"{}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (justify left bottom)) (uuid \"{lu}\"))",
                escape_sexpr(&route.net_id),
                lp.x,
                lp.y
            );
        }
        // Emit native KiCad `(power)` symbol instances for GND / supply rails,
        // `(global_label ...)` for cross-sheet signals, and crisp `(label ...)` for local stubs!
        for (t_idx, term) in route.terminals.iter().enumerate() {
            if term.kind == "gnd_symbol" || term.kind == "power_symbol" {
                pwr_idx += 1;
                let is_gnd = term.kind == "gnd_symbol";
                let esc = escape_sexpr(&term.label);
                let sym_id_raw = if term.label.contains('/') {
                    term.label.trim_start_matches('/').replace('/', "_")
                } else {
                    term.label.clone()
                };
                let sym_id = escape_sexpr(&sym_id_raw);
                let pwr_ref = format!("#PWR{page_num:02}{pwr_idx:03}");
                let pu = det_uuid("pwr_sym", &format!("{sheet_key}_{r_idx}_{t_idx}_{}", term.label));
                let pin_u = det_uuid("pwr_pin", &format!("{sheet_key}_{r_idx}_{t_idx}_{}", term.label));
                let val_y = if is_gnd {
                    term.pos.y + 3.81
                } else {
                    term.pos.y - 3.556
                };
                let ref_y = if is_gnd {
                    term.pos.y + 6.35
                } else {
                    term.pos.y + 3.81
                };
                let _ = writeln!(out, "  (symbol");
                let _ = writeln!(out, "    (lib_id \"power:{sym_id}\")");
                let _ = writeln!(out, "    (at {:.2} {:.2} 0)", term.pos.x, term.pos.y);
                let _ = writeln!(out, "    (unit 1)");
                let _ = writeln!(out, "    (exclude_from_sim no)");
                let _ = writeln!(out, "    (in_bom no)");
                let _ = writeln!(out, "    (on_board no)");
                let _ = writeln!(out, "    (dnp no)");
                let _ = writeln!(out, "    (uuid \"{pu}\")");
                let _ = writeln!(
                    out,
                    "    (property \"Reference\" \"{pwr_ref}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
                    term.pos.x, ref_y
                );
                let _ = writeln!(
                    out,
                    "    (property \"Value\" \"{esc}\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27))))",
                    term.pos.x, val_y
                );
                let _ = writeln!(
                    out,
                    "    (property \"Footprint\" \"\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
                    term.pos.x, term.pos.y
                );
                let _ = writeln!(
                    out,
                    "    (property \"Datasheet\" \"\" (at {:.2} {:.2} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
                    term.pos.x, term.pos.y
                );
                let _ = writeln!(out, "    (pin \"1\" (uuid \"{pin_u}\"))");
                let _ = writeln!(out, "    (instances");
                let _ = writeln!(out, "      (project \"{}\"", escape_sexpr(&ir.project.name));
                let _ = writeln!(
                    out,
                    "        (path \"{inst_path}\" (reference \"{pwr_ref}\") (unit 1))"
                );
                let _ = writeln!(out, "      )");
                let _ = writeln!(out, "    )");
                let _ = writeln!(out, "  )");
                continue;
            }
            let lu = det_uuid("term_lbl", &format!("{sheet_key}_{r_idx}_{t_idx}_{}", term.label));
            let use_global = multi_sheet && (term.kind == "global_flag" || route.is_power || route.is_ground);
            if use_global {
                let (rot, justify) = match term.dir {
                    Side::West => (180, "right"),
                    Side::East => (0, "left"),
                    Side::North => (90, "left"),
                    Side::South => (270, "right"),
                };
                let _ = writeln!(
                    out,
                    "  (global_label \"{}\" (shape bidirectional) (at {:.2} {:.2} {rot}) (fields_autoplaced yes) (effects (font (size 1.27 1.27)) (justify {justify})) (uuid \"{lu}\"))",
                    escape_sexpr(&term.label),
                    term.pos.x,
                    term.pos.y
                );
            } else {
                let (rot, justify) = match term.dir {
                    Side::West => (0, "right bottom"),
                    Side::East => (0, "left bottom"),
                    Side::North => (0, "left bottom"),
                    Side::South => (0, "left top"),
                };
                let _ = writeln!(
                    out,
                    "  (label \"{}\" (at {:.2} {:.2} {rot}) (effects (font (size 1.27 1.27)) (justify {justify})) (uuid \"{lu}\"))",
                    escape_sexpr(&term.label),
                    term.pos.x,
                    term.pos.y
                );
            }
        }
    }

    let _ = writeln!(out, "  (sheet_instances");
    let _ = writeln!(out, "    (path \"/\" (page \"{page_num}\"))");
    let _ = writeln!(out, "  )");
    let _ = writeln!(out, ")");
    out
}

/// Export an Option-A `ExplicitSchematicSheetIr` directly to a lossless KiCad 9 `.kicad_sch` file.
fn export_explicit_kicad_sheet(
    ir: &CanonicalIr,
    es: &ExplicitSchematicSheetIr,
    inst_paths: &[(String, String)], // (semantic_path, kicad_uuid_path)
    page_num: usize,
) -> String {
    let mut out = String::new();
    let root_uuid = det_uuid("root", &es.sheet_id);
    let fallback_paths = vec![("/".to_string(), format!("/{root_uuid}"))];
    let active_paths = if inst_paths.is_empty() {
        fallback_paths.as_slice()
    } else {
        inst_paths
    };

    // Format KiCad paper spec (e.g. "A3 portrait" -> `(paper "A3" portrait)`, "A4 landscape" -> `(paper "A4")`)
    let paper_parts: Vec<&str> = es.paper.split_whitespace().collect();
    let paper_size = paper_parts.first().copied().unwrap_or("A4");
    let paper_portrait = paper_parts.iter().any(|p| p.eq_ignore_ascii_case("portrait"));

    let _ = writeln!(out, "(kicad_sch");
    let _ = writeln!(out, "  (version 20250114)");
    let _ = writeln!(out, "  (generator \"edl\")");
    let _ = writeln!(out, "  (generator_version \"2.1.0\")");
    let _ = writeln!(out, "  (uuid \"{root_uuid}\")");
    if paper_portrait {
        let _ = writeln!(out, "  (paper \"{}\" portrait)", escape_sexpr(paper_size));
    } else {
        let _ = writeln!(out, "  (paper \"{}\")", escape_sexpr(paper_size));
    }
    let _ = writeln!(out, "  (title_block");
    let _ = writeln!(
        out,
        "    (title \"{}\")",
        escape_sexpr(es.title.as_deref().unwrap_or(&ir.project.name))
    );
    if let Some(ref rev) = es.rev {
        let _ = writeln!(out, "    (rev \"{}\")", escape_sexpr(rev));
    }
    if let Some(ref auth) = es.author {
        let _ = writeln!(out, "    (company \"{}\")", escape_sexpr(auth));
    }
    let _ = writeln!(out, "    (comment 1 \"Generated by EDL v2.1.0\")");
    let _ = writeln!(out, "  )");

    // 1. Verbatim embedded `(lib_symbols ...)`
    let _ = writeln!(out, "  (lib_symbols");
    for raw in es.raw_lib_symbols.values() {
        for line in raw.lines() {
            let _ = writeln!(out, "    {line}");
        }
    }
    let _ = writeln!(out, "  )");

    // 1b. Named bus aliases (`(bus_alias "DDR4" (members ...))`)
    for (bname, members) in &es.bus_aliases {
        let mem_s = members
            .iter()
            .map(|m| format!("\"{}\"", escape_sexpr(m)))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "  (bus_alias \"{}\" (members {mem_s}))",
            escape_sexpr(bname)
        );
    }

    // 2. Non-electrical graphics (lines / rectangles) and wrapped comment text boxes
    let fmt_stroke = |stype: Option<&str>, width: Option<f64>, color: Option<&str>| -> String {
        let w = width.unwrap_or(0.0);
        let t = stype.unwrap_or("default");
        let mut s = format!("(stroke (width {w:.4}) (type {t})");
        if let Some(c) = color {
            let _ = write!(s, " (color {c})");
        }
        s.push(')');
        s
    };
    let fmt_fill = |color: Option<&str>| -> String {
        if let Some(c) = color {
            format!("(fill (type color) (color {c}))")
        } else {
            "(fill (type none))".to_string()
        }
    };
    let fmt_effects = |font_size: Option<f64>, justify: Option<&str>, hide: bool| -> String {
        let sz = font_size.unwrap_or(1.27);
        let mut s = format!("(effects (font (size {sz:.4} {sz:.4}))");
        if let Some(j) = justify {
            let _ = write!(s, " (justify {j})");
        }
        if hide {
            s.push_str(" (hide yes)");
        }
        s.push(')');
        s
    };

    for (gi, gr) in es.graphics.iter().enumerate() {
        let gu = det_uuid("exp_gr", &format!("{}_{gi}", es.sheet_id));
        let st = fmt_stroke(gr.stroke_type.as_deref(), gr.stroke_width, gr.stroke_color.as_deref());
        let fl = fmt_fill(gr.fill_color.as_deref());
        if gr.kind == "rectangle" && gr.pts.len() >= 2 {
            let _ = writeln!(
                out,
                "  (rectangle (start {:.4} {:.4}) (end {:.4} {:.4}) {st} {fl} (uuid \"{gu}\"))",
                gr.pts[0].x, gr.pts[0].y, gr.pts[1].x, gr.pts[1].y
            );
        } else if gr.pts.len() >= 2 {
            let pts_s = gr
                .pts
                .iter()
                .map(|p| format!("(xy {:.4} {:.4})", p.x, p.y))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = writeln!(
                out,
                "  (polyline (pts {pts_s}) {st} {fl} (uuid \"{gu}\"))"
            );
        }
    }
    for (tbi, tb) in es.text_boxes.iter().enumerate() {
        let tbu = det_uuid("exp_tb", &format!("{}_{tbi}", es.sheet_id));
        let st = fmt_stroke(tb.stroke_type.as_deref(), tb.stroke_width, tb.stroke_color.as_deref());
        let fl = fmt_fill(tb.fill_color.as_deref());
        let eff = fmt_effects(tb.font_size, tb.justify.as_deref(), false);
        let _ = writeln!(
            out,
            "  (text_box \"{}\" (exclude_from_sim no) (at {:.4} {:.4} {}) (size {:.4} {:.4}) (margins 0.9525 0.9525 0.9525 0.9525) {st} {fl} {eff} (uuid \"{tbu}\"))",
            escape_sexpr(&tb.text),
            tb.x,
            tb.y,
            tb.orient_deg,
            tb.w,
            tb.h
        );
    }

    // 3. Wires, buses, bus entries, junctions, no-connects
    for (wi, seg) in es.wires.iter().enumerate() {
        let wu = det_uuid("exp_wire", &format!("{}_{wi}", es.sheet_id));
        let _ = writeln!(
            out,
            "  (wire (pts (xy {:.4} {:.4}) (xy {:.4} {:.4})) (stroke (width 0) (type default)) (uuid \"{wu}\"))",
            seg.p0.x, seg.p0.y, seg.p1.x, seg.p1.y
        );
    }
    for (bi, seg) in es.buses.iter().enumerate() {
        let bu = det_uuid("exp_bus", &format!("{}_{bi}", es.sheet_id));
        let _ = writeln!(
            out,
            "  (bus (pts (xy {:.4} {:.4}) (xy {:.4} {:.4})) (stroke (width 0) (type default)) (uuid \"{bu}\"))",
            seg.p0.x, seg.p0.y, seg.p1.x, seg.p1.y
        );
    }
    for (bei, be) in es.bus_entries.iter().enumerate() {
        let beu = det_uuid("exp_be", &format!("{}_{bei}", es.sheet_id));
        let _ = writeln!(
            out,
            "  (bus_entry (at {:.4} {:.4}) (size {:.4} {:.4}) (stroke (width 0) (type default)) (uuid \"{beu}\"))",
            be.x, be.y, be.dx, be.dy
        );
    }
    for (ji, pt) in es.junctions.iter().enumerate() {
        let ju = det_uuid("exp_junc", &format!("{}_{ji}", es.sheet_id));
        let _ = writeln!(
            out,
            "  (junction (at {:.4} {:.4}) (diameter 0) (color 0 0 0 0) (uuid \"{ju}\"))",
            pt.x, pt.y
        );
    }
    for (ni, pt) in es.no_connects.iter().enumerate() {
        let nu = det_uuid("exp_nc", &format!("{}_{ni}", es.sheet_id));
        let _ = writeln!(
            out,
            "  (no_connect (at {:.4} {:.4}) (uuid \"{nu}\"))",
            pt.x, pt.y
        );
    }

    // 4. Labels (local, global, hierarchical, text)
    for (li, lb) in es.labels.iter().enumerate() {
        let lu = det_uuid("exp_lbl", &format!("{}_{li}_{}", es.sheet_id, lb.text));
        let eff = fmt_effects(lb.font_size, lb.justify.as_deref(), false);
        match lb.kind.as_str() {
            "global_label" => {
                let sh = lb.shape.as_deref().unwrap_or("bidirectional");
                let _ = writeln!(
                    out,
                    "  (global_label \"{}\" (shape {sh}) (at {:.4} {:.4} {}) (fields_autoplaced yes) {eff} (uuid \"{lu}\"))",
                    escape_sexpr(&lb.text),
                    lb.x,
                    lb.y,
                    lb.orient_deg
                );
            }
            "hierarchical_label" => {
                let sh = lb.shape.as_deref().unwrap_or("bidirectional");
                let _ = writeln!(
                    out,
                    "  (hierarchical_label \"{}\" (shape {sh}) (at {:.4} {:.4} {}) (fields_autoplaced yes) {eff} (uuid \"{lu}\"))",
                    escape_sexpr(&lb.text),
                    lb.x,
                    lb.y,
                    lb.orient_deg
                );
            }
            "text" => {
                let _ = writeln!(
                    out,
                    "  (text \"{}\" (exclude_from_sim no) (at {:.4} {:.4} {}) {eff} (uuid \"{lu}\"))",
                    escape_sexpr(&lb.text),
                    lb.x,
                    lb.y,
                    lb.orient_deg
                );
            }
            _ => {
                let _ = writeln!(
                    out,
                    "  (label \"{}\" (at {:.4} {:.4} {}) (fields_autoplaced yes) {eff} (uuid \"{lu}\"))",
                    escape_sexpr(&lb.text),
                    lb.x,
                    lb.y,
                    lb.orient_deg
                );
            }
        }
    }

    // 5. Placed component symbol instances (preserving exact coordinates, rotation, mirror, unit, props & field styling!)
    for (si, sym) in es.symbols.iter().enumerate() {
        let su = det_uuid("exp_sym", &format!("{}_{}_{}_{si}", es.sheet_id, sym.ref_id, sym.unit));
        let _ = writeln!(out, "  (symbol");
        let _ = writeln!(out, "    (lib_id \"{}\")", escape_sexpr(&sym.lib_id));
        let _ = writeln!(out, "    (at {:.4} {:.4} {})", sym.x, sym.y, sym.orient_deg);
        if let Some(ref mir) = sym.mirror {
            let _ = writeln!(out, "    (mirror {mir})");
        }
        let _ = writeln!(out, "    (unit {})", sym.unit);
        let _ = writeln!(out, "    (exclude_from_sim no)");
        let _ = writeln!(out, "    (in_bom {})", if sym.in_bom { "yes" } else { "no" });
        let _ = writeln!(out, "    (on_board {})", if sym.on_board { "yes" } else { "no" });
        let _ = writeln!(out, "    (dnp {})", if sym.dnp { "yes" } else { "no" });
        let _ = writeln!(out, "    (fields_autoplaced yes)");
        let _ = writeln!(out, "    (uuid \"{su}\")");

        let find_field = |fname: &str| sym.fields.iter().find(|f| f.name == fname);

        // Reference
        let (rx, ry, rrot) = sym
            .ref_at
            .map(|a| (a[0], a[1], a[2].round() as i32))
            .unwrap_or((sym.x, sym.y - 2.54, 0));
        if let Some(f) = find_field("Reference") {
            let eff = fmt_effects(f.font_size, f.justify.as_deref(), f.hide);
            let _ = writeln!(
                out,
                "    (property \"Reference\" \"{}\" (at {:.4} {:.4} {}) {eff})",
                escape_sexpr(&sym.ref_id),
                f.x,
                f.y,
                f.orient_deg
            );
        } else {
            let _ = writeln!(
                out,
                "    (property \"Reference\" \"{}\" (at {:.4} {:.4} {rrot}) (effects (font (size 1.27 1.27))))",
                escape_sexpr(&sym.ref_id),
                rx,
                ry
            );
        }

        // Value
        let (vx, vy, vrot) = sym
            .val_at
            .map(|a| (a[0], a[1], a[2].round() as i32))
            .unwrap_or((sym.x, sym.y + 2.54, 0));
        if let Some(f) = find_field("Value") {
            let eff = fmt_effects(f.font_size, f.justify.as_deref(), f.hide);
            let vtext = f.value.as_deref().unwrap_or(&sym.value);
            let _ = writeln!(
                out,
                "    (property \"Value\" \"{}\" (at {:.4} {:.4} {}) {eff})",
                escape_sexpr(vtext),
                f.x,
                f.y,
                f.orient_deg
            );
        } else {
            let _ = writeln!(
                out,
                "    (property \"Value\" \"{}\" (at {:.4} {:.4} {vrot}) (effects (font (size 1.27 1.27))))",
                escape_sexpr(&sym.value),
                vx,
                vy
            );
        }

        // Footprint
        if let Some(f) = find_field("Footprint") {
            let eff = fmt_effects(f.font_size, f.justify.as_deref(), f.hide);
            let ftext = f
                .value
                .as_deref()
                .or(sym.footprint.as_deref())
                .unwrap_or("");
            let _ = writeln!(
                out,
                "    (property \"Footprint\" \"{}\" (at {:.4} {:.4} {}) {eff})",
                escape_sexpr(ftext),
                f.x,
                f.y,
                f.orient_deg
            );
        } else {
            let _ = writeln!(
                out,
                "    (property \"Footprint\" \"{}\" (at {:.4} {:.4} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
                escape_sexpr(sym.footprint.as_deref().unwrap_or("")),
                sym.x,
                sym.y
            );
        }

        // Datasheet
        if let Some(f) = find_field("Datasheet") {
            let eff = fmt_effects(f.font_size, f.justify.as_deref(), f.hide);
            let dtext = f
                .value
                .as_deref()
                .or(sym.datasheet.as_deref())
                .unwrap_or("");
            let _ = writeln!(
                out,
                "    (property \"Datasheet\" \"{}\" (at {:.4} {:.4} {}) {eff})",
                escape_sexpr(dtext),
                f.x,
                f.y,
                f.orient_deg
            );
        } else {
            let _ = writeln!(
                out,
                "    (property \"Datasheet\" \"{}\" (at {:.4} {:.4} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
                escape_sexpr(sym.datasheet.as_deref().unwrap_or("")),
                sym.x,
                sym.y
            );
        }

        // Additional visible/styled custom fields (e.g. `Description` "±1%", `Val` "2M2") + remaining hidden props
        let mut emitted_props: BTreeSet<String> = [
            "Reference".to_string(),
            "Value".to_string(),
            "Footprint".to_string(),
            "Datasheet".to_string(),
        ]
        .into_iter()
        .collect();
        for f in &sym.fields {
            if !emitted_props.insert(f.name.clone()) {
                continue;
            }
            let val = f
                .value
                .as_deref()
                .or_else(|| {
                    sym.props
                        .iter()
                        .find(|(k, _)| k == &f.name)
                        .map(|(_, v)| v.as_str())
                })
                .unwrap_or("");
            let eff = fmt_effects(f.font_size, f.justify.as_deref(), f.hide);
            let _ = writeln!(
                out,
                "    (property \"{}\" \"{}\" (at {:.4} {:.4} {}) {eff})",
                escape_sexpr(&f.name),
                escape_sexpr(val),
                f.x,
                f.y,
                f.orient_deg
            );
        }
        for (pk, pv) in &sym.props {
            if !emitted_props.insert(pk.clone()) {
                continue;
            }
            let _ = writeln!(
                out,
                "    (property \"{}\" \"{}\" (at {:.4} {:.4} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
                escape_sexpr(pk),
                escape_sexpr(pv),
                sym.x,
                sym.y
            );
        }
        let _ = writeln!(
            out,
            "    (property \"EDL_ID\" \"{}\" (at {:.4} {:.4} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            escape_sexpr(&sym.ref_id),
            sym.x,
            sym.y
        );
        for pnum in &sym.pin_numbers {
            let pu = det_uuid("exp_pin", &format!("{}_{}_{}_{}", es.sheet_id, sym.ref_id, sym.unit, pnum));
            let _ = writeln!(out, "    (pin \"{}\" (uuid \"{pu}\"))", escape_sexpr(pnum));
        }
        let _ = writeln!(out, "    (instances");
        let _ = writeln!(out, "      (project \"{}\"", escape_sexpr(&ir.project.name));
        for (sem_path, uuid_path) in active_paths {
            let inst_ref = sym
                .instances
                .iter()
                .find(|(sp, _)| sp == sem_path)
                .map(|(_, r)| r.as_str())
                .unwrap_or(&sym.ref_id);
            let _ = writeln!(
                out,
                "        (path \"{uuid_path}\" (reference \"{}\") (unit {}))",
                escape_sexpr(inst_ref),
                sym.unit
            );
        }
        let _ = writeln!(out, "      )");
        let _ = writeln!(out, "    )");
        let _ = writeln!(out, "  )");
    }

    // 6. Placed power / PWR_FLAG symbol instances
    for (pi, pw) in es.powers.iter().enumerate() {
        let pu = det_uuid("exp_pwr", &format!("{}_{pi}_{}", es.sheet_id, pw.reference));
        let pin_u = det_uuid("exp_pwr_pin", &format!("{}_{pi}_{}", es.sheet_id, pw.reference));
        let _ = writeln!(out, "  (symbol");
        let _ = writeln!(out, "    (lib_id \"{}\")", escape_sexpr(&pw.lib_id));
        let _ = writeln!(out, "    (at {:.4} {:.4} {})", pw.x, pw.y, pw.orient_deg);
        if let Some(ref mir) = pw.mirror {
            let _ = writeln!(out, "    (mirror {mir})");
        }
        let _ = writeln!(out, "    (unit 1)");
        let _ = writeln!(out, "    (exclude_from_sim no)");
        let _ = writeln!(out, "    (in_bom no)");
        let _ = writeln!(out, "    (on_board no)");
        let _ = writeln!(out, "    (dnp no)");
        let _ = writeln!(out, "    (fields_autoplaced yes)");
        let _ = writeln!(out, "    (uuid \"{pu}\")");
        let _ = writeln!(
            out,
            "    (property \"Reference\" \"{}\" (at {:.4} {:.4} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            escape_sexpr(&pw.reference),
            pw.x,
            pw.y
        );
        let (pvx, pvy, pvrot) = pw
            .val_at
            .map(|a| (a[0], a[1], a[2].round() as i32))
            .unwrap_or((pw.x, pw.y, 0));
        let pveff = fmt_effects(None, pw.val_justify.as_deref(), pw.val_hide);
        let _ = writeln!(
            out,
            "    (property \"Value\" \"{}\" (at {:.4} {:.4} {pvrot}) {pveff})",
            escape_sexpr(&pw.value),
            pvx,
            pvy
        );
        let _ = writeln!(
            out,
            "    (property \"Footprint\" \"\" (at {:.4} {:.4} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            pw.x, pw.y
        );
        let _ = writeln!(
            out,
            "    (property \"Datasheet\" \"\" (at {:.4} {:.4} 0) (effects (font (size 1.27 1.27)) (hide yes)))",
            pw.x, pw.y
        );
        let _ = writeln!(out, "    (pin \"1\" (uuid \"{pin_u}\"))");
        let _ = writeln!(out, "    (instances");
        let _ = writeln!(out, "      (project \"{}\"", escape_sexpr(&ir.project.name));
        for (idx_p, (_sem_path, uuid_path)) in active_paths.iter().enumerate() {
            let pref = if active_paths.len() > 1 {
                format!("{}_{}", pw.reference, idx_p + 1)
            } else {
                pw.reference.clone()
            };
            let _ = writeln!(
                out,
                "        (path \"{uuid_path}\" (reference \"{}\") (unit 1))",
                escape_sexpr(&pref)
            );
        }
        let _ = writeln!(out, "      )");
        let _ = writeln!(out, "    )");
        let _ = writeln!(out, "  )");
    }

    // 7. Hierarchical sheet blocks (if any)
    for (bi, sb) in es.sheet_blocks.iter().enumerate() {
        let su = det_uuid("exp_sheet", &format!("{}_{bi}_{}", es.sheet_id, sb.name));
        let _ = writeln!(out, "  (sheet");
        let _ = writeln!(out, "    (at {:.4} {:.4})", sb.x, sb.y);
        let _ = writeln!(out, "    (size {:.4} {:.4})", sb.w, sb.h);
        let _ = writeln!(out, "    (fields_autoplaced yes)");
        let _ = writeln!(out, "    (stroke (width 0.1524) (type solid))");
        let sfill = sb.fill_color.as_deref().unwrap_or("0 0 0 0.0000");
        let _ = writeln!(out, "    (fill (color {sfill}))");
        let _ = writeln!(out, "    (uuid \"{su}\")");
        let (nx, ny, nrot) = sb
            .name_at
            .map(|a| (a[0], a[1], a[2].round() as i32))
            .unwrap_or((sb.x, sb.y - 1.27, 0));
        let (fx, fy, frot) = sb
            .file_at
            .map(|a| (a[0], a[1], a[2].round() as i32))
            .unwrap_or((sb.x, sb.y + sb.h + 1.27, 0));
        let fjust = sb.file_justify.as_deref().unwrap_or("left top");
        let _ = writeln!(
            out,
            "    (property \"Sheetname\" \"{}\" (at {:.4} {:.4} {nrot}) (effects (font (size 1.27 1.27)) (justify left bottom)))",
            escape_sexpr(&sb.name),
            nx,
            ny
        );
        let _ = writeln!(
            out,
            "    (property \"Sheetfile\" \"{}\" (at {:.4} {:.4} {frot}) (effects (font (size 1.27 1.27)) (justify {fjust})))",
            escape_sexpr(&sb.file),
            fx,
            fy
        );
        for (pi, sp) in sb.pins.iter().enumerate() {
            let spu = det_uuid("exp_spin", &format!("{}_{bi}_{pi}_{}", es.sheet_id, sp.name));
            let peff = fmt_effects(None, sp.justify.as_deref(), false);
            let _ = writeln!(
                out,
                "    (pin \"{}\" {} (at {:.4} {:.4} {}) {peff} (uuid \"{spu}\"))",
                escape_sexpr(&sp.name),
                sp.shape,
                sp.x,
                sp.y,
                sp.orient_deg
            );
        }
        let _ = writeln!(out, "  )");
    }

    let _ = writeln!(out, "  (sheet_instances");
    let _ = writeln!(out, "    (path \"/\" (page \"{page_num}\"))");
    let _ = writeln!(out, "  )");
    let _ = writeln!(out, ")");
    out
}

fn kicad_pin_etype(role: PinRole, is_power_driver: bool) -> &'static str {
    if is_power_driver {
        return "power_out";
    }
    match role {
        PinRole::PowerOut => "power_out",
        PinRole::PowerIn => "power_in",
        PinRole::Ground => "power_in",
        PinRole::Input => "input",
        PinRole::Output => "output",
        PinRole::Bidir => "bidirectional",
        PinRole::Passive => "passive",
        PinRole::Nc => "no_connect",
    }
}

fn snap254(v: f64) -> f64 {
    (v / 2.54).round() * 2.54
}

fn prefix_of(id: &str) -> String {
    let p: String = id.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if p.is_empty() {
        "U".to_string()
    } else {
        p
    }
}

fn escape_sexpr(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "")
}

/// Simple deterministic FNV-1a-based UUIDv4-formatted identifier for reproducible KiCad output.
fn det_uuid(kind: &str, key: &str) -> String {
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x84222325cbf29ce4;
    for b in kind.bytes().chain([b':']).chain(key.bytes()) {
        h1 ^= b as u64;
        h1 = h1.wrapping_mul(0x100000001b3);
        h2 ^= (b as u64).wrapping_add(0x55);
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (h1 >> 32) as u32,
        ((h1 >> 16) & 0xffff) as u16,
        (h1 & 0x0fff) as u16,
        ((h2 >> 48) & 0x0fff) as u16,
        h2 & 0x0000_ffff_ffff_ffff
    )
}
