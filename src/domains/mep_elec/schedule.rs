use crate::core::ir::{CanonicalIr, MepCircuitIr};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelScheduleReport {
    pub project: String,
    pub circuit_count: usize,
    pub total_connected_load_w: f64,
    pub total_design_current_a: f64,
    pub total_cable_run_m: f64,
    pub circuits: Vec<MepCircuitIr>,
}

pub fn build_panel_schedule(ir: &CanonicalIr) -> PanelScheduleReport {
    let total_load_w: f64 = ir.mep_circuits.iter().map(|c| c.total_load_w).sum();
    let total_current_a: f64 = ir.mep_circuits.iter().map(|c| c.design_current_a).sum();
    let total_cable_m: f64 = ir.mep_circuits.iter().map(|c| c.run_length_m).sum();
    PanelScheduleReport {
        project: ir.project.name.clone(),
        circuit_count: ir.mep_circuits.len(),
        total_connected_load_w: (total_load_w * 100.0).round() / 100.0,
        total_design_current_a: (total_current_a * 100.0).round() / 100.0,
        total_cable_run_m: (total_cable_m * 100.0).round() / 100.0,
        circuits: ir.mep_circuits.clone(),
    }
}

pub fn export_panel_schedule_csv(ir: &CanonicalIr) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Circuit,Panel,Kind,Breaker_A,RCD_mA,Cable_mm2,Devices,Load_W,Current_A,Run_m,VDrop_pct,MaxDrop_pct,Status"
    );
    for c in &ir.mep_circuits {
        let rcd = c
            .rcd_ma
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(
            out,
            "{},{},{},{:.0},{},{:.1},{},{:.1},{:.2},{:.2},{:.2},{:.1},{}",
            c.id,
            c.panel_id,
            c.kind,
            c.rating_a,
            rcd,
            c.cable_mm2,
            c.connected_devices.len(),
            c.total_load_w,
            c.design_current_a,
            c.run_length_m,
            c.voltage_drop_pct,
            c.max_voltage_drop_pct,
            c.status
        );
    }
    out
}
