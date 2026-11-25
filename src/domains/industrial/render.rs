//! IEC 60617 SVG and AutoCAD R12 DXF renderer for Industrial Motor Control & Contactor Ladders.
//!
//! Draws BOTH the Main 3-Phase Power Circuit (left: L1/L2/L3 bus, 3-pole MCB, thermal overload,
//! reversing pair with L1<->L3 phase-swap, optional Star-Delta pair, and M3~ motor with PE)
//! and the Vertical Control Ladder (right: master E-Stop/overload NC, parallel start/latch rungs,
//! NC interlocks, contactor coils A1-A2, and automatic coil<->contact cross-reference tables).

use crate::core::ir::*;
use crate::core::span::Severity;
use crate::domains::industrial::compile::IndustrialModel;
use std::fmt::Write;

/// Geometry primitive emitted by the shared IEC layout pass (rendered identically to SVG & DXF!).
enum IecDraw {
    Line { layer: &'static str, x1: f64, y1: f64, x2: f64, y2: f64, dashed: bool, thick: bool },
    Rect { layer: &'static str, x: f64, y: f64, w: f64, h: f64, fill: bool },
    Circle { layer: &'static str, cx: f64, cy: f64, r: f64, fill: bool },
    Text { layer: &'static str, x: f64, y: f64, size: f64, anchor: &'static str, bold: bool, text: String },
}

fn extract_model(ir: &CanonicalIr) -> Option<IndustrialModel> {
    ir.entities.iter().find_map(|e| {
        if e.kind == "industrial.panel_model" {
            if let EntityGeometryIr::Custom { data, .. } = &e.geometry {
                return serde_json::from_value::<IndustrialModel>(data.clone()).ok();
            }
        }
        None
    })
}

fn build_iec_geometry(model: &IndustrialModel) -> (f64, f64, Vec<IecDraw>) {
    let mut d: Vec<IecDraw> = Vec::new();
    let has_sd = model.main_circuit.star_delta.is_some();

    let rung_w = 168.0;
    let ladder_x0 = 490.0;
    let ladder_w = (model.rungs.len().max(2) as f64) * rung_w + 80.0;
    let total_w = (ladder_x0 + ladder_w + 50.0).max(1060.0);
    let total_h = if has_sd { 840.0 } else { 700.0 };

    // Section headers
    d.push(IecDraw::Text {
        layer: "IEC_TEXT",
        x: 235.0,
        y: 52.0,
        size: 15.0,
        anchor: "middle",
        bold: true,
        text: format!("MAIN POWER CIRCUIT ({})", model.supply_power),
    });
    d.push(IecDraw::Text {
        layer: "IEC_TEXT",
        x: ladder_x0 + ladder_w * 0.5 - 20.0,
        y: 52.0,
        size: 15.0,
        anchor: "middle",
        bold: true,
        text: format!("CONTROL CIRCUIT ({})", model.supply_control),
    });

    // =========================================================================
    // LEFT: MAIN 3-PHASE POWER CIRCUIT (IEC 60617)
    // =========================================================================
    let px = [120.0_f64, 152.0, 184.0]; // L1, L2, L3 main vertical columns
    let y_bus_top = 82.0;

    // Incoming L1 / L2 / L3 horizontal taps + terminals
    for (i, phase) in model.main_circuit.bus.iter().take(3).enumerate() {
        let y_in = y_bus_top + (i as f64) * 18.0;
        d.push(IecDraw::Circle { layer: "IEC_SYMBOLS", cx: 56.0, cy: y_in, r: 3.5, fill: false });
        d.push(IecDraw::Text {
            layer: "IEC_TEXT",
            x: 46.0,
            y: y_in + 4.0,
            size: 10.5,
            anchor: "end",
            bold: true,
            text: phase.clone(),
        });
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: 59.5, y1: y_in, x2: px[i], y2: y_in, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: px[i], y1: y_in, x2: px[i], y2: 155.0, dashed: false, thick: true });
    }

    // 1. 3-Pole MCB (Q1) at y = 155..195
    let mcb_id = model.main_circuit.stages.first().cloned().unwrap_or_else(|| "Q1".into());
    draw_3pole_switch(&mut d, px, 155.0, 195.0, &mcb_id, true);

    // 2. 3-Pole Thermal Overload (F2) at y = 215..255
    for &x in &px {
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: x, y1: 195.0, x2: x, y2: 215.0, dashed: false, thick: true });
    }
    let ov_id = model.main_circuit.stages.get(1).cloned().unwrap_or_else(|| "F2".into());
    draw_3pole_overload(&mut d, px, 215.0, 255.0, &ov_id);

    // 3. Reversing Contactor Pair (FC straight on left `px`, RC phase-swapping on right `rx`)
    let y_tap_top = [278.0, 294.0, 310.0];
    let rx = [264.0_f64, 296.0, 328.0];
    let y_cont_top = 340.0;
    let y_cont_bot = 385.0;
    let y_join_bot = [412.0, 428.0, 444.0];

    let (fc_id, rc_id) = model
        .main_circuit
        .reversing_pair
        .clone()
        .unwrap_or_else(|| ("FC".into(), "RC".into()));

    for i in 0..3 {
        // Straight down through FC
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: px[i], y1: 255.0, x2: px[i], y2: y_cont_top, dashed: false, thick: true });
        // Tap horizontally across to RC (same order L1, L2, L3 at top)
        d.push(IecDraw::Circle { layer: "IEC_POWER_BUS", cx: px[i], cy: y_tap_top[i], r: 3.0, fill: true });
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: px[i], y1: y_tap_top[i], x2: rx[i], y2: y_tap_top[i], dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: rx[i], y1: y_tap_top[i], x2: rx[i], y2: y_cont_top, dashed: false, thick: true });
    }
    draw_3pole_switch(&mut d, px, y_cont_top, y_cont_bot, &fc_id, false);
    draw_3pole_switch(&mut d, rx, y_cont_top, y_cont_bot, &rc_id, false);

    // Phase-swap reconnection below RC: L1(rx[0])->px[2], L2(rx[1])->px[1], L3(rx[2])->px[0]!
    let swap_target = [2usize, 1, 0];
    let y_motor_top = if has_sd { 625.0 } else { 525.0 };
    for i in 0..3 {
        // FC straight down toward motor U1, V1, W1
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: px[i], y1: y_cont_bot, x2: px[i], y2: y_motor_top, dashed: false, thick: true });
        // RC bottom -> swapped main line
        let t_idx = swap_target[i];
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: rx[i], y1: y_cont_bot, x2: rx[i], y2: y_join_bot[i], dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: rx[i], y1: y_join_bot[i], x2: px[t_idx], y2: y_join_bot[i], dashed: false, thick: true });
        d.push(IecDraw::Circle { layer: "IEC_POWER_BUS", cx: px[t_idx], cy: y_join_bot[i], r: 3.0, fill: true });
    }

    // 4. Optional Star-Delta Pair (SC & DC) wired to U2/V2/W2 below motor (or beside motor)
    if let Some((ref sc_id, ref dc_id)) = model.main_circuit.star_delta {
        let sx = [254.0, 282.0, 310.0];
        let dx = [354.0, 382.0, 410.0];
        let y_sd_top = 500.0;
        let y_sd_bot = 545.0;
        // Star shorting bridge across top of SC
        d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: sx[0], y1: y_sd_top - 14.0, x2: sx[2], y2: y_sd_top - 14.0, dashed: false, thick: true });
        for &x in &sx {
            d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: x, y1: y_sd_top - 14.0, x2: x, y2: y_sd_top, dashed: false, thick: true });
        }
        draw_3pole_switch(&mut d, sx, y_sd_top, y_sd_bot, sc_id, false);
        // Delta top taps from motor supply bus (shifted phase order)
        for i in 0..3 {
            let y_dt = 460.0 + (i as f64) * 12.0;
            let src = px[(i + 1) % 3];
            d.push(IecDraw::Circle { layer: "IEC_POWER_BUS", cx: src, cy: y_dt, r: 2.8, fill: true });
            d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: src, y1: y_dt, x2: dx[i], y2: y_dt, dashed: false, thick: true });
            d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: dx[i], y1: y_dt, x2: dx[i], y2: y_sd_top, dashed: false, thick: true });
        }
        draw_3pole_switch(&mut d, dx, y_sd_top, y_sd_bot, dc_id, false);
        // Tie SC & DC bottoms to motor U2/V2/W2 tail bus (y = 708, 722, 736)
        let y_tail = [708.0, 722.0, 736.0];
        for i in 0..3 {
            d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: sx[i], y1: y_sd_bot, x2: sx[i], y2: y_tail[i], dashed: false, thick: true });
            d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: dx[i], y1: y_sd_bot, x2: dx[i], y2: y_tail[i], dashed: false, thick: true });
            d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: px[i], y1: y_motor_top + 66.0, x2: px[i], y2: y_tail[i], dashed: false, thick: true });
            d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: px[i], y1: y_tail[i], x2: dx[i], y2: y_tail[i], dashed: false, thick: true });
            d.push(IecDraw::Circle { layer: "IEC_POWER_BUS", cx: sx[i], cy: y_tail[i], r: 2.8, fill: true });
        }
    }

    // 5. 3-Phase Induction Motor Circle (M3~) + Terminal Labels + PE Earth
    let m_cx = px[1];
    let m_cy = y_motor_top + 33.0;
    let m_r = 28.0;
    d.push(IecDraw::Circle { layer: "IEC_SYMBOLS", cx: m_cx, cy: m_cy, r: m_r, fill: true });
    let motor_id = if model.main_circuit.load_motor.is_empty() {
        "M1"
    } else {
        &model.main_circuit.load_motor
    };
    let motor_pwr = model
        .devices
        .get(motor_id)
        .map(|m| m.rating.as_str())
        .unwrap_or("");
    d.push(IecDraw::Text {
        layer: "IEC_TEXT",
        x: m_cx,
        y: m_cy + 1.0,
        size: 13.0,
        anchor: "middle",
        bold: true,
        text: format!("{motor_id} 3~"),
    });
    if !motor_pwr.is_empty() {
        d.push(IecDraw::Text {
            layer: "IEC_TEXT",
            x: m_cx,
            y: m_cy + 14.0,
            size: 9.0,
            anchor: "middle",
            bold: false,
            text: motor_pwr.to_string(),
        });
    }
    for (i, t) in ["U1", "V1", "W1"].iter().enumerate() {
        d.push(IecDraw::Text {
            layer: "IEC_TEXT",
            x: px[i] - 4.0,
            y: y_motor_top - 4.0,
            size: 8.5,
            anchor: "end",
            bold: false,
            text: t.to_string(),
        });
    }
    if has_sd {
        for (i, t) in ["U2", "V2", "W2"].iter().enumerate() {
            d.push(IecDraw::Text {
                layer: "IEC_TEXT",
                x: px[i] - 4.0,
                y: y_motor_top + 76.0,
                size: 8.5,
                anchor: "end",
                bold: false,
                text: t.to_string(),
            });
        }
    }
    // PE Earth on left of motor
    d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: m_cx - m_r, y1: m_cy, x2: m_cx - m_r - 24.0, y2: m_cy, dashed: false, thick: false });
    let pe_x = m_cx - m_r - 24.0;
    d.push(IecDraw::Line { layer: "IEC_POWER_BUS", x1: pe_x, y1: m_cy, x2: pe_x, y2: m_cy + 16.0, dashed: false, thick: false });
    d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: pe_x - 10.0, y1: m_cy + 16.0, x2: pe_x + 10.0, y2: m_cy + 16.0, dashed: false, thick: true });
    d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: pe_x - 6.5, y1: m_cy + 20.0, x2: pe_x + 6.5, y2: m_cy + 20.0, dashed: false, thick: false });
    d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: pe_x - 3.0, y1: m_cy + 24.0, x2: pe_x + 3.0, y2: m_cy + 24.0, dashed: false, thick: false });

    // =========================================================================
    // RIGHT: VERTICAL CONTROL LADDER CIRCUIT (IEC 60617)
    // =========================================================================
    let master_x = ladder_x0 + 54.0;
    let y_rail_top = 86.0;
    d.push(IecDraw::Circle { layer: "IEC_SYMBOLS", cx: ladder_x0, cy: y_rail_top, r: 3.5, fill: false });
    d.push(IecDraw::Text {
        layer: "IEC_TEXT",
        x: ladder_x0 - 8.0,
        y: y_rail_top + 4.0,
        size: 10.5,
        anchor: "end",
        bold: true,
        text: model.rail_top.clone(),
    });
    d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: ladder_x0 + 3.5, y1: y_rail_top, x2: master_x, y2: y_rail_top, dashed: false, thick: false });

    // Master series NC trip contacts (Thermal Overload 95-96 + Stop Pushbutton S0)
    let mut cur_my = y_rail_top;
    for mc in &model.master_series {
        let next_y = cur_my + 22.0;
        d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: master_x, y1: cur_my, x2: master_x, y2: next_y, dashed: false, thick: false });
        let is_pb = model
            .devices
            .get(&mc.device)
            .map(|dev| dev.kind == "pushbutton")
            .unwrap_or(false);
        draw_vertical_contact(&mut d, master_x, next_y, next_y + 34.0, &mc.device, &mc.terminal, true, is_pb);
        cur_my = next_y + 34.0;
    }

    let y_bus_ctrl = cur_my + 32.0;
    let y_rail_bot = total_h - 145.0;
    d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: master_x, y1: cur_my, x2: master_x, y2: y_bus_ctrl, dashed: false, thick: false });

    // Draw horizontal distribution rail across all rung columns
    let rung_xs: Vec<f64> = (0..model.rungs.len())
        .map(|i| master_x + (i as f64) * rung_w)
        .collect();
    if let (Some(&first_x), Some(&last_x)) = (rung_xs.first(), rung_xs.last()) {
        d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: first_x, y1: y_bus_ctrl, x2: last_x + 52.0, y2: y_bus_ctrl, dashed: false, thick: false });
        d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: ladder_x0 + 3.5, y1: y_rail_bot, x2: last_x, y2: y_rail_bot, dashed: false, thick: false });
    }
    d.push(IecDraw::Circle { layer: "IEC_SYMBOLS", cx: ladder_x0, cy: y_rail_bot, r: 3.5, fill: false });
    d.push(IecDraw::Text {
        layer: "IEC_TEXT",
        x: ladder_x0 - 8.0,
        y: y_rail_bot + 4.0,
        size: 10.5,
        anchor: "end",
        bold: true,
        text: model.rail_bottom.clone(),
    });

    // Draw each control rung column
    for (r_idx, rung) in model.rungs.iter().enumerate() {
        let rx0 = rung_xs[r_idx];
        let rx_par = rx0 + 52.0; // Parallel branch (latching NO or second enable NO)

        // Column header badge `C1`, `C2`, ... shifted cleanly over the branch pair
        let badge_x = if r_idx == 0 { rx0 + 36.0 } else { rx0 + 26.0 };
        d.push(IecDraw::Text {
            layer: "IEC_CROSSREF",
            x: badge_x,
            y: y_bus_ctrl - 9.0,
            size: 9.8,
            anchor: "middle",
            bold: true,
            text: format!("C{} ({})", rung.column, rung.id),
        });
        d.push(IecDraw::Circle { layer: "IEC_CONTROL_WIRE", cx: rx0, cy: y_bus_ctrl, r: 2.8, fill: true });

        let y_top_blk = y_bus_ctrl + 24.0;
        let y_bot_blk = y_top_blk + 38.0;
        let y_merge = y_bot_blk + 20.0;

        // Top parallel section: either Start PB || Latch NO, OR parallel enable_no contacts
        if let Some(ref pb) = rung.start_pb {
            d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: y_bus_ctrl, x2: rx0, y2: y_top_blk, dashed: false, thick: false });
            draw_vertical_contact(&mut d, rx0, y_top_blk, y_bot_blk, pb, "3-4", false, true);
            d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: y_bot_blk, x2: rx0, y2: y_merge, dashed: false, thick: false });

            if let Some(ref lt) = rung.latch {
                d.push(IecDraw::Circle { layer: "IEC_CONTROL_WIRE", cx: rx_par, cy: y_bus_ctrl, r: 2.8, fill: true });
                d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx_par, y1: y_bus_ctrl, x2: rx_par, y2: y_top_blk, dashed: false, thick: false });
                draw_vertical_contact(&mut d, rx_par, y_top_blk, y_bot_blk, &lt.device, &lt.terminal, false, false);
                d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx_par, y1: y_bot_blk, x2: rx_par, y2: y_merge, dashed: false, thick: false });
                d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: y_merge, x2: rx_par, y2: y_merge, dashed: false, thick: false });
                d.push(IecDraw::Circle { layer: "IEC_CONTROL_WIRE", cx: rx0, cy: y_merge, r: 2.8, fill: true });
            }
        } else if !rung.enable_no.is_empty() {
            for (ei, en) in rung.enable_no.iter().take(2).enumerate() {
                let ex = if ei == 0 { rx0 } else { rx_par };
                d.push(IecDraw::Circle { layer: "IEC_CONTROL_WIRE", cx: ex, cy: y_bus_ctrl, r: 2.8, fill: true });
                d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: ex, y1: y_bus_ctrl, x2: ex, y2: y_top_blk, dashed: false, thick: false });
                draw_vertical_contact(&mut d, ex, y_top_blk, y_bot_blk, &en.device, &en.terminal, false, false);
                d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: ex, y1: y_bot_blk, x2: ex, y2: y_merge, dashed: false, thick: false });
                if ei > 0 {
                    d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: y_merge, x2: ex, y2: y_merge, dashed: false, thick: false });
                    d.push(IecDraw::Circle { layer: "IEC_CONTROL_WIRE", cx: rx0, cy: y_merge, r: 2.8, fill: true });
                }
            }
        } else {
            d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: y_bus_ctrl, x2: rx0, y2: y_merge, dashed: false, thick: false });
        }

        // Series electrical interlock NC contacts
        let mut cy = y_merge;
        for il in &rung.interlock_nc {
            let ny = cy + 20.0;
            d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: cy, x2: rx0, y2: ny, dashed: false, thick: false });
            draw_vertical_contact(&mut d, rx0, ny, ny + 36.0, &il.device, &il.terminal, true, false);
            cy = ny + 36.0;
        }

        // Contactor coil rectangle (A1 top, A2 bottom)
        let coil_w = 40.0;
        let coil_h = 26.0;
        let coil_y = y_rail_bot - 60.0;
        d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: cy, x2: rx0, y2: coil_y, dashed: false, thick: false });
        d.push(IecDraw::Rect { layer: "IEC_SYMBOLS", x: rx0 - coil_w * 0.5, y: coil_y, w: coil_w, h: coil_h, fill: true });
        d.push(IecDraw::Text {
            layer: "IEC_TEXT",
            x: rx0,
            y: coil_y + 17.0,
            size: 11.5,
            anchor: "middle",
            bold: true,
            text: rung.coil.clone(),
        });
        d.push(IecDraw::Text { layer: "IEC_TEXT", x: rx0 - 16.0, y: coil_y - 4.0, size: 8.5, anchor: "end", bold: false, text: "A1".into() });
        d.push(IecDraw::Text { layer: "IEC_TEXT", x: rx0 - 16.0, y: coil_y + coil_h + 10.0, size: 8.5, anchor: "end", bold: false, text: "A2".into() });
        d.push(IecDraw::Line { layer: "IEC_CONTROL_WIRE", x1: rx0, y1: coil_y + coil_h, x2: rx0, y2: y_rail_bot, dashed: false, thick: false });
        d.push(IecDraw::Circle { layer: "IEC_CONTROL_WIRE", cx: rx0, cy: y_rail_bot, r: 2.8, fill: true });

        // Automatic IEC 60617 Cross-Reference Table right beneath the 0V rail!
        if let Some(xr) = model.cross_refs.get(&rung.coil) {
            let mut xy = y_rail_bot + 16.0;
            if let Some(ref pl) = xr.power_loc {
                d.push(IecDraw::Text {
                    layer: "IEC_CROSSREF",
                    x: rx0,
                    y: xy,
                    size: 8.0,
                    anchor: "middle",
                    bold: true,
                    text: format!("PWR: {pl}"),
                });
                xy += 12.0;
            }
            if !xr.no_locs.is_empty() {
                let compact: Vec<String> = xr.no_locs.iter().map(|s| s.replace(" (", "(")).collect();
                d.push(IecDraw::Text {
                    layer: "IEC_CROSSREF",
                    x: rx0,
                    y: xy,
                    size: 7.6,
                    anchor: "middle",
                    bold: false,
                    text: format!("NO: {}", compact.join(", ")),
                });
                xy += 12.0;
            }
            if !xr.nc_locs.is_empty() {
                let compact: Vec<String> = xr.nc_locs.iter().map(|s| s.replace(" (", "(")).collect();
                d.push(IecDraw::Text {
                    layer: "IEC_CROSSREF",
                    x: rx0,
                    y: xy,
                    size: 7.6,
                    anchor: "middle",
                    bold: false,
                    text: format!("NC: {}", compact.join(", ")),
                });
            }
        }
    }

    (total_w, total_h, d)
}

