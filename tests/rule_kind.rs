//! RuleKind is the single source of truth for how a rule behaves when it sees
//! no signal. These tests ensure the metadata cannot drift from reality: a
//! newly added rule with a wrong `kind` must fail here, not silently become a
//! false green in production.

use prodgate::adapter::SourceFile;
use prodgate::check::{all_rules, RuleCtx, RuleKind, Verdict};
use prodgate::gate;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn empty_project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-kind-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=web\n").unwrap();
    // One benign source file so no config.no-sources finding appears.
    fs::write(dir.join("src/app.rs"), "fn main() {}\n").unwrap();
    dir
}

#[test]
fn kind_matches_no_signal_behavior() {
    let dir = empty_project();
    let report = gate::run_gate(&dir);
    for rule in all_rules() {
        let finding = report.findings.iter().find(|f| f.id == rule.id).unwrap();
        match rule.kind {
            RuleKind::Declaration => assert_eq!(
                finding.verdict, Verdict::Unconfirmed,
                "rule {} is marked Declaration but reports {:?}",
                rule.id, finding.verdict
            ),
            RuleKind::Signal => assert_eq!(
                finding.verdict, Verdict::Pass,
                "rule {} is marked Signal but reports {:?}",
                rule.id, finding.verdict
            ),
        }
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn every_kind_is_used() {
    // A typo that creates a third kind, or marks everything Signal, would
    // defeat the whole point of the metadata.
    let rules = all_rules();
    assert!(rules.iter().any(|r| r.kind == RuleKind::Signal));
    assert!(rules.iter().any(|r| r.kind == RuleKind::Declaration));
}

#[test]
fn declaration_rules_only_read_comments() {
    // A declaration rule must ignore an opt-out marker buried in a string
    // literal. This is the behavioral reason they are a separate kind.
    use prodgate::check;
    let decl = all_rules().into_iter()
        .find(|r| r.kind == RuleKind::Declaration)
        .unwrap();
    let src = "let s = \"prodgate: rate_limit=off\";\n";
    let file = SourceFile::new("src/app.rs".into(), src.into());
    assert_eq!(
        (decl.run)(&file, &RuleCtx { surface: "web" }).map(|f| f.verdict),
        None,
        "{} must not treat a string literal as a declaration",
        decl.id
    );
    let _ = check::Verdict::Pass; // keep import meaningful
}
