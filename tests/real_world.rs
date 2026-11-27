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

#[test]
fn wildcard_cors_as_function_argument() {
    // Real Express pattern injected into expressjs/express:
    //   res.setHeader('Access-Control-Allow-Origin', '*')
    // The key and '*' are function arguments separated by a comma, not by
    // `:` or `=`. The original rule only recognized assignment syntax.
    let src = "app.use(function(req, res, next) {\n  res.setHeader('Access-Control-Allow-Origin', '*');\n  next();\n});";
    assert_eq!(
        verdict_web("cors.wildcard", "lib/express.js", src),
        Verdict::Fail,
        "setHeader(name, '*') is a real wildcard CORS configuration"
    );
}

#[test]
fn comparison_with_wildcard_is_not_a_configuration() {
    // Must stay PASS: checking a value is not setting it.
    let src = "if (options.origin === '*') { return; }";
    assert_eq!(
        verdict_web("cors.wildcard", "lib/index.js", src),
        Verdict::Pass
    );
}

#[test]
fn wildcard_cors_in_python_and_go_real_styles() {
    // Injected into real flask/app.py and gin/gin.go during the web benchmark.
    let py = "response.headers['Access-Control-Allow-Origin'] = '*'";
    assert_eq!(
        verdict_web("cors.wildcard", "src/flask/app.py", py),
        Verdict::Fail
    );
    let go = "c.Header(\"Access-Control-Allow-Origin\", \"*\")";
    assert_eq!(verdict_web("cors.wildcard", "gin.go", go), Verdict::Fail);
}

#[test]
fn hardcoded_env_fallback_is_detected() {
    // Verbatim from securevault@3db9bd2f, server/config/config.js before the fix.
    // `JWT_SECRET: process.env.JWT_SECRET || 'literal'` uses the literal when
    // the env var is missing — a real leak, not a test fixture.
    let src = "  JWT_SECRET: process.env.JWT_SECRET || 'securevault_default_secret_change_me',";
    assert_eq!(
        verdict("secrets.hardcoded", "server/config/config.js", src),
        Verdict::Fail
    );
}

#[test]
fn env_only_object_property_is_not_a_leak() {
    // The fixed line from the same commit: no fallback literal.
    let src = "  JWT_SECRET: process.env.JWT_SECRET,";
    assert_eq!(
        verdict("secrets.hardcoded", "server/config/config.js", src),
        Verdict::Pass
    );
}

#[test]
fn custom_key_name_fallback_is_detected() {
    // Second hunk of securevault@3db9bd2f. CHAT_KEY is not in the exact
    // whitelist; the `_key` suffix pattern must catch it.
    let src = "const CHAT_KEY = process.env.CHAT_AES_KEY || 'SecureVaultChatKey2024!';";
    assert_eq!(
        verdict("secrets.hardcoded", "server/controllers/chatController.js", src),
        Verdict::Fail
    );
}

#[test]
fn non_credential_words_are_not_flagged_as_keys() {
    // `MONKEY` ends in "key" without an underscore separator, and public
    // keys are not secrets. Both must stay clear to avoid false positives.
    for src in [
        "const MONKEY = \"Abcd1234Efgh5678\";",
        "const public_key = \"Abcd1234Efgh5678\";",
        "const test_key = \"Abcd1234Efgh5678\";",
    ] {
        assert_eq!(
            verdict("secrets.hardcoded", "src/config.rs", src),
            Verdict::Pass,
            "false positive on: {}",
            src
        );
    }
}

#[test]
fn ai_generated_code_patterns_are_detected() {
    // Written by an AI assistant in a natural "quick web service" scenario,
    // without mentioning prodgate. All three were initially missed.
    let cors = "app.use(cors());";
    assert_eq!(
        verdict_web("cors.wildcard", "src/server.js", cors),
        Verdict::Fail,
        "bare cors() defaults to wildcard origin"
    );
    let placeholder = "WEBHOOK_SECRET = \"whsec_example_key_replace_this\"";
    assert_eq!(
        verdict_web("secrets.hardcoded", "src/main.py", placeholder),
        Verdict::Fail,
        "AI placeholder secret is often deployed unchanged"
    );
}

#[test]
fn typed_config_key_reference_is_not_a_secret() {
    // Flask framework code: ConfigAttribute("SECRET_KEY") is a key NAME,
    // not an assigned credential value. Must not be flagged.
    let src = "secret_key = ConfigAttribute[str | bytes | None](\"SECRET_KEY\")";
    assert_eq!(
        verdict("secrets.hardcoded", "src/flask/sansio/app.py", src),
        Verdict::Pass
    );
}

#[test]
fn explicit_cors_config_is_not_a_wildcard() {
    for src in [
        "app.use(cors({ origin: 'https://example.com' }));",
        "const cors = require('cors');",
        "app.use(cors(opts));",
    ] {
        assert_eq!(
            verdict_web("cors.wildcard", "src/server.js", src),
            Verdict::Pass,
            "false positive on: {}",
            src
        );
    }
}

#[test]
fn ai_db_connection_uri_password_is_detected() {
    // Written naturally by AI in a "database helpers" scenario. The variable
    // is `engine`, not a credential name — key-name rules cannot see this.
    let src = "engine = create_engine(\"postgresql://admin:password123@localhost/prod\")";
    assert_eq!(
        verdict("secrets.hardcoded", "src/users.py", src),
        Verdict::Fail
    );
}

#[test]
fn database_uri_placeholders_are_not_flagged() {
    for src in [
        "engine = create_engine(\"postgresql://localhost/prod\")",
        "url = \"postgresql://admin:password@localhost/db\"",
        "url = \"postgresql://admin:${DB_PASS}@localhost/db\"",
        "url = \"https://user:pass@example.com\"",
    ] {
        assert_eq!(
            verdict("secrets.hardcoded", "src/db.py", src),
            Verdict::Pass,
            "false positive on: {}",
            src
        );
    }
}
