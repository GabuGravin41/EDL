use crate::core::ir::ModuleProvenanceIr;
use crate::core::span::Diagnostic;
use crate::domains::arch::ast::*;
use crate::syntax::ast::*;
use crate::syntax::parser::{parse_file, parse_module_body};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Elaborates `import "..."` files and expands all `use Inst = Module(...)` instantiations
/// across BOTH architecture and electronics domains, attaching full module provenance.
pub fn elaborate_project(
    project: &mut ProjectAst,
    base_dir: Option<&Path>,
) -> Result<(), Diagnostic> {
    // 1. Recursively load imported module libraries
    let mut modules: BTreeMap<String, ModuleDecl> = BTreeMap::new();
    let mut visited_files: BTreeSet<PathBuf> = BTreeSet::new();

    if let Some(dir) = base_dir {
        let initial_imports = project.imports.clone();
        for imp in &initial_imports {
            load_import_recursive(dir, imp, &mut visited_files, &mut modules)?;
        }
    } else if !project.imports.is_empty() {
        return Err(Diagnostic::error(
            "E_IMPORT_NO_BASE_DIR",
            "imports",
            format!(
                "Cannot resolve import '{}' without a filesystem base directory (use compile_file)",
                project.imports[0].path
            ),
            Some(project.imports[0].span.clone()),
        ));
    }

    // Local project modules override or extend imported modules
    for m in &project.modules {
        modules.insert(m.name.clone(), m.clone());
    }

    // 2. Elaborate electronics `use` statements inside `circuit` blocks
    let mut inst_to_components: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for circuit in &mut project.elec.circuits {
        struct PortTarget {
            pins: Vec<(String, String)>,
            absorbed_internal_nets: Vec<usize>,
        }
        let mut internal_nets: Vec<crate::domains::elec::ast::NetDecl> = Vec::new();
        let mut port_rewrites: BTreeMap<(String, String), PortTarget> = BTreeMap::new();
        let mut auto_wire_bindings: Vec<(String, String, String, crate::core::span::Span)> = Vec::new();
        let uses = std::mem::take(&mut circuit.uses);

        for u in uses {
            let Some(mdecl) = modules.get(&u.module) else {
                return Err(Diagnostic::error(
                    "E_MODULE_UNKNOWN",
                    "modules",
                    format!("Instance '{}' references undeclared module '{}'", u.instance, u.module),
                    Some(u.span.clone()),
                ));
            };
            let env = bind_module_args(mdecl, &u)?;
            let body = parse_module_body(mdecl, env)?;
            let prov = ModuleProvenanceIr {
                module: mdecl.name.clone(),
                instance: u.instance.clone(),
            };

            let mut expanded_comp_ids = Vec::new();
            for mut comp in body.components {
                let new_id = format!("{}_{}", u.instance, comp.id);
                comp.id = new_id.clone();
                comp.provenance = Some(prov.clone());
                expanded_comp_ids.push(new_id);
                circuit.components.push(comp);
            }
            let net_start_idx = internal_nets.len();
            for mut net in body.nets {
                net.name = format!("{}_{}", u.instance, net.name);
                for pref in &mut net.connects {
                    pref.component = format!("{}_{}", u.instance, pref.component);
                }
                internal_nets.push(net);
            }
            for b in body.port_binds {
                let target_comp = format!("{}_{}", u.instance, b.target.component);
                let target_pin = b.target.pin;
                let mut found_idx = None;
                for (idx, inet) in internal_nets.iter().enumerate().skip(net_start_idx) {
                    if inet
                        .connects
                        .iter()
                        .any(|p| p.component == target_comp && p.pin == target_pin)
                    {
                        found_idx = Some(idx);
                        break;
                    }
                }
                let entry = port_rewrites
                    .entry((u.instance.clone(), b.port))
                    .or_insert_with(|| PortTarget {
                        pins: Vec::new(),
                        absorbed_internal_nets: Vec::new(),
                    });
                if let Some(idx) = found_idx {
                    for p in &internal_nets[idx].connects {
                        let pair = (p.component.clone(), p.pin.clone());
                        if !entry.pins.contains(&pair) {
                            entry.pins.push(pair);
                        }
                    }
                    if !entry.absorbed_internal_nets.contains(&idx) {
                        entry.absorbed_internal_nets.push(idx);
                    }
                } else {
                    let pair = (target_comp, target_pin);
                    if !entry.pins.contains(&pair) {
                        entry.pins.push(pair);
                    }
                }
            }
            for (port_k, net_v, sp) in u.port_nets {
                auto_wire_bindings.push((u.instance.clone(), port_k, net_v, sp));
            }
            inst_to_components.insert(u.instance, expanded_comp_ids);
        }

        // Inject `{ PORT: NET }` bindings straight into parent `circuit.nets` before rewriting!
        for (inst, port_k, net_v, sp) in auto_wire_bindings {
            let pref = crate::domains::elec::ast::PinRef {
                component: inst,
                pin: port_k,
                span: sp.clone(),
            };
            if let Some(existing) = circuit.nets.iter_mut().find(|n| n.name == net_v) {
                existing.connects.push(pref);
            } else {
                circuit.nets.push(crate::domains::elec::ast::NetDecl {
                    name: net_v,
                    hierarchy: Vec::new(),
                    connects: vec![pref],
                    power_flag: false,
                    span: sp,
                });
            }
        }

        // Rewrite parent net connections referencing `Inst.PORT` -> expanded internal pins,
        // absorbing any internal module nets that were merged into parent nets.
        let mut absorbed_nets: BTreeSet<usize> = BTreeSet::new();
        for net in &mut circuit.nets {
            let mut new_connects = Vec::new();
            let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
            for pref in std::mem::take(&mut net.connects) {
                if let Some(target) = port_rewrites.get(&(pref.component.clone(), pref.pin.clone())) {
                    for &idx in &target.absorbed_internal_nets {
                        absorbed_nets.insert(idx);
                    }
                    for (tc, tp) in &target.pins {
                        if seen.insert((tc.clone(), tp.clone())) {
                            new_connects.push(crate::domains::elec::ast::PinRef {
                                component: tc.clone(),
                                pin: tp.clone(),
                                span: pref.span.clone(),
                            });
                        }
                    }
                } else if seen.insert((pref.component.clone(), pref.pin.clone())) {
                    new_connects.push(pref);
                }
            }
            net.connects = new_connects;
        }

        // Emit remaining purely-internal module nets (e.g. R-to-LED series midpoint)
        for (idx, inet) in internal_nets.into_iter().enumerate() {
            if !absorbed_nets.contains(&idx) {
                circuit.nets.push(inet);
            }
        }
    }

    // Expand module instance names inside `schematic group { members: [StatusLed] }`!
    for schem in &mut project.elec.schematics {
        for grp in &mut schem.groups {
            let mut expanded = Vec::new();
            for m in &grp.members {
                if let Some(comp_ids) = inst_to_components.get(m) {
                    expanded.extend(comp_ids.iter().cloned());
                } else {
                    expanded.push(m.clone());
                }
            }
            grp.members = expanded;
        }
    }

    // 3. Elaborate architectural `use` statements inside `project`
    // Track single-room modules so `use Bath1 = StandardBath()` lets parent write `Bath1.west` directly!
    let mut single_room_aliases: BTreeMap<String, String> = BTreeMap::new();
    let arch_uses = std::mem::take(&mut project.uses);

    for u in arch_uses {
        let Some(mdecl) = modules.get(&u.module) else {
            return Err(Diagnostic::error(
                "E_MODULE_UNKNOWN",
                "modules",
                format!("Instance '{}' references undeclared module '{}'", u.instance, u.module),
                Some(u.span.clone()),
            ));
        };
        let env = bind_module_args(mdecl, &u)?;
        let body = parse_module_body(mdecl, env)?;
        let prov = ModuleProvenanceIr {
            module: mdecl.name.clone(),
            instance: u.instance.clone(),
        };

        let is_single_room = body.arch.rooms.len() == 1;
        let mut local_room_map: BTreeMap<String, String> = BTreeMap::new();

        for mut r in body.arch.rooms {
            let orig = r.id.clone();
            let new_id = if is_single_room {
                // Single-room module takes the instance name directly (`Bath1`) AND aliases `Bath1_Space`!
                single_room_aliases.insert(format!("{}_{}", u.instance, orig), u.instance.clone());
                u.instance.clone()
            } else {
                format!("{}_{}", u.instance, orig)
            };
            local_room_map.insert(orig, new_id.clone());
            r.id = new_id;
            r.provenance = Some(prov.clone());
            project.arch.rooms.push(r);
        }

        for mut lay in body.arch.layouts {
            for c in &mut lay.constraints {
                rewrite_linear_expr(&mut c.lhs, &local_room_map);
                rewrite_linear_expr(&mut c.rhs, &local_room_map);
            }
            project.arch.layouts.push(lay);
        }

        for mut op in body.arch.openings {
            op.id = format!("{}_{}", u.instance, op.id);
            op.host = match op.host {
                WallHost::Exterior { room, side } => WallHost::Exterior {
                    room: local_room_map.get(&room).cloned().unwrap_or(room),
                    side,
                },
                WallHost::SharedWall { room_a, room_b, side } => WallHost::SharedWall {
                    room_a: local_room_map.get(&room_a).cloned().unwrap_or(room_a),
                    room_b: local_room_map.get(&room_b).cloned().unwrap_or(room_b),
                    side,
                },
            };
            if let Some(sw) = op.swing_into {
                op.swing_into = Some(local_room_map.get(&sw).cloned().unwrap_or(sw));
            }
            op.provenance = Some(prov.clone());
            project.arch.openings.push(op);
        }

        for mut fx in body.arch.fixtures {
            fx.id = format!("{}_{}", u.instance, fx.id);
            fx.room = local_room_map.get(&fx.room).cloned().unwrap_or(fx.room);
            fx.provenance = Some(prov.clone());
            project.arch.fixtures.push(fx);
        }
    }

    // Apply single-room aliases (`Bath1_Space` -> `Bath1`) in parent layouts/openings/fixtures if used
    if !single_room_aliases.is_empty() {
        for lay in &mut project.arch.layouts {
            for c in &mut lay.constraints {
                rewrite_linear_expr(&mut c.lhs, &single_room_aliases);
                rewrite_linear_expr(&mut c.rhs, &single_room_aliases);
            }
        }
        for op in &mut project.arch.openings {
            op.host = match op.host.clone() {
                WallHost::Exterior { room, side } => WallHost::Exterior {
                    room: single_room_aliases.get(&room).cloned().unwrap_or(room),
                    side,
                },
                WallHost::SharedWall { room_a, room_b, side } => WallHost::SharedWall {
                    room_a: single_room_aliases.get(&room_a).cloned().unwrap_or(room_a),
                    room_b: single_room_aliases.get(&room_b).cloned().unwrap_or(room_b),
                    side,
                },
            };
            if let Some(ref sw) = op.swing_into {
                if let Some(mapped) = single_room_aliases.get(sw) {
                    op.swing_into = Some(mapped.clone());
                }
            }
        }
        for fx in &mut project.arch.fixtures {
            if let Some(mapped) = single_room_aliases.get(&fx.room) {
                fx.room = mapped.clone();
            }
        }
    }

    Ok(())
}

