//! IEC 60617 / Siemens SIMARIS-style Single-Line Diagram SVG & AutoCAD R12 DXF renderer (v1.6.0).

use crate::core::ir::*;
use crate::core::span::Severity;
use crate::domains::sld::compile::{SldBus, SldLoad, SldModel, SldSource};
use std::collections::BTreeMap;
use std::fmt::Write;

enum SldDraw {
    Line { layer: &'static str, x1: f64, y1: f64, x2: f64, y2: f64, dashed: bool, thick: bool, bus: bool },
    Rect { layer: &'static str, x: f64, y: f64, w: f64, h: f64, fill: &'static str, stroke: &'static str, rx: f64 },
    Circle { layer: &'static str, cx: f64, cy: f64, r: f64, fill: &'static str, stroke: &'static str },
    Text { layer: &'static str, x: f64, y: f64, size: f64, anchor: &'static str, bold: bool, color: &'static str, text: String },
}

pub fn extract_sld_model(ir: &CanonicalIr) -> Option<SldModel> {
    ir.entities.iter().find_map(|e| {
        if e.kind == "sld.model" {
            if let EntityGeometryIr::Custom { data, .. } = &e.geometry {
                return serde_json::from_value::<SldModel>(data.clone()).ok();
            }
        }
        None
    })
}

fn build_sld_geometry(model: &SldModel) -> (f64, f64, Vec<SldDraw>) {
    let mut d: Vec<SldDraw> = Vec::new();

    // Partition buses into Main (tier == "main") and Sub (tier != "main")
    let main_buses: Vec<&SldBus> = model.buses.iter().filter(|b| b.tier == "main").collect();
    let sub_buses: Vec<&SldBus> = model.buses.iter().filter(|b| b.tier != "main").collect();

    // Compute horizontal width required for each sub-bus (based on its child loads)
    let load_pitch = 148.0;
    let mut sub_widths: BTreeMap<&str, f64> = BTreeMap::new();
    for sb in &sub_buses {
        let n_loads = model.loads.iter().filter(|l| l.bus == sb.id).count().max(1);
        sub_widths.insert(sb.id.as_str(), (n_loads as f64) * load_pitch);
    }

    // Group sub-buses + direct main-bus loads under each main bus
    struct MainSlot<'a> {
        bus: &'a SldBus,
        sub_ids: Vec<&'a str>,
        direct_loads: Vec<&'a SldLoad>,
        sources: Vec<&'a SldSource>,
        x0: f64,
        x1: f64,
    }

    let mut cur_x = 76.0;
    let coupler_gap = 136.0;
    let sub_gap = 38.0;
    let mut main_slots: Vec<MainSlot> = Vec::new();

    for (mi, mb) in main_buses.iter().enumerate() {
        let sub_ids: Vec<&str> = model
            .feeders
            .iter()
            .filter(|f| f.from_bus == mb.id)
            .map(|f| f.to_bus.as_str())
            .collect();
        let direct_loads: Vec<&SldLoad> = model.loads.iter().filter(|l| l.bus == mb.id).collect();
        let srcs: Vec<&SldSource> = model.sources.iter().filter(|s| s.bus == mb.id).collect();

        let sub_total_w: f64 = sub_ids
            .iter()
            .map(|sid| *sub_widths.get(sid).unwrap_or(&load_pitch))
            .sum::<f64>()
            + (sub_ids.len().saturating_sub(1) as f64) * sub_gap;
        let direct_w = (direct_loads.len() as f64) * load_pitch
            + if !direct_loads.is_empty() && !sub_ids.is_empty() { sub_gap } else { 0.0 };
        let src_min_w = (srcs.len().max(1) as f64) * 190.0;
        let width = (sub_total_w + direct_w).max(src_min_w).max(340.0);

        if mi > 0 {
            cur_x += coupler_gap;
        }
        main_slots.push(MainSlot {
            bus: mb,
            sub_ids,
            direct_loads,
            sources: srcs,
            x0: cur_x,
            x1: cur_x + width,
        });
        cur_x += width;
    }

    let total_w = (cur_x + 76.0).max(1180.0);
    let total_h = 965.0;

    let y_src = 104.0;
    let y_incomer_top = 176.0;
    let y_main_bus = 290.0;
    let y_sub_bus = 532.0;
    let y_load_sym = 738.0;

    // =========================================================================
    // 1. MAIN BUSBARS & SOURCES
    // =========================================================================
    let mut main_bus_geom: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    let mut sub_bus_geom: BTreeMap<&str, (f64, f64)> = BTreeMap::new();

    for slot in &main_slots {
        main_bus_geom.insert(slot.bus.id.as_str(), (slot.x0, slot.x1));
        // Heavy horizontal Main Busbar
        d.push(SldDraw::Line {
            layer: "SLD_BUSBAR",
            x1: slot.x0,
            y1: y_main_bus,
            x2: slot.x1,
            y2: y_main_bus,
            dashed: false,
            thick: true,
            bus: true,
        });
        // Busbar title & computed IEC 60909 metrics with clean white pill halos (never cut by vertical risers!)
        let cx_bus = (slot.x0 + slot.x1) * 0.5;
        let bus_lbl = format!(
            "{} ({:.0}V {} | {:.0}A)",
            slot.bus.id, slot.bus.voltage_v, slot.bus.system, slot.bus.rating_a
        );
        let bl_w = (bus_lbl.len() as f64) * 6.9 + 10.0;
        d.push(SldDraw::Rect { layer: "SLD_TEXT", x: slot.x0 + 1.0, y: y_main_bus - 23.0, w: bl_w, h: 16.0, fill: "#ffffff", stroke: "none", rx: 2.0 });
        d.push(SldDraw::Text {
            layer: "SLD_TEXT",
            x: slot.x0 + 5.0,
            y: y_main_bus - 11.0,
            size: 11.0,
            anchor: "start",
            bold: true,
            color: "#1e3a8a",
            text: bus_lbl,
        });
        let met_lbl = format!(
            "S={:.0}kVA  I={:.0}A  PF={:.2}  Ik3''={:.1}kA",
            slot.bus.s_kva, slot.bus.current_a, slot.bus.pf, slot.bus.ik3_ka
        );
        let ml_w = (met_lbl.len() as f64) * 5.7 + 10.0;
        d.push(SldDraw::Rect { layer: "SLD_METRICS", x: slot.x1 - ml_w - 1.0, y: y_main_bus - 22.0, w: ml_w, h: 15.0, fill: "#ffffff", stroke: "none", rx: 2.0 });
        d.push(SldDraw::Text {
            layer: "SLD_METRICS",
            x: slot.x1 - 5.0,
            y: y_main_bus - 11.0,
            size: 9.0,
            anchor: "end",
            bold: true,
            color: "#0f766e",
            text: met_lbl,
        });

        // Draw primary & DER sources feeding this main bus
        let ns = slot.sources.len().max(1);
        for (si, src) in slot.sources.iter().enumerate() {
            let sx = if ns == 1 {
                cx_bus
            } else {
                let pad = 95.0_f64.min((slot.x1 - slot.x0) * 0.22);
                slot.x0 + pad + (si as f64) * ((slot.x1 - slot.x0 - 2.0 * pad) / ((ns - 1) as f64))
            };
            draw_source_symbol(&mut d, src, sx, y_src);
            // Vertical feeder from source down through CT & Incomer Breaker to Main Busbar
            let y_sym_bot = y_src + 36.0;
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: sx, y1: y_sym_bot, x2: sx, y2: y_incomer_top, dashed: false, thick: true, bus: false });
            // CT Metering Ring on primary incomers
            if src.kind == "transformer" || src.kind == "generator" {
                let ct_y = y_sym_bot + 16.0;
                d.push(SldDraw::Circle { layer: "SLD_SYMBOLS", cx: sx, cy: ct_y, r: 5.5, fill: "none", stroke: "#475569" });
                d.push(SldDraw::Text { layer: "SLD_TEXT", x: sx + 10.0, y: ct_y + 3.0, size: 7.8, anchor: "start", bold: false, color: "#475569", text: "CT / Wh".into() });
            }
            // Incomer Circuit Breaker
            draw_breaker_vertical(
                &mut d,
                sx,
                y_incomer_top,
                y_incomer_top + 46.0,
                &src.incomer,
                &format!("{:.0}A / {:.0}kA", src.incomer_a, src.icu_ka),
                if src.standby { "ACB (STBY)" } else { "ACB" },
            );
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: sx, y1: y_incomer_top + 46.0, x2: sx, y2: y_main_bus, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Circle { layer: "SLD_BUSBAR", cx: sx, cy: y_main_bus, r: 3.5, fill: "#1e3a8a", stroke: "#1e3a8a" });
        }

        // Lay out outgoing Sub-Distribution Boards + Direct Main-Bus Loads
        let mut bx = slot.x0 + 18.0;
        for &sid in &slot.sub_ids {
            let sw = *sub_widths.get(sid).unwrap_or(&load_pitch);
            let sx0 = bx;
            let sx1 = bx + sw - 20.0;
            let fcx = (sx0 + sx1) * 0.5;
            sub_bus_geom.insert(sid, (sx0, sx1));

            // Draw Feeder from Main Bus down to Sub-Bus
            if let Some(f) = model.feeders.iter().find(|f| f.from_bus == slot.bus.id && f.to_bus == sid) {
                d.push(SldDraw::Circle { layer: "SLD_BUSBAR", cx: fcx, cy: y_main_bus, r: 3.2, fill: "#1e3a8a", stroke: "#1e3a8a" });
                d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: fcx, y1: y_main_bus, x2: fcx, y2: y_main_bus + 28.0, dashed: false, thick: false, bus: false });
                draw_breaker_vertical(
                    &mut d,
                    fcx,
                    y_main_bus + 28.0,
                    y_main_bus + 72.0,
                    &f.breaker,
                    &format!("{:.0}A / {:.0}kA", f.rating_a, f.icu_ka),
                    &f.kind.to_uppercase(),
                );
                d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: fcx, y1: y_main_bus + 72.0, x2: fcx, y2: y_sub_bus, dashed: false, thick: false, bus: false });
                // Cable annotation & 3-phase tick
                let cy_cab = y_main_bus + 126.0;
                draw_cable_tick(&mut d, fcx, cy_cab, &f.cable, f.length_m, f.ib_a, f.du_pct);
                d.push(SldDraw::Circle { layer: "SLD_BUSBAR", cx: fcx, cy: y_sub_bus, r: 3.2, fill: "#1d4ed8", stroke: "#1d4ed8" });
            }

            // Sub-Distribution Busbar
            if let Some(sb) = model.buses.iter().find(|b| b.id == sid) {
                d.push(SldDraw::Line { layer: "SLD_BUSBAR", x1: sx0, y1: y_sub_bus, x2: sx1, y2: y_sub_bus, dashed: false, thick: true, bus: true });
                d.push(SldDraw::Text {
                    layer: "SLD_TEXT",
                    x: sx0 + 2.0,
                    y: y_sub_bus - 9.0,
                    size: 10.0,
                    anchor: "start",
                    bold: true,
                    color: "#1e3a8a",
                    text: format!("{} ({:.0}A)", sb.id, sb.rating_a),
                });
                d.push(SldDraw::Text {
                    layer: "SLD_METRICS",
                    x: sx1 - 2.0,
                    y: y_sub_bus - 9.0,
                    size: 8.2,
                    anchor: "end",
                    bold: true,
                    color: "#0f766e",
                    text: format!("S={:.0}kVA  Ik3={:.1}kA", sb.s_kva, sb.ik3_ka),
                });

                // Outgoing loads from this Sub-Bus
                let s_loads: Vec<&SldLoad> = model.loads.iter().filter(|l| l.bus == sb.id).collect();
                let nl = s_loads.len().max(1);
                for (li, ld) in s_loads.iter().enumerate() {
                    let lx = sx0 + (li as f64 + 0.5) * ((sx1 - sx0) / (nl as f64));
                    draw_load_branch(&mut d, ld, lx, y_sub_bus, y_load_sym);
                }
            }
            bx += sw + sub_gap;
        }

        // Direct loads on the Main Busbar (e.g., PFC Capacitor Bank!)
        for ld in &slot.direct_loads {
            let lx = bx + load_pitch * 0.45;
            draw_load_branch(&mut d, ld, lx, y_main_bus, y_main_bus + 175.0);
            bx += load_pitch;
        }
    }

    // =========================================================================
    // 2. BUS COUPLERS & ATS 2-OF-3 INTERLOCK DIAGRAM
    // =========================================================================
    for cp in &model.couplers {
        if let (Some(&(_, ax1)), Some(&(bx0, _))) = (
            main_bus_geom.get(cp.bus_a.as_str()),
            main_bus_geom.get(cp.bus_b.as_str()),
        ) {
            let mx = (ax1 + bx0) * 0.5;
            d.push(SldDraw::Line { layer: "SLD_BUSBAR", x1: ax1, y1: y_main_bus, x2: mx - 20.0, y2: y_main_bus, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Line { layer: "SLD_BUSBAR", x1: mx + 20.0, y1: y_main_bus, x2: bx0, y2: y_main_bus, dashed: false, thick: true, bus: false });
            // Horizontal Bus Coupler Breaker Symbol (NO blade + X)
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: mx - 20.0, y1: y_main_bus, x2: mx - 12.0, y2: y_main_bus, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: mx - 12.0, y1: y_main_bus, x2: mx + 12.0, y2: y_main_bus - 11.0, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: mx + 12.0, y1: y_main_bus, x2: mx + 20.0, y2: y_main_bus, dashed: false, thick: true, bus: false });
            // Breaker trip cross X
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: mx + 9.0, y1: y_main_bus - 3.5, x2: mx + 15.0, y2: y_main_bus + 3.5, dashed: false, thick: false, bus: false });
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: mx + 15.0, y1: y_main_bus - 3.5, x2: mx + 9.0, y2: y_main_bus + 3.5, dashed: false, thick: false, bus: false });

            d.push(SldDraw::Text {
                layer: "SLD_TEXT",
                x: mx,
                y: y_main_bus - 22.0,
                size: 9.5,
                anchor: "middle",
                bold: true,
                color: "#b45309",
                text: format!("{} ({:.0}A)", cp.id, cp.rating_a),
            });
            // ATS Interlock Badge Box below coupler
            let badge = if cp.mode == "ats_2_of_3" {
                "ATS 2-of-3 (N.O.)"
            } else {
                cp.mode.as_str()
            };
            d.push(SldDraw::Rect {
                layer: "SLD_INTERLOCK",
                x: mx - 48.0,
                y: y_main_bus + 16.0,
                w: 96.0,
                h: 22.0,
                fill: "#fef3c7",
                stroke: "#d97706",
                rx: 4.0,
            });
            d.push(SldDraw::Line { layer: "SLD_INTERLOCK", x1: mx, y1: y_main_bus + 2.0, x2: mx, y2: y_main_bus + 16.0, dashed: true, thick: false, bus: false });
            d.push(SldDraw::Text {
                layer: "SLD_INTERLOCK",
                x: mx,
                y: y_main_bus + 30.0,
                size: 8.2,
                anchor: "middle",
                bold: true,
                color: "#92400e",
                text: badge.to_string(),
            });
        }
    }

    // =========================================================================
    // 3. ENGINEERING SUMMARY & N-1 CONTINGENCY BLOCK (Bottom Right — Image 2 style!)
    // =========================================================================
    if !model.contingencies.is_empty() {
        let box_w = 495.0;
        let box_h = 38.0 + (model.contingencies.len() as f64) * 18.0;
        let bx = total_w - box_w - 34.0;
        let by = total_h - 82.0 - box_h;
        d.push(SldDraw::Rect {
            layer: "SLD_METRICS",
            x: bx,
            y: by,
            w: box_w,
            h: box_h,
            fill: "#f8fafc",
            stroke: "#94a3b8",
            rx: 4.0,
        });
        d.push(SldDraw::Text {
            layer: "SLD_TEXT",
            x: bx + 10.0,
            y: by + 16.0,
            size: 9.2,
            anchor: "start",
            bold: true,
            color: "#0f172a",
            text: "IEC 60909 / N-1 CONTINGENCY VERIFICATION SUMMARY".to_string(),
        });
        for (i, c) in model.contingencies.iter().enumerate() {
            let status = if c.ok { "PASS" } else { "OVERLOAD" };
            let col = if c.ok { "#15803d" } else { "#dc2626" };
            d.push(SldDraw::Text {
                layer: "SLD_METRICS",
                x: bx + 10.0,
                y: by + 34.0 + (i as f64) * 18.0,
                size: 8.6,
                anchor: "start",
                bold: true,
                color: col,
                text: format!(
                    "{} -> {}: S={:.0}kVA | I={:.0}A | Kz={:.2} [{}]",
                    c.scenario, c.active_source, c.total_s_kva, c.current_a, c.loading_factor, status
                ),
            });
        }
    }

    (total_w, total_h, d)
}

