//! v0.8 integration tests: bidirectional sync (KiCad + DXF) and the live watch bridge.

use edl::core::sync::{apply_plan_to_file, parse_kicad_symbols, plan_from_dxf, plan_from_kicad, run_sync};
use edl::watch::{handle_request, WatchState};
use edl::{compile_file, export_dxf, export_kicad_sch};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("edl_v08_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_examples(dst: &Path) {
    fn rec(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for e in fs::read_dir(src).unwrap() {
            let e = e.unwrap();
            let t = dst.join(e.file_name());
            if e.path().is_dir() {
                rec(&e.path(), &t);
            } else {
                fs::copy(e.path(), &t).unwrap();
            }
        }
    }
    rec(Path::new("examples"), dst);
}

/// Human drags U1 and edits C1's value inside KiCad -> `edl sync --apply` -> source patched surgically.
#[test]
fn kicad_round_trip_moves_and_value_edits_patch_source_surgically() {
    let dir = scratch("kicad");
    copy_examples(&dir);
    let edl = dir.join("mcu_board_3v3.edl");
    let original = fs::read_to_string(&edl).unwrap();

    let ir0 = compile_file(&edl).unwrap();
    let kicad_text = export_kicad_sch(&ir0);
    assert_eq!(kicad_text.matches("(property \"EDL_ID\"").count(), ir0.entities.len());

    // An untouched export must produce an empty plan (no phantom diffs).
    let plan0 = plan_from_kicad(&ir0, &kicad_text).unwrap();
    assert_eq!(plan0.patch_count(), 0, "{:?}", plan0.actions);

    // Simulate the human edit.
    let u1_before = {
        let syms = parse_kicad_symbols(&kicad_text).unwrap();
        let u1 = syms.iter().find(|s| s.reference == "U1").unwrap();
        (u1.x, u1.y)
    };
    let edited = kicad_text
        .replacen(
            &format!("(at {:.2} {:.2} 0)", u1_before.0, u1_before.1),
            &format!("(at {:.2} {:.2} 0)", u1_before.0 + 15.24, u1_before.1 + 10.16),
            1,
        )
        .replace(
            "(property \"Reference\" \"C1\"",
            "(property \"Reference\" \"C1\"",
        );
    // C1's placed Value property is the one that follows its Reference property.
    let c1_ref = edited.find("(property \"Reference\" \"C1\"").unwrap();
    let val_rel = edited[c1_ref..].find("(property \"Value\" \"10uF\"").unwrap();
    let mut edited = edited;
    edited.replace_range(
        c1_ref + val_rel..c1_ref + val_rel + "(property \"Value\" \"10uF\"".len(),
        "(property \"Value\" \"22uF\"",
    );
    let edited_path = dir.join("edited.kicad_sch");
    fs::write(&edited_path, &edited).unwrap();

    // Dry run first: nothing is written, but the patch is proven to re-verify.
    let (plan, res) = run_sync(&edl, Some(&edited_path), None, false).unwrap();
    assert_eq!(plan.patch_count(), 2, "{}", edl::core::sync::format_plan(&plan));
    assert!(res.applied && !res.written);
    assert_eq!(fs::read_to_string(&edl).unwrap(), original);

    // Apply.
    let (_, res) = run_sync(&edl, Some(&edited_path), None, true).unwrap();
    assert!(res.written, "{}", res.message);
    let patched = fs::read_to_string(&edl).unwrap();

    // Surgical: exactly one changed line + one inserted `place` line; everything else byte-identical.
    let (a, b): (Vec<_>, Vec<_>) = (original.lines().collect(), patched.lines().collect());
    assert_eq!(b.len(), a.len() + 1);
    assert_eq!(patched.matches("value: 22uF").count(), 1);
    assert!(patched.contains("place U1 at: ("));
    let same = a.iter().filter(|l| b.contains(l)).count();
    assert!(same >= a.len() - 1);

    // Recompiled IR reflects the human's intent and still verifies.
    let ir1 = compile_file(&edl).unwrap();
    assert!(!ir1.project.has_errors, "{:?}", ir1.diagnostics);
    let c1 = ir1.entities.iter().find(|e| e.id == "C1").unwrap();
    assert_eq!(c1.props["value"], "22uF");
    let rect = |ir: &edl::core::ir::CanonicalIr, id: &str| match &ir.entities.iter().find(|e| e.id == id).unwrap().geometry {
        edl::core::ir::EntityGeometryIr::Component { body_rect, .. } => (body_rect.min_x, body_rect.min_y),
        _ => panic!(),
    };
    let (before, after) = (rect(&ir0, "U1"), rect(&ir1, "U1"));
    assert!((after.0 - before.0 - 15.24).abs() < 1.3, "{before:?} -> {after:?}");
    assert!((after.1 - before.1 - 10.16).abs() < 1.3, "{before:?} -> {after:?}");

    // Idempotent: re-exporting the synced project and syncing again has nothing left to do.
    let again = plan_from_kicad(&ir1, &export_kicad_sch(&ir1)).unwrap();
    assert_eq!(again.patch_count(), 0, "{:?}", again.actions);

    // Second sync of a *moved* placement must update the existing `place` line, not duplicate it.
    let k1 = export_kicad_sch(&ir1);
    let s = parse_kicad_symbols(&k1).unwrap();
    let u1 = s.iter().find(|x| x.reference == "U1").unwrap();
    let moved = k1.replacen(
        &format!("(at {:.2} {:.2} 0)", u1.x, u1.y),
        &format!("(at {:.2} {:.2} 0)", u1.x - 10.16, u1.y),
        1,
    );
    fs::write(&edited_path, moved).unwrap();
    let (_, res) = run_sync(&edl, Some(&edited_path), None, true).unwrap();
    assert!(res.written, "{}", res.message);
    assert_eq!(fs::read_to_string(&edl).unwrap().matches("place U1 at:").count(), 1);
}

