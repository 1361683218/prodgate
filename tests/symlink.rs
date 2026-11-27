//! Recursive walking must not follow symlinks or rescan aliased directories.
//! A symlink to an ancestor caused 2 real files to be scanned 64 times.

use prodgate::gate;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-symlink-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/sub")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=cli\nrecursive=true\n").unwrap();
    fs::write(dir.join("src/a.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.join("src/sub/b.rs"), "fn other() {}\n").unwrap();
    dir
}

#[test]
fn symlink_to_ancestor_does_not_cause_rescan() {
    let dir = project();
    #[cfg(unix)]
    std::os::unix::fs::symlink("..", dir.join("src/sub/loop")).unwrap();

    let report = gate::run_gate(&dir);
    assert_eq!(
        report.files_scanned, 2,
        "only the two real source files should be scanned, not symlink aliases"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn symlink_to_file_is_skipped() {
    let dir = project();
    #[cfg(unix)]
    std::os::unix::fs::symlink("a.rs", dir.join("src/link.rs")).unwrap();

    let report = gate::run_gate(&dir);
    assert_eq!(report.files_scanned, 2, "symlinked file must not be scanned twice");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn deep_recursion_still_works() {
    let dir = project();
    fs::create_dir_all(dir.join("src/deep/a/b/c")).unwrap();
    fs::write(dir.join("src/deep/a/b/c/deep.rs"), "fn deep() {}\n").unwrap();
    let report = gate::run_gate(&dir);
    assert_eq!(report.files_scanned, 3, "genuinely nested files must all be found");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn paths_with_spaces_are_scanned() {
    let dir = project();
    fs::write(dir.join("src/my file.rs"), "fn spaced() {}\n").unwrap();
    let report = gate::run_gate(&dir);
    assert_eq!(report.files_scanned, 3, "spaces in filenames must not break scanning");
    fs::remove_dir_all(&dir).ok();
}
