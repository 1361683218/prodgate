//! Baseline test. A legacy project can adopt the gate with existing debt,
//! but must not add new debt. The baseline is a shrink-only ratchet.

use prodgate::check::Verdict;
use prodgate::gate;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("prodgate-baseline-{}-{}", std::process::id(), nanos));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("prodgate.conf"),
        "surface=web\nroots=src\n",
    )
    .unwrap();
    dir
}

fn write_debt(dir: &Path, name: &str, marker: &str) {
    fs::write(dir.join("src").join(name), format!("// prodgate: {}\n", marker)).unwrap();
}

#[test]
fn baseline_masks_known_debt_but_blocks_new_debt() {
    let dir = scratch();

    // Start with one known debt and freeze it.
    write_debt(&dir, "app.rs", "rls=off");
    let raw = gate::run_gate(&dir);
    assert!(raw.findings.iter().any(|f| f.verdict == Verdict::Fail));

    let recorded = gate::write_baseline(&dir);
    assert_eq!(recorded, 1, "exactly one debt should be recorded");

    // With the baseline, the known debt no longer blocks.
    let masked = gate::run_gate(&dir);
    assert!(masked.passed(), "known debt must not block");
    assert!(masked.findings.iter().any(|f| f.verdict == Verdict::Baseline));

    // Add a second, NEW debt. It must block even though baseline exists.
    write_debt(&dir, "ops.ts", "backup=off");
    let with_new = gate::run_gate(&dir);
    assert!(!with_new.passed(), "new debt must block");
    assert!(with_new.findings.iter().any(|f| f.verdict == Verdict::Fail));
    assert!(with_new.findings.iter().any(|f| f.verdict == Verdict::Baseline));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_is_shrink_only_when_debt_disappears() {
    let dir = scratch();

    write_debt(&dir, "app.rs", "rls=off");
    gate::write_baseline(&dir);

    // Remove the debt: gate still passes, and re-baseline shrinks to zero.
    fs::remove_file(dir.join("src/app.rs")).unwrap();
    let after = gate::run_gate(&dir);
    assert!(after.passed());
    assert!(!after.findings.iter().any(|f| f.verdict == Verdict::Baseline));

    let recorded_again = gate::write_baseline(&dir);
    assert_eq!(recorded_again, 0, "baseline should shrink when debt is fixed");

    fs::remove_dir_all(&dir).ok();
}
