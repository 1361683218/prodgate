//! The gate must report ALL problems in a file at once. An AI fixing findings
//! one at a time (fix, re-scan, fix the next) is a bad workflow.

use prodgate::gate;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn project(src: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-multi-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=web\n").unwrap();
    fs::write(dir.join("src").join(name), src).unwrap();
    dir
}

#[test]
fn reports_every_occurrence_of_a_rule() {
    let dir = project(
        "const a = eval(req.body.x);\nconst b = eval(req.query.y);\nconst c = eval(req.body.z);\n",
        "a.js",
    );
    let report = gate::run_gate(&dir);
    let evals: Vec<_> = report.findings.iter()
        .filter(|f| f.id == "eval.injection" && f.verdict == prodgate::check::Verdict::Fail)
        .collect();
    assert_eq!(evals.len(), 3, "all three eval sites must be reported at once");
    assert!(evals.iter().any(|f| f.evidence.ends_with(":1")));
    assert!(evals.iter().any(|f| f.evidence.ends_with(":2")));
    assert!(evals.iter().any(|f| f.evidence.ends_with(":3")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reports_across_multiple_files() {
    let dir = project("const a = eval(req.body.x);\n", "a.js");
    fs::write(dir.join("src/b.js"), "const b = eval(req.query.y);\n").unwrap();
    let report = gate::run_gate(&dir);
    let evals: Vec<_> = report.findings.iter()
        .filter(|f| f.id == "eval.injection" && f.verdict == prodgate::check::Verdict::Fail)
        .collect();
    assert_eq!(evals.len(), 2, "one eval in each file must both be reported");
    assert!(evals.iter().any(|f| f.evidence.contains("a.js")));
    assert!(evals.iter().any(|f| f.evidence.contains("b.js")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn no_signal_still_reports_one_pass() {
    // When a rule finds nothing, it must still emit a single PASS/UNCONFIRMED
    // row — the blank-and-rescan loop must not create spurious duplicates.
    let dir = project("const x = JSON.parse(req.body.value);\n", "a.js");
    let report = gate::run_gate(&dir);
    let evals: Vec<_> = report.findings.iter().filter(|f| f.id == "eval.injection").collect();
    assert_eq!(evals.len(), 1, "no-signal rules emit exactly one row");
    assert_eq!(evals[0].verdict, prodgate::check::Verdict::Pass);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn finding_cap_prevents_runaway() {
    // A file with more than the cap of findings must terminate. Collect all
    // reported lines and assert they are unique and bounded.
    let src = "eval(req.body.x);\n".repeat(60);
    let dir = project(&src, "a.js");
    let report = gate::run_gate(&dir);
    let evals: Vec<_> = report.findings.iter()
        .filter(|f| f.id == "eval.injection" && f.verdict == prodgate::check::Verdict::Fail)
        .collect();
    assert!(!evals.is_empty());
    assert!(evals.len() <= 50, "finding cap must hold");
    let lines: std::collections::HashSet<_> = evals.iter().map(|f| &f.evidence).collect();
    assert_eq!(lines.len(), evals.len(), "no duplicate evidence");
    fs::remove_dir_all(&dir).ok();
}
