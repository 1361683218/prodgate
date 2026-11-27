//! Real-world false-positive regression snapshots.
//!
//! These are code shapes from real projects that LOOK like rule triggers but
//! are safe — a Rust method named `eval`, a hash used for a wire format, a
//! parameterized SQL query. Every false-positive fix from the last rounds is
//! frozen here so a future rule change cannot silently reintroduce it.
//!
//! Unlike `efficacy`, these snapshots are hand-curated (not AI-generated).
//! They guard precision, not prompt coverage.

use crate::json::Json;
use crate::{adapter::SourceFile, check, gate};
use std::fs;
use std::path::{Path, PathBuf};

const SNAPSHOT_DIR: &str = "benchmark/regression";

pub struct RegressionReport {
    pub snapshots: usize,
    pub files_scanned: usize,
    pub false_positives: Vec<String>,
    pub passed: bool,
}

fn snapshot_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT_DIR)
}

pub fn run() -> RegressionReport {
    let root = snapshot_root();
    let files: Vec<PathBuf> = fs::read_dir(&root)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();

    // Build a SourceFile list directly and run every applicable rule, so the
    // check does not depend on a prodgate.conf in the snapshot directory.
    let sources: Vec<SourceFile> = files
        .iter()
        .filter_map(|p| {
            let lang = crate::adapter::Language::for_path(
                &p.file_name().unwrap_or_default().to_string_lossy(),
            );
            if !lang.is_source() {
                return None;
            }
            fs::read_to_string(p)
                .ok()
                .map(|src| SourceFile::new(p.file_name().unwrap().to_string_lossy().into(), src))
        })
        .collect();

    let ctx = check::RuleCtx { surface: "web" };
    let mut false_positives = Vec::new();
    for rule in check::all_rules() {
        if !gate::applies_to_surface(&rule, "web") {
            continue;
        }
        for file in &sources {
            if let Some(f) = (rule.run)(file, &ctx) {
                if f.verdict == check::Verdict::Fail {
                    false_positives.push(format!("{} @ {} — {}", rule.id, f.evidence, f.note));
                }
            }
        }
    }

    let passed = !sources.is_empty() && false_positives.is_empty();
    RegressionReport {
        snapshots: files.len(),
        files_scanned: sources.len(),
        false_positives,
        passed,
    }
}

pub fn to_json(r: &RegressionReport) -> Json {
    let fps: Vec<Json> = r.false_positives.iter().map(|s| Json::Str(s.clone())).collect();
    Json::Obj(vec![
        ("tool".into(), Json::Str("prodgate".into())),
        ("command".into(), Json::Str("regression".into())),
        ("passed".into(), Json::Bool(r.passed)),
        ("snapshots".into(), Json::Num(r.snapshots as f64)),
        ("files_scanned".into(), Json::Num(r.files_scanned as f64)),
        ("false_positives".into(), Json::Arr(fps)),
    ])
}
