//! `history` 命令的 JSON 输出契约。CI/MCP 消费者只读 `passed` 与 `history`
//! 数组，所以结构必须稳定。测试数据由 `history::record` 生成——手写 JSON
//! 会绕过真正的序列化/读取路径，测不到命令实际看到的内容。

use prodgate::json::{self, Json};
use prodgate::{gate, history, report};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "prodgate-history-cli-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("prodgate.conf"), "roots=src\nsurface=web\n").unwrap();
    fs::write(dir.join("src/a.js"), "const a = eval(req.body.x);\n").unwrap();
    dir
}

fn num(v: Option<&Json>) -> Option<i64> {
    match v {
        Some(Json::Num(n)) => Some(*n as i64),
        _ => None,
    }
}

#[test]
fn history_json_without_history_is_empty_but_well_formed() {
    let dir = project();

    let doc = report::history_json(&dir);

    // 输出必须能被项目自己的解析器读回，否则下游拿不到数据。
    assert!(json::parse(&doc.stringify()).is_ok());
    for key in ["tool", "command", "passed", "count", "history"] {
        assert!(doc.get(key).is_some(), "missing key: {}", key);
    }
    assert_eq!(doc.get("tool").and_then(Json::as_str), Some("prodgate"));
    assert_eq!(doc.get("command").and_then(Json::as_str), Some("history"));
    // 查看历史不是门禁：没有快照也不该让 CI 变红。
    assert_eq!(doc.get("passed"), Some(&Json::Bool(true)));
    assert_eq!(num(doc.get("count")), Some(0));
    match doc.get("history") {
        Some(Json::Arr(a)) => assert!(a.is_empty(), "no history means empty array"),
        _ => panic!("history must be an array"),
    }

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn history_json_with_history_matches_recorded_snapshots() {
    let dir = project();
    // 用 record 生成真实历史，而不是手写 .prodgate/history.jsonl。
    history::record(&dir, &gate::run_gate(&dir)).unwrap(); // FAIL: eval
    fs::write(dir.join("src/a.js"), "const a = JSON.parse(req.body.x);\n").unwrap();
    history::record(&dir, &gate::run_gate(&dir)).unwrap(); // PASS: fixed

    let snaps = history::load(&dir);
    assert_eq!(snaps.len(), 2);

    let doc = report::history_json(&dir);

    assert!(json::parse(&doc.stringify()).is_ok());
    assert_eq!(doc.get("command").and_then(Json::as_str), Some("history"));
    assert_eq!(doc.get("passed"), Some(&Json::Bool(true)));
    assert_eq!(num(doc.get("count")), Some(snaps.len() as i64));

    let arr = match doc.get("history") {
        Some(Json::Arr(a)) => a,
        _ => panic!("history must be an array"),
    };
    assert_eq!(arr.len(), snaps.len());

    for (entry, snap) in arr.iter().zip(&snaps) {
        for key in ["ts", "files", "fail", "pass", "baseline", "unconfirmed", "na", "passed"] {
            assert!(entry.get(key).is_some(), "snapshot missing key: {}", key);
        }
        assert_eq!(num(entry.get("ts")), Some(snap.ts as i64));
        assert_eq!(num(entry.get("files")), Some(snap.files as i64));
        assert_eq!(num(entry.get("fail")), Some(snap.fail as i64));
        assert_eq!(num(entry.get("pass")), Some(snap.pass as i64));
        assert_eq!(entry.get("passed"), Some(&Json::Bool(snap.passed)));
    }
    assert!(!snaps[0].passed && snaps[1].passed, "fixture change must be visible");

    // 读历史必须只读：查看命令不能改变被查看的对象。
    assert_eq!(history::load(&dir).len(), snaps.len());

    fs::remove_dir_all(&dir).ok();
}
