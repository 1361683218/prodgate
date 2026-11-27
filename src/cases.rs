//! Contribution-ready case library. A case is a directory with case.toml
//! metadata plus good/ and bad/ code samples. Contributors add cases, never
//! rule code. This module validates provenance and replays every case.
//!
//! TOML is parsed with a tiny reader — no external crates, keeps offline build.

use crate::adapter::SourceFile;
use crate::check::{all_rules, RuleCtx, Verdict};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CaseMeta {
    pub id: String,
    pub rule: String,
    pub title: String,
    pub source: String,
    pub source_url: String,
    pub contributor: String,
    /// Whether the good/bad sample text is taken from the upstream commit or
    /// written by us. `real` / `synthetic` / empty (treated as `real` for
    /// backward compatibility with older cases).
    pub expression: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// Real commit, real sample text.
    Upstream,
    /// Real, citable commit anchors WHY the control matters, but the good/bad
    /// sample uses prodgate's expression layer (e.g. an opt-out marker) rather
    /// than verbatim upstream text. Verifiable source, synthetic sample.
    Anchored,
    /// No verifiable source — reported, never passed off as real.
    Synthetic,
    /// Missing or malformed source — rejected from the trusted set.
    Unverified,
}

impl Provenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::Upstream => "upstream",
            Provenance::Anchored => "anchored",
            Provenance::Synthetic => "synthetic",
            Provenance::Unverified => "UNVERIFIED",
        }
    }
    /// A source-backed case can be cited; synthetic cannot.
    pub fn is_citable(self) -> bool {
        matches!(self, Provenance::Upstream | Provenance::Anchored)
    }
}

#[derive(Debug)]
pub struct CaseResult {
    pub meta: CaseMeta,
    pub provenance: Provenance,
    pub good: Verdict,
    pub bad: Verdict,
    pub errors: Vec<String>,
}

impl CaseResult {
    pub fn replay_ok(&self) -> bool {
        self.good == Verdict::Pass && self.bad == Verdict::Fail
    }
    pub fn valid(&self) -> bool {
        self.replay_ok() && self.errors.is_empty()
    }
}

fn parse_toml(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(text) = fs::read_to_string(path) else { return out };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"');
            out.insert(k.trim().to_string(), v.to_string());
        }
    }
    out
}

fn provenance_of(meta: &CaseMeta, errors: &mut Vec<String>) -> Provenance {
    if meta.source.is_empty() {
        errors.push("missing source field".into());
        return Provenance::Unverified;
    }

    // Synthetic: no verifiable source, and it must not pretend to have one.
    if meta.source == "synthetic" {
        if !meta.source_url.is_empty() {
            errors.push("synthetic case must not carry a source_url".into());
            return Provenance::Unverified;
        }
        return Provenance::Synthetic;
    }

    // Real source: must look like owner/repo@sha and carry a matching https URL.
    let shape_ok = meta.source.contains('@')
        && meta.source.contains('/')
        && meta.source_url.starts_with("https://")
        && meta.source_url.contains(meta.source.split('@').next().unwrap_or(""));
    if !shape_ok {
        errors.push("source must be owner/repo@sha with matching https source_url".into());
        return Provenance::Unverified;
    }

    // A real source with a synthetic sample is anchored, not upstream: the
    // commit proves the risk, but the sample text is ours. Claiming upstream
    // here would misrepresent where the good/bad code came from.
    match meta.expression.as_str() {
        "synthetic" => Provenance::Anchored,
        "" | "real" => Provenance::Upstream,
        other => {
            errors.push(format!("unknown expression value '{}'", other));
            Provenance::Unverified
        }
    }
}

fn load_sample(dir: &Path) -> Option<SourceFile> {
    let entry = fs::read_dir(dir).ok()?.flatten().next()?;
    let path = entry.path();
    let name = path.file_name()?.to_string_lossy().to_string();
    Some(SourceFile::new(name, fs::read_to_string(&path).ok()?))
}

pub fn validate(root: &Path) -> Vec<CaseResult> {
    let base = root.join("cases");
    let rules: HashSet<&str> = all_rules().iter().map(|r| r.id).collect();
    let ctx = RuleCtx { surface: "web" };
    let mut out = Vec::new();

    let Ok(rule_dirs) = fs::read_dir(&base) else { return out };
    let mut paths: Vec<PathBuf> = rule_dirs.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    paths.sort();

    for rule_dir in paths {
        let Ok(case_dirs) = fs::read_dir(&rule_dir) else { continue };
        let mut cases: Vec<PathBuf> = case_dirs.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        cases.sort();

        for case_dir in cases {
            let mut errors = Vec::new();
            let fields = parse_toml(&case_dir.join("case.toml"));
            let get = |k: &str| fields.get(k).cloned().unwrap_or_default();
            let meta = CaseMeta {
                id: get("id"),
                rule: get("rule"),
                title: get("title"),
                source: get("source"),
                source_url: get("source_url"),
                contributor: get("contributor"),
                expression: get("expression"),
            };

            for (field, value) in [
                ("id", &meta.id),
                ("rule", &meta.rule),
                ("title", &meta.title),
                ("contributor", &meta.contributor),
            ] {
                if value.is_empty() {
                    errors.push(format!("missing {} field", field));
                }
            }
            if !rules.contains(meta.rule.as_str()) {
                errors.push(format!("unknown rule '{}'", meta.rule));
            }

            let provenance = provenance_of(&meta, &mut errors);

            let good = load_sample(&case_dir.join("good"));
            let bad = load_sample(&case_dir.join("bad"));
            if good.is_none() {
                errors.push("missing good sample".into());
            }
            if bad.is_none() {
                errors.push("missing bad sample".into());
            }

            let rule_match = all_rules().into_iter().find(|r| r.id == meta.rule);
            let verdict_of = |sample: Option<SourceFile>| match (sample, &rule_match) {
                (Some(file), Some(rule)) => (rule.run)(&file, &ctx)
                    .map(|f| f.verdict)
                    .unwrap_or(Verdict::Pass),
                (Some(_), None) => Verdict::NotAssessed,
                (None, _) => Verdict::NotAssessed,
            };

            out.push(CaseResult {
                meta,
                provenance,
                good: verdict_of(good),
                bad: verdict_of(bad),
                errors,
            });
        }
    }
    out
}

/// Trusted cases are upstream-sourced and replayable. Synthetic ones are
/// allowed only when explicitly marked, and unverified ones are rejected.
pub fn trusted_count(results: &[CaseResult]) -> usize {
    results
        .iter()
        .filter(|r| r.valid() && r.provenance.is_citable())
        .count()
}
