//! Artifact-on-disk verification. The collaboration protocol says a named
//! output missing from disk means the phase failed — regardless of how
//! fluent the agent's report sounds. This is its mechanical enforcement.

use prodgate::{artifacts, report};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-artifacts-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("output")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=web\n").unwrap();
    fs::write(dir.join("src/a.rs"), "fn main() {}\n").unwrap();
    dir
}

fn manifest(dir: &std::path::Path, lines: &[&str]) {
    fs::write(dir.join("artifacts.tsv"), lines.join("\n") + "\n").unwrap();
}

#[test]
fn absent_manifest_is_a_legitimate_pass() {
    let dir = project();
    let r = artifacts::check_root(&dir);
    assert!(!r.manifest_present);
    assert_eq!(r.checked, 0);
    assert!(r.passed());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn present_artifact_passes() {
    let dir = project();
    fs::write(dir.join("output/draft.md"), "content\n").unwrap();
    manifest(&dir, &["phase1\trequired\toutput/draft.md"]);
    let r = artifacts::check_root(&dir);
    assert!(r.manifest_present);
    assert_eq!(r.checked, 1);
    assert!(r.passed(), "{:?}", r.failures);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_artifact_fails() {
    let dir = project();
    manifest(&dir, &["phase1\trequired\toutput/never.md"]);
    let r = artifacts::check_root(&dir);
    assert!(!r.passed());
    assert!(r.failures[0].contains("missing artifact"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn empty_file_and_empty_dir_fail() {
    let dir = project();
    fs::write(dir.join("output/empty.md"), "").unwrap();
    fs::create_dir_all(dir.join("output/emptydir")).unwrap();
    manifest(&dir, &[
        "a\trequired\toutput/empty.md",
        "b\trequired\toutput/emptydir",
    ]);
    let r = artifacts::check_root(&dir);
    assert!(!r.passed());
    assert_eq!(r.failures.len(), 2);
    assert!(r.failures[0].contains("empty artifact file"));
    assert!(r.failures[1].contains("empty artifact dir"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn not_assessed_without_reason_is_a_failure() {
    // A silent skip is indistinguishable from a pass — the dangerous case.
    let dir = project();
    manifest(&dir, &["phase1\tnot-assessed\toutput/x.md"]);
    let r = artifacts::check_root(&dir);
    assert!(!r.passed());
    assert!(r.failures[0].contains("without a reason"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn not_assessed_with_reason_is_recorded() {
    let dir = project();
    manifest(&dir, &["phase1\tnot-assessed\toutput/x.md\tno browser available"]);
    let r = artifacts::check_root(&dir);
    assert!(r.passed());
    assert_eq!(r.not_assessed.len(), 1);
    assert!(r.not_assessed[0].contains("no browser available"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn comments_and_unknown_status() {
    let dir = project();
    manifest(&dir, &[
        "# a comment",
        "",
        "phase1\twat\toutput/x.md",
    ]);
    let r = artifacts::check_root(&dir);
    assert_eq!(r.checked, 1, "comments and blank lines must not count");
    assert!(!r.passed());
    assert!(r.failures[0].contains("unknown status"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn all_gate_inherits_artifact_failure() {
    // This is the point of folding artifacts into `all`: the Ralph Loop
    // judge wraps `all`, so it inherits artifact checking for free.
    let dir = project();
    manifest(&dir, &["phase1\trequired\toutput/never.md"]);
    let doc = report::all_json(&dir);
    assert_eq!(doc.get("passed"), Some(&prodgate::json::Json::Bool(false)));
    match doc.get("artifacts") {
        Some(a) => assert_eq!(a.get("passed"), Some(&prodgate::json::Json::Bool(false))),
        _ => panic!("all_json must carry the artifacts section"),
    }
    fs::remove_dir_all(&dir).ok();
}
