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
