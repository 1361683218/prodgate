//! Every FAIL from a signal rule must carry an actionable fix. A finding
//! that only says "bad" forces the AI agent to guess what good looks like.

use prodgate::adapter::SourceFile;
use prodgate::check::{all_rules, RuleCtx, Verdict};
use prodgate::json::Json;

fn verdict_with(rule_id: &str, path: &str, src: &str) -> (Verdict, Option<&'static str>) {
    let rule = all_rules().into_iter().find(|r| r.id == rule_id).unwrap();
    let f = SourceFile::new(path.into(), src.into());
    match (rule.run)(&f, &RuleCtx { surface: "web" }) {
        Some(finding) => (finding.verdict, finding.suggestion),
        None => (Verdict::Pass, None),
    }
}

#[test]
fn signal_rule_findings_carry_a_fix() {
    let cases = [
        ("cors.wildcard", "app.use(cors());"),
        ("cors.wildcard", "res.setHeader('Access-Control-Allow-Origin', '*');"),
        ("secrets.hardcoded", "const api_key = \"AKIAabcdefghijklmnop\";"),
        ("secrets.hardcoded", "url = \"postgresql://admin:password123@localhost/db\""),
        ("secrets.hardcoded", "WEBHOOK_SECRET = \"whsec_example_key_replace_this\""),
        ("sql.injection", "conn.execute(f\"SELECT * FROM users WHERE id = {uid}\")"),
    ];
    for (rule, src) in cases {
        let (verdict, suggestion) = verdict_with(rule, "src/app.js", src);
        assert_eq!(verdict, Verdict::Fail, "expected FAIL for: {}", src);
        assert!(suggestion.is_some() && !suggestion.unwrap().is_empty(),
            "{} finding must carry a fix: {}", rule, src);
    }
}

#[test]
fn no_signal_findings_have_no_fix() {
    // PASS / UNCONFIRMED are not actionable; only FAIL gets fix guidance.
    let (verdict, suggestion) = verdict_with("sql.injection", "src/app.js", "console.log('ok')");
    assert_eq!(verdict, Verdict::Pass);
    assert!(suggestion.is_none());
}

#[test]
fn json_report_exposes_suggestion_field() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = prodgate::report::gate_json(root);
    let findings = match doc.get("findings") {
        Some(Json::Arr(a)) => a,
        _ => panic!("findings missing"),
    };
    for f in findings {
        assert!(f.get("suggestion").is_some(),
            "every finding must serialize a suggestion field (null is fine)");
    }
}
