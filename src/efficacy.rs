//! Prompt efficacy experiment.
//!
//! HONEST SCOPE: this does not measure whether an AI changes its behavior
//! after reading the prompt — this environment cannot generate fresh AI code
//! on demand. It measures the weaker but still useful claim:
//!
//! 1. BASELINE — the archived AI-written samples (written without the
//!    prompt) contain N real findings.
//! 2. COVERAGE — for every rule that produced a finding, the prompt carries
//!    a complete entry (bad example, good example, fix).
//! 3. COUNTERFACTUAL — the good example the prompt teaches actually passes
//!    the gate. If an AI copies the good form, that finding would not occur.
//!
//! Coverage + counterfactual together mean "following the prompt is
//! sufficient to avoid every observed finding". That is an information-
//! sufficiency result, NOT a behavioral one. The README states this clearly.

use crate::json::Json;
use crate::{adapter::SourceFile, cases, check, gate, prompt};
use std::fs;
use std::path::{Path, PathBuf};

const BENCH_ROOT: &str = "benchmark/ai-code";

pub struct EfficacyReport {
    pub samples: usize,
    pub samples_with_findings: usize,
    pub baseline_findings: usize,
    pub rules_hit: Vec<String>,
    pub covered: usize,
    pub counterfactual_ok: usize,
    pub gaps: Vec<String>,
    pub passed: bool,
}

fn bench_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(BENCH_ROOT)
}

fn sample_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let root = bench_root();
    let Ok(batches) = fs::read_dir(&root) else { return out };
    for batch in batches.flatten().filter(|e| e.path().is_dir()) {
        let Ok(samples) = fs::read_dir(batch.path()) else { continue };
        for s in samples.flatten().filter(|e| e.path().is_dir()) {
            out.push(s.path());
        }
    }
    out.sort();
    out
}

/// Do the delivered prompt sections carry everything needed for `rule_id`?
fn section_covers(sections: &[Json], rule_id: &str) -> bool {
    sections.iter().any(|s| {
        s.get("id").and_then(Json::as_str) == Some(rule_id)
            && matches!(s.get("bad"), Some(Json::Str(_)))
            && matches!(s.get("good"), Some(Json::Str(_)))
            && matches!(s.get("suggestion"), Some(Json::Str(_)))
    })
}

/// The good example taught by the prompt must not itself trigger the rule.
/// This is the counterfactual: copying the taught form avoids the finding.
fn counterfactual_holds(root: &Path, sections: &[Json], rule_id: &str) -> bool {
    let Some(section) = sections.iter().find(|s| {
        s.get("id").and_then(Json::as_str) == Some(rule_id)
    }) else { return false };
    let Some(Json::Str(good_text)) = section.get("good") else { return false };

    let rule = check::all_rules().into_iter().find(|r| r.id == rule_id);
    let Some(rule) = rule else { return false };
    // Good examples come from case files with language-specific extensions;
    // pick one the rule's adapter can classify, then run the rule directly.
    let cases_root = root;
    let results = cases::validate(cases_root);
    let Some(c) = results.iter().find(|r| r.meta.rule == rule_id && r.valid()) else {
        return false;
    };
    let dir = cases_root
        .join("cases")
        .join(rule_id)
        .join(c.meta.id.split_once('/').map(|(_, id)| id).unwrap_or("c001"));
    let good_dir = dir.join("good");
    let Ok(mut entries) = fs::read_dir(good_dir) else { return false };
    let Some(Ok(entry)) = entries.next() else { return false };
    let name = entry.file_name().to_string_lossy().to_string();
    let Ok(src) = fs::read_to_string(entry.path()) else { return false };
    let file = SourceFile::new(name, src);
    let _ = good_text; // section already validated as Str by section_covers
    (rule.run)(&file, &check::RuleCtx { surface: "web" }).is_none()
}

pub fn run(root: &Path) -> EfficacyReport {
    let dirs = sample_dirs();
    let mut baseline = 0;
    let mut with_findings = 0;
    let mut rules_hit: Vec<String> = Vec::new();

    for d in &dirs {
        // Each archived sample already has prodgate.conf; scan in place.
        let report = gate::run_gate(d);
        let fails: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.verdict == check::Verdict::Fail)
            .collect();
        if !fails.is_empty() {
            with_findings += 1;
        }
        baseline += fails.len();
        for f in fails {
            if !rules_hit.contains(&f.id.to_string()) {
                rules_hit.push(f.id.to_string());
            }
        }
    }

    // Prompt is generated at web surface (what the archived samples target).
    let prompt_sections = prompt::sections(root, "web");
    let mut covered = 0;
    let mut counterfactual_ok = 0;
    let mut gaps = Vec::new();
    for id in &rules_hit {
        if !section_covers(&prompt_sections, id) {
            gaps.push(format!("{}: prompt entry incomplete", id));
            continue;
        }
        covered += 1;
        if !counterfactual_holds(root, &prompt_sections, id) {
            gaps.push(format!("{}: taught good example still triggers the rule", id));
        } else {
            counterfactual_ok += 1;
        }
    }

    let passed = !dirs.is_empty()
        && baseline > 0
        && covered == rules_hit.len()
        && counterfactual_ok == rules_hit.len()
        && gaps.is_empty();

    EfficacyReport {
        samples: dirs.len(),
        samples_with_findings: with_findings,
        baseline_findings: baseline,
        rules_hit,
        covered,
        counterfactual_ok,
        gaps,
        passed,
    }
}

pub fn to_json(r: &EfficacyReport) -> Json {
    let hits: Vec<Json> = r.rules_hit.iter().map(|s| Json::Str(s.clone())).collect();
    let gaps: Vec<Json> = r.gaps.iter().map(|s| Json::Str(s.clone())).collect();
    Json::Obj(vec![
        ("tool".into(), Json::Str("prodgate".into())),
        ("command".into(), Json::Str("efficacy".into())),
        ("passed".into(), Json::Bool(r.passed)),
        ("samples".into(), Json::Num(r.samples as f64)),
        ("samples_with_findings".into(), Json::Num(r.samples_with_findings as f64)),
        ("baseline_findings".into(), Json::Num(r.baseline_findings as f64)),
        ("rules_hit".into(), Json::Arr(hits)),
        ("covered".into(), Json::Num(r.covered as f64)),
        ("counterfactual_ok".into(), Json::Num(r.counterfactual_ok as f64)),
        ("gaps".into(), Json::Arr(gaps)),
        ("scope".into(), Json::Str(
            "information sufficiency only; does not measure AI behavior change".into()
        )),
    ])
}
