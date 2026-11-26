//! MCP transport tests. These lock down the protocol contract the CLI does
//! not cover: notifications must be silent, bad paths must never yield PASS,
//! and a failing gate must surface isError so the agent knows to act.

use prodgate::mcp;
use std::fs;
use std::path::Path;

fn call(body: &str) -> serde_json_like::Rpc {
    let out = mcp::handle(body).expect("expected a response");
    serde_json_like::Rpc::parse(&out)
}

// A tiny test-only JSON reader so we don't need serde in production code.
mod serde_json_like {
    use prodgate::json::{self, Json};

    pub struct Rpc {
        pub id: i64,
        pub result: Option<Json>,
        pub error: Option<Json>,
    }

    impl Rpc {
        pub fn parse(text: &str) -> Self {
            let d = json::parse(text).expect("valid JSON response");
            let id = d.get("id").and_then(Json::as_str).map_or_else(
                || {
                    d.get("id")
                        .and_then(|v| match v {
                            Json::Num(n) => Some(*n as i64),
                            _ => None,
                        })
                        .unwrap_or(-1)
                },
                |_| -1,
            );
            Self {
                id,
                result: d.get("result").cloned(),
                error: d.get("error").cloned(),
            }
        }
        pub fn tool_text(&self) -> String {
            let content = self
                .result
                .as_ref()
                .and_then(|r| r.get("content"))
                .and_then(|c| match c {
                    Json::Arr(a) => a.first(),
                    _ => None,
                })
                .and_then(|t| t.get("text"))
                .and_then(Json::as_str)
                .unwrap_or("");
            content.into()
        }
        pub fn is_error(&self) -> bool {
            self.result
                .as_ref()
                .and_then(|r| r.get("isError"))
                .map(|v| matches!(v, Json::Bool(true)))
                .unwrap_or(false)
        }
    }
}

#[test]
fn initialize_returns_server_info() {
    let r = call(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
    );
    assert!(r.error.is_none());
    assert_eq!(r.id, 1, "response must echo the request id");
    let name = r
        .result
        .as_ref()
        .and_then(|x| x.get("serverInfo"))
        .and_then(|x| x.get("name"))
        .and_then(prodgate::json::Json::as_str);
    assert_eq!(name, Some("prodgate"));
}

#[test]
fn notifications_produce_no_response() {
    let out = mcp::handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    assert!(out.is_none(), "notifications must not get a response");
}

#[test]
fn tools_list_exposes_four_read_only_tools() {
    let r = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
    let text = r.result.as_ref().unwrap().stringify();
    for name in [
        "prodgate_rules",
        "prodgate_gate",
        "prodgate_replay",
        "prodgate_cases",
    ] {
        assert!(text.contains(name), "missing tool {}", name);
    }
    assert!(!text.contains("baseline"), "baseline must not be exposed to agents");
}

#[test]
fn nonexistent_root_is_rejected_instead_of_reporting_pass() {
    let r = call(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"prodgate_gate","arguments":{"root":"/no/such/dir/for/prodgate"}}}"#,
    );
    assert!(r.is_error());
    assert!(r.tool_text().contains("not an existing directory"));
    assert!(!r.tool_text().contains("gate: PASS"));
}

#[test]
fn failing_gate_sets_is_error() {
    let unique = format!(
        "prodgate-mcp-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let tmp = std::env::temp_dir().join(unique);
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("src")).unwrap();
    fs::write(tmp.join("prodgate.conf"), "surface=web\nroots=src\n").unwrap();
    fs::write(tmp.join("src/db.rs"), "// prodgate: rls=off\n").unwrap();

    let body = format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"prodgate_gate","arguments":{{"root":"{}"}}}}}}"#,
        tmp.display()
    );
    let r = call(&body);
    assert!(r.is_error(), "a FAIL gate must set isError=true");
    assert!(r.tool_text().contains("gate: FAIL"));
    assert!(r.tool_text().contains("rls"));
    assert!(r.tool_text().contains("at src/db.rs:1"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn replay_on_real_repo_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"prodgate_replay","arguments":{{"root":"{}"}}}}}}"#,
        root.display()
    );
    let r = call(&body);
    assert!(!r.is_error());
    assert!(r.tool_text().contains("replay: PASS"));
}

#[test]
fn malformed_json_returns_parse_error() {
    let out = mcp::handle("{not json").expect("parse error must still be a response");
    assert!(out.contains("-32700"));
}
