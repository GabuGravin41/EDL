//! Bidirectional host-tool sync (v0.8): turn edits made in a native tool (KiCad, AutoCAD/DXF) back into
//! *surgical* patches of the `.edl` source.
//!
//! Design rules:
//! * Every exported entity carries its stable EDL id (`EDL_ID` property in KiCad, `EDL_ANCHORS` XDATA in DXF).
//! * We never regenerate the `.edl` file. We emit a small structured patch plan (`SyncOp`s) and apply it to the
//!   original text line-by-line, so comments, formatting, parameters, and modules survive.
//! * Anything that cannot be represented faithfully (rotation, rewiring, semantic constraints such as
//!   `against: west`, module-instantiated parts) is reported as `report_only` / `unsupported` with a reason,
//!   never silently dropped and never guessed.
//! * `apply` re-compiles the patched text first and refuses to write if it would introduce new errors.

use crate::core::ir::{CanonicalIr, EntityGeometryIr, EntityIr};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

const EDL_GRID_MM: f64 = 2.54;
const KICAD_MOVE_EPS_MM: f64 = 0.5;
const DXF_MOVE_EPS_MM: f64 = 1.0;

// ---------------------------------------------------------------------------------------------------------
// Patch plan model
// ---------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SyncOp {
    /// Set (or insert) `key: value` inside the source block spanning `start_line..=end_line` (1-based).
    SetProp {
        start_line: usize,
        end_line: usize,
        key: String,
        value: String,
    },
    /// Upsert `place <component> at: (x mm, y mm)` inside the first `schematic { ... }` block.
    UpsertPlace { component: String, x_mm: f64, y_mm: f64 },
    /// Informational only; nothing is written.
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncAction {
    pub kind: String,
    pub entity: String,
    /// `patch` (will be written by --apply), `report_only` (valid change, no safe source form), `unsupported`
    pub status: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
    pub op: SyncOp,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncPlan {
    pub origin: String,
    pub actions: Vec<SyncAction>,
    pub warnings: Vec<String>,
    pub entities_compared: usize,
    pub unchanged: usize,
}

impl SyncPlan {
    pub fn patch_count(&self) -> usize {
        self.actions.iter().filter(|a| a.status == "patch").count()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncApplyResult {
    pub applied: bool,
    pub written: bool,
    pub patches: usize,
    pub new_source_errors: usize,
    pub evidence_after: Vec<String>,
    pub message: String,
}

// ---------------------------------------------------------------------------------------------------------
// S-expression reader (KiCad)
// ---------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Sx {
    Atom(String),
    Str(String),
    List(Vec<Sx>),
}

impl Sx {
    fn list(&self) -> Option<&Vec<Sx>> {
        match self {
            Sx::List(v) => Some(v),
            _ => None,
        }
    }
    fn head(&self) -> Option<&str> {
        match self.list()?.first()? {
            Sx::Atom(a) => Some(a.as_str()),
            _ => None,
        }
    }
    fn text(&self) -> Option<&str> {
        match self {
            Sx::Atom(s) | Sx::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

pub fn parse_sexpr(src: &str) -> Result<Sx, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut stack: Vec<Vec<Sx>> = vec![Vec::new()];
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' => {
                stack.push(Vec::new());
                i += 1;
            }
            ')' => {
                if stack.len() < 2 {
                    return Err("unbalanced ')' in S-expression".to_string());
                }
                let done = stack.pop().unwrap();
                stack.last_mut().unwrap().push(Sx::List(done));
                i += 1;
            }
            '"' => {
                let mut s = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                        match chars[i] {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            'r' => s.push('\r'),
                            other => s.push(other),
                        }
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                i += 1;
                stack.last_mut().unwrap().push(Sx::Str(s));
            }
            c if c.is_whitespace() => i += 1,
            _ => {
                let mut s = String::new();
                while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '(' && chars[i] != ')' {
                    s.push(chars[i]);
                    i += 1;
                }
                stack.last_mut().unwrap().push(Sx::Atom(s));
            }
        }
    }
    if stack.len() != 1 {
        return Err("unbalanced '(' in S-expression".to_string());
    }
    stack
        .pop()
        .unwrap()
        .into_iter()
        .next()
        .ok_or_else(|| "empty S-expression".to_string())
}

#[derive(Debug, Clone)]
pub struct KicadSymbol {
    pub edl_id: Option<String>,
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub x: f64,
    pub y: f64,
    pub rot: f64,
    pub mirrored: bool,
}

/// Read every *placed* symbol instance (top-level `(symbol (lib_id ..) ..)`; library definitions are skipped).
pub fn parse_kicad_symbols(text: &str) -> Result<Vec<KicadSymbol>, String> {
    let root = parse_sexpr(text)?;
    if root.head() != Some("kicad_sch") {
        return Err("not a KiCad schematic (missing (kicad_sch ...) root)".to_string());
    }
    let mut out = Vec::new();
    for child in root.list().unwrap().iter().skip(1) {
        if child.head() != Some("symbol") {
            continue;
        }
        let items = child.list().unwrap();
        if !items.iter().any(|c| c.head() == Some("lib_id")) {
            continue;
        }
        let (mut x, mut y, mut rot) = (0.0, 0.0, 0.0);
        let mut mirrored = false;
        let (mut edl_id, mut reference, mut value, mut footprint) = (None, String::new(), String::new(), String::new());
        for it in items.iter().skip(1) {
            match it.head() {
                Some("at") => {
                    let l = it.list().unwrap();
                    x = l.get(1).and_then(|v| v.text()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    y = l.get(2).and_then(|v| v.text()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    rot = l.get(3).and_then(|v| v.text()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                }
                Some("mirror") => mirrored = true,
                Some("property") => {
                    let l = it.list().unwrap();
                    let name = l.get(1).and_then(|v| v.text()).unwrap_or("");
                    let val = l.get(2).and_then(|v| v.text()).unwrap_or("").to_string();
                    match name {
                        "EDL_ID" => edl_id = Some(val),
                        "Reference" => reference = val,
                        "Value" => value = val,
                        "Footprint" => footprint = val,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        out.push(KicadSymbol {
            edl_id,
            reference,
            value,
            footprint,
            x,
            y,
            rot,
            mirrored,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------
// DXF reader / anchors
// ---------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DxfAnchor {
    pub id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
}

/// The anchor point we tag each movable EDL entity with when exporting DXF: (`kind`, x, y) in mm.
pub fn dxf_anchor(ent: &EntityIr) -> Option<(&'static str, f64, f64)> {
    match &ent.geometry {
        EntityGeometryIr::Fixture { body_rect, .. } => {
            let c = body_rect.center();
            Some(("fixture", c.x, c.y))
        }
        EntityGeometryIr::MepDevice { pos, .. } => Some(("mep_device", pos.x, pos.y)),
        EntityGeometryIr::Rect { min, max, .. } if ent.kind == "arch.room" => {
            Some(("room", (min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5))
        }
        _ => None,
    }
}

/// Parse `EDL_ANCHORS` POINT entities (with `1001 EDL / 1000 <id> / 1000 <kind>` XDATA) from an ASCII DXF.
pub fn parse_dxf_anchors(text: &str) -> Vec<DxfAnchor> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut pairs: Vec<(i32, &str)> = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if let Ok(code) = lines[i].parse::<i32>() {
            pairs.push((code, lines[i + 1]));
        }
        i += 2;
    }
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < pairs.len() {
        if pairs[idx].0 == 0 && pairs[idx].1 == "POINT" {
            let mut layer = "";
            let (mut x, mut y) = (f64::NAN, f64::NAN);
            let mut tags: Vec<&str> = Vec::new();
            idx += 1;
            while idx < pairs.len() && pairs[idx].0 != 0 {
                match pairs[idx].0 {
                    8 => layer = pairs[idx].1,
                    10 => x = pairs[idx].1.parse().unwrap_or(f64::NAN),
                    20 => y = pairs[idx].1.parse().unwrap_or(f64::NAN),
                    1000 => tags.push(pairs[idx].1),
                    _ => {}
                }
                idx += 1;
            }
            if layer == "EDL_ANCHORS" && tags.len() >= 2 && x.is_finite() && y.is_finite() {
                out.push(DxfAnchor {
                    id: tags[0].to_string(),
                    kind: tags[1].to_string(),
                    x,
                    y,
                });
            }
        } else {
            idx += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------------------------------------

fn snap_grid(v: f64) -> f64 {
    (v / EDL_GRID_MM).round() * EDL_GRID_MM
}
fn snap254_center(v: f64) -> f64 {
    // identical to the KiCad exporter's snapping of symbol centers
    (v / 2.54).round() * 2.54
}
fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Format a KiCad `Value` back into EDL source: bare quantity (`22uF`) when it is one, quoted string otherwise.
pub fn edl_value_token(v: &str) -> String {
    let t = v.trim();
    let bytes: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
        i += 1;
    }
    let (num, unit) = bytes.split_at(i);
    let unit: String = unit.iter().collect();
    let numstr: String = num.iter().collect();
    let valid_num = !numstr.is_empty() && numstr.parse::<f64>().is_ok();
    let valid_unit = {
        let u = unit.as_str();
        let base = ["ohm", "F", "H", "V", "A", "W", "Hz"];
        let prefixes = ["", "p", "n", "u", "m", "k", "M"];
        prefixes
            .iter()
            .any(|p| base.iter().any(|b| u == format!("{p}{b}")))
    };
    if valid_num && valid_unit {
        t.to_string()
    } else {
        format!("\"{}\"", t.replace('"', "\\\""))
    }
}

pub fn plan_from_kicad(ir: &CanonicalIr, kicad_text: &str) -> Result<SyncPlan, String> {
    let syms = parse_kicad_symbols(kicad_text)?;
    let mut plan = SyncPlan {
        origin: "kicad".to_string(),
        actions: Vec::new(),
        warnings: Vec::new(),
        entities_compared: 0,
        unchanged: 0,
    };
    let mut seen: Vec<String> = Vec::new();

    for s in &syms {
        let key = s.edl_id.clone().unwrap_or_else(|| s.reference.clone());
        let Some(ent) = ir.entities.iter().find(|e| e.id == key && e.kind == "elec.component") else {
            plan.warnings.push(format!(
                "Symbol '{}' exists in KiCad but not in EDL (added in KiCad). Add a `component` declaration to bring it under EDL control.",
                key
            ));
            continue;
        };
        seen.push(ent.id.clone());
        plan.entities_compared += 1;
        let EntityGeometryIr::Component {
            value,
            footprint,
            body_rect,
            ..
        } = &ent.geometry
        else {
            continue;
        };
        let mut changed = false;

        // --- rotation / mirroring (not representable in EDL yet) ---
        if s.rot.abs() > 0.01 || s.mirrored {
            plan.actions.push(SyncAction {
                kind: "rotate_component".to_string(),
                entity: ent.id.clone(),
                status: "unsupported".to_string(),
                description: format!(
                    "{} was rotated/mirrored in KiCad (rot={}, mirror={}); EDL pin sides are semantic and are not changed by sync.",
                    ent.id, s.rot, s.mirrored
                ),
                before: Some(json!({"rot": 0})),
                after: Some(json!({"rot": s.rot, "mirror": s.mirrored})),
                op: SyncOp::None,
            });
            changed = true;
        }

        // --- moved ---
        let exp_cx = snap254_center(body_rect.center().x);
        let exp_cy = snap254_center(body_rect.center().y);
        let (dx, dy) = (s.x - exp_cx, s.y - exp_cy);
        if dx.abs() > KICAD_MOVE_EPS_MM || dy.abs() > KICAD_MOVE_EPS_MM {
            let nx = snap_grid(body_rect.min_x + dx);
            let ny = snap_grid(body_rect.min_y + dy);
            let (before, after) = (
                json!({"x_mm": r2(body_rect.min_x), "y_mm": r2(body_rect.min_y)}),
                json!({"x_mm": r2(nx), "y_mm": r2(ny)}),
            );
            plan.actions.push(SyncAction {
                kind: "move_component".to_string(),
                entity: ent.id.clone(),
                status: "patch".to_string(),
                description: format!(
                    "{} moved by ({:+.2}, {:+.2}) mm -> place {} at: ({:.2}mm, {:.2}mm)",
                    ent.id, dx, dy, ent.id, nx, ny
                ),
                before: Some(before),
                after: Some(after),
                op: SyncOp::UpsertPlace {
                    component: ent.id.clone(),
                    x_mm: r2(nx),
                    y_mm: r2(ny),
                },
            });
            changed = true;
        }

        // --- value / footprint ---
        let span = ent.span.as_ref();
        if s.value.trim() != value.trim() {
            changed = true;
            plan.actions.push(prop_action(
                ent,
                span.map(|s| (s.start_line, s.end_line)),
                "set_value",
                "value",
                value,
                s.value.trim(),
                edl_value_token(&s.value),
            ));
        }
        let old_fp = footprint.clone().unwrap_or_default();
        if s.footprint.trim() != old_fp.trim() {
            changed = true;
            plan.actions.push(prop_action(
                ent,
                span.map(|s| (s.start_line, s.end_line)),
                "set_footprint",
                "footprint",
                &old_fp,
                s.footprint.trim(),
                format!("\"{}\"", s.footprint.trim().replace('"', "\\\"")),
            ));
        }
        if !changed {
            plan.unchanged += 1;
        }
    }

    for e in ir.entities.iter().filter(|e| e.kind == "elec.component") {
        if !seen.contains(&e.id) {
            plan.warnings.push(format!(
                "Component '{}' is in EDL but missing from the KiCad file (deleted in KiCad?). Not removed from EDL.",
                e.id
            ));
        }
    }
    plan.warnings.push(
        "Wire/net/label edits made in KiCad are not synced: EDL connectivity is declared in `net` blocks. Re-wire in EDL (or via the AI) and re-export."
            .to_string(),
    );
    Ok(plan)
}

fn prop_action(
    ent: &EntityIr,
    span: Option<(usize, usize)>,
    kind: &str,
    key: &str,
    old: &str,
    new: &str,
    token: String,
) -> SyncAction {
    let base = SyncAction {
        kind: kind.to_string(),
        entity: ent.id.clone(),
        status: "patch".to_string(),
        description: format!("{}: {} '{}' -> '{}'", ent.id, key, old, new),
        before: Some(json!(old)),
        after: Some(json!(new)),
        op: SyncOp::None,
    };
    if let Some(p) = &ent.provenance {
        return SyncAction {
            status: "unsupported".to_string(),
            description: format!(
                "{} is instantiated from module '{}' ({}). Change the module parameter or `use` override instead of the expanded part.",
                ent.id, p.module, base.description
            ),
            ..base
        };
    }
    match span {
        Some((a, b)) => SyncAction {
            op: SyncOp::SetProp {
                start_line: a,
                end_line: b,
                key: key.to_string(),
                value: token,
            },
            ..base
        },
        None => SyncAction {
            status: "unsupported".to_string(),
            description: format!("{} (no source span available)", base.description),
            ..base
        },
    }
}

pub fn plan_from_dxf(ir: &CanonicalIr, dxf_text: &str) -> Result<SyncPlan, String> {
    let anchors = parse_dxf_anchors(dxf_text);
    if anchors.is_empty() {
        return Err(
            "No EDL_ANCHORS found in DXF. Export with `edl export --dxf` (v0.8+) so entities carry EDL_ID tags."
                .to_string(),
        );
    }
    let mut plan = SyncPlan {
        origin: "dxf".to_string(),
        actions: Vec::new(),
        warnings: Vec::new(),
        entities_compared: 0,
        unchanged: 0,
    };
    let mut seen: Vec<String> = Vec::new();
    for a in &anchors {
        let Some(ent) = ir.entities.iter().find(|e| e.id == a.id) else {
            plan.warnings.push(format!(
                "Entity '{}' exists in DXF but not in EDL; ignored (draw new geometry in EDL for it to be tracked).",
                a.id
            ));
            continue;
        };
        let Some((kind, ox, oy)) = dxf_anchor(ent) else { continue };
        seen.push(ent.id.clone());
        plan.entities_compared += 1;
        let (dx, dy) = (a.x - ox, a.y - oy);
        if dx.abs() <= DXF_MOVE_EPS_MM && dy.abs() <= DXF_MOVE_EPS_MM {
            plan.unchanged += 1;
            continue;
        }
        let before = json!({"x_mm": r2(ox), "y_mm": r2(oy)});
        let after = json!({"x_mm": r2(a.x), "y_mm": r2(a.y)});

        // Freestanding fixtures have an exact source form: `at: (x, y)` relative to the room's min corner.
        if kind == "fixture"
            && ent.props.get("placement").and_then(|v| v.as_str()) == Some("freestanding")
        {
            let room_id = ent.props.get("room").and_then(|v| v.as_str()).unwrap_or("");
            let room = ir.entities.iter().find(|e| e.id == room_id);
            let (EntityGeometryIr::Fixture { body_rect, .. }, Some(room)) = (&ent.geometry, room) else {
                continue;
            };
            let EntityGeometryIr::Rect { min, .. } = room.geometry else { continue };
            let new_min_x = body_rect.min_x + dx;
            let new_min_y = body_rect.min_y + dy;
            let at_x = (new_min_x - min[0]).round();
            let at_y = (new_min_y - min[1]).round();
            let token = format!("({}mm, {}mm)", at_x as i64, at_y as i64);
            let action = match (&ent.provenance, &ent.span) {
                (Some(p), _) => SyncAction {
                    kind: "move_fixture".into(),
                    entity: ent.id.clone(),
                    status: "unsupported".into(),
                    description: format!(
                        "{} is instantiated from module '{}'; adjust the module's fixture offset parameter instead.",
                        ent.id, p.module
                    ),
                    before: Some(before),
                    after: Some(after),
                    op: SyncOp::None,
                },
                (None, Some(sp)) => SyncAction {
                    kind: "move_fixture".into(),
                    entity: ent.id.clone(),
                    status: "patch".into(),
                    description: format!(
                        "{} moved by ({:+.0}, {:+.0}) mm -> at: {}",
                        ent.id, dx, dy, token
                    ),
                    before: Some(before),
                    after: Some(after),
                    op: SyncOp::SetProp {
                        start_line: sp.start_line,
                        end_line: sp.end_line,
                        key: "at".into(),
                        value: token,
                    },
                },
                _ => continue,
            };
            plan.actions.push(action);
            continue;
        }

        let (k, why) = match kind {
            "fixture" => (
                "move_fixture",
                "fixture is wall-anchored (`against:`): its position is a semantic constraint, not coordinates. Change `against`/`offset` in EDL, or convert it to a freestanding `at:` fixture.",
            ),
            "mep_device" => (
                "move_device",
                "electrical devices are anchored to walls/doors/rooms (`host:`/`at:`); moving the symbol does not change the semantic anchor.",
            ),
            _ => (
                "move_room",
                "rooms are solved from dimensional constraints; a dragged outline is a *violation of intent*, not an edit. Change the constraint in EDL.",
            ),
        };
        plan.actions.push(SyncAction {
            kind: k.into(),
            entity: ent.id.clone(),
            status: "report_only".into(),
            description: format!("{} drifted by ({:+.0}, {:+.0}) mm in CAD: {}", ent.id, dx, dy, why),
            before: Some(before),
            after: Some(after),
            op: SyncOp::None,
        });
    }
    for e in &ir.entities {
        if dxf_anchor(e).is_some() && !seen.contains(&e.id) {
            plan.warnings.push(format!(
                "'{}' has no anchor in the DXF (anchor point deleted?). Not removed from EDL.",
                e.id
            ));
        }
    }
    Ok(plan)
}

// ---------------------------------------------------------------------------------------------------------
// Applying plans to source text
// ---------------------------------------------------------------------------------------------------------

fn indent_of(line: &str) -> String {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Find the byte range of the value token that follows `key:` on `line`.
fn find_prop_token(line: &str, key: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(rel) = line[from..].find(key) {
        let p = from + rel;
        let before_ok = line[..p].chars().next_back().map(|c| !is_ident_char(c)).unwrap_or(true);
        let rest = &line[p + key.len()..];
        let rest_trim = rest.trim_start();
        if before_ok && rest_trim.starts_with(':') {
            let after_colon = &rest_trim[1..];
            let lead = after_colon.len() - after_colon.trim_start().len();
            let start = line.len() - after_colon.len() + lead;
            let tail = &line[start..];
            let end_rel = if tail.starts_with('"') {
                let mut esc = false;
                let mut e = tail.len();
                for (i, c) in tail.char_indices().skip(1) {
                    if esc {
                        esc = false;
                    } else if c == '\\' {
                        esc = true;
                    } else if c == '"' {
                        e = i + 1;
                        break;
                    }
                }
                e
            } else if tail.starts_with('(') {
                tail.find(')').map(|i| i + 1).unwrap_or(tail.len())
            } else {
                tail.find(|c: char| c.is_whitespace() || c == '}' || c == ',')
                    .unwrap_or(tail.len())
            };
            return Some((start, start + end_rel));
        }
        from = p + key.len();
    }
    None
}

fn set_prop(lines: &mut Vec<String>, start: usize, end: usize, key: &str, value: &str) -> Result<(), String> {
    if start == 0 || end < start || end > lines.len() {
        return Err(format!("invalid span {start}..{end} for property '{key}'"));
    }
    let (s0, e0) = (start - 1, end - 1);
    for i in s0..=e0 {
        if let Some((a, b)) = find_prop_token(&lines[i], key) {
            let l = &lines[i];
            lines[i] = format!("{}{}{}", &l[..a], value, &l[b..]);
            return Ok(());
        }
    }
    // Not present: insert.
    if e0 > s0 && lines[e0].trim_start().starts_with('}') {
        let body_indent = lines
            .get(s0 + 1)
            .map(|l| indent_of(l))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}    ", indent_of(&lines[e0])));
        lines.insert(e0, format!("{body_indent}{key}: {value}"));
        return Ok(());
    }
    // single-line declaration: `... { a: b }`
    if let Some(pos) = lines[e0].rfind('}') {
        let l = lines[e0].clone();
        let head = l[..pos].trim_end();
        lines[e0] = format!("{head}  {key}: {value} {}", &l[pos..]);
        return Ok(());
    }
    Err(format!("could not locate a block body to insert '{key}'"))
}

fn upsert_place(lines: &mut Vec<String>, comp: &str, x: f64, y: f64) -> Result<(), String> {
    let token = format!("({x:.2}mm, {y:.2}mm)");
    // 1. existing `place <comp> at: (...)`
    for i in 0..lines.len() {
        let t = lines[i].trim_start();
        if let Some(rest) = t.strip_prefix("place") {
            let rest = rest.trim_start();
            if let Some(after) = rest.strip_prefix(comp) {
                if after.starts_with(|c: char| c.is_whitespace()) && after.trim_start().starts_with("at") {
                    if let Some((a, b)) = find_prop_token(&lines[i], "at") {
                        let l = lines[i].clone();
                        lines[i] = format!("{}{}{}", &l[..a], token, &l[b..]);
                        return Ok(());
                    }
                }
            }
        }
    }
    // 2. insert before closing brace of the first `schematic` block
    let sch = lines
        .iter()
        .position(|l| l.trim_start().starts_with("schematic ") || l.trim_start() == "schematic")
        .ok_or_else(|| "no `schematic` block found to hold a `place` statement".to_string())?;
    let mut depth = 0i32;
    let mut started = false;
    let mut close = None;
    'outer: for (i, l) in lines.iter().enumerate().skip(sch) {
        let mut in_str = false;
        let cs: Vec<char> = l.chars().collect();
        let mut j = 0;
        while j < cs.len() {
            let c = cs[j];
            if in_str {
                if c == '"' {
                    in_str = false;
                }
            } else if c == '"' {
                in_str = true;
            } else if c == '/' && j + 1 < cs.len() && cs[j + 1] == '/' {
                break;
            } else if c == '{' {
                depth += 1;
                started = true;
            } else if c == '}' {
                depth -= 1;
                if started && depth == 0 {
                    close = Some(i);
                    break 'outer;
                }
            }
            j += 1;
        }
    }
    let close = close.ok_or_else(|| "unterminated schematic block".to_string())?;
    let ind = format!("{}    ", indent_of(&lines[close]));
    lines.insert(close, format!("{ind}place {comp} at: {token}"));
    Ok(())
}

/// Apply every `patch` action to `src` and return the new text (spans are processed bottom-up so line numbers
/// stay valid; scan-based `place` edits run last).
pub fn apply_ops_to_source(src: &str, plan: &SyncPlan) -> Result<String, String> {
    let trailing_nl = src.ends_with('\n');
    let mut lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();
    let mut prop_ops: Vec<&SyncOp> = plan
        .actions
        .iter()
        .filter(|a| a.status == "patch")
        .map(|a| &a.op)
        .filter(|o| matches!(o, SyncOp::SetProp { .. }))
        .collect();
    prop_ops.sort_by_key(|o| match o {
        SyncOp::SetProp { start_line, .. } => std::cmp::Reverse(*start_line),
        _ => std::cmp::Reverse(0),
    });
    for op in prop_ops {
        if let SyncOp::SetProp {
            start_line,
            end_line,
            key,
            value,
        } = op
        {
            set_prop(&mut lines, *start_line, *end_line, key, value)?;
        }
    }
    for a in plan.actions.iter().filter(|a| a.status == "patch") {
        if let SyncOp::UpsertPlace { component, x_mm, y_mm } = &a.op {
            upsert_place(&mut lines, component, *x_mm, *y_mm)?;
        }
    }
    let mut out = lines.join("\n");
    if trailing_nl {
        out.push('\n');
    }
    Ok(out)
}

/// Apply a plan to a `.edl` file: patch -> recompile in place (so relative imports resolve) -> only then commit.
pub fn apply_plan_to_file(path: &Path, plan: &SyncPlan, write: bool) -> Result<SyncApplyResult, String> {
    let patches = plan.patch_count();
    if patches == 0 {
        return Ok(SyncApplyResult {
            applied: false,
            written: false,
            patches: 0,
            new_source_errors: 0,
            evidence_after: Vec::new(),
            message: "No patchable changes.".to_string(),
        });
    }
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let before_has_errors = crate::compile_file(path).map(|ir| ir.project.has_errors).unwrap_or(true);
    let patched = apply_ops_to_source(&src, plan)?;

    let tmp = path.with_file_name(format!(
        ".{}.sync-check.edl",
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("edl")
    ));
    std::fs::write(&tmp, &patched).map_err(|e| format!("write temp: {e}"))?;
    let compiled = crate::compile_file(&tmp);
    let _ = std::fs::remove_file(&tmp);

    match compiled {
        Err(d) => Ok(SyncApplyResult {
            applied: false,
            written: false,
            patches,
            new_source_errors: 1,
            evidence_after: Vec::new(),
            message: format!("Rejected: patched source no longer compiles ({}: {}). File untouched.", d.code, d.message),
        }),
        Ok(ir) => {
            let errs = ir
                .diagnostics
                .iter()
                .filter(|d| d.severity == crate::core::span::Severity::Error)
                .count();
            if ir.project.has_errors && !before_has_errors {
                return Ok(SyncApplyResult {
                    applied: false,
                    written: false,
                    patches,
                    new_source_errors: errs,
                    evidence_after: Vec::new(),
                    message: "Rejected: patch would introduce verification errors. File untouched.".to_string(),
                });
            }
            let evidence_after = ir
                .evidence
                .iter()
                .map(|e| format!("{:?} {}", e.status, e.check_id))
                .collect();
            if write {
                std::fs::write(path, &patched).map_err(|e| format!("write {}: {e}", path.display()))?;
            }
            Ok(SyncApplyResult {
                applied: true,
                written: write,
                patches,
                new_source_errors: errs,
                evidence_after,
                message: if write {
                    format!("Applied {patches} patch(es) and re-verified.")
                } else {
                    format!("Dry run: {patches} patch(es) would apply cleanly and re-verify.")
                },
            })
        }
    }
}

/// Human-readable plan rendering for the CLI.
pub fn format_plan(plan: &SyncPlan) -> String {
    let mut s = format!(
        "EDL sync plan (from {}): {} compared, {} unchanged, {} action(s)\n",
        plan.origin,
        plan.entities_compared,
        plan.unchanged,
        plan.actions.len()
    );
    for a in &plan.actions {
        s.push_str(&format!("  [{}] {} — {}\n", a.status, a.kind, a.description));
    }
    for w in &plan.warnings {
        s.push_str(&format!("  note: {w}\n"));
    }
    s
}

/// One-call entry point shared by the CLI, the MCP `edl_sync` tool and `edl watch`:
/// compile `edl_path`, diff against the edited host file, and (optionally) apply + re-verify.
/// `apply = false` is a dry run that still proves the patch would re-verify.
pub fn run_sync(
    edl_path: &Path,
    from_kicad: Option<&Path>,
    from_dxf: Option<&Path>,
    apply: bool,
) -> Result<(SyncPlan, SyncApplyResult), String> {
    let ir = crate::compile_file(edl_path).map_err(|d| format!("{}: {}", d.code, d.message))?;
    let plan = match (from_kicad, from_dxf) {
        (Some(k), None) => {
            let t = std::fs::read_to_string(k).map_err(|e| format!("read {}: {e}", k.display()))?;
            plan_from_kicad(&ir, &t)?
        }
        (None, Some(d)) => {
            let t = std::fs::read_to_string(d).map_err(|e| format!("read {}: {e}", d.display()))?;
            plan_from_dxf(&ir, &t)?
        }
        _ => return Err("Provide exactly one of from_kicad / from_dxf".to_string()),
    };
    let res = apply_plan_to_file(edl_path, &plan, apply)?;
    Ok((plan, res))
}
