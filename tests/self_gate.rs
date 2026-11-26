//! The second gate: cargo test must prove the rule library is replayable and
//! that the tool passes its own gate. If either breaks, CI goes red.

use prodgate::{check::Verdict, fixture, gate};
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn every_rule_distinguishes_good_from_bad() {
    let results = fixture::replay(root());
    assert!(!results.is_empty(), "rule library must not be empty");
    for r in results {
        assert_eq!(
            r.good,
            Verdict::Pass,
            "rule {} failed its good fixture",
            r.rule
        );
        assert_eq!(
            r.bad,
            Verdict::Fail,
            "rule {} failed to catch its bad fixture",
            r.rule
        );
    }
}

#[test]
fn gate_passes_on_its_own_repository() {
    let report = gate::run_gate(root());
    assert!(report.files_scanned > 0, "gate must scan actual source files");
    assert!(report.passed(), "self-gate failed: {:?}", report.findings);
}

#[test]
fn inapplicable_rules_are_not_assessed_not_silently_skipped() {
    let report = gate::run_gate(root());
    let na: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.verdict == Verdict::NotAssessed)
        .collect();
    assert!(!na.is_empty(), "cli repo should have web-only rules unassessed");
    for f in na {
        assert!(!f.note.is_empty(), "NOT-ASSESSED must carry a reason");
    }
}
