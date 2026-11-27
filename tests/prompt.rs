//! The feed-forward prompt must stay in lockstep with the gate: every rule
//! appears, with a real bad/good pair and actionable guidance.

use prodgate::check::all_rules;
use prodgate::json::Json;
use prodgate::prompt;
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn prompt_covers_every_rule() {
    let text = prompt::render(root(), "web");
    for rule in all_rules() {
        assert!(text.contains("## ") && text.contains(rule.id),
            "prompt missing rule {}", rule.id);
    }
    assert!(text.contains("Bad (do not write this)"));
    assert!(text.contains("Good:"));
    assert!(text.contains("Real-world basis:"));
}

#[test]
fn prompt_has_actionable_guidance_for_signal_rules() {
    let text = prompt::render(root(), "web");
    assert!(text.contains("Replace the '*' origin") || text.contains("Replace bare cors()"));
    assert!(text.contains("parameterized query"));
    assert!(text.contains("secret manager"));
    // Declaration rules must tell the AI to add the control, not just scan.
    assert!(text.contains("declaration control"));
    assert!(text.contains("Never silently omit it"));
}

#[test]
fn json_prompt_has_structured_sections() {
    let sections = prompt::sections(root(), "web");
    assert_eq!(sections.len(), all_rules().len());
    for (section, rule) in sections.iter().zip(all_rules().iter()) {
        assert_eq!(section.get("id").and_then(Json::as_str), Some(rule.id));
        assert!(section.get("bad").is_some());
        assert!(section.get("good").is_some());
        assert!(section.get("suggestion").is_some());
    }
    // Signal rules carry fix text; declaration rules do not.
    let sql = sections.iter().find(|s| s.get("id").and_then(Json::as_str) == Some("sql.injection")).unwrap();
    assert!(matches!(sql.get("suggestion"), Some(Json::Str(_))));
}

#[test]
fn cli_prompt_marks_web_rules_not_applicable() {
    let cli = prompt::render(root(), "cli");
    for web_rule in ["cors.wildcard", "rate.limit", "webhook.idempotency", "rls"] {
        assert!(
            cli.contains(&format!("{} — NOT APPLICABLE", web_rule)),
            "{} should be marked N/A on cli surface",
            web_rule
        );
    }
    // Signal rules still apply to cli.
    assert!(!cli.contains("secrets.hardcoded — NOT APPLICABLE"));
    assert!(!cli.contains("sql.injection — NOT APPLICABLE"));
}

#[test]
fn web_prompt_has_no_not_applicable_markers() {
    let web = prompt::render(root(), "web");
    assert!(!web.contains("NOT APPLICABLE"));
    assert!(web.contains("Bad (do not write this)"));
}

#[test]
fn json_prompt_exposes_applicability() {
    let cli = prompt::sections(root(), "cli");
    let web = prompt::sections(root(), "web");
    for (c, w) in cli.iter().zip(web.iter()) {
        assert_eq!(c.get("id").and_then(Json::as_str), w.get("id").and_then(Json::as_str));
    }
    let cors_cli = cli.iter().find(|s| s.get("id").and_then(Json::as_str) == Some("cors.wildcard")).unwrap();
    let cors_web = web.iter().find(|s| s.get("id").and_then(Json::as_str) == Some("cors.wildcard")).unwrap();
    assert_eq!(cors_cli.get("applicable"), Some(&Json::Bool(false)));
    assert_eq!(cors_web.get("applicable"), Some(&Json::Bool(true)));
}

#[test]
fn surface_applicability_is_shared_with_gate() {
    // The gate marks the same rules NOT-ASSESSED that prompt marks N/A.
    use prodgate::check::Verdict;
    let dir = std::env::temp_dir().join(format!("prodgate-prompt-surface-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=cli\n").unwrap();
    std::fs::write(dir.join("src/app.rs"), "fn main() {}\n").unwrap();
    let report = prodgate::gate::run_gate(&dir);
    for id in ["cors.wildcard", "rate.limit", "webhook.idempotency", "rls"] {
        let f = report.findings.iter().find(|f| f.id == id).unwrap();
        assert_eq!(f.verdict, Verdict::NotAssessed, "{} should be NOT-ASSESSED on cli", id);
    }
    std::fs::remove_dir_all(&dir).ok();
}
