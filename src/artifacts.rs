//! Artifact-on-disk verification — the bridge between the collaboration
//! protocol and prodgate's gate.
//!
//! From AGENT-COLLAB-PROTOCOL.md §4: "a named artifact not on disk means the
//! phase failed, no matter how fluent the answer sounds." An agent can burn
//! a whole phase, return a confident summary, and have written nothing.
//! That is neither BLOCKED nor an error — ordinary error handling misses it.
//!
//! This module makes that check mechanical and folds it into `all`, so the
//! Ralph Loop judge inherits it without modification.
//!
//! Manifest format (TSV, compatible with protocol-check.sh):
//!   phase_id<TAB>required<TAB>path
//!   phase_id<TAB>not-assessed<TAB>path<TAB>reason
//!
//! NOT ASSESSED is a first-class verdict: a silent skip is indistinguishable
//! from a pass, which is the dangerous case. An un-reasoned not-assessed is a
//! failure.

use crate::json::Json;
use std::fs;
use std::path::{Path, PathBuf};

/// Default manifest location inside a project. Absent manifest = no artifact
/// contract declared, which is a legitimate PASS (not every project uses the
/// multi-agent protocol).
pub const MANIFEST_FILE: &str = "artifacts.tsv";

pub struct ArtifactReport {
    pub checked: usize,
    pub failures: Vec<String>,
    pub not_assessed: Vec<String>,
    pub manifest_present: bool,
}

impl ArtifactReport {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn manifest_path(root: &Path) -> PathBuf {
    root.join(MANIFEST_FILE)
}

/// Paths that may never count as a phase artifact. Pointing a required
/// artifact at a judge file is a cheap way to manufacture a pass: the file
/// exists and is non-empty, but the agent produced nothing. These paths are
/// all part of the judging apparatus (see REDTEAM.md attack #6).
const FORBIDDEN_ARTIFACT_PREFIXES: &[&str] = &[
    "tests/",
    ".github/workflows/",
    "loop/judge.sh",
    "src/check.rs",
    "src/mutation.rs",
    "src/cases.rs",
    "src/fixture.rs",
    "prodgate.conf",
    "artifacts.tsv",
];

fn is_judge_file(raw_path: &str) -> bool {
    let p = raw_path.trim_start_matches("./").replace('\\', "/");
    FORBIDDEN_ARTIFACT_PREFIXES.iter().any(|f| p == *f || p.starts_with(f))
}

/// Verify artifacts named in a manifest. `root` is used to resolve relative
/// paths so a manifest is portable across checkouts.
pub fn check(root: &Path, manifest: &Path) -> ArtifactReport {
    let mut report = ArtifactReport {
        checked: 0,
        failures: Vec::new(),
        not_assessed: Vec::new(),
        manifest_present: manifest.is_file(),
    };
    let Ok(text) = fs::read_to_string(manifest) else {
        return report;
    };

    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        report.checked += 1;
        let cols: Vec<&str> = line.split('\t').collect();
        let phase = cols.first().copied().unwrap_or("").trim();
        let status = cols.get(1).copied().unwrap_or("").trim();
        let raw_path = cols.get(2).copied().unwrap_or("").trim();

        if phase.is_empty() || raw_path.is_empty() {
            report.failures.push(format!(
                "line {}: malformed entry (need phase<TAB>status<TAB>path)",
                line_no + 1
            ));
            continue;
        }
        if is_judge_file(raw_path) {
            report.failures.push(format!(
                "{}: {} is judge apparatus, not a phase artifact",
                phase, raw_path
            ));
            continue;
        }
        let path = Path::new(raw_path);
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };

        match status {
            "required" => {
                if !full.exists() {
                    report
                        .failures
                        .push(format!("{}: missing artifact {}", phase, raw_path));
                } else if full.is_dir() {
                    let empty = fs::read_dir(&full)
                        .map(|mut d| d.next().is_none())
                        .unwrap_or(true);
                    if empty {
                        report
                            .failures
                            .push(format!("{}: empty artifact dir {}", phase, raw_path));
                    }
                } else if fs::metadata(&full).map(|m| m.len() == 0).unwrap_or(true) {
                    report
                        .failures
                        .push(format!("{}: empty artifact file {}", phase, raw_path));
                }
            }
            "not-assessed" => {
                let reason = cols.get(3).copied().unwrap_or("").trim();
                if reason.is_empty() {
                    report.failures.push(format!(
                        "{}: NOT ASSESSED without a reason (a silent skip is a pass)",
                        phase
                    ));
                } else {
                    report
                        .not_assessed
                        .push(format!("{}: {}", phase, reason));
                }
            }
            other => {
                report
                    .failures
                    .push(format!("{}: unknown status {:?}", phase, other));
            }
        }
    }
    report
}

/// Check the conventional manifest in a project root.
pub fn check_root(root: &Path) -> ArtifactReport {
    check(root, &manifest_path(root))
}

pub fn to_json(r: &ArtifactReport) -> Json {
    let fails: Vec<Json> = r.failures.iter().map(|s| Json::Str(s.clone())).collect();
    let na: Vec<Json> = r.not_assessed.iter().map(|s| Json::Str(s.clone())).collect();
    Json::Obj(vec![
        ("tool".into(), Json::Str("prodgate".into())),
        ("command".into(), Json::Str("artifacts".into())),
        ("passed".into(), Json::Bool(r.passed())),
        ("checked".into(), Json::Num(r.checked as f64)),
        ("manifest_present".into(), Json::Bool(r.manifest_present)),
        ("failures".into(), Json::Arr(fails)),
        ("not_assessed".into(), Json::Arr(na)),
    ])
}
