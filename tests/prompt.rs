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
    let text = prompt::render(root());
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
    let text = prompt::render(root());
    assert!(text.contains("Replace the '*' origin") || text.contains("Replace bare cors()"));
    assert!(text.contains("parameterized query"));
    assert!(text.contains("secret manager"));
    // Declaration rules must tell the AI to add the control, not just scan.
    assert!(text.contains("declaration control"));
    assert!(text.contains("Never silently omit it"));
}

#[test]
fn json_prompt_has_structured_sections() {
    let sections = prompt::sections(root());
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
