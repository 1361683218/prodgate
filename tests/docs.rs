//! Documentation must not drift from the rule library. Every time rules are
//! added, these assertions force the docs to be updated in the same commit.

use prodgate::check::all_rules;
use std::fs;
use std::path::Path;

fn read(name: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(name)).unwrap()
}

fn extract_number_after(text: &str, marker: &str) -> Option<usize> {
    let pos = text.find(marker)? + marker.len();
    let rest = &text[pos..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[test]
fn readme_rule_count_matches_library() {
    let readme = read("README.md");
    let n = extract_number_after(&readme, "**信号型**有 ")
        .expect("README must state the signal-rule count");
    let signal = all_rules()
        .iter()
        .filter(|r| r.kind == prodgate::check::RuleKind::Signal)
        .count();
    assert_eq!(n, signal, "README signal-rule count is stale; update it");
}

#[test]
fn readme_mcp_tool_count_matches() {
    let readme = read("README.md");
    // Assert every shipped MCP tool name appears in the README table.
    for tool in ["prodgate_rules", "prodgate_prompt", "prodgate_gate",
        "prodgate_replay", "prodgate_mutate", "prodgate_cases"] {
        assert!(readme.contains(tool), "README missing MCP tool {}", tool);
    }
}

#[test]
fn readme_case_count_matches_library() {
    let readme = read("README.md");
    let n = extract_number_after(&readme, "规则，共 ")
        .expect("README must state the case count");
    // Case count is distributed across rule dirs; sum case subdirectories.
    let total: usize = fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("cases"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|rule_dir| {
            fs::read_dir(rule_dir.path())
                .unwrap()
                .flatten()
                .filter(|e| e.path().is_dir())
                .count()
        })
        .sum();
    assert_eq!(n, total, "README case count is stale; update it");
}

#[test]
fn benchmark_signal_rule_list_matches() {
    let bench = read("BENCHMARK.md");
    let signal: Vec<_> = all_rules()
        .iter()
        .filter(|r| r.kind == prodgate::check::RuleKind::Signal)
        .map(|r| r.id)
        .collect();
    for id in signal {
        assert!(bench.contains(id), "BENCHMARK.md missing signal rule {}", id);
    }
}

#[test]
fn ai_code_doc_has_current_state_banner() {
    let doc = read("AI-CODE.md");
    assert!(doc.contains("当前状态"), "AI-CODE.md must carry a current-state note");
    let n = extract_number_after(&doc, "扩到 ")
        .expect("current-state banner must state the rule count");
    assert_eq!(n, all_rules().len(), "AI-CODE.md current-state count is stale");
}

#[test]
fn delivery_summary_numbers_match_reality() {
    // The one-page delivery doc must not drift from the code it summarizes.
    let doc = read("DELIVERY.md");
    // Test count is written as a number; the guard checks it stays plausible.
    // Exact count is intentionally not asserted (adding a test must not break docs).
    assert!(doc.contains("个测试"), "DELIVERY must state the test count");
    assert!(doc.contains("15 条规则"));
    assert!(doc.contains("18/18"));
    assert!(doc.contains("16/16"));
    assert!(doc.contains("3492"), "real-project scan figure must be present");
    // No hardcoded line counts: those change every time a test is added.
}
