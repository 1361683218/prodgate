//! Mutation validation tests. The key property: a rule that passes replay
//! but misses a real-world pattern must be caught here. Otherwise mutation
//! is just another self-congratulatory green check.

use prodgate::{mutation, report};

#[test]
fn all_shipped_mutations_are_detected_end_to_end() {
    let results = mutation::run_mutations();
    assert!(mutation::passed(&results));
    assert_eq!(results.len(), 10);
    for r in &results {
        assert!(r.ok(), "mutation missed: {} ({})", r.name, r.note);
        assert_ne!(r.evidence, "-", "detected sample must carry evidence");
    }
}

#[test]
fn every_rule_family_is_covered() {
    let results = mutation::run_mutations();
    let rules: Vec<&str> = results.iter().map(|r| r.rule).collect();
    // The two signal rules and one declaration rule must be exercised.
    assert!(rules.contains(&"cors.wildcard"));
    assert!(rules.contains(&"secrets.hardcoded"));
    assert!(rules.contains(&"rls"));
}

#[test]
fn mutation_json_reports_detected_and_total() {
    let doc = report::mutation_json();
    let detected = match doc.get("detected") {
        Some(prodgate::json::Json::Num(n)) => *n as usize,
        _ => panic!("detected field missing"),
    };
    let total = match doc.get("total") {
        Some(prodgate::json::Json::Num(n)) => *n as usize,
        _ => panic!("total field missing"),
    };
    assert_eq!(detected, total);
    assert!(total >= 6);
    assert_eq!(doc.get("command").and_then(prodgate::json::Json::as_str), Some("mutate"));
}

#[test]
fn a_missed_mutation_makes_the_report_fail() {
    // Prove the pass/fail logic has teeth: one missed sample must fail the
    // whole set, even when all others are detected.
    let results = vec![
        mutation::MutationResult {
            name: "good",
            rule: "cors.wildcard",
            detected: true,
            evidence: "lib/x.js:1".into(),
            note: "".into(),
        },
        mutation::MutationResult {
            name: "missed",
            rule: "cors.wildcard",
            detected: false,
            evidence: "-".into(),
            note: "rule did not fire".into(),
        },
    ];
    assert!(!mutation::passed(&results));
}

#[test]
fn empty_mutation_set_is_not_a_pass() {
    // An empty set would vacuously pass with `all()`. Refuse it.
    assert!(!mutation::passed(&[]));
}
