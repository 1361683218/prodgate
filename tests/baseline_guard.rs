//! Red-team guard: an AI (or anyone) with shell access can run
//! `prodgate baseline` to freeze FAILs and make the gate green without
//! improving the code. A small baseline is legitimate gradual adoption;
//! a large one is a bypass and must be loud.

use prodgate::{gate, report};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn project(n: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-baseline-guard-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=web\n").unwrap();
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!("const v{} = eval(req.body.x{});\n", i, i));
    }
    fs::write(dir.join("src/a.js"), src).unwrap();
    dir
}

#[test]
fn small_baseline_is_allowed_and_counted() {
    let dir = project(3);
    assert_eq!(gate::write_baseline(&dir).unwrap(), 3);
    // gate alone goes green (the intended gradual-adoption behavior)
    assert!(gate::run_gate(&dir).passed());
    let doc = report::all_json(&dir);
    assert_eq!(
        doc.get("baseline_count").and_then(|v| match v {
            prodgate::json::Json::Num(n) => Some(*n as usize),
            _ => None,
        }),
        Some(3),
        "all must surface how many issues are frozen"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn oversized_baseline_fails_all() {
    // 25 frozen issues, cap is 20 — the gate must refuse to call this a pass.
    let dir = project(25);
    assert_eq!(gate::write_baseline(&dir).unwrap(), 25);
    assert!(gate::run_gate(&dir).passed(), "gate itself is fooled (that is the attack)");
    let doc = report::all_json(&dir);
    assert_eq!(doc.get("passed"), Some(&prodgate::json::Json::Bool(false)),
        "all must reject an oversized baseline even though gate says PASS");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_has_audit_header() {
    let dir = project(2);
    gate::write_baseline(&dir).unwrap();
    let text = fs::read_to_string(dir.join(".prodgate/baseline.txt")).unwrap();
    assert!(text.contains("recorded_at_unix="), "must record when frozen");
    assert!(text.contains("count=2"), "must record how many frozen");
    assert!(text.contains("20 issue cap"), "must state the policy");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn cap_constant_is_reported() {
    let dir = project(1);
    let doc = report::all_json(&dir);
    assert_eq!(
        doc.get("baseline_limit").and_then(|v| match v {
            prodgate::json::Json::Num(n) => Some(*n as usize),
            _ => None,
        }),
        Some(gate::MAX_BASELINE),
        "reported limit must match the enforced constant"
    );
    fs::remove_dir_all(&dir).ok();
}