fn draw_source_symbol(d: &mut Vec<SldDraw>, src: &SldSource, cx: f64, cy: f64) {
    match src.kind.as_str() {
        "transformer" => {
            // Classic IEC interlocking double circles for MV/LV Transformer
            d.push(SldDraw::Circle { layer: "SLD_SYMBOLS", cx, cy: cy - 8.0, r: 16.0, fill: "#ffffff", stroke: "#0f172a" });
            d.push(SldDraw::Circle { layer: "SLD_SYMBOLS", cx, cy: cy + 12.0, r: 16.0, fill: "none", stroke: "#0f172a" });
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: cx, y1: cy - 38.0, x2: cx, y2: cy - 24.0, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: cx, y1: cy + 28.0, x2: cx, y2: cy + 36.0, dashed: false, thick: true, bus: false });
            let hv = src.hv_kv.map(|k| format!("{k:.0}/0.4kV")).unwrap_or_else(|| "20/0.4kV".into());
            let vec = src.vector.as_deref().unwrap_or("Dyn11");
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 22.0, y: cy - 10.0, size: 10.0, anchor: "start", bold: true, color: "#0f172a", text: src.id.clone() });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 22.0, y: cy + 3.0, size: 8.4, anchor: "start", bold: false, color: "#334155", text: format!("{:.0} kVA  uk={:.1}%", src.rating_kva, src.uk_pct) });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 22.0, y: cy + 15.0, size: 8.2, anchor: "start", bold: false, color: "#475569", text: format!("{hv} {vec}") });
        }
        "generator" => {
            d.push(SldDraw::Circle { layer: "SLD_SYMBOLS", cx, cy: cy + 4.0, r: 19.0, fill: "#fef9c3", stroke: "#854d0e" });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx, y: cy + 9.0, size: 14.0, anchor: "middle", bold: true, color: "#854d0e", text: "G".into() });
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: cx, y1: cy + 23.0, x2: cx, y2: cy + 36.0, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 24.0, y: cy - 2.0, size: 10.0, anchor: "start", bold: true, color: "#854d0e", text: format!("{} (GEN)", src.id) });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 24.0, y: cy + 11.0, size: 8.4, anchor: "start", bold: false, color: "#334155", text: format!("{:.0} kVA  xd={:.0}%", src.rating_kva, src.uk_pct) });
        }
        "solar_pv" => {
            d.push(SldDraw::Rect { layer: "SLD_SYMBOLS", x: cx - 20.0, y: cy - 12.0, w: 40.0, h: 32.0, fill: "#ecfdf5", stroke: "#047857", rx: 3.0 });
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: cx - 20.0, y1: cy + 20.0, x2: cx + 20.0, y2: cy - 12.0, dashed: false, thick: false, bus: false });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx - 10.0, y: cy + 2.0, size: 9.0, anchor: "middle", bold: true, color: "#047857", text: "=".into() });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 10.0, y: cy + 14.0, size: 9.5, anchor: "middle", bold: true, color: "#047857", text: "~".into() });
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: cx, y1: cy + 20.0, x2: cx, y2: cy + 36.0, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 25.0, y: cy - 1.0, size: 9.5, anchor: "start", bold: true, color: "#047857", text: format!("{} (PV)", src.id) });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 25.0, y: cy + 12.0, size: 8.2, anchor: "start", bold: false, color: "#334155", text: format!("{:.0} kVA Inv.", src.rating_kva) });
        }
        _ => {
            // BESS Battery Energy Storage System
            d.push(SldDraw::Rect { layer: "SLD_SYMBOLS", x: cx - 22.0, y: cy - 12.0, w: 44.0, h: 32.0, fill: "#eff6ff", stroke: "#1d4ed8", rx: 3.0 });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx, y: cy + 7.0, size: 9.5, anchor: "middle", bold: true, color: "#1d4ed8", text: "BESS".into() });
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: cx, y1: cy + 20.0, x2: cx, y2: cy + 36.0, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 26.0, y: cy - 1.0, size: 9.5, anchor: "start", bold: true, color: "#1d4ed8", text: src.id.clone() });
            let cap = src.capacity_kwh.map(|k| format!("{:.0} kWh", k)).unwrap_or_else(|| format!("{:.0} kVA", src.rating_kva));
            d.push(SldDraw::Text { layer: "SLD_TEXT", x: cx + 26.0, y: cy + 12.0, size: 8.2, anchor: "start", bold: false, color: "#334155", text: cap });
        }
    }
}

