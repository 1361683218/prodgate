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
/// Cap by count AND age. Count alone has no time dimension: a daily scan
/// keeps 500 days, a CI scan every second keeps eight minutes. Both are
/// wrong for a trend view. Age-based trimming was the fix large projects
/// apply to caches (craft-libs' scheduled full runs keep caches from going
/// stale) — applied here to history records.
const MAX_SNAPSHOTS: usize = 500;
const MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;

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

    // Trim by BOTH count and age. Read + rewrite is rare (past the cap or
    // after a long gap). Parsed snapshots let us drop stale ones; malformed
    // lines are dropped too — a corrupt line must not pin the file forever.
    let text = fs::read_to_string(&path)?;
    let mut kept: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let v = crate::json::parse(l).ok()?;
            let ts = match v.get("ts") { Some(Json::Num(n)) => *n as u64, _ => return None };
            if snap.ts.saturating_sub(ts) > MAX_AGE_SECS { None } else { Some(l.to_string()) }
        })
        .collect();
    // Always keep the newest record, even if every record is older than the
    // window — history must never become empty just because time passed.
    if kept.is_empty() {
        kept.push(line);
    }
    if kept.len() > MAX_SNAPSHOTS {
        let drain = kept.len() - MAX_SNAPSHOTS;
        kept.drain(..drain);
    }
    let trimmed = kept.join("\n") + "\n";
    fs::write(&path, trimmed)?;
    Ok(kept.len())
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

/// Delete snapshots older than the 30-day window, always keeping the newest
/// one. Returns the number of snapshots removed. Unlike `record`, no new
/// snapshot is appended — pruning is a standalone maintenance operation.
pub fn prune(root: &Path) -> std::io::Result<usize> {
    let path = history_path(root);
    let Ok(text) = fs::read_to_string(&path) else { return Ok(0) };

    // 与 record 同一套容错语义：空行与破损行都不算快照，破损行顺手丢弃。
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Ok(0);
    }
    let parsed: Vec<(&str, Option<u64>)> = lines
        .iter()
        .map(|&l| {
            let ts = crate::json::parse(l).ok().and_then(|v| match v.get("ts") {
                Some(Json::Num(n)) => Some(*n as u64),
                _ => None,
            });
            (l, ts)
        })
        .collect();
    let newest = parsed.iter().filter_map(|(_, ts)| *ts).max();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    let kept: Vec<&str> = parsed
        .iter()
        .copied()
        .filter_map(|(l, ts)| {
            let ts = ts?;
            // 窗口外的一律删除；但最新一条必须留下，历史不能因时间流逝而清空。
            (now.saturating_sub(ts) <= MAX_AGE_SECS || Some(ts) == newest).then_some(l)
        })
        .collect();

    let removed = lines.len() - kept.len();
    if removed > 0 {
        let body = if kept.is_empty() {
            String::new()
        } else {
            kept.join("\n") + "\n"
        };
        fs::write(&path, body)?;
    }
    Ok(removed)
}

pub fn to_json(snaps: &[Snapshot]) -> Json {
    let arr: Vec<Json> = snaps.iter().map(snapshot_json).collect();
    Json::Arr(arr)
}
