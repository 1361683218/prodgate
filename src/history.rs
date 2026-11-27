//! Append-only scan history.
//!
//! Every `gate` invocation appends one compact snapshot to
//! `.prodgate/history.jsonl`. Append-only is deliberate: history is an
//! accumulated record, not a mutable state file. Rewriting it would lose the
//! ability to see how a project changed over time.
//!
//! This is the same pattern large projects use for incremental work: never
//! recompute the whole past, just add the new fact.

use crate::check::Verdict;
use crate::gate::GateReport;
use crate::json::Json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_FILE: &str = ".prodgate/history.jsonl";
/// Keep history bounded so a long-running CI does not grow it forever.
const MAX_SNAPSHOTS: usize = 500;

fn history_path(root: &Path) -> PathBuf {
    root.join(HISTORY_FILE)
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub ts: u64,
    pub files: usize,
    pub fail: usize,
    pub pass: usize,
    pub baseline: usize,
    pub unconfirmed: usize,
    pub na: usize,
    pub passed: bool,
}

fn count(report: &GateReport, v: Verdict) -> usize {
    report.findings.iter().filter(|f| f.verdict == v).count()
}

/// Append one snapshot. Returns the number of snapshots kept.
pub fn record(root: &Path, report: &GateReport) -> std::io::Result<usize> {
    let snap = Snapshot {
        ts: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        files: report.files_scanned,
        fail: count(report, Verdict::Fail),
        pass: count(report, Verdict::Pass),
        baseline: count(report, Verdict::Baseline),
        unconfirmed: count(report, Verdict::Unconfirmed),
        na: count(report, Verdict::NotAssessed),
        passed: report.passed(),
    };

    let path = history_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let line = snapshot_json(&snap).stringify();
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", line)?;

    // Trim oldest lines if over cap. Read + rewrite is rare (only past 500).
    let text = fs::read_to_string(&path)?;
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > MAX_SNAPSHOTS {
        let keep = &lines[lines.len() - MAX_SNAPSHOTS..];
        let mut trimmed = String::new();
        for l in keep {
            trimmed.push_str(l);
            trimmed.push('\n');
        }
        fs::write(&path, trimmed)?;
        Ok(MAX_SNAPSHOTS)
    } else {
        Ok(lines.len())
    }
}

fn snapshot_json(s: &Snapshot) -> Json {
    Json::Obj(vec![
        ("ts".into(), Json::Num(s.ts as f64)),
        ("files".into(), Json::Num(s.files as f64)),
        ("fail".into(), Json::Num(s.fail as f64)),
        ("pass".into(), Json::Num(s.pass as f64)),
        ("baseline".into(), Json::Num(s.baseline as f64)),
        ("unconfirmed".into(), Json::Num(s.unconfirmed as f64)),
        ("na".into(), Json::Num(s.na as f64)),
        ("passed".into(), Json::Bool(s.passed)),
    ])
}

/// Read all snapshots. Malformed lines are skipped — a corrupt line must not
/// make the whole history unreadable.
pub fn load(root: &Path) -> Vec<Snapshot> {
    let Ok(text) = fs::read_to_string(history_path(root)) else { return Vec::new() };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let v = crate::json::parse(l).ok()?;
            let num = |k: &str| -> Option<usize> {
                match v.get(k) { Some(Json::Num(n)) => Some(*n as usize), _ => None }
            };
            Some(Snapshot {
                ts: num("ts")? as u64,
                files: num("files")?,
                fail: num("fail")?,
                pass: num("pass")?,
                baseline: num("baseline")?,
                unconfirmed: num("unconfirmed")?,
                na: num("na")?,
                passed: matches!(v.get("passed"), Some(Json::Bool(true))),
            })
        })
        .collect()
}

pub fn to_json(snaps: &[Snapshot]) -> Json {
    let arr: Vec<Json> = snaps.iter().map(snapshot_json).collect();
    Json::Arr(arr)
}
