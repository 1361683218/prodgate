//! HTML report must be self-contained (opens offline) and safe (source code
//! from scanned projects is embedded — escaping is a security requirement).

use prodgate::html_report::{escape_html, render};
use prodgate::json::Json;

fn finding(rule: &str, verdict: &str, evidence: &str, note: &str, fix: Option<&str>) -> Json {
    Json::Obj(vec![
        ("rule".into(), Json::Str(rule.into())),
        ("verdict".into(), Json::Str(verdict.into())),
        ("evidence".into(), Json::Str(evidence.into())),
        ("note".into(), Json::Str(note.into())),
        ("suggestion".into(), fix.map(|s| Json::Str(s.into())).unwrap_or(Json::Null)),
    ])
}

fn doc(findings: Vec<Json>) -> Json {
    Json::Obj(vec![
        ("tool".into(), Json::Str("prodgate".into())),
        ("command".into(), Json::Str("gate".into())),
        ("passed".into(), Json::Bool(false)),
        ("files_scanned".into(), Json::Num(3.0)),
        ("known_issues".into(), Json::Num(1.0)),
        ("findings".into(), Json::Arr(findings)),
    ])
}

#[test]
fn html_escape_covers_all_dangerous_chars() {
    assert_eq!(escape_html("<script>&\"'"), "&lt;script&gt;&amp;&quot;&#39;");
    assert_eq!(escape_html("plain text"), "plain text");
}

#[test]
fn report_is_self_contained() {
    let d = doc(vec![finding("eval.injection", "FAIL", "src/a.js:1", "bad", Some("fix it"))]);
    let html = render(&d);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<style>"));
    assert!(html.contains("<script>"));
    // No external resources — must open offline.
    assert!(!html.contains("<link"));
    assert!(!html.contains("src=\"http"));
    assert!(!html.contains("@import"));
}

#[test]
fn malicious_source_cannot_break_out_of_html() {
    // A scanned file's note contains a script tag; it must appear escaped,
    // never as live markup.
    let d = doc(vec![finding(
        "test.rule", "FAIL", "a.js:1",
        "<img src=x onerror=alert(1)> & 'quoted'",
        Some("<script>alert('xss')</script>"),
    )]);
    let html = render(&d);
    assert!(!html.contains("<img src=x onerror"));
    assert!(!html.contains("<script>alert"));
    assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(html.contains("&lt;script&gt;alert"));
}

#[test]
fn report_shows_stats_and_fix_guidance() {
    let d = doc(vec![
        finding("eval.injection", "FAIL", "a.js:1", "bad eval", Some("use JSON.parse")),
        finding("secrets.hardcoded", "BASELINE", "b.js:2", "known key", None),
    ]);
    let html = render(&d);
    assert!(html.contains("Gate FAIL"));
    assert!(html.contains("eval.injection"));
    assert!(html.contains("use JSON.parse"));
    assert!(html.contains("class=\"fix\""));
    assert!(html.contains("BASELINE"));
    assert!(html.contains("data-k=\"fail\""));
    assert!(html.contains("data-k=\"baseline\""));
}

#[test]
fn empty_findings_do_not_render_filter_bar() {
    let d = doc(vec![]);
    let html = render(&d);
    assert!(html.contains("Gate FAIL"));
    assert!(!html.contains("class=\"filter\""));
}
