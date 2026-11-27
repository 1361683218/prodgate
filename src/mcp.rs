//! MCP stdio transport over the existing kernel.
//!
//! Design rule: this module is a *thin* adapter. It contains no detection
//! logic of its own; every tool calls the same functions the CLI uses. If the
//! CLI and MCP ever disagree, that is a bug in this file, not in a rule.
//!
//! Deliberately NOT exposed: `baseline`. Letting an agent freeze known issues would
//! turn a gate into a rubber stamp.

use crate::json::Json;
use crate::{cases, check, fixture, gate, mutation};
use std::path::Path;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_VERSION: &str = "0.1.0";

/// Handle one JSON-RPC message. Returns None for notifications (which must
/// not produce a response). Pure and side-effect free apart from tool calls.
pub fn handle(input: &str) -> Option<String> {
    let req = match crate::json::parse(input) {
        Ok(v) => v,
        Err(e) => {
            return Some(
                error(Json::Null, -32700, &format!("parse error: {}", e))
                    .stringify(),
            )
        }
    };

    let id = req.get("id").cloned().unwrap_or(Json::Null);
    let is_notification = req.get("id").is_none();
    let method = req.get("method").and_then(Json::as_str).unwrap_or("");

    let result = match method {
        "initialize" => Ok(initialize_result(&req)),
        "ping" => Ok(Json::Obj(vec![])),
        "tools/list" => Ok(tools_list()),
        "tools/call" => return handle_tool_call(id, req.get("params")),
        _ if is_notification => return None,
        _ => Err((-32601, format!("method not found: {}", method))),
    };

    match result {
        Ok(_) if is_notification => None,
        Ok(value) => Some(success(id, value).stringify()),
        Err((code, msg)) => Some(error(id, code, &msg).stringify()),
    }
}

fn success(id: Json, result: Json) -> Json {
    Json::Obj(vec![
        ("jsonrpc".into(), Json::Str("2.0".into())),
        ("id".into(), id),
        ("result".into(), result),
    ])
}

fn error(id: Json, code: i64, message: &str) -> Json {
    Json::Obj(vec![
        ("jsonrpc".into(), Json::Str("2.0".into())),
        ("id".into(), id),
        (
            "error".into(),
            Json::Obj(vec![
                ("code".into(), Json::Num(code as f64)),
                ("message".into(), Json::Str(message.into())),
            ]),
        ),
    ])
}

fn initialize_result(req: &Json) -> Json {
    let requested = req
        .get("params")
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Json::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    Json::Obj(vec![
        ("protocolVersion".into(), Json::Str(requested.into())),
        (
            "capabilities".into(),
            Json::Obj(vec![("tools".into(), Json::Obj(vec![]))]),
        ),
        (
            "serverInfo".into(),
            Json::Obj(vec![
                ("name".into(), Json::Str("prodgate".into())),
                ("version".into(), Json::Str(SERVER_VERSION.into())),
            ]),
        ),
    ])
}

fn schema(properties: Vec<(&str, Json)>, required: &[&str]) -> Json {
    Json::Obj(vec![
        ("type".into(), Json::Str("object".into())),
        (
            "properties".into(),
            Json::Obj(properties.into_iter().map(|(k, v)| (k.into(), v)).collect()),
        ),
        (
            "required".into(),
            Json::Arr(required.iter().map(|s| Json::Str((*s).into())).collect()),
        ),
    ])
}

fn root_property() -> Json {
    Json::Obj(vec![
        ("type".into(), Json::Str("string".into())),
        (
            "description".into(),
            Json::Str("Absolute path to the project root to inspect.".into()),
        ),
    ])
}

fn tool(name: &str, description: &str, input_schema: Json) -> Json {
    Json::Obj(vec![
        ("name".into(), Json::Str(name.into())),
        ("description".into(), Json::Str(description.into())),
        ("inputSchema".into(), input_schema),
    ])
}

