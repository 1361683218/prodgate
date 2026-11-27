//! Machine-readable reports for gate / replay / cases.
//!
//! Every report has the same top-level shape: `tool`, `command`, `passed`,
//! plus command-specific data. Consumers only need to read `passed`.

use crate::json::Json;
use crate::{cases, check, fixture, gate, mutation};
use std::path::Path;

fn s(value: &str) -> Json {
    Json::Str(value.into())
}

/// Serialize one finding into a JSON object.
fn finding(f: &check::Finding) -> Json {
    Json::Obj(vec![
        ("rule".into(), s(f.id)),
        ("verdict".into(), s(f.verdict.as_str())),
        ("evidence".into(), s(&f.evidence)),
        ("note".into(), s(&f.note)),
    ])
}

pub fn gate_json(root: &Path) -> Json {
    let report = gate::run_gate(root);
    let known_issues = report
        .findings
        .iter()
        .filter(|f| f.verdict == check::Verdict::Baseline)
        .count();
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("gate")),
        ("passed".into(), Json::Bool(report.passed())),
        ("files_scanned".into(), Json::Num(report.files_scanned as f64)),
        ("known_issues".into(), Json::Num(known_issues as f64)),
        (
            "findings".into(),
            Json::Arr(report.findings.iter().map(finding).collect()),
        ),
    ])
}

pub fn replay_json(root: &Path) -> Json {
    let results = fixture::replay(root);
    let passed = results.iter().all(|r| r.ok());
    let entries: Vec<Json> = results
        .iter()
        .map(|r| {
            Json::Obj(vec![
                ("rule".into(), s(r.rule)),
                ("good".into(), s(r.good.as_str())),
                ("bad".into(), s(r.bad.as_str())),
                ("ok".into(), Json::Bool(r.ok())),
            ])
        })
        .collect();
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("replay")),
        ("passed".into(), Json::Bool(passed)),
        ("results".into(), Json::Arr(entries)),
    ])
}

pub fn cases_json(root: &Path) -> Json {
    let results = cases::validate(root);
    let passed = results.iter().all(|r| r.valid());
    let entries: Vec<Json> = results
        .iter()
        .map(|r| {
            let errors: Vec<Json> = r.errors.iter().map(|e| s(e)).collect();
            Json::Obj(vec![
                ("case".into(), s(&r.meta.id)),
                ("rule".into(), s(&r.meta.rule)),
                ("provenance".into(), s(r.provenance.as_str())),
                ("good".into(), s(r.good.as_str())),
                ("bad".into(), s(r.bad.as_str())),
                ("valid".into(), Json::Bool(r.valid())),
                ("errors".into(), Json::Arr(errors)),
            ])
        })
        .collect();
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("cases")),
        ("passed".into(), Json::Bool(passed)),
        ("source_backed".into(), Json::Num(cases::trusted_count(&results) as f64)),
        ("invalid".into(), Json::Num(results.iter().filter(|r| !r.valid()).count() as f64)),
        ("cases".into(), Json::Arr(entries)),
    ])
}

pub fn mutation_json() -> Json {
    let results = mutation::run_mutations();
    let detected = results.iter().filter(|r| r.ok()).count();
    let entries: Vec<Json> = results
        .iter()
        .map(|r| {
            Json::Obj(vec![
                ("name".into(), s(r.name)),
                ("rule".into(), s(r.rule)),
                ("detected".into(), Json::Bool(r.detected)),
                ("evidence".into(), s(&r.evidence)),
                ("note".into(), s(&r.note)),
            ])
        })
        .collect();
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("mutate")),
        ("passed".into(), Json::Bool(mutation::passed(&results))),
        ("detected".into(), Json::Num(detected as f64)),
        ("total".into(), Json::Num(results.len() as f64)),
        ("results".into(), Json::Arr(entries)),
    ])
}

/// Combined report for `all`: replay, mutation, then gate. All three must
/// pass. A rule that passes replay but misses mutation is not protecting
/// anything; this is why mutation is part of the self-bootstrap criteria.
pub fn all_json(root: &Path) -> Json {
    let replay = replay_json(root);
    let mutation = mutation_json();
    let gate = gate_json(root);
    let is_true = |d: &Json| {
        d.get("passed")
            .map(|v| matches!(v, Json::Bool(true)))
            .unwrap_or(false)
    };
    let passed = is_true(&replay) && is_true(&mutation) && is_true(&gate);
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("all")),
        ("passed".into(), Json::Bool(passed)),
        ("replay".into(), replay),
        ("mutation".into(), mutation),
        ("gate".into(), gate),
    ])
}
