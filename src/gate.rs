//! Gate evaluation over a project tree.

use crate::adapter::{Language, SourceFile};
use crate::check::{all_rules, Finding, RuleCtx, Verdict};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const BASELINE_FILE: &str = ".prodgate/baseline.txt";

/// Stable identity of a finding. Deliberately excludes the note text: an issue
/// stays the same issue when its description is reworded.
fn fingerprint(f: &Finding) -> String {
    format!("{}|{}", f.id, f.evidence)
}

fn baseline_path(root: &Path) -> std::path::PathBuf {
    root.join(BASELINE_FILE)
}

pub fn load_baseline(root: &Path) -> HashSet<String> {
    fs::read_to_string(baseline_path(root))
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Freeze the current FAIL set as known issues. Returns the number recorded.
/// Errors are surfaced: a silently failed write would make a caller believe
/// issues were frozen when nothing was saved.
pub fn write_baseline(root: &Path) -> std::io::Result<usize> {
    let report = run_gate_raw(root);
    let issues: Vec<String> = report
        .findings
        .iter()
        .filter(|f| f.verdict == Verdict::Fail)
        .map(fingerprint)
        .collect();

    let mut text = String::from("# prodgate baseline — known issues, shrink only\n");
    for d in &issues {
        text.push_str(d);
        text.push('\n');
    }
    let path = baseline_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(issues.len())
}

#[derive(Debug, Clone)]
pub struct Config {
    pub roots: Vec<String>,
    pub surface: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: vec!["src".into()],
            surface: "cli".into(),
        }
    }
}

pub fn load_config(root: &Path) -> Config {
    let mut cfg = Config::default();
    let path = root.join("prodgate.conf");
    if let Ok(text) = fs::read_to_string(path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "roots" => {
                        cfg.roots = v
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    "surface" => cfg.surface = v.trim().to_string(),
                    _ => {}
                }
            }
        }
    }
    cfg
}

fn collect_sources(root: &Path, roots: &[String]) -> Vec<SourceFile> {
    let mut out = Vec::new();
    for r in roots {
        let dir = root.join(r);
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if Language::for_path(&rel) == Language::Unknown {
                continue;
            }
            if let Ok(src) = fs::read_to_string(&path) {
                out.push(SourceFile::new(rel, src));
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[derive(Debug)]
pub struct GateReport {
    pub findings: Vec<Finding>,
    pub files_scanned: usize,
}

impl GateReport {
    pub fn passed(&self) -> bool {
        !self.findings.iter().any(|f| f.verdict == Verdict::Fail)
    }
}

/// Scan without baseline comparison. Used by `write_baseline` so it captures
/// the raw issues rather than the already-masked set.
fn run_gate_raw(root: &Path) -> GateReport {
    let cfg = load_config(root);
    let sources = collect_sources(root, &cfg.roots);
    let ctx = RuleCtx {
        surface: &cfg.surface,
    };
    let mut findings = Vec::new();

    for rule in all_rules() {
        // A rule that cannot be evaluated on this surface is NOT-ASSESSED,
        // never silently skipped.
        let applicable = match rule.id {
            "cors.wildcard" | "rate.limit" | "webhook.idempotency" | "rls" => {
                cfg.surface == "web"
            }
            _ => true,
        };
        if !applicable {
            findings.push(Finding {
                id: rule.id,
                verdict: Verdict::NotAssessed,
                evidence: "-".into(),
                note: format!(
                    "rule targets web surface; declared surface is '{}'",
                    cfg.surface
                ),
            });
            continue;
        }

        let mut hit = None;
        for file in &sources {
            if let Some(f) = (rule.run)(file, &ctx) {
                hit = Some(f);
                break;
            }
        }
        findings.push(hit.unwrap_or(Finding {
            id: rule.id,
            verdict: Verdict::Pass,
            evidence: "-".into(),
            note: format!("{} source file(s) scanned, no signal", sources.len()),
        }));
    }

    GateReport {
        findings,
        files_scanned: sources.len(),
    }
}

/// Scan with baseline applied. A FAIL whose fingerprint is recorded becomes
/// BASELINE (reported, non-blocking). Anything new stays FAIL and blocks.
pub fn run_gate(root: &Path) -> GateReport {
    let baseline = load_baseline(root);
    let mut report = run_gate_raw(root);
    for f in &mut report.findings {
        if f.verdict == Verdict::Fail && baseline.contains(&fingerprint(f)) {
            f.verdict = Verdict::Baseline;
            f.note = format!("known issue in baseline: {}", f.note);
        }
    }
    report
}