fn load_import_recursive(
    base_dir: &Path,
    imp: &ImportDecl,
    visited: &mut BTreeSet<PathBuf>,
    modules: &mut BTreeMap<String, ModuleDecl>,
) -> Result<(), Diagnostic> {
    let resolved = base_dir.join(&imp.path);
    let canon = fs::canonicalize(&resolved).map_err(|e| {
        Diagnostic::error(
            "E_IMPORT_NOT_FOUND",
            "imports",
            format!("Failed to load import '{}' ({}): {e}", imp.path, resolved.display()),
            Some(imp.span.clone()),
        )
    })?;
    if !visited.insert(canon.clone()) {
        return Ok(()); // already loaded (guards against diamond/cyclic imports cleanly)
    }
    let src = fs::read_to_string(&canon).map_err(|e| {
        Diagnostic::error(
            "E_IMPORT_READ",
            "imports",
            format!("Failed to read '{}': {e}", canon.display()),
            Some(imp.span.clone()),
        )
    })?;
    let file_ast = parse_file(&canon.display().to_string(), &src)?;
    let next_dir = canon.parent().unwrap_or(base_dir);
    for nested in &file_ast.imports {
        load_import_recursive(next_dir, nested, visited, modules)?;
    }
    for m in file_ast.modules {
        modules.insert(m.name.clone(), m);
    }
    Ok(())
}