#[test]
fn kicad_sync_reports_rotation_and_module_parts_instead_of_guessing() {
    let dir = scratch("kicad_mod");
    copy_examples(&dir);
    let edl = dir.join("dual_rail_modular.edl");
    let ir = compile_file(&edl).unwrap();
    let k = export_kicad_sch(&ir);

    // Module-instantiated part: value change is `unsupported` (must be changed via module parameter).
    let pos = k.find("(property \"Reference\" \"PwrLed_R\"").unwrap();
    let rel = k[pos..].find("(property \"Value\" \"1kohm\"").unwrap();
    let mut edited = k.clone();
    edited.replace_range(
        pos + rel..pos + rel + "(property \"Value\" \"1kohm\"".len(),
        "(property \"Value\" \"2kohm\"",
    );
    let plan = plan_from_kicad(&ir, &edited).unwrap();
    let a = plan.actions.iter().find(|a| a.entity == "PwrLed_R").unwrap();
    assert_eq!(a.status, "unsupported");
    assert!(a.description.contains("LedIndicator"), "{}", a.description);
    assert_eq!(plan.patch_count(), 0);
    let res = apply_plan_to_file(&edl, &plan, true).unwrap();
    assert!(!res.written);

    // Rotation is reported, never silently ignored.
    let syms = parse_kicad_symbols(&k).unwrap();
    let s = syms.iter().find(|s| s.reference == "Reg3V3_U").unwrap();
    let rotated = k.replacen(
        &format!("(at {:.2} {:.2} 0)", s.x, s.y),
        &format!("(at {:.2} {:.2} 90)", s.x, s.y),
        1,
    );
    let plan = plan_from_kicad(&ir, &rotated).unwrap();
    assert!(plan.actions.iter().any(|a| a.kind == "rotate_component" && a.status == "unsupported"));
}

