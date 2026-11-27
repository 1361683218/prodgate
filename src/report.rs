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
        (
            "suggestion".into(),
            f.suggestion.map(s).unwrap_or(Json::Null),
        ),
    ])
}

pub fn gate_json(root: &Path) -> Json {
    build_gate_json(&gate::run_gate(root))
}

/// Machine-readable scan history, read from `.prodgate/history.jsonl`.
/// Viewing history never gates a build, so `passed` is always true — same
/// convention as the other informational commands (report, prompt, baseline).
pub fn history_json(root: &Path) -> Json {
    let snaps = crate::history::load(root);
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("history")),
        ("passed".into(), Json::Bool(true)),
        ("count".into(), Json::Num(snaps.len() as f64)),
        ("history".into(), crate::history::to_json(&snaps)),
    ])
}

/// `prune` 是维护操作而非门禁：清理过期快照不会让构建失败，只报告删了几条。
pub fn prune_json(removed: usize) -> Json {
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("prune")),
        ("passed".into(), Json::Bool(true)),
        ("removed".into(), Json::Num(removed as f64)),
    ])
}

/// CLI gate uses this variant so a user-facing scan appends to history.
/// `all` and `report` call `gate_json` directly and do not pollute history.
pub fn gate_json_recording(root: &Path) -> Json {
    let report = gate::run_gate(root);
    let _ = crate::history::record(root, &report);
    build_gate_json(&report)
}

fn build_gate_json(report: &gate::GateReport) -> Json {
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
    let cases = cases_json(root);
    let regression = crate::regression::to_json(&crate::regression::run());
    // Artifact-on-disk: a phase whose named output is missing is failed, no
    // matter how convincing the agent's report sounds. Absent manifest means
    // the project declares no artifact contract — legitimate PASS.
    let artifacts = crate::artifacts::to_json(&crate::artifacts::check_root(root));
    // Efficacy is an experiment report, not a gate: it measures information
    // sufficiency and can legitimately be inspected without blocking a build.
    let efficacy = crate::efficacy::to_json(&crate::efficacy::run(root));
    let is_true = |d: &Json| {
        d.get("passed")
            .map(|v| matches!(v, Json::Bool(true)))
            .unwrap_or(false)
    };
    // Baseline abuse guard: an AI with shell access can freeze every FAIL
    // with `prodgate baseline`. A small baseline is legitimate (gradual
    // adoption); a large one means the gate is being bypassed. Surface the
    // count AND refuse to call an oversized baseline a pass.
    let baseline_count = gate::load_baseline(root).len();
    let baseline_ok = baseline_count <= gate::MAX_BASELINE;
    // Every guard must pass. A false positive reintroduced by a rule change
    // fails `regression`, even if replay/gate still look fine.
    let passed = is_true(&replay)
        && is_true(&mutation)
        && is_true(&gate)
        && is_true(&cases)
        && is_true(&regression)
        && is_true(&artifacts)
        && baseline_ok;
    Json::Obj(vec![
        ("tool".into(), s("prodgate")),
        ("command".into(), s("all")),
        ("passed".into(), Json::Bool(passed)),
        ("replay".into(), replay),
        ("mutation".into(), mutation),
        ("gate".into(), gate),
        ("cases".into(), cases),
        ("regression".into(), regression),
        ("artifacts".into(), artifacts),
        ("baseline_count".into(), Json::Num(baseline_count as f64)),
        ("baseline_limit".into(), Json::Num(gate::MAX_BASELINE as f64)),
        ("efficacy".into(), efficacy),
    ])
}
