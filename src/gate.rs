//! Gate evaluation over a project tree.

use crate::adapter::{Language, SourceFile};
use crate::check::{all_rules, Finding, RuleCtx, Verdict};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const BASELINE_FILE: &str = ".prodgate/baseline.txt";

/// Synthetic rule id for a scan that matched no source files. Kept separate
/// from the real rule set so baseline cannot freeze a misconfiguration.
const NO_SOURCES_RULE: &str = "config.no-sources";

/// Declaration rules look for an explicit opt-out marker. No marker means
/// "nobody said anything", NOT "the control is confirmed present". Report
/// that as UNCONFIRMED rather than a clean PASS.
const DECLARATION_RULES: &[&str] = &[
    "rate.limit",
    "error.tracking",
    "webhook.idempotency",
    "rls",
    "backup",
];

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
        .filter(|f| f.verdict == Verdict::Fail && f.id != NO_SOURCES_RULE)
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
    /// Scan every source file under each root, including nested crates.
    pub recursive: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: vec!["src".into()],
            surface: "cli".into(),
            recursive: false,
        }
    }
}

/// Directories that never contain hand-written source code worth scanning.
const SKIP_DIRS: &[&str] = &[
    "target", "node_modules", ".git", ".venv", "venv", "dist", "build",
    "__pycache__", ".idea", ".vscode", "coverage", "vendor",
];

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
                    "recursive" => cfg.recursive = matches!(v.trim(), "true" | "1" | "yes"),
                    _ => {}
                }
            }
        }
    }
    cfg
}

fn push_if_source(root: &Path, path: &Path, out: &mut Vec<SourceFile>) {
    if !path.is_file() {
        return;
    }
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    if Language::for_path(&rel) == Language::Unknown {
        return;
    }
    if let Ok(src) = fs::read_to_string(path) {
        out.push(SourceFile::new(rel, src));
    }
}

fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<SourceFile>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            walk_dir(root, &path, out);
        } else {
            push_if_source(root, &path, out);
        }
    }
}

fn collect_sources(root: &Path, cfg: &Config) -> Vec<SourceFile> {
    let mut out = Vec::new();
    for r in &cfg.roots {
        let dir = root.join(r);
        if cfg.recursive {
            if dir.is_dir() {
                walk_dir(root, &dir, &mut out);
            }
        } else {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                push_if_source(root, &entry.path(), &mut out);
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
    let sources = collect_sources(root, &cfg);

    // Zero source files is a configuration failure, not a clean result.
    // Without this, a wrong `roots` value makes every rule report PASS and
    // the gate hands out a false green light.
    if sources.is_empty() {
        return GateReport {
            findings: vec![Finding {
                id: NO_SOURCES_RULE,
                verdict: Verdict::Fail,
                evidence: "-".into(),
                note: format!(
                    "no source files found under roots=[{}]; check prodgate.conf (surface={})",
                    cfg.roots.join(", "),
                    cfg.surface
                ),
                suggestion: Some("Fix roots in prodgate.conf to point at the source directory, or set recursive=true for a Cargo workspace."),
            }],
            files_scanned: 0,
        };
    }

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
                suggestion: None,
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
        findings.push(hit.unwrap_or_else(|| {
            let is_declaration = DECLARATION_RULES.contains(&rule.id);
            Finding {
                id: rule.id,
                verdict: if is_declaration {
                    Verdict::Unconfirmed
                } else {
                    Verdict::Pass
                },
                evidence: "-".into(),
                note: if is_declaration {
                    format!(
                        "no opt-out declared in {} source file(s); presence of the control is not confirmed",
                        sources.len()
                    )
                } else {
                    format!("{} source file(s) scanned, no signal", sources.len())
                },
                suggestion: None,
            }
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
        // A misconfiguration must always block, even if someone manually
        // writes its fingerprint into the baseline file.
        if f.id == NO_SOURCES_RULE {
            continue;
        }
        if f.verdict == Verdict::Fail && baseline.contains(&fingerprint(f)) {
            f.verdict = Verdict::Baseline;
            f.note = format!("known issue in baseline: {}", f.note);
        }
    }
    report
}
