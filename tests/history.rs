//! Scan history is append-only and bounded. It is the incremental record of
//! a project's gate state over time — never rewritten, just added to.

use prodgate::{gate, history};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-history-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=web\n").unwrap();
    fs::write(dir.join("src/a.js"), "const a = eval(req.body.x);\n").unwrap();
    dir
}

#[test]
fn record_appends_and_load_roundtrips() {
    let dir = project();
    let report = gate::run_gate(&dir);
    assert_eq!(history::record(&dir, &report).unwrap(), 1);
    assert_eq!(history::record(&dir, &report).unwrap(), 2);

    let snaps = history::load(&dir);
    assert_eq!(snaps.len(), 2);
    assert_eq!(snaps[0].fail, 1);
    assert!(!snaps[0].passed);
    assert_eq!(snaps[0].files, 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn history_reflects_fix() {
    let dir = project();
    history::record(&dir, &gate::run_gate(&dir)).unwrap();
    fs::write(dir.join("src/a.js"), "const a = JSON.parse(req.body.x);\n").unwrap();
    history::record(&dir, &gate::run_gate(&dir)).unwrap();

    let snaps = history::load(&dir);
    assert_eq!(snaps.len(), 2);
    assert_eq!(snaps[0].fail, 1);
    assert_eq!(snaps[1].fail, 0);
    assert!(!snaps[0].passed && snaps[1].passed);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn corrupt_line_does_not_break_history() {
    let dir = project();
    history::record(&dir, &gate::run_gate(&dir)).unwrap();
    use std::io::Write;
    let mut f = fs::OpenOptions::new().append(true)
        .open(dir.join(".prodgate/history.jsonl")).unwrap();
    writeln!(f, "{{broken json").unwrap();
    drop(f);
    let snaps = history::load(&dir);
    assert_eq!(snaps.len(), 1, "malformed line must be skipped, not crash");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_history_is_empty() {
    let dir = project();
    assert!(history::load(&dir).is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn old_snapshots_are_trimmed_by_age() {
    // Count alone has no time dimension: a 40-day-old snapshot must go even
    // when there are only three records. Age cap is 30 days.
    let dir = project();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let old = now - 40 * 24 * 60 * 60;
    let recent = now - 2 * 24 * 60 * 60;
    use std::io::Write;
    fs::create_dir_all(dir.join(".prodgate")).unwrap();
    {
        let mut f = fs::OpenOptions::new().create(true).append(true)
            .open(dir.join(".prodgate/history.jsonl")).unwrap();
        for ts in [old, old, recent] {
            writeln!(f, "{{\"ts\":{},\"files\":1,\"fail\":0,\"pass\":1,\"baseline\":0,\"unconfirmed\":0,\"na\":0,\"passed\":true}}", ts).unwrap();
        }
    }
    // Appending one fresh record triggers trimming.
    history::record(&dir, &gate::run_gate(&dir)).unwrap();
    let snaps = history::load(&dir);
    assert_eq!(snaps.len(), 2, "two old snapshots must be dropped, recent + fresh kept");
    assert!(snaps.iter().all(|s| s.ts >= recent), "no stale snapshot should survive");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn all_stale_history_keeps_newest_record() {
    // History must never become empty just because every record aged out.
    let dir = project();
    let old = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - 90 * 24 * 60 * 60;
    fs::create_dir_all(dir.join(".prodgate")).unwrap();
    fs::write(
        dir.join(".prodgate/history.jsonl"),
        format!("{{\"ts\":{},\"files\":1,\"fail\":0,\"pass\":1,\"baseline\":0,\"unconfirmed\":0,\"na\":0,\"passed\":true}}\n", old),
    ).unwrap();
    history::record(&dir, &gate::run_gate(&dir)).unwrap();
    assert!(!history::load(&dir).is_empty(), "newest record must survive age trimming");
    fs::remove_dir_all(&dir).ok();
}