#[test]
fn dxf_sync_patches_freestanding_fixture_and_reports_semantic_drift() {
    let dir = scratch("dxf");
    copy_examples(&dir);
    let edl = dir.join("apartment_2br_electrical.edl");
    let ir0 = compile_file(&edl).unwrap();
    let dxf = export_dxf(&ir0);
    assert!(dxf.contains("EDL_ANCHORS") && dxf.contains("APPID"));

    // Untouched export: no drift.
    assert!(plan_from_dxf(&ir0, &dxf).unwrap().actions.is_empty());

    // In AutoCAD the user nudges the dining table (freestanding) and the sofa (wall-anchored) and a room.
    let move_anchor = |dxf: &str, id: &str, dx: f64, dy: f64| -> String {
        let lines: Vec<&str> = dxf.lines().collect();
        let mut out = lines.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for i in 0..lines.len() {
            if lines[i] == "1000" && lines[i + 1] == id && lines[i - 1] == "EDL" {
                // walk back to the POINT's 10/20 pairs
                let mut j = i;
                while lines[j] != "POINT" {
                    j -= 1;
                }
                for k in j..i {
                    if lines[k] == "10" {
                        out[k + 1] = format!("{:.4}", lines[k + 1].parse::<f64>().unwrap() + dx);
                    }
                    if lines[k] == "20" {
                        out[k + 1] = format!("{:.4}", lines[k + 1].parse::<f64>().unwrap() + dy);
                    }
                }
            }
        }
        out.join("\n") + "\n"
    };
    let edited = move_anchor(&dxf, "DiningTable", 100.0, 100.0);
    let edited = move_anchor(&edited, "Sofa", 0.0, 150.0);
    let edited = move_anchor(&edited, "Bed1", 500.0, 0.0);
    let dxf_path = dir.join("edited.dxf");
    fs::write(&dxf_path, &edited).unwrap();

    let (plan, res) = run_sync(&edl, None, Some(&dxf_path), true).unwrap();
    assert!(res.written, "{}\n{}", res.message, edl::core::sync::format_plan(&plan));
    let by = |id: &str| plan.actions.iter().find(|a| a.entity == id).unwrap();
    assert_eq!(by("DiningTable").status, "patch");
    assert_eq!(by("Sofa").status, "report_only");
    assert_eq!(by("Bed1").kind, "move_room");
    assert_eq!(by("Bed1").status, "report_only");

    let patched = fs::read_to_string(&edl).unwrap();
    assert!(patched.contains("at: (2700mm, 1500mm)"), "{patched}");
    let ir1 = compile_file(&edl).unwrap();
    assert!(!ir1.project.has_errors, "{:?}", ir1.diagnostics);
    // Room geometry (constraint-solved) is untouched by the drift.
    assert_eq!(ir1.project.exterior_footprint_mm, ir0.project.exterior_footprint_mm);
}

#[test]
fn sync_rejects_patches_that_would_break_verification_or_compilation() {
    let dir = scratch("reject");
    copy_examples(&dir);
    let edl = dir.join("apartment_2br_electrical.edl");
    let ir0 = compile_file(&edl).unwrap();
    let before = fs::read_to_string(&edl).unwrap();
    let dxf = export_dxf(&ir0);
    // Shove the dining table 5 m east: out of the room -> would violate fixture checks.
    let lines: Vec<&str> = dxf.lines().collect();
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    for i in 1..lines.len() - 1 {
        if lines[i] == "1000" && lines[i + 1] == "DiningTable" && lines[i - 1] == "EDL" {
            let mut j = i;
            while lines[j] != "POINT" {
                j -= 1;
            }
            for k in j..i {
                if lines[k] == "10" {
                    out[k + 1] = format!("{:.4}", lines[k + 1].parse::<f64>().unwrap() + 5000.0);
                }
            }
        }
    }
    let p = dir.join("bad.dxf");
    fs::write(&p, out.join("\n") + "\n").unwrap();
    let (plan, res) = run_sync(&edl, None, Some(&p), true).unwrap();
    assert_eq!(plan.patch_count(), 1);
    assert!(!res.written && !res.applied, "{}", res.message);
    assert_eq!(fs::read_to_string(&edl).unwrap(), before, "file must be untouched on rejection");
}

