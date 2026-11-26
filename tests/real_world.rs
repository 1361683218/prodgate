//! Regressions found by scanning real projects, not synthesized fixtures.
//! Each test here corresponds to a false positive observed on an actual
//! codebase (craft-anything, 829 source files). These are the cases that
//! toy fixtures never covered.

use prodgate::adapter::{Language, SourceFile};
use prodgate::check::{all_rules, RuleCtx, Verdict};

fn rule(id: &str) -> prodgate::check::Rule {
    all_rules().into_iter().find(|r| r.id == id).unwrap()
}

fn verdict(id: &str, path: &str, source: &str) -> Verdict {
    let file = SourceFile::new(path.into(), source.into());
    (rule(id).run)(&file, &RuleCtx { surface: "cli" })
        .map(|f| f.verdict)
        .unwrap_or(Verdict::Pass)
}

const CTX_WEB: RuleCtx<'static> = RuleCtx { surface: "web" };

fn verdict_web(id: &str, path: &str, source: &str) -> Verdict {
    let file = SourceFile::new(path.into(), source.into());
    (rule(id).run)(&file, &CTX_WEB)
        .map(|f| f.verdict)
        .unwrap_or(Verdict::Pass)
}

#[test]
fn credential_word_in_string_value_is_not_a_key() {
    // Found in craft-anything crates/ai/src/agent.rs:2302. The right-hand
    // string mentions "tokens"; the assignment key is `window_only`.
    let src = r#"let window_only = "This model's maximum context length is 2048 tokens.";"#;
    assert_eq!(
        verdict("secrets.hardcoded", "crates/ai/src/agent.rs", src),
        Verdict::Pass
    );
}

#[test]
fn prose_secret_value_is_not_a_leaked_key() {
    // Found in craft-anything crates/analysis/src/standin/tests.rs:264.
    let src = r#"let secret = "my secret/value";"#;
    assert_eq!(
        verdict("secrets.hardcoded", "crates/analysis/tests.rs", src),
        Verdict::Pass
    );
}

#[test]
fn plain_word_value_is_not_a_leaked_key() {
    // Found in craft-anything regression_har_metadata.rs:6.
    let src = r#"let secret = "privatevalue";"#;
    assert_eq!(
        verdict("secrets.hardcoded", "crates/analysis/tests.rs", src),
        Verdict::Pass
    );
}

#[test]
fn ipv6_literal_in_test_is_not_a_leaked_key() {
    // Found in craft-anything regression_har_normalized_metadata.rs:12.
    let src = r#"let secret = "[2001:DB8::BEEF]";"#;
    assert_eq!(
        verdict("secrets.hardcoded", "crates/analysis/tests.rs", src),
        Verdict::Pass
    );
}

#[test]
fn real_credential_in_production_code_still_detected() {
    let src = r#"let api_key = "FAKE_sample_51H8xQ2eZvKYlo2Cabcdefgh";"#;
    assert_eq!(
        verdict("secrets.hardcoded", "src/config.rs", src),
        Verdict::Fail
    );
}

#[test]
fn recursive_scan_detects_source_in_nested_crates() {
    // Cargo workspace layout: no top-level src/, sources under crates/*/src.
    // This is what made craft-anything report 0 files scanned.
    assert_eq!(Language::for_path("crates/ai/src/main.rs"), Language::Rust);
    assert_eq!(Language::for_path("crates/ui/src/app.ts"), Language::TypeScript);
}

#[test]
fn wildcard_cors_across_two_lines_from_real_diff() {
    // Shape from expressjs/cors PR #39 — key and value on separate lines.
    let src = "key: 'Access-Control-Allow-Origin'\nvalue: '*'";
    assert_eq!(
        verdict_web("cors.wildcard", "lib/index.js", src),
        Verdict::Fail
    );
}