fn draw_3pole_switch(d: &mut Vec<IecDraw>, xs: [f64; 3], y0: f64, y1: f64, label: &str, is_mcb: bool) {
    let ym = (y0 + y1) * 0.5;
    let terms = [("1", "2"), ("3", "4"), ("5", "6")];
    for (i, &x) in xs.iter().enumerate() {
        // Fixed top terminal + angled moving blade
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: y0, x2: x, y2: y0 + 6.0, dashed: false, thick: true });
        if is_mcb {
            // IEC circuit-breaker X trip mark at top contact
            d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 3.0, y1: y0 + 3.0, x2: x + 3.0, y2: y0 + 9.0, dashed: false, thick: false });
            d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x + 3.0, y1: y0 + 3.0, x2: x - 3.0, y2: y0 + 9.0, dashed: false, thick: false });
        }
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 9.0, y1: y0 + 7.0, x2: x, y2: y1 - 5.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: y1 - 5.0, x2: x, y2: y1, dashed: false, thick: true });
        d.push(IecDraw::Text { layer: "IEC_TEXT", x: x + 4.0, y: y0 + 6.0, size: 7.5, anchor: "start", bold: false, text: terms[i].0.into() });
        d.push(IecDraw::Text { layer: "IEC_TEXT", x: x + 4.0, y: y1 - 1.0, size: 7.5, anchor: "start", bold: false, text: terms[i].1.into() });
    }
    // Dashed mechanical link tying all 3 poles together!
    d.push(IecDraw::Line { layer: "IEC_MECH_LINK", x1: xs[0] - 14.0, y1: ym, x2: xs[2] - 4.0, y2: ym, dashed: true, thick: false });
    d.push(IecDraw::Text {
        layer: "IEC_TEXT",
        x: xs[0] - 18.0,
        y: ym + 4.0,
        size: 11.0,
        anchor: "end",
        bold: true,
        text: label.to_string(),
    });
}

