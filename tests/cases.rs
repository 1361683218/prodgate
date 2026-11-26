//! Case library as a gate. Cases are contributions, not code: a bad case
//! must be rejected, and provenance must never be silently invented.

use prodgate::cases::{self, Provenance};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn all_shipped_cases_are_valid_and_replayable() {
    let results = cases::validate(root());
    assert!(!results.is_empty(), "case library must not be empty");
    for r in &results {
        assert!(r.valid(), "case {} is invalid: {:?}", r.meta.id, r.errors);
        assert!(r.replay_ok(), "case {} cannot be replayed", r.meta.id);
    }
}

#[test]
fn upstream_cases_have_citable_sources() {
    let results = cases::validate(root());
    let trusted = cases::trusted_count(&results);
    assert!(trusted >= 2, "expected at least 2 upstream-sourced cases");
    for r in results.iter().filter(|r| r.provenance == Provenance::Upstream) {
        assert!(r.meta.source.contains('@'));
        assert!(r.meta.source_url.starts_with("https://"));
    }
}

#[test]
fn synthetic_case_is_labeled_not_passed_off_as_upstream() {
    let results = cases::validate(root());
    let synthetic: Vec<_> = results
        .iter()
        .filter(|r| r.provenance == Provenance::Synthetic)
        .collect();
    assert!(!synthetic.is_empty(), "at least one case should be honestly synthetic");
    for r in synthetic {
        assert_eq!(r.meta.source, "synthetic");
        assert!(r.meta.source_url.is_empty());
    }
}

#[test]
fn case_without_source_is_rejected() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir: PathBuf = std::env::temp_dir().join(format!("prodgate-case-{}-{}", std::process::id(), nanos));
    let case = dir.join("cases/rls/bad001");
    fs::create_dir_all(case.join("good")).unwrap();
    fs::create_dir_all(case.join("bad")).unwrap();
    fs::write(
        case.join("case.toml"),
        "id = \"rls/bad001\"\nrule = \"rls\"\ntitle = \"no source\"\nsource = \"\"\ncontributor = \"test\"\n",
    )
    .unwrap();
    fs::write(case.join("good/a.js"), "// prodgate: rls=enabled\n").unwrap();
    fs::write(case.join("bad/a.js"), "// prodgate: rls=off\n").unwrap();

    let results = cases::validate(&dir);
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.provenance, Provenance::Unverified);
    assert!(!r.valid(), "source-less case must not pass validation");
    assert!(r.errors.iter().any(|e| e.contains("source")));

    fs::remove_dir_all(&dir).ok();
}