fn tools_list() -> Json {
    Json::Obj(vec![(
        "tools".into(),
        Json::Arr(vec![
            tool(
                "prodgate_rules",
                "List all production-readiness rules with their descriptions. \
                 Call this BEFORE writing code so you know which controls to add \
                 in the first place (feed-forward, not just review).",
                schema(vec![], &[]),
            ),
            tool(
                "prodgate_prompt",
                "Get the full AI coding guide: every rule with a real bad/good code example \
                 from the case library and an actionable fix. Call this BEFORE writing code \
                 so you avoid the mistakes instead of fixing them afterward.",
                schema(vec![("root", root_property())], &[]),
            ),
            tool(
                "prodgate_efficacy",
                "Prompt information-sufficiency experiment: scans archived AI-written code, \
                 checks that every rule it violated is fully covered by the prompt, and that \
                 the good form the prompt teaches passes the gate. Reports coverage and \
                 counterfactual results. Does NOT measure AI behavior change.",
                schema(vec![], &[]),
            ),
            tool(
                "prodgate_regression",
                "Check curated real-world code snapshots for false positives. Each snapshot is a \
                 real shape that looks like a rule trigger but is safe. Guards precision when \
                 rules change.",
                schema(vec![], &[]),
            ),
            tool(
                "prodgate_gate",
                "Scan a project root against the production-readiness rules. \
                 Returns PASS/FAIL/NOT-ASSESSED for every rule with evidence. \
                 Call after generating or editing code.",
                schema(vec![("root", root_property())], &["root"]),
            ),
            tool(
                "prodgate_replay",
                "Verify that every rule still distinguishes its good/bad fixture \
                 pair. Use after editing a rule: a green gate means nothing if the \
                 rule itself is broken.",
                schema(vec![("root", root_property())], &["root"]),
            ),
            tool(
                "prodgate_cases",
                "Validate the contribution case library: provenance, and whether \
                 each real-world case still separates good from bad.",
                schema(vec![("root", root_property())], &["root"]),
            ),
            tool(
                "prodgate_mutate",
                "Inject known-bad samples into temporary projects that mirror real \
                 frameworks (express/flask/gin) and verify every rule still detects \
                 them end-to-end. Use after editing a rule: replay only calls the \
                 rule function, mutation runs the full scan pipeline.",
                schema(vec![], &[]),
            ),
        ]),
    )])
}

struct ToolResult {
    text: String,
    is_error: bool,
}

impl ToolResult {
    fn ok(text: String) -> Self {
        Self {
            text,
            is_error: false,
        }
    }
    fn err(text: String) -> Self {
        Self {
            text,
            is_error: true,
        }
    }
}

fn handle_tool_call(id: Json, params: Option<&Json>) -> Option<String> {
    let params = match params {
        Some(p) => p,
        None => return Some(error(id, -32602, "missing params").stringify()),
    };
    let name = params
        .get("name")
        .and_then(Json::as_str)
        .unwrap_or("");
    let args = params.get("arguments");

    let result = match name {
        "prodgate_rules" => call_rules(),
        "prodgate_prompt" => call_prompt(args),
        "prodgate_efficacy" => call_efficacy(),
        "prodgate_regression" => call_regression(),
        "prodgate_gate" => with_root(args, call_gate),
        "prodgate_replay" => with_root(args, call_replay),
        "prodgate_cases" => with_root(args, call_cases),
        "prodgate_mutate" => call_mutate(),
        _ => ToolResult::err(format!("unknown tool: {}", name)),
    };

    let content = Json::Arr(vec![Json::Obj(vec![
        ("type".into(), Json::Str("text".into())),
        ("text".into(), Json::Str(result.text)),
    ])]);
    Some(
        success(
            id,
            Json::Obj(vec![
                ("content".into(), content),
                ("isError".into(), Json::Bool(result.is_error)),
            ]),
        )
        .stringify(),
    )
}

/// Validate the root argument before touching the kernel. Without this, a
/// typo'd path scans zero files and the gate reports PASS — the most dangerous
/// possible failure mode for an agent-driven workflow.
fn with_root(args: Option<&Json>, f: fn(&Path) -> ToolResult) -> ToolResult {
    let raw = args
        .and_then(|a| a.get("root"))
        .and_then(Json::as_str)
        .unwrap_or("");
    if raw.trim().is_empty() {
        return ToolResult::err("argument 'root' is required".into());
    }
    let path = Path::new(raw);
    if !path.is_dir() {
        return ToolResult::err(format!(
            "root is not an existing directory: {} (refusing to report PASS on an empty scan)",
            raw
        ));
    }
    f(path)
}

fn call_rules() -> ToolResult {
    let rules = check::all_rules();
    let mut text = format!("{} production-readiness rules:\n\n", rules.len());
    for rule in &rules {
        text.push_str(&format!("- {}: {}\n", rule.id, rule.description));
    }
    text.push_str(
        "\nRules of type cors/rate/webhook/rls apply only when prodgate.conf declares surface=web; \
         elsewhere they report NOT-ASSESSED. Declaration rules look for explicit `prodgate: key=off` \
         opt-outs in comments — a string literal is not a declaration.",
    );
    ToolResult::ok(text)
}

fn call_prompt(args: Option<&Json>) -> ToolResult {
    // Content comes from prodgate's shipped case library; the surface filter
    // comes from the project being worked on (optional `root` argument).
    let cases_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let surface = args
        .and_then(|a| a.get("root"))
        .and_then(Json::as_str)
        .filter(|r| !r.trim().is_empty())
        .map(|r| gate::load_config(Path::new(r)).surface)
        .unwrap_or_else(|| "cli".to_string());
    ToolResult::ok(crate::prompt::render(cases_root, &surface))
}