fn draw_3pole_overload(d: &mut Vec<IecDraw>, xs: [f64; 3], y0: f64, y1: f64, label: &str) {
    let ym = (y0 + y1) * 0.5;
    for &x in &xs {
        // IEC bimetallic thermal overload heater loop
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: y0, x2: x, y2: ym - 8.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: ym - 8.0, x2: x - 7.0, y2: ym - 8.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 7.0, y1: ym - 8.0, x2: x - 7.0, y2: ym + 8.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 7.0, y1: ym + 8.0, x2: x, y2: ym + 8.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: ym + 8.0, x2: x, y2: y1, dashed: false, thick: true });
    }
    d.push(IecDraw::Line { layer: "IEC_MECH_LINK", x1: xs[0] - 14.0, y1: ym, x2: xs[2] - 7.0, y2: ym, dashed: true, thick: false });
    d.push(IecDraw::Text {
        layer: "IEC_TEXT",
        x: xs[0] - 18.0,
        y: ym + 4.0,
        size: 11.0,
        anchor: "end",
        bold: true,
        text: label.to_string(),
    });
}

fn draw_vertical_contact(
    d: &mut Vec<IecDraw>,
    x: f64,
    y0: f64,
    y1: f64,
    label: &str,
    term: &str,
    is_nc: bool,
    is_pushbutton: bool,
) {
    let ym = (y0 + y1) * 0.5;
    let (t1, t2) = term.split_once('-').unwrap_or((term, ""));
    d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: y0, x2: x, y2: y0 + 5.0, dashed: false, thick: true });
    if is_nc {
        // IEC NC contact: top return tick + blade resting against fixed contact
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: y0 + 5.0, x2: x + 8.0, y2: y0 + 5.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x + 7.0, y1: y0 + 3.0, x2: x, y2: y1 - 5.0, dashed: false, thick: true });
    } else {
        // IEC NO contact: blade angled open to the left
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 9.0, y1: y0 + 6.0, x2: x, y2: y1 - 5.0, dashed: false, thick: true });
    }
    d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x, y1: y1 - 5.0, x2: x, y2: y1, dashed: false, thick: true });
    if is_pushbutton {
        // Dashed manual pushbutton actuator + U-cap on left
        d.push(IecDraw::Line { layer: "IEC_MECH_LINK", x1: x - 18.0, y1: ym, x2: x - 3.0, y2: ym, dashed: true, thick: false });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 22.0, y1: ym - 6.0, x2: x - 18.0, y2: ym - 6.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 18.0, y1: ym - 6.0, x2: x - 18.0, y2: ym + 6.0, dashed: false, thick: true });
        d.push(IecDraw::Line { layer: "IEC_SYMBOLS", x1: x - 22.0, y1: ym + 6.0, x2: x - 18.0, y2: ym + 6.0, dashed: false, thick: true });
    }
    let lx = if is_pushbutton { x - 25.0 } else { x - 12.0 };
    d.push(IecDraw::Text { layer: "IEC_TEXT", x: lx, y: ym + 3.5, size: 10.0, anchor: "end", bold: true, text: label.to_string() });
    let t1_x = if is_nc { x + 11.0 } else { x + 5.0 };
    d.push(IecDraw::Text { layer: "IEC_TEXT", x: t1_x, y: y0 + 7.0, size: 8.0, anchor: "start", bold: false, text: t1.to_string() });
    if !t2.is_empty() {
        d.push(IecDraw::Text { layer: "IEC_TEXT", x: x + 6.0, y: y1 - 1.0, size: 8.0, anchor: "start", bold: false, text: t2.to_string() });
    }
}