fn draw_breaker_vertical(d: &mut Vec<SldDraw>, x: f64, y0: f64, y1: f64, id: &str, rating: &str, kind: &str) {
    let ym = (y0 + y1) * 0.5;
    d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x, y1: y0, x2: x, y2: y0 + 8.0, dashed: false, thick: true, bus: false });
    // IEC circuit breaker X mark + angled blade
    d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x - 3.5, y1: y0 + 5.0, x2: x + 3.5, y2: y0 + 12.0, dashed: false, thick: false, bus: false });
    d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x + 3.5, y1: y0 + 5.0, x2: x - 3.5, y2: y0 + 12.0, dashed: false, thick: false, bus: false });
    d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x - 9.0, y1: y0 + 10.0, x2: x, y2: y1 - 8.0, dashed: false, thick: true, bus: false });
    d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x, y1: y1 - 8.0, x2: x, y2: y1, dashed: false, thick: true, bus: false });

    d.push(SldDraw::Text { layer: "SLD_TEXT", x: x + 8.0, y: ym - 4.0, size: 8.8, anchor: "start", bold: true, color: "#0f172a", text: format!("{id} ({kind})") });
    d.push(SldDraw::Text { layer: "SLD_TEXT", x: x + 8.0, y: ym + 8.0, size: 7.8, anchor: "start", bold: false, color: "#475569", text: rating.to_string() });
}