fn bind_module_args(
    mdecl: &ModuleDecl,
    u: &UseDecl,
) -> Result<BTreeMap<String, ArgValue>, Diagnostic> {
    let mut env: BTreeMap<String, ArgValue> = BTreeMap::new();
    let valid_names: BTreeSet<&str> = mdecl.params.iter().map(|p| p.name.as_str()).collect();

    for p in &mdecl.params {
        if let Some(ref d) = p.default {
            env.insert(p.name.clone(), d.clone());
        }
    }
    for (k, v, span) in &u.args {
        if !valid_names.contains(k.as_str()) {
            return Err(Diagnostic::error(
                "E_MODULE_UNKNOWN_ARG",
                "modules",
                format!(
                    "Module '{}' has no parameter '{}' (valid: [{}])",
                    mdecl.name,
                    k,
                    mdecl
                        .params
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Some(span.clone()),
            ));
        }
        env.insert(k.clone(), v.clone());
    }
    for p in &mdecl.params {
        if !env.contains_key(&p.name) {
            return Err(Diagnostic::error(
                "E_MODULE_MISSING_ARG",
                "modules",
                format!(
                    "Instantiation '{}' of module '{}' is missing required parameter '{}'",
                    u.instance, mdecl.name, p.name
                ),
                Some(u.span.clone()),
            ));
        }
    }
    Ok(env)
}

fn rewrite_linear_expr(expr: &mut LinearExprAst, room_map: &BTreeMap<String, String>) {
    match expr {
        LinearExprAst::RoomEdge { room, .. } | LinearExprAst::RoomDim { room, .. } => {
            if let Some(mapped) = room_map.get(room) {
                *room = mapped.clone();
            }
        }
        LinearExprAst::Add(a, b) | LinearExprAst::Sub(a, b) => {
            rewrite_linear_expr(a, room_map);
            rewrite_linear_expr(b, room_map);
        }
        LinearExprAst::Scale(_, inner) => rewrite_linear_expr(inner, room_map),
        LinearExprAst::Param { .. } | LinearExprAst::Quantity(_, _) => {}
    }
}
