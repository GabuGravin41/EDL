use crate::core::ir::{CanonicalIr, EntityGeometryIr};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BomLineItem {
    pub item: usize,
    pub qty: usize,
    pub category: String,
    pub value: String,
    pub footprint: String,
    pub designators: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BomReport {
    pub project: String,
    pub total_components: usize,
    pub unique_parts: usize,
    pub items: Vec<BomLineItem>,
}

/// Group identical electronic components by `(category, value, footprint)` into a production BOM.
pub fn build_bom(ir: &CanonicalIr) -> BomReport {
    let mut groups: BTreeMap<(String, String, String), (Vec<String>, BTreeSet<String>)> =
        BTreeMap::new();
    let mut total_components = 0usize;

    for ent in &ir.entities {
        let EntityGeometryIr::Component {
            category,
            value,
            footprint,
            ..
        } = &ent.geometry
        else {
            continue;
        };
        total_components += 1;
        let key = (
            category.clone(),
            value.clone(),
            footprint.clone().unwrap_or_default(),
        );
        let entry = groups.entry(key).or_default();
        entry.0.push(ent.id.clone());
        if let Some(ref prov) = ent.provenance {
            entry.1.insert(prov.module.clone());
        }
    }

    let mut items = Vec::new();
    for (idx, ((category, value, footprint), (mut designators, mods))) in
        groups.into_iter().enumerate()
    {
        designators.sort();
        items.push(BomLineItem {
            item: idx + 1,
            qty: designators.len(),
            category,
            value,
            footprint,
            designators,
            modules: mods.into_iter().collect(),
        });
    }

    BomReport {
        project: ir.project.name.clone(),
        total_components,
        unique_parts: items.len(),
        items,
    }
}

/// Export a clean RFC-4180 CSV Bill of Materials ready for JLCPCB / DigiKey / Mouser / KiCad.
pub fn export_bom_csv(ir: &CanonicalIr) -> String {
    let bom = build_bom(ir);
    let mut out = String::new();
    let _ = writeln!(out, "Item,Qty,Designators,Category,Value,Footprint,Modules");
    for row in &bom.items {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{}",
            row.item,
            row.qty,
            csv_escape(&row.designators.join(" ")),
            csv_escape(&row.category),
            csv_escape(&row.value),
            csv_escape(&row.footprint),
            csv_escape(&row.modules.join(" "))
        );
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