fn draw_cable_tick(d: &mut Vec<SldDraw>, x: f64, y: f64, cable: &str, len_m: f64, ib_a: f64, du_pct: f64) {
    // 3-phase cable slash mark
    d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x - 5.0, y1: y + 4.0, x2: x + 5.0, y2: y - 4.0, dashed: false, thick: false, bus: false });
    d.push(SldDraw::Text { layer: "SLD_TEXT", x: x + 8.0, y: y - 5.0, size: 7.8, anchor: "start", bold: false, color: "#334155", text: format!("{cable} ({:.0}m)", len_m) });
    d.push(SldDraw::Text { layer: "SLD_METRICS", x: x + 8.0, y: y + 6.0, size: 7.6, anchor: "start", bold: false, color: "#0369a1", text: format!("Ib={:.0}A  dU={:.2}%", ib_a, du_pct) });
}

fn draw_load_branch(d: &mut Vec<SldDraw>, ld: &SldLoad, x: f64, y_bus: f64, y_sym: f64) {
    d.push(SldDraw::Circle { layer: "SLD_BUSBAR", cx: x, cy: y_bus, r: 2.8, fill: "#1d4ed8", stroke: "#1d4ed8" });
    d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: x, y1: y_bus, x2: x, y2: y_bus + 22.0, dashed: false, thick: false, bus: false });
    draw_breaker_vertical(
        d,
        x,
        y_bus + 22.0,
        y_bus + 64.0,
        &ld.breaker,
        &format!("{:.0}A/{:.0}kA", ld.rating_a, ld.icu_ka),
        &ld.breaker_kind.to_uppercase(),
    );
    d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: x, y1: y_bus + 64.0, x2: x, y2: y_sym - 18.0, dashed: false, thick: false, bus: false });

    if ld.kind != "capacitor_bank" {
        let cy_cab = y_bus + 114.0;
        draw_cable_tick(d, x, cy_cab, &ld.cable, ld.length_m, ld.ib_a, ld.du_total_pct);
    }

    // Load symbol at bottom
    match ld.kind.as_str() {
        "motor" => {
            d.push(SldDraw::Circle { layer: "SLD_SYMBOLS", cx: x, cy: y_sym, r: 17.0, fill: "#fefce8", stroke: "#0f172a" });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x, y: y_sym + 4.0, size: 11.0, anchor: "middle", bold: true, color: "#0f172a", text: "M 3~".into() });
        }
        "capacitor_bank" => {
            // IEC Parallel-Plate PFC Capacitor Symbol
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x - 14.0, y1: y_sym - 5.0, x2: x + 14.0, y2: y_sym - 5.0, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x - 14.0, y1: y_sym + 3.0, x2: x + 14.0, y2: y_sym + 3.0, dashed: false, thick: true, bus: false });
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: x, y1: y_sym - 18.0, x2: x, y2: y_sym - 5.0, dashed: false, thick: false, bus: false });
            d.push(SldDraw::Line { layer: "SLD_FEEDER", x1: x, y1: y_sym + 3.0, x2: x, y2: y_sym + 15.0, dashed: false, thick: false, bus: false });
            d.push(SldDraw::Line { layer: "SLD_SYMBOLS", x1: x - 8.0, y1: y_sym + 15.0, x2: x + 8.0, y2: y_sym + 15.0, dashed: false, thick: true, bus: false });
        }
        "ev_charger" => {
            d.push(SldDraw::Rect { layer: "SLD_SYMBOLS", x: x - 18.0, y: y_sym - 15.0, w: 36.0, h: 30.0, fill: "#dcfce7", stroke: "#15803d", rx: 4.0 });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x, y: y_sym + 4.0, size: 10.0, anchor: "middle", bold: true, color: "#15803d", text: "EV".into() });
        }
        "ups" => {
            d.push(SldDraw::Rect { layer: "SLD_SYMBOLS", x: x - 20.0, y: y_sym - 15.0, w: 40.0, h: 30.0, fill: "#f3e8ff", stroke: "#7e22ce", rx: 3.0 });
            d.push(SldDraw::Text { layer: "SLD_TEXT", x, y: y_sym + 4.0, size: 9.5, anchor: "middle", bold: true, color: "#7e22ce", text: "UPS".into() });
        }
        _ => {
            d.push(SldDraw::Circle { layer: "SLD_SYMBOLS", cx: x, cy: y_sym, r: 16.0, fill: "#f1f5f9", stroke: "#334155" });
            let tag = if ld.kind == "lighting" { "L" } else { "P" };
            d.push(SldDraw::Text { layer: "SLD_TEXT", x, y: y_sym + 4.0, size: 11.5, anchor: "middle", bold: true, color: "#334155", text: tag.into() });
        }
    }

    // Load name & rating below symbol
    d.push(SldDraw::Text {
        layer: "SLD_TEXT",
        x,
        y: y_sym + 31.0,
        size: 8.8,
        anchor: "middle",
        bold: true,
        color: "#0f172a",
        text: ld.id.clone(),
    });
    let rating_txt = if ld.kind == "capacitor_bank" {
        format!("{:.0} kvar PFC", ld.kvar)
    } else {
        format!("{:.0}kW (Kd={:.2})", ld.power_kw, ld.demand)
    };
    d.push(SldDraw::Text {
        layer: "SLD_TEXT",
        x,
        y: y_sym + 43.0,
        size: 7.8,
        anchor: "middle",
        bold: false,
        color: "#475569",
        text: rating_txt,
    });
}

