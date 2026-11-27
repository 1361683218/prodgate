//! JSON report tests. CI and MCP consumers read `passed` and the finding
//! arrays, so these shapes are a contract — not an implementation detail.

use prodgate::json::{self, Json};
use prodgate::report;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn tmp_project(name: &str) -> std::path::PathBuf {
    let unique = format!(
        "prodgate-report-{}-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let dir = std::env::temp_dir().join(unique);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "surface=web\nroots=src\n").unwrap();
    dir
}

fn passed(doc: &Json) -> bool {
    doc.get("passed")
        .map(|v| matches!(v, Json::Bool(true)))
        .unwrap_or(false)
}

#[test]
fn gate_json_has_stable_top_level_shape() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = report::gate_json(root);
    for key in ["tool", "command", "passed", "files_scanned", "known_issues", "findings"] {
        assert!(doc.get(key).is_some(), "missing key: {}", key);
    }
    assert!(json::parse(&doc.stringify()).is_ok(), "output must be valid JSON");
    assert!(passed(&doc), "prodgate's own gate should pass");
}

#[test]
fn gate_json_reports_finding_with_evidence() {
    let dir = tmp_project("fail");
    fs::write(dir.join("src/db.rs"), "// prodgate: rls=off\n").unwrap();

    let doc = report::gate_json(&dir);
    assert!(!passed(&doc));
    let findings = match doc.get("findings").unwrap() {
        Json::Arr(a) => a,
        _ => panic!("findings must be an array"),
    };
    let rls = findings
        .iter()
        .find(|f| f.get("rule").and_then(Json::as_str) == Some("rls"))
        .expect("rls finding missing");
    assert_eq!(
        rls.get("verdict").and_then(Json::as_str),
        Some("FAIL")
    );
    assert_eq!(
        rls.get("evidence").and_then(Json::as_str),
        Some("src/db.rs:1")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn replay_and_cases_json_report_passed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let replay = report::replay_json(root);
    assert!(passed(&replay));
    assert!(replay.get("results").and_then(|v| match v {
        Json::Arr(a) => Some(a.len()),
        _ => None,
    }) == Some(15));

    let cases = report::cases_json(root);
    assert!(passed(&cases));
    assert_eq!(
        cases.get("source_backed").and_then(|v| match v {
            Json::Num(n) => Some(*n as i64),
            _ => None,
        }),
        Some(9)
    );
}

#[test]
fn all_json_aggregates_every_guard() {
    // A guard that exists but is not in `all` will not run in CI. Assert the
    // aggregate carries every one, so adding a sentinel means updating here.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = report::all_json(root);
    assert!(passed(&doc));
    for guard in ["replay", "mutation", "gate", "cases", "regression", "efficacy"] {
        assert!(doc.get(guard).is_some(), "all_json must aggregate {}", guard);
    }
    // Baseline abuse guard must stay in the aggregate — an AI with shell
    // access can freeze FAILs, so the count has to be visible to CI.
    assert!(doc.get("baseline_count").is_some(), "all_json must report frozen baseline count");
    assert!(doc.get("baseline_limit").is_some(), "all_json must report the baseline cap");
    assert_eq!(doc.get("command").and_then(Json::as_str), Some("all"));

    // Gating guards must pass; efficacy is informational but reported.
    for guard in ["replay", "mutation", "gate", "cases", "regression"] {
        let section = doc.get(guard).unwrap();
        assert!(
            matches!(section.get("passed"), Some(Json::Bool(true))),
            "{} must pass inside all_json", guard
        );
    }
}
