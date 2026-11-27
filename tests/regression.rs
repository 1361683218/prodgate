//! False-positive regression snapshots. Every FP fix from the last rounds is
//! frozen here, so a future rule change that reintroduces one fails the test.

use prodgate::check::{all_rules, RuleCtx};
use prodgate::regression;
use std::fs;
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn curated_snapshots_have_zero_false_positives() {
    let r = regression::run();
    assert!(r.snapshots >= 10, "all curated snapshots must be present");
    assert_eq!(r.files_scanned, r.snapshots, "every snapshot must be a recognized source file");
    assert!(r.false_positives.is_empty(), "false positives: {:?}", r.false_positives);
    assert!(r.passed);
}

#[test]
fn snapshots_are_not_trivially_empty() {
    // A regression suite of empty files would pass vacuously. Each snapshot
    // must carry real code.
    let dir = root().join("benchmark/regression");
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let src = fs::read_to_string(entry.path()).unwrap();
        assert!(src.len() > 40, "snapshot {} is too short to be meaningful", entry.file_name().to_string_lossy());
    }
}

#[test]
fn known_false_positive_shapes_are_present() {
    // Guard the guard: the exact shapes that caused past FPs must stay in the
    // suite, otherwise someone could "fix" the test by deleting them.
    let dir = root().join("benchmark/regression");
    let all: String = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(all.contains("fn eval"), "Rust ordinary eval() shape missing");
    assert!(all.contains("app_fingerprint"), "print-in-identifier shape missing");
    assert!(all.contains("SHA-1"), "format-required hash shape missing");
    assert!(all.contains("secure_filename"), "safe path shape missing");
    assert!(all.contains("shell=False"), "subprocess list shape missing");
    assert!(all.contains("JSON.parse"), "safe eval alternative missing");
}

#[test]
fn json_reports_false_positives() {
    let r = regression::run();
    let doc = regression::to_json(&r);
    assert!(matches!(doc.get("passed"), Some(prodgate::json::Json::Bool(true))));
    assert!(doc.get("false_positives").is_some());
}

#[test]
fn each_snapshot_is_classified_as_a_source() {
    // The regression runner skips unknown languages; if a snapshot extension
    // were wrong it would pass vacuously. Ensure all are recognized.
    use prodgate::adapter::Language;
    let dir = root().join("benchmark/regression");
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        assert_ne!(Language::for_path(&name), Language::Unknown,
            "snapshot {} has an unrecognized extension", name);
    }
    let _ = all_rules(); // guard import stays meaningful
    let _ = RuleCtx { surface: "web" };
}
