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
fn every_case_is_either_citable_or_honestly_synthetic() {
    // Core rules have verifiable sources; AI-benchmark rules are synthetic
    // but must never masquerade as upstream. Both states are first-class.
    let results = cases::validate(root());
    assert!(results.len() >= 16);
    let citable = results.iter().filter(|r| r.provenance.is_citable()).count();
    assert!(citable >= 9, "at least the original rules must stay source-backed");
    for r in &results {
        if r.provenance.is_citable() {
            assert!(r.meta.source_url.starts_with("https://github.com/"));
        } else if r.provenance == Provenance::Synthetic {
            assert_eq!(r.meta.source, "synthetic");
            assert!(r.meta.source_url.is_empty());
        } else {
            panic!("case {} has neither a citable source nor a synthetic marker", r.meta.id);
        }
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

#[test]
fn all_seven_rules_have_a_case() {
    use prodgate::check::all_rules;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let results = prodgate::cases::validate(root);
    let covered: Vec<&str> = results.iter().map(|r| r.meta.rule.as_str()).collect();
    for rule in all_rules() {
        assert!(
            covered.contains(&rule.id),
            "rule {} has no case in the library",
            rule.id
        );
    }
    assert!(results.len() >= 7);
    assert!(results.iter().all(|r| r.valid()));
}

#[test]
fn anchored_cases_are_citable_but_not_upstream() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let results = prodgate::cases::validate(root);
    let anchored: Vec<_> = results
        .iter()
        .filter(|r| r.provenance == prodgate::cases::Provenance::Anchored)
        .collect();
    assert!(anchored.len() >= 4, "declaration rules and SQL case should be anchored");
    for r in &anchored {
        assert!(r.provenance.is_citable());
        assert!(
            r.meta.source_url.starts_with("https://github.com/"),
            "anchored case must carry a verifiable URL"
        );
        assert_eq!(r.meta.expression, "synthetic");
    }
    let upstream: Vec<_> = results
        .iter()
        .filter(|r| r.provenance == prodgate::cases::Provenance::Upstream)
        .collect();
    assert!(upstream.len() >= 3, "cors, rls and secrets should be upstream");
    assert_eq!(prodgate::cases::trusted_count(&results), results.iter().filter(|r| r.provenance.is_citable() && r.valid()).count());
}