pub fn render_sld_svg(ir: &CanonicalIr) -> String {
    let Some(model) = extract_sld_model(ir) else {
        return "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_string();
    };
    let (w, h, items) = build_sld_geometry(&model);
    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0} {h:.0}" width="{w:.0}" height="{h:.0}">"#
    );
    let _ = writeln!(
        out,
        r##"  <defs><style>
    .bg {{ fill: #f8fafc; }}
    .sheet {{ fill: #ffffff; stroke: #cbd5e1; stroke-width: 1.5; }}
    .title {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 15px; font-weight: 700; fill: #0f172a; }}
    .meta {{ font-family: 'Inter', 'Segoe UI', system-ui, sans-serif; font-size: 11px; fill: #475569; }}
    .ok {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #15803d; text-anchor: end; }}
    .err {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #dc2626; text-anchor: end; }}
  </style></defs>"##
    );
    let _ = writeln!(out, r#"  <rect class="bg" x="0" y="0" width="{w:.0}" height="{h:.0}"/>"#);
    let _ = writeln!(out, r#"  <rect class="sheet" x="14" y="14" width="{:.0}" height="{:.0}" rx="6"/>"#, w - 28.0, h - 28.0);
    let _ = writeln!(
        out,
        r##"  <text class="title" x="32" y="42">{}</text>"##,
        model.title
    );

    for item in &items {
        match item {
            SldDraw::Line { x1, y1, x2, y2, dashed, thick, bus, .. } => {
                let (stroke, sw) = if *bus {
                    ("#1e3a8a", 4.5)
                } else if *dashed {
                    ("#d97706", 1.3)
                } else if *thick {
                    ("#0f172a", 2.0)
                } else {
                    ("#334155", 1.4)
                };
                let dash = if *dashed { " stroke-dasharray=\"5 3\"" } else { "" };
                let _ = writeln!(
                    out,
                    r#"  <line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="{stroke}" stroke-width="{sw}" stroke-linecap="round"{dash}/>"#
                );
            }
            SldDraw::Rect { x, y, w, h, fill, stroke, rx, .. } => {
                let _ = writeln!(
                    out,
                    r#"  <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{rx:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>"#
                );
            }
            SldDraw::Circle { cx, cy, r, fill, stroke, .. } => {
                let _ = writeln!(
                    out,
                    r#"  <circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.6"/>"#
                );
            }
            SldDraw::Text { x, y, size, anchor, bold, color, text, .. } => {
                let fw = if *bold { "700" } else { "400" };
                let esc = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
                let _ = writeln!(
                    out,
                    r#"  <text x="{x:.1}" y="{y:.1}" font-family="monospace" font-size="{size:.1}" font-weight="{fw}" text-anchor="{anchor}" fill="{color}">{esc}</text>"#
                );
            }
        }
    }

    // Title Block & Verification Footer
    let fy = h - 46.0;
    let _ = writeln!(out, r##"  <line x1="28" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#e2e8f0" stroke-width="1.2"/>"##, fy - 18.0, w - 28.0, fy - 18.0);
    let _ = writeln!(out, r#"  <text class="title" x="32" y="{fy:.1}">EDL Power SLD: {}</text>"#, model.name);
    let _ = writeln!(
        out,
        r#"  <text class="meta" x="32" y="{:.1}">Standard: {} | Sources: {} | Switchboards: {} | Feeders: {} | Loads: {}</text>"#,
        fy + 18.0,
        model.standard,
        model.sources.len(),
        model.buses.len(),
        model.feeders.len(),
        model.loads.len()
    );
    if ir.project.has_errors {
        let errs = ir.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
        let _ = writeln!(out, r#"  <text class="err" x="{:.1}" y="{fy:.1}">{errs} IEC 60909 / 60364 ERROR(S)</text>"#, w - 32.0);
    } else {
        let _ = writeln!(
            out,
            r#"  <text class="ok" x="{:.1}" y="{fy:.1}">ALL IEC 60909 &amp; 60364 CHECKS PROVED ({} rules)</text>"#,
            w - 32.0,
            ir.evidence.len()
        );
    }
    let _ = writeln!(out, "</svg>");
    out
}

/// Export the Single-Line Diagram to layered AutoCAD R12 DXF (`SLD_BUSBAR`, `SLD_FEEDER`, `SLD_SYMBOLS`, `SLD_INTERLOCK`, `SLD_TEXT`, `SLD_METRICS`).
pub fn export_sld_dxf(ir: &CanonicalIr) -> String {
    let Some(model) = extract_sld_model(ir) else {
        return "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n".to_string();
    };
    let (_w, h, items) = build_sld_geometry(&model);
    let mut out = String::new();
    let _ = writeln!(out, "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC");
    let _ = writeln!(out, "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n6");
    for (layer, color) in [
        ("SLD_BUSBAR", 5),
        ("SLD_FEEDER", 7),
        ("SLD_SYMBOLS", 3),
        ("SLD_INTERLOCK", 2),
        ("SLD_TEXT", 7),
        ("SLD_METRICS", 4),
    ] {
        let _ = writeln!(out, "0\nLAYER\n2\n{layer}\n70\n0\n62\n{color}\n6\nCONTINUOUS");
    }
    let _ = writeln!(out, "0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES");
    let flip = |y: f64| h - y;
    for item in &items {
        match item {
            SldDraw::Line { layer, x1, y1, x2, y2, .. } => {
                let _ = writeln!(out, "0\nLINE\n8\n{layer}\n10\n{x1:.2}\n20\n{:.2}\n30\n0.0\n11\n{x2:.2}\n21\n{:.2}\n31\n0.0", flip(*y1), flip(*y2));
            }
            SldDraw::Rect { layer, x, y, w, h: rh, .. } => {
                let (y0, y1) = (flip(*y + *rh), flip(*y));
                for (a, b, c, d) in [(*x, y0, *x + *w, y0), (*x + *w, y0, *x + *w, y1), (*x + *w, y1, *x, y1), (*x, y1, *x, y0)] {
                    let _ = writeln!(out, "0\nLINE\n8\n{layer}\n10\n{a:.2}\n20\n{b:.2}\n30\n0.0\n11\n{c:.2}\n21\n{d:.2}\n31\n0.0");
                }
            }
            SldDraw::Circle { layer, cx, cy, r, .. } => {
                let _ = writeln!(out, "0\nCIRCLE\n8\n{layer}\n10\n{cx:.2}\n20\n{:.2}\n30\n0.0\n40\n{r:.2}", flip(*cy));
            }
            SldDraw::Text { layer, x, y, size, text, .. } => {
                let _ = writeln!(out, "0\nTEXT\n8\n{layer}\n10\n{x:.2}\n20\n{:.2}\n30\n0.0\n40\n{:.2}\n1\n{text}", flip(*y), size * 0.85);
            }
        }
    }
    let _ = writeln!(out, "0\nENDSEC\n0\nEOF");
    out
}
