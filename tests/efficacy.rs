//! Efficacy experiment: information sufficiency of the prompt against the
//! archived AI-written code. Honest scope — no behavioral claim.

use prodgate::json::Json;
use prodgate::{efficacy, prompt};
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn archived_samples_exist_and_have_findings() {
    let r = efficacy::run(root());
    assert!(r.samples >= 13, "all three AI batches must be archived");
    assert!(r.samples_with_findings >= 11);
    assert!(r.baseline_findings >= 13);
    assert!(!r.rules_hit.is_empty());
}

#[test]
fn every_observed_finding_is_covered_by_prompt() {
    let r = efficacy::run(root());
    assert_eq!(r.covered, r.rules_hit.len(),
        "prompt must fully describe every rule the AI violated; gaps: {:?}", r.gaps);
}

#[test]
fn taught_good_form_passes_the_rule() {
    let r = efficacy::run(root());
    assert_eq!(r.counterfactual_ok, r.rules_hit.len(),
        "the good example the prompt teaches must not trigger the rule; gaps: {:?}", r.gaps);
    assert!(r.passed);
    assert!(r.gaps.is_empty());
}

#[test]
fn json_report_is_honest_about_scope() {
    let r = efficacy::run(root());
    let doc = efficacy::to_json(&r);
    assert!(matches!(doc.get("passed"), Some(Json::Bool(true))));
    let scope = doc.get("scope").and_then(Json::as_str).unwrap();
    assert!(scope.contains("information sufficiency"));
    assert!(scope.contains("not measure AI behavior"));
    // Coverage numbers must be present for machine consumers.
    assert!(matches!(doc.get("covered"), Some(Json::Num(_))));
    assert!(matches!(doc.get("counterfactual_ok"), Some(Json::Num(_))));
}

#[test]
fn prompt_sections_have_bad_good_and_fix() {
    // Coverage is only meaningful if sections are complete. Guard the shape.
    let sections = prompt::sections(root(), "web");
    for s in &sections {
        assert!(matches!(s.get("bad"), Some(Json::Str(_))));
        assert!(matches!(s.get("good"), Some(Json::Str(_))));
        // Signal rules carry a fix; declaration rules deliberately do not
        // (whether to disable a control is a human decision).
        assert!(s.get("suggestion").is_some());
    }
}