pub fn render_industrial_svg(ir: &CanonicalIr) -> String {
    let Some(model) = extract_model(ir) else {
        return "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_string();
    };
    let (w, h, items) = build_iec_geometry(&model);
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
    .pwr {{ stroke: #0f172a; stroke-width: 1.9; fill: none; stroke-linecap: round; }}
    .ctrl {{ stroke: #1e293b; stroke-width: 1.4; fill: none; stroke-linecap: round; }}
    .mech {{ stroke: #64748b; stroke-width: 1.1; stroke-dasharray: 4 3; fill: none; }}
    .sym-fill {{ fill: #fef9c3; stroke: #0f172a; stroke-width: 1.8; }}
    .dot {{ fill: #0f172a; }}
    .term {{ fill: #ffffff; stroke: #0f172a; stroke-width: 1.5; }}
    .txt {{ font-family: 'Inter', system-ui, sans-serif; fill: #0f172a; }}
    .xref {{ font-family: monospace; fill: #0369a1; }}
    .ok {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #15803d; text-anchor: end; }}
    .err {{ font-family: monospace; font-size: 11px; font-weight: 700; fill: #dc2626; text-anchor: end; }}
  </style></defs>"##
    );
    let _ = writeln!(out, r#"  <rect class="bg" x="0" y="0" width="{w:.0}" height="{h:.0}"/>"#);
    let _ = writeln!(out, r#"  <rect class="sheet" x="14" y="14" width="{:.0}" height="{:.0}" rx="6"/>"#, w - 28.0, h - 28.0);
    // Divider line between Power and Control halves
    let _ = writeln!(out, r##"  <line x1="448" y1="36" x2="448" y2="{:.0}" stroke="#e2e8f0" stroke-width="1.2" stroke-dasharray="6 4"/>"##, h - 78.0);

    for item in &items {
        match item {
            IecDraw::Line { layer, x1, y1, x2, y2, dashed, thick } => {
                let cls = if *dashed || *layer == "IEC_MECH_LINK" {
                    "mech"
                } else if *thick {
                    "pwr"
                } else {
                    "ctrl"
                };
                let _ = writeln!(out, r#"  <line class="{cls}" x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"/>"#);
            }
            IecDraw::Rect { x, y, w, h, fill, .. } => {
                let cls = if *fill { "sym-fill" } else { "pwr" };
                let _ = writeln!(out, r#"  <rect class="{cls}" x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="2"/>"#);
            }
            IecDraw::Circle { cx, cy, r, fill, .. } => {
                let cls = if *fill && *r <= 4.0 {
                    "dot"
                } else if *fill {
                    "sym-fill"
                } else {
                    "term"
                };
                let _ = writeln!(out, r#"  <circle class="{cls}" cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}"/>"#);
            }
            IecDraw::Text { layer, x, y, size, anchor, bold, text } => {
                let cls = if *layer == "IEC_CROSSREF" { "xref" } else { "txt" };
                let fw = if *bold { "700" } else { "400" };
                let esc = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
                let _ = writeln!(
                    out,
                    r#"  <text class="{cls}" x="{x:.1}" y="{y:.1}" font-size="{size:.1}" font-weight="{fw}" text-anchor="{anchor}">{esc}</text>"#
                );
            }
        }
    }

    // Footer title block + formal safety badge
    let fy = h - 42.0;
    let _ = writeln!(out, r##"  <line x1="24" y1="{:.0}" x2="{:.0}" y2="{:.0}" stroke="#cbd5e1" stroke-width="1"/>"##, fy - 20.0, w - 24.0, fy - 20.0);
    let _ = writeln!(
        out,
        r#"  <text class="txt" x="30" y="{fy:.0}" font-size="14" font-weight="700">EDL IEC 60617: {}</text>"#,
        model.title
    );
    let _ = writeln!(
        out,
        r##"  <text class="txt" x="30" y="{:.0}" font-size="10.5" fill="#475569">Power: {} | Control: {} | Devices: {} | Rungs: {}</text>"##,
        fy + 16.0,
        model.supply_power,
        model.supply_control,
        model.devices.len(),
        model.rungs.len()
    );
    if ir.project.has_errors {
        let nerr = ir.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
        let _ = writeln!(out, r#"  <text class="err" x="{:.0}" y="{fy:.0}">SAFETY VIOLATION ({nerr} error(s))</text>"#, w - 30.0);
    } else {
        let _ = writeln!(
            out,
            r#"  <text class="ok" x="{:.0}" y="{fy:.0}">ALL SAFETY CHECKS PROVED ({} rules)</text>"#,
            w - 30.0,
            ir.evidence.len()
        );
    }
    let _ = writeln!(out, "</svg>");
    out
}

pub fn export_industrial_dxf(ir: &CanonicalIr) -> String {
    let Some(model) = extract_model(ir) else {
        return String::new();
    };
    let (_w, h, items) = build_iec_geometry(&model);
    let mut out = String::new();
    let _ = writeln!(out, "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC");
    let _ = writeln!(out, "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n6");
    for (name, color) in [
        ("IEC_POWER_BUS", 7),
        ("IEC_CONTROL_WIRE", 5),
        ("IEC_SYMBOLS", 3),
        ("IEC_MECH_LINK", 8),
        ("IEC_TEXT", 2),
        ("IEC_CROSSREF", 4),
    ] {
        let _ = writeln!(out, "0\nLAYER\n2\n{name}\n70\n0\n62\n{color}\n6\nCONTINUOUS");
    }
    let _ = writeln!(out, "0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES");
    // Flip Y for AutoCAD Y-up
    let fy = |y: f64| h - y;
    for item in &items {
        match item {
            IecDraw::Line { layer, x1, y1, x2, y2, .. } => {
                let _ = writeln!(
                    out,
                    "0\nLINE\n8\n{layer}\n10\n{x1:.2}\n20\n{:.2}\n30\n0.0\n11\n{x2:.2}\n21\n{:.2}\n31\n0.0",
                    fy(*y1), fy(*y2)
                );
            }
            IecDraw::Rect { layer, x, y, w, h: rh, .. } => {
                let (x1, y1, x2, y2) = (*x, fy(*y), *x + *w, fy(*y + *rh));
                for (a, b, c, d) in [(x1, y1, x2, y1), (x2, y1, x2, y2), (x2, y2, x1, y2), (x1, y2, x1, y1)] {
                    let _ = writeln!(out, "0\nLINE\n8\n{layer}\n10\n{a:.2}\n20\n{b:.2}\n30\n0.0\n11\n{c:.2}\n21\n{d:.2}\n31\n0.0");
                }
            }
            IecDraw::Circle { layer, cx, cy, r, .. } => {
                let _ = writeln!(out, "0\nCIRCLE\n8\n{layer}\n10\n{cx:.2}\n20\n{:.2}\n30\n0.0\n40\n{r:.2}", fy(*cy));
            }
            IecDraw::Text { layer, x, y, size, text, .. } => {
                let _ = writeln!(out, "0\nTEXT\n8\n{layer}\n10\n{x:.2}\n20\n{:.2}\n30\n0.0\n40\n{size:.2}\n1\n{text}", fy(*y));
            }
        }
    }
    let _ = writeln!(out, "0\nENDSEC\n0\nEOF");
    out
}