#[test]
fn watch_bridge_hot_reloads_keeps_last_good_and_syncs_kicad_back() {
    let dir = scratch("watch");
    copy_examples(&dir);
    let edl = dir.join("mcu_board_3v3.edl");
    let kicad = dir.join("live.kicad_sch");

    let state = Mutex::new(WatchState::new(&edl, Some(&kicad)));
    {
        let s = state.lock().unwrap();
        assert!(s.ok);
        assert!(s.svg.contains("<svg"));
        assert!(kicad.exists(), "initial KiCad export");
    }

    // HTTP surface (pure handler; no sockets needed).
    let (code, ctype, body) = handle_request(&state, "/");
    assert_eq!((code, ctype), (200, "text/html; charset=utf-8"));
    assert!(String::from_utf8(body).unwrap().contains("EDL Live"));
    let (_, _, body) = handle_request(&state, "/api/state");
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["summary"]["evidence_total"], 7); // E1-E5 + E11_sheet_frame_fit + E12_no_component_overlap
    let ver = v["version"].as_u64().unwrap();
    let (_, _, body) = handle_request(&state, &format!("/api/state?since={ver}"));
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["changed"], false);
    assert_eq!(handle_request(&state, "/nope").0, 404);

    // AI edits the .edl -> hot reload + KiCad re-export.
    std::thread::sleep(std::time::Duration::from_millis(30));
    let src = fs::read_to_string(&edl).unwrap();
    fs::write(&edl, src.replacen("value: 10uF", "value: 47uF", 1)).unwrap();
    assert!(state.lock().unwrap().refresh());
    assert!(fs::read_to_string(&kicad).unwrap().contains("\"47uF\""));

    // Broken edit -> error overlay, last good SVG retained.
    std::thread::sleep(std::time::Duration::from_millis(30));
    let good_svg = state.lock().unwrap().svg.clone();
    fs::write(&edl, "project \"Broken\" { units: mm ").unwrap();
    assert!(state.lock().unwrap().refresh());
    {
        let s = state.lock().unwrap();
        assert!(!s.ok);
        assert!(!s.errors.is_empty());
        assert_eq!(s.svg, good_svg);
    }
    std::thread::sleep(std::time::Duration::from_millis(30));
    fs::write(&edl, src.replacen("value: 10uF", "value: 47uF", 1)).unwrap();
    assert!(state.lock().unwrap().refresh());
    assert!(state.lock().unwrap().ok);

    // Human saves in KiCad (changes R1 value) -> synced back into the .edl automatically.
    std::thread::sleep(std::time::Duration::from_millis(30));
    let k = fs::read_to_string(&kicad).unwrap();
    let pos = k.find("(property \"Reference\" \"R1\"").unwrap();
    let rel = k[pos..].find("(property \"Value\" \"10kohm\"").unwrap();
    let mut edited = k.clone();
    edited.replace_range(
        pos + rel..pos + rel + "(property \"Value\" \"10kohm\"".len(),
        "(property \"Value\" \"4.7kohm\"",
    );
    fs::write(&kicad, edited).unwrap();
    assert!(state.lock().unwrap().refresh());
    let after = fs::read_to_string(&edl).unwrap();
    assert!(after.contains("4.7kohm"), "{after}");
    assert!(after.contains("47uF"), "AI's earlier edit must survive the sync");
    assert!(state.lock().unwrap().ok);
    let events = state.lock().unwrap().events.join("\n");
    assert!(events.contains("kicad save detected: 1 patch"), "{events}");
}

#[test]
fn watch_http_server_serves_over_real_socket() {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let dir = scratch("sock");
    copy_examples(&dir);
    let edl = dir.join("power_5v.edl");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let state = Arc::new(Mutex::new(WatchState::new(&edl, None)));
    let stop = Arc::new(AtomicBool::new(false));
    let (st, sp) = (state.clone(), stop.clone());
    let h = std::thread::spawn(move || edl::watch::serve(listener, st, sp));

    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(b"GET /api/state HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    assert!(resp.starts_with("HTTP/1.1 200 OK"), "{resp}");
    assert!(resp.contains("\"svg\""));
    stop.store(true, Ordering::Relaxed);
    h.join().unwrap();
}
