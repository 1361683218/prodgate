//! A scan that matches no source files must FAIL, not report a clean result.
//! Otherwise a wrong `roots` value silently disables the entire gate.

use prodgate::check::Verdict;
use prodgate::gate;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn empty_project() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-no-sources-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn zero_source_files_is_a_failure_not_a_pass() {
    let dir = empty_project();
    fs::write(dir.join("prodgate.conf"), "roots=does-not-exist\nsurface=web\n").unwrap();

    let report = gate::run_gate(&dir);
    assert!(!report.passed(), "zero-file scan must not be PASS");
    assert_eq!(report.files_scanned, 0);
    let finding = &report.findings[0];
    assert_eq!(finding.id, "config.no-sources");
    assert_eq!(finding.verdict, Verdict::Fail);
    assert!(finding.note.contains("does-not-exist"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn no_sources_finding_cannot_be_baselined() {
    let dir = empty_project();
    fs::write(dir.join("prodgate.conf"), "roots=missing\nsurface=web\n").unwrap();

    // Even with the synthetic fingerprint manually present in baseline, the
    // gate must still fail. A misconfiguration is never a known issue.
    let baseline_dir = dir.join(".prodgate");
    fs::create_dir_all(&baseline_dir).unwrap();
    fs::write(
        baseline_dir.join("baseline.txt"),
        "config.no-sources|-\n",
    )
    .unwrap();

    let report = gate::run_gate(&dir);
    assert!(!report.passed(), "no-sources must ignore baseline");
    assert_eq!(report.findings[0].verdict, Verdict::Fail);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn real_source_files_still_scan_normally() {
    let dir = empty_project();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=cli\n").unwrap();
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();

    let report = gate::run_gate(&dir);
    assert_eq!(report.files_scanned, 1);
    assert!(report.passed());
    assert!(report.findings.iter().all(|f| f.id != "config.no-sources"));

    fs::remove_dir_all(&dir).ok();
}