fn call_efficacy() -> ToolResult {
    let r = crate::efficacy::run(Path::new(env!("CARGO_MANIFEST_DIR")));
    let mut text = format!(
        "Prompt efficacy (information sufficiency only, not AI behavior):\n\
         samples={} with_findings={} baseline_fails={}\n\
         rules_hit={} prompt_covered={}/{} counterfactual_ok={}/{}\n",
        r.samples, r.samples_with_findings, r.baseline_findings,
        r.rules_hit.len(), r.covered, r.rules_hit.len(),
        r.counterfactual_ok, r.rules_hit.len()
    );
    for g in &r.gaps { text.push_str(&format!("GAP: {}\n", g)); }
    text.push_str(&format!("result: {}\n", if r.passed { "PASS" } else { "FAIL" }));
    ToolResult::ok(text)
}

fn call_regression() -> ToolResult {
    let r = crate::regression::run();
    let mut text = format!(
        "False-positive regression:\nsnapshots={} files={} false_positives={}\n",
        r.snapshots, r.files_scanned, r.false_positives.len()
    );
    for fp in &r.false_positives { text.push_str(&format!("FP: {}\n", fp)); }
    text.push_str(&format!("result: {}\n", if r.passed { "PASS" } else { "FAIL" }));
    ToolResult::ok(text)
}

fn call_gate(root: &Path) -> ToolResult {
    let report = gate::run_gate(root);
    let mut text = String::from("RULE                 VERDICT       NOTE\n");
    for f in &report.findings {
        text.push_str(&format!(
            "{:<20} {:<14} {}\n",
            f.id,
            f.verdict.as_str(),
            f.note
        ));
        if f.verdict != check::Verdict::Pass && f.evidence != "-" {
            text.push_str(&format!("    at {}\n", f.evidence));
        }
        if let Some(fix) = f.suggestion {
            text.push_str(&format!("    fix: {}\n", fix));
        }
    }
    let baselined = report
        .findings
        .iter()
        .filter(|f| f.verdict == check::Verdict::Baseline)
        .count();
    text.push_str(&format!(
        "\nscanned {} source file(s), {} known issue(s)\ngate: {}\n",
        report.files_scanned,
        baselined,
        if report.passed() { "PASS" } else { "FAIL" }
    ));
    if report.passed() {
        ToolResult::ok(text)
    } else {
        ToolResult::err(text)
    }
}

fn call_replay(root: &Path) -> ToolResult {
    let results = fixture::replay(root);
    let mut text = String::from("RULE                 GOOD          BAD           STATUS\n");
    let mut ok = true;
    for r in &results {
        text.push_str(&format!(
            "{:<20} {:<14} {:<14} {}\n",
            r.rule,
            r.good.as_str(),
            r.bad.as_str(),
            if r.ok() { "OK" } else { "BROKEN" }
        ));
        ok &= r.ok();
    }
    text.push_str(&format!(
        "\nreplay: {}\n",
        if ok { "PASS" } else { "FAIL" }
    ));
    if ok {
        ToolResult::ok(text)
    } else {
        ToolResult::err(text)
    }
}

fn call_cases(root: &Path) -> ToolResult {
    let results = cases::validate(root);
    let mut text = String::from("CASE                       PROVENANCE   GOOD     BAD      STATUS\n");
    let mut ok = true;
    for r in &results {
        text.push_str(&format!(
            "{:<26} {:<12} {:<8} {:<8} {}\n",
            r.meta.id,
            r.provenance.as_str(),
            r.good.as_str(),
            r.bad.as_str(),
            if r.valid() { "OK" } else { "INVALID" }
        ));
        for e in &r.errors {
            text.push_str(&format!("    error: {}\n", e));
        }
        ok &= r.valid();
    }
    text.push_str(&format!(
        "\n{} case(s), {} source-backed, {} invalid\ncases: {}\n",
        results.len(),
        cases::trusted_count(&results),
        results.iter().filter(|r| !r.valid()).count(),
        if ok { "PASS" } else { "FAIL" }
    ));
    if ok {
        ToolResult::ok(text)
    } else {
        ToolResult::err(text)
    }
}

fn call_mutate() -> ToolResult {
    let results = mutation::run_mutations();
    let mut text = format!(
        "{:<38} {:<20} {:<8} STATUS\n",
        "SAMPLE", "RULE", "DETECTED"
    );
    for r in &results {
        text.push_str(&format!(
            "{:<38} {:<20} {:<8} {}\n",
            r.name,
            r.rule,
            if r.detected { "yes" } else { "no" },
            if r.ok() { "OK" } else { "MISS" }
        ));
        if !r.ok() {
            text.push_str(&format!("    note: {}\n", r.note));
        }
    }
    let detected = results.iter().filter(|r| r.ok()).count();
    let ok = mutation::passed(&results);
    text.push_str(&format!(
        "\nmutation: {}/{} detected — {}\n",
        detected,
        results.len(),
        if ok { "PASS" } else { "FAIL" }
    ));
    if ok {
        ToolResult::ok(text)
    } else {
        ToolResult::err(text)
    }
}
