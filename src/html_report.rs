//! Self-contained HTML report generation.
//!
//! Turns a gate report into a single HTML file with inline CSS/JS — no CDN,
//! no external assets, opens offline. This is a real product capability: the
//! terminal report is for machines, this is for humans you need to convince.

use crate::json::Json;

/// Escape text for safe embedding in HTML. Source code can contain `<script>`
/// and finding notes come from scanned files — unescaped output would be an
/// injection vector in the report itself.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn jstr(v: Option<&Json>) -> String {
    v.and_then(Json::as_str).unwrap_or("").to_string()
}

fn verdict_class(v: &str) -> &'static str {
    match v {
        "FAIL" => "fail",
        "BASELINE" => "baseline",
        "PASS" => "pass",
        "UNCONFIRMED" => "unconfirmed",
        "NOT-ASSESSED" => "na",
        _ => "na",
    }
}

const CSS: &str = r#"
:root{--bg:#f7f8fa;--card:#fff;--ink:#1f2328;--muted:#656d76;--line:#d0d7de;
--fail:#cf222e;--pass:#1a7f37;--warn:#9a6700;--baseline:#8250df;--accent:#0969da}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
.wrap{max-width:1080px;margin:0 auto;padding:32px 20px 64px}
h1{font-size:26px;margin:0 0 4px}
.sub{color:var(--muted);margin:0 0 24px}
.stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-bottom:28px}
.stat{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:16px}
.stat .num{font-size:28px;font-weight:650;line-height:1.1}
.stat .lbl{color:var(--muted);font-size:12px;margin-top:6px}
.stat.fail .num{color:var(--fail)} .stat.pass .num{color:var(--pass)}
.bar{height:8px;border-radius:4px;background:#eaeef2;overflow:hidden;display:flex;margin-bottom:28px}
.bar span{height:100%}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;margin-bottom:12px;overflow:hidden}
.card h2{font-size:14px;margin:0;padding:14px 16px;border-bottom:1px solid var(--line);display:flex;align-items:center;gap:10px}
.tag{font-size:11px;font-weight:600;padding:2px 8px;border-radius:20px;text-transform:uppercase;letter-spacing:.03em}
.tag.fail{background:#ffebe9;color:var(--fail)} .tag.pass{background:#dafbe1;color:var(--pass)}
.tag.unconfirmed{background:#fff8c5;color:var(--warn)} .tag.baseline{background:#fbefff;color:var(--baseline)}
.tag.na{background:#eaeef2;color:var(--muted)}
.body{padding:14px 16px}
.body .note{color:var(--ink)}
.body .evidence{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;color:var(--muted);margin-top:6px}
.fix{background:#f0f7ff;border-left:3px solid var(--accent);padding:10px 12px;border-radius:0 6px 6px 0;margin-top:10px;font-size:13px}
.fix b{color:var(--accent)}
.filter{display:flex;gap:8px;margin-bottom:16px;flex-wrap:wrap}
.filter button{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:6px 12px;cursor:pointer;font:inherit;color:var(--muted)}
.filter button.on{background:var(--accent);border-color:var(--accent);color:#fff}
.hidden{display:none}
.trend{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:16px;margin-bottom:28px}
.trend h2{font-size:14px;margin:0 0 12px;font-weight:500}
.trend .hint{color:var(--muted);font-size:12px;margin-top:8px}
.foot{color:var(--muted);font-size:12px;margin-top:32px;border-top:1px solid var(--line);padding-top:16px}
"#;

const JS: &str = r#"
function filter(kind){
  document.querySelectorAll('.filter button').forEach(b=>b.classList.toggle('on',b.dataset.k===kind));
  document.querySelectorAll('.card').forEach(c=>{
    c.classList.toggle('hidden', kind!=='all' && c.dataset.k!==kind);
  });
}
"#;

fn render_finding(f: &Json) -> String {
    let rule = jstr(f.get("rule"));
    let verdict = jstr(f.get("verdict"));
    let evidence = jstr(f.get("evidence"));
    let note = jstr(f.get("note"));
    let suggestion = f.get("suggestion").and_then(Json::as_str).unwrap_or("");
    let cls = verdict_class(&verdict);

    let mut s = String::new();
    s.push_str(&format!(
        "<div class=\"card\" data-k=\"{}\"><h2><span class=\"tag {}\">{}</span>{}</h2><div class=\"body\">",
        escape_html(cls), cls, escape_html(&verdict), escape_html(&rule)
    ));
    s.push_str(&format!("<div class=\"note\">{}</div>", escape_html(&note)));
    if evidence != "-" && !evidence.is_empty() {
        s.push_str(&format!("<div class=\"evidence\">{}</div>", escape_html(&evidence)));
    }
    if !suggestion.is_empty() {
        s.push_str(&format!(
            "<div class=\"fix\"><b>fix:</b> {}</div>",
            escape_html(suggestion)
        ));
    }
    s.push_str("</div></div>");
    s
}

/// Render an inline SVG trend line from scan history. No external library.
/// Returns an empty string when there is nothing to plot.
fn render_trend(snaps: &[crate::history::Snapshot]) -> String {
    if snaps.len() < 2 { return String::new(); }
    let w = 1000.0_f64;
    let h = 140.0_f64;
    let pad = 8.0_f64;
    let n = snaps.len();
    let step = if n > 1 { (w - 2.0 * pad) / (n - 1) as f64 } else { 0.0 };
    let y = |fail: usize| -> f64 {
        // Top = more fails is worse; scale to max fail in the series.
        let max_fail = snaps.iter().map(|s| s.fail).max().unwrap_or(1).max(1);
        pad + (1.0 - fail as f64 / max_fail as f64) * (h - 2.0 * pad)
    };
    let mut pts = Vec::new();
    for (i, s) in snaps.iter().enumerate() {
        pts.push(format!("{:.1},{:.1}", pad + i as f64 * step, y(s.fail)));
    }
    let polyline = pts.join(" ");
    let first = snaps.first().unwrap();
    let last = snaps.last().unwrap();
    let mut out = String::new();
    out.push_str("<div class=\"trend\"><h2>FAIL count over scans</h2>");
    out.push_str(&format!(
        "<svg viewBox=\"0 0 {:.0} {:.0}\" width=\"100%\" height=\"160\" role=\"img\">",
        w, h
    ));
    out.push_str(&format!(
        "<path d=\"M0,{:.1} H{:.0}\" stroke=\"var(--color-border-tertiary)\" stroke-width=\"0.5\" fill=\"none\"/>",
        h - pad, w
    ));
    out.push_str(&format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"var(--fail-color,#cf222e)\" stroke-width=\"2\"/>",
        polyline
    ));
    for (i, s) in snaps.iter().enumerate() {
        let color = if s.passed { "#1a7f37" } else { "#cf222e" };
        out.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" fill=\"{}\"/>",
            pad + i as f64 * step, y(s.fail), color
        ));
    }
    out.push_str("</svg>");
    out.push_str(&format!(
        "<div class=\"hint\">{} scans recorded · FAIL {} → {} · green = PASS, red = FAIL</div>",
        n, first.fail, last.fail
    ));
    out.push_str("</div>");
    out
}

/// Render a gate JSON document into a complete standalone HTML page.
/// Render without history (used by tests and callers without a project root).
pub fn render(doc: &Json) -> String {
    render_with_history(doc, &[])
}

pub fn render_with_history(doc: &Json, snaps: &[crate::history::Snapshot]) -> String {
    let passed = matches!(doc.get("passed"), Some(Json::Bool(true)));
    let files = doc.get("files_scanned")
        .and_then(|v| match v { Json::Num(n) => Some(*n as usize), _ => None })
        .unwrap_or(0);
    let known = doc.get("known_issues")
        .and_then(|v| match v { Json::Num(n) => Some(*n as usize), _ => None })
        .unwrap_or(0);
    let findings: Vec<&Json> = match doc.get("findings") {
        Some(Json::Arr(a)) => a.iter().collect(),
        _ => Vec::new(),
    };
    let count = |v: &str| findings.iter().filter(|f| jstr(f.get("verdict")) == v).count();
    let n_fail = count("FAIL");
    let n_pass = count("PASS");
    let n_base = count("BASELINE");
    let n_unconf = count("UNCONFIRMED");
    let n_na = count("NOT-ASSESSED");

    let mut s = String::new();
    s.push_str("<!DOCTYPE html>\n<html lang=\"zh-CN\"><head><meta charset=\"utf-8\">");
    s.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    s.push_str("<title>prodgate report</title><style>");
    s.push_str(CSS);
    s.push_str("</style></head><body><div class=\"wrap\">");

    s.push_str(&format!(
        "<h1>{}</h1><p class=\"sub\">prodgate · {} source files scanned · {} known issue(s)</p>",
        if passed { "Gate PASS" } else { "Gate FAIL" }, files, known
    ));

    s.push_str("<div class=\"stats\">");
    let stats = [
        ("fail", n_fail, "FAIL"),
        ("pass", n_pass, "PASS"),
        ("baseline", n_base, "BASELINE"),
        ("warn", n_unconf, "UNCONFIRMED"),
        ("na", n_na, "NOT-ASSESSED"),
    ];
    for (cls, n, label) in stats {
        s.push_str(&format!(
            "<div class=\"stat {}\"><div class=\"num\">{}</div><div class=\"lbl\">{}</div></div>",
            cls, n, label
        ));
    }
    s.push_str("</div>");

    if !findings.is_empty() {
        let total = findings.len() as f64;
        s.push_str("<div class=\"bar\">");
        for (cls, n) in [("var(--fail)", n_fail), ("var(--pass)", n_pass),
                         ("var(--baseline)", n_base), ("var(--warn)", n_unconf),
                         ("var(--muted)", n_na)] {
            if n > 0 {
                s.push_str(&format!(
                    "<span style=\"width:{}%;background:{}\"></span>",
                    n as f64 / total * 100.0, cls
                ));
            }
        }
        s.push_str("</div>");

        s.push_str(&render_trend(snaps));

        s.push_str("<div class=\"filter\">");
        s.push_str("<button class=\"on\" data-k=\"all\" onclick=\"filter('all')\">All</button>");
        for (k, label) in [("fail", "FAIL"), ("pass", "PASS"), ("baseline", "BASELINE"),
                           ("unconfirmed", "UNCONFIRMED"), ("na", "NOT-ASSESSED")] {
            s.push_str(&format!(
                "<button data-k=\"{}\" onclick=\"filter('{}')\">{}</button>", k, k, label
            ));
        }
        s.push_str("</div>");

        for f in findings {
            s.push_str(&render_finding(f));
        }
    }

    s.push_str("<div class=\"foot\">Generated by prodgate. Self-contained — no external assets. ");
    s.push_str("FAIL blocks; UNCONFIRMED means the gate cannot prove the control exists; ");
    s.push_str("NOT-ASSESSED means the rule does not apply to this project surface.</div>");
    s.push_str("</div><script>");
    s.push_str(JS);
    s.push_str("</script></body></html>");
    s
}
