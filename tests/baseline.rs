//! Baseline test. A legacy project can adopt the gate with existing issues,
//! but must not add new ones. The baseline is a shrink-only ratchet.

use prodgate::check::Verdict;
use prodgate::gate;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "prodgate-baseline-{}-{}-{}",
        std::process::id(),
        nanos,
        seq
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("prodgate.conf"),
        "surface=web\nroots=src\n",
    )
    .unwrap();
    dir
}

fn write_issue(dir: &Path, name: &str, marker: &str) {
    fs::write(dir.join("src").join(name), format!("// prodgate: {}\n", marker)).unwrap();
}

#[test]
fn baseline_masks_known_issue_but_blocks_new_issue() {
    let dir = scratch();

    // Start with one known issue and freeze it.
    write_issue(&dir, "app.rs", "rls=off");
    let raw = gate::run_gate(&dir);
    assert!(raw.findings.iter().any(|f| f.verdict == Verdict::Fail));

    let recorded = gate::write_baseline(&dir).unwrap();
    assert_eq!(recorded, 1, "exactly one issue should be recorded");

    // With the baseline, the known issue no longer blocks.
    let masked = gate::run_gate(&dir);
    assert!(masked.passed(), "known issue must not block");
    assert!(masked.findings.iter().any(|f| f.verdict == Verdict::Baseline));

    // Add a second, NEW issue. It must block even though baseline exists.
    write_issue(&dir, "ops.ts", "backup=off");
    let with_new = gate::run_gate(&dir);
    assert!(!with_new.passed(), "new issue must block");
    assert!(with_new.findings.iter().any(|f| f.verdict == Verdict::Fail));
    assert!(with_new.findings.iter().any(|f| f.verdict == Verdict::Baseline));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_is_shrink_only_when_issue_disappears() {
    let dir = scratch();

    write_issue(&dir, "app.rs", "rls=off");
    gate::write_baseline(&dir).unwrap();

    // Remove the issue but keep a clean source file: an empty project would
    // now fail with config.no-sources, which is a different condition.
    fs::write(dir.join("src/app.rs"), "fn main() {}\n").unwrap();
    let after = gate::run_gate(&dir);
    assert!(after.passed());
    assert!(!after.findings.iter().any(|f| f.verdict == Verdict::Baseline));

    let recorded_again = gate::write_baseline(&dir).unwrap();
    assert_eq!(recorded_again, 0, "baseline should shrink when issue is fixed");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_write_failure_is_reported_not_swallowed() {
    let dir = scratch();
    write_issue(&dir, "app.rs", "rls=off");

    // Replace the baseline file with a directory. fs::write to it must fail,
    // and write_baseline must surface that failure instead of returning Ok.
    let marker = dir.join(".prodgate/baseline.txt");
    fs::create_dir_all(&marker).unwrap();

    let result = gate::write_baseline(&dir);
    assert!(result.is_err(), "a failed write must return Err, not Ok(1)");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_survives_line_shifts() {
    // A baseline keyed by line number breaks the moment someone adds an
    // import at the top: old issues reappear as new FAILs. Content-addressed
    // keys must keep matching when the offending line itself is unchanged.
    let dir = scratch();
    fs::write(
        dir.join("src/a.js"),
        "const a = eval(req.body.x);\nconst b = eval(req.body.y);\n",
    )
    .unwrap();
    assert_eq!(gate::write_baseline(&dir).unwrap(), 2);
    assert!(gate::run_gate(&dir).passed());

    // Insert two lines at the top; both findings move down two lines.
    fs::write(
        dir.join("src/a.js"),
        "// header\nconst cfg = 1;\nconst a = eval(req.body.x);\nconst b = eval(req.body.y);\n",
    )
    .unwrap();
    let shifted = gate::run_gate(&dir);
    assert!(shifted.passed(), "line shifts must not resurrect baselined issues");
    assert_eq!(
        shifted.findings.iter().filter(|f| f.verdict == Verdict::Baseline).count(),
        2,
        "both issues should still be recognized as known"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_detects_when_offending_code_changes() {
    // The mirror of the line-shift test: changing the actual code must make
    // it a NEW issue, otherwise the baseline would hide real regressions.
    let dir = scratch();
    fs::write(dir.join("src/a.js"), "const a = eval(req.body.x);\n").unwrap();
    gate::write_baseline(&dir).unwrap();
    assert!(gate::run_gate(&dir).passed());

    fs::write(dir.join("src/a.js"), "const a = eval(req.body.NEW);\n").unwrap();
    let changed = gate::run_gate(&dir);
    assert!(!changed.passed(), "changed code must block as a new issue");
    assert!(changed.findings.iter().any(|f| f.verdict == Verdict::Fail));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_file_uses_content_addressed_format() {
    let dir = scratch();
    fs::write(dir.join("src/a.js"), "const a = eval(req.body.x);\n").unwrap();
    gate::write_baseline(&dir).unwrap();
    let text = fs::read_to_string(dir.join(".prodgate/baseline.txt")).unwrap();
    assert!(text.contains("eval.injection|src/a.js|"), "new format must be content-addressed");
    // Old format had a line number after a colon; new one has a hash.
    let entry = text.lines().find(|l| l.starts_with("eval.injection|")).unwrap();
    assert!(!entry.rsplit('|').next().unwrap().parse::<usize>().is_ok(),
        "the trailing component must be a hash, not a line number");
    fs::remove_dir_all(&dir).ok();
}
