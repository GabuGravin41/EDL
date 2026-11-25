use edl::domains::elec::kicad_import::{compare_kicad_netlists, import_kicad_project};
use std::fs;
use std::path::Path;

fn assert_kicad_demo_roundtrip(rel_path: &str, expected_min_comps: usize, expected_min_nets: usize) {
    let orig = Path::new("/usr/share/kicad/demos").join(rel_path);
    if !orig.is_file() {
        return; // Skip gracefully on machines without KiCad demos installed
    }
    let stem = orig.file_stem().and_then(|s| s.to_str()).unwrap_or("demo");
    let out_dir = std::env::temp_dir().join(format!(
        "edl_corpus_test_{}_{}",
        std::process::id(),
        stem.replace(' ', "_")
    ));
    let _ = fs::remove_dir_all(&out_dir);

    let rep = import_kicad_project(&orig, &out_dir)
        .unwrap_or_else(|e| panic!("import_kicad_project failed for {rel_path}: {e}"));
    let ir = edl::compile_file(&rep.edl_path)
        .unwrap_or_else(|e| panic!("compile_file failed for {rel_path}: {e:?}"));

    let rt_dir = out_dir.join("roundtrip_kicad");
    fs::create_dir_all(&rt_dir).unwrap();
    let files = edl::export_kicad_project(&ir);
    for (fname, content) in &files {
        let dest = rt_dir.join(fname);
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(&dest, content).unwrap();
    }

    let root_name = orig.file_name().and_then(|s| s.to_str()).unwrap();
    let rt_root = rt_dir.join(root_name);
    let cmp = compare_kicad_netlists(&orig, &rt_root)
        .unwrap_or_else(|e| panic!("compare_kicad_netlists failed for {rel_path}: {e}"));

    assert!(
        cmp.exact_match,
        "Netlist mismatch on {rel_path}: orig=({} comps, {} nets) vs rt=({} comps, {} nets), missing_comps={:?}, extra_comps={:?}, missing_nets={:?}, extra_nets={:?}",
        cmp.orig_components,
        cmp.orig_nets,
        cmp.rt_components,
        cmp.rt_nets,
        cmp.missing_components,
        cmp.extra_components,
        &cmp.missing_pin_pairs[..cmp.missing_pin_pairs.len().min(3)],
        &cmp.extra_pin_pairs[..cmp.extra_pin_pairs.len().min(3)],
    );
    assert!(cmp.orig_components >= expected_min_comps);
    assert!(cmp.orig_nets >= expected_min_nets);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn kicad_corpus_single_sheet_stickhub_and_smps_roundtrip_100_percent() {
    assert_kicad_demo_roundtrip("stickhub/StickHub.kicad_sch", 90, 40);
    assert_kicad_demo_roundtrip("simulation/power_supplies/boost/smps-com.kicad_sch", 20, 14);
    assert_kicad_demo_roundtrip("ecc83/ecc83-pp.kicad_sch", 15, 9);
}

#[test]
fn kicad_corpus_multisheet_subdirectories_and_reused_hierarchy_roundtrip_100_percent() {
    assert_kicad_demo_roundtrip("complex_hierarchy/complex_hierarchy.kicad_sch", 65, 45);
    assert_kicad_demo_roundtrip("royalblue54L_feather/RoyalBlue54L-Feather.kicad_sch", 70, 65);
    assert_kicad_demo_roundtrip("multichannel/multichannel_mixer.kicad_sch", 110, 75);
    assert_kicad_demo_roundtrip("video/video.kicad_sch", 180, 350);
}

#[test]
fn kicad_corpus_cern_vme_wren_1500_component_fpga_roundtrip_100_percent() {
    assert_kicad_demo_roundtrip("vme-wren/vme-wren.kicad_sch", 1500, 1300);
}

#[test]
fn kicad_corpus_visual_fidelity_text_boxes_dashed_frames_and_field_visibility() {
    let vme = Path::new("/usr/share/kicad/demos/vme-wren/vme-wren.kicad_sch");
    let stickhub = Path::new("/usr/share/kicad/demos/stickhub/StickHub.kicad_sch");
    if !vme.is_file() || !stickhub.is_file() {
        return;
    }

    // 1. CERN vme-wren: wrapped yellow comment text_boxes, dashed black graphic frames,
    //    hidden long Value + visible Description ("±1%") & Val ("2M2") on R398, and white sheet fill.
    let vme_dir = std::env::temp_dir().join(format!("edl_vis_vme_{}", std::process::id()));
    let _ = fs::remove_dir_all(&vme_dir);
    let rep = import_kicad_project(vme, &vme_dir).expect("import vme-wren");
    let ir = edl::compile_file(&rep.edl_path).expect("compile vme-wren");
    let files = edl::export_kicad_project(&ir);
    let (_, root_sch) = files
        .iter()
        .find(|(f, _)| f.as_str() == "vme-wren.kicad_sch")
        .expect("root vme-wren.kicad_sch");
    assert!(root_sch.contains("(text_box \"SFP Cage."), "missing SFP Cage text_box");
    assert!(root_sch.contains("(text_box \"Really slow VME control signals"), "missing VME control signals text_box");
    assert!(root_sch.contains("(type dash) (color 0 0 0 1)"), "missing black dashed Fiducials/ESD frame");
    assert!(root_sch.contains("(property \"Description\" \"±1%\""), "missing visible Description field on R398");
    assert!(root_sch.contains("(property \"Val\" \"2M2\""), "missing visible Val field on R398");
    assert!(root_sch.contains("(fill (color 255 255 255 1.0000))"), "missing white sheet fill");
    let _ = fs::remove_dir_all(&vme_dir);

    // 2. StickHub: LOGO1..LOGO4 hidden Reference + visible 0.9906mm left-justified Footprint & Value.
    let sh_dir = std::env::temp_dir().join(format!("edl_vis_sh_{}", std::process::id()));
    let _ = fs::remove_dir_all(&sh_dir);
    let rep2 = import_kicad_project(stickhub, &sh_dir).expect("import StickHub");
    let ir2 = edl::compile_file(&rep2.edl_path).expect("compile StickHub");
    let files2 = edl::export_kicad_project(&ir2);
    let (_, sh_sch) = files2
        .iter()
        .find(|(f, _)| f.as_str() == "StickHub.kicad_sch")
        .expect("StickHub.kicad_sch");
    assert!(
        sh_sch.contains("(property \"Reference\" \"LOGO1\"")
            && sh_sch.contains("(font (size 0.9906 0.9906)) (justify left)"),
        "StickHub LOGO1 field styling did not round-trip"
    );
    let _ = fs::remove_dir_all(&sh_dir);
}
