//! Declaration rules must not report a clean PASS when they simply saw no
//! opt-out marker. Absence of evidence is not evidence of a control.

use prodgate::gate;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn project() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-unconfirmed-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=web\n").unwrap();
    fs::write(dir.join("src/app.py"), "logger.error('failed')\n").unwrap();
    dir
}

#[test]
fn declaration_rules_report_unconfirmed_when_no_marker() {
    let dir = project();
    let report = gate::run_gate(&dir);
    for id in ["rate.limit", "error.tracking", "webhook.idempotency", "rls", "backup"] {
        let f = report.findings.iter().find(|f| f.id == id).unwrap();
        assert_eq!(
            f.verdict,
            prodgate::check::Verdict::Unconfirmed,
            "{} should be UNCONFIRMED, not PASS",
            id
        );
        assert!(f.note.contains("not confirmed"));
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn signal_rules_still_report_pass_when_no_signal() {
    let dir = project();
    let report = gate::run_gate(&dir);
    for id in ["cors.wildcard", "secrets.hardcoded"] {
        let f = report.findings.iter().find(|f| f.id == id).unwrap();
        assert_eq!(f.verdict, prodgate::check::Verdict::Pass);
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unconfirmed_does_not_block_the_gate() {
    let dir = project();
    let report = gate::run_gate(&dir);
    assert!(report.passed(), "UNCONFIRMED is reported but non-blocking");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn declaration_with_opt_out_is_still_a_fail() {
    let dir = project();
    fs::write(dir.join("src/app.rs"), "// prodgate: rate_limit=off\n").unwrap();
    let report = gate::run_gate(&dir);
    let f = report.findings.iter().find(|f| f.id == "rate.limit").unwrap();
    assert_eq!(f.verdict, prodgate::check::Verdict::Fail);
    fs::remove_dir_all(&dir).ok();
}
