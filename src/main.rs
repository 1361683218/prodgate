use prodgate::{cases, check::Verdict, fixture, gate, mutation, report};
use std::env;
use std::path::Path;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn help() {
    println!("prodgate {} — production-readiness gate", VERSION);
    println!();
    println!("Usage:");
    println!("  prodgate <command> [options] [root]");
    println!();
    println!("Commands:");
    println!("  gate       Scan a project against the rules");
    println!("  history    Show scan history from .prodgate/history.jsonl");
    println!("  prune      Remove snapshots older than the 30-day window");
    println!("  replay     Check that every rule separates good from bad fixtures");
    println!("  cases      Validate the contribution case library");
    println!("  mutate     Inject known-bad samples into real project layouts");
    println!("  baseline   Record current FAILs as known issues");
    println!("  all        Replay + mutate + gate (self-bootstrap check, the default)");
    println!("  prompt     Emit AI coding rules with real good/bad examples (feed-forward)");
    println!("  efficacy   Prompt information-sufficiency experiment on archived AI code");
    println!("  regression Check real-world snapshots for false positives");
    println!("  report     Generate a self-contained HTML report (--out FILE)");
    println!("  artifacts  Verify phase artifacts named in artifacts.tsv");
    println!();
    println!("Options:");
    println!("  --json     Emit a machine-readable JSON report on stdout");
    println!("  -h, --help Show this help");
    println!("  -V, --version Show the version");
    println!();
    println!("Default root is the current directory.");
}

fn print_gate(root: &Path) -> bool {
    let report = gate::run_gate(root);
    let _ = prodgate::history::record(root, &report);
    println!("{:<22} {:<14} NOTE", "RULE", "VERDICT");
    for f in &report.findings {
        println!("{:<22} {:<14} {}", f.id, f.verdict.as_str(), f.note);
        if f.verdict != Verdict::Pass && f.evidence != "-" {
            println!("    at {}", f.evidence);
        }
        if let Some(fix) = f.suggestion {
            println!("    fix: {}", fix);
        }
    }
    let known = report
        .findings
        .iter()
        .filter(|f| f.verdict == Verdict::Baseline)
        .count();
    println!(
        "\nscanned {} source file(s), {} known issue(s)",
        report.files_scanned, known
    );
    println!("gate: {}", if report.passed() { "PASS" } else { "FAIL" });
    report.passed()
}

/// Format a UTC unix timestamp as YYYY-MM-DD HH:MM:SS. No chrono here — the
/// project is zero-dependency, so use the civil-from-days algorithm directly.
fn format_ts(ts: u64) -> String {
    let secs = ts as i64;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    if m <= 2 { y += 1; }
    let hh = sod / 3600;
    let mm = (sod % 3600) / 60;
    let ss = sod % 60;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hh, mm, ss)
}

/// Text view of the append-only scan history. Read-only: it must not append
/// a snapshot, otherwise viewing history would change the thing being viewed.
fn print_history(root: &Path) -> bool {
    let snaps = prodgate::history::load(root);
    println!(
        "{:<21} {:<6} {:<5} {:<5} PASSED",
        "TIME", "FILES", "FAIL", "PASS"
    );
    for s in &snaps {
        println!(
            "{:<21} {:<6} {:<5} {:<5} {}",
            format_ts(s.ts),
            s.files,
            s.fail,
            s.pass,
            if s.passed { "PASS" } else { "FAIL" }
        );
    }
    println!("\nhistory: {} snapshot(s)", snaps.len());
    true
}

/// Text view of pruning: a write operation, but it reports instead of gating.
fn print_prune(root: &Path) -> bool {
    match prodgate::history::prune(root) {
        Ok(removed) => {
            println!("prune: removed {} stale snapshot(s)", removed);
            true
        }
        Err(e) => {
            eprintln!("prune: failed to rewrite .prodgate/history.jsonl: {}", e);
            false
        }
    }
}

fn print_replay(root: &Path) -> bool {
    let results = fixture::replay(root);
    println!("{:<22} {:<14} {:<14} STATUS", "RULE", "GOOD", "BAD");
    let mut ok = true;
    for r in &results {
        println!(
            "{:<22} {:<14} {:<14} {}",
            r.rule,
            r.good.as_str(),
            r.bad.as_str(),
            if r.ok() { "OK" } else { "BROKEN" }
        );
        ok &= r.ok();
    }
    println!("\nreplay: {}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn print_artifacts(root: &Path) -> bool {
    let r = prodgate::artifacts::check_root(root);
    if !r.manifest_present {
        println!("ARTIFACTS            no manifest (no artifact contract declared)");
        println!("\nartifacts: PASS");
        return true;
    }
    println!("ARTIFACTS            CHECKED   NOT-ASSESSED");
    println!("manifest             {:<10} {}", r.checked, r.not_assessed.len());
    for f in &r.failures { println!("    FAIL: {}", f); }
    for n in &r.not_assessed { println!("    NOT-ASSESSED: {}", n); }
    println!("\nartifacts: {}", if r.passed() { "PASS" } else { "FAIL" });
    r.passed()
}

fn print_regression() -> bool {
    let r = prodgate::regression::run();
    println!("{:<22} FILES   FALSE-POSITIVES", "REGRESSION");
    println!(
        "{:<22} {:<7} {}",
        "real-world snapshots", r.files_scanned, r.false_positives.len()
    );
    for fp in &r.false_positives {
        println!("    FP: {}", fp);
    }
    println!(
        "\nregression: {}",
        if r.passed { "PASS" } else { "FAIL" }
    );
    r.passed
}

/// Efficacy is reported but does not gate: it measures information
/// sufficiency, and a temporary experiment gap should not block a build.
fn print_efficacy_summary() {
    let r = prodgate::efficacy::run(Path::new(env!("CARGO_MANIFEST_DIR")));
    println!(
        "efficacy (information only): {}/{} rules covered, {}/{} counterfactual ok — {}",
        r.covered,
        r.rules_hit.len(),
        r.counterfactual_ok,
        r.rules_hit.len(),
        if r.passed { "PASS" } else { "FAIL" }
    );
    for g in &r.gaps {
        println!("    GAP: {}", g);
    }
}

fn print_cases(root: &Path) -> bool {
    let results = cases::validate(root);
    println!(
        "{:<26} {:<12} {:<7} {:<7} STATUS",
        "CASE", "PROVENANCE", "GOOD", "BAD"
    );
    let mut ok = true;
    for r in &results {
        let status = if r.valid() { "OK" } else { "INVALID" };
        println!(
            "{:<26} {:<12} {:<7} {:<7} {}",
            r.meta.id,
            r.provenance.as_str(),
            r.good.as_str(),
            r.bad.as_str(),
            status
        );
        for e in &r.errors {
            println!("    error: {}", e);
        }
        ok &= r.valid();
    }
    let trusted = cases::trusted_count(&results);
    println!(
        "\n{} case(s), {} source-backed, {} invalid",
        results.len(),
        trusted,
        results.iter().filter(|r| !r.valid()).count()
    );
    println!("cases: {}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn print_mutation() -> bool {
    let results = mutation::run_mutations();
    println!("{:<38} {:<20} {:<8} STATUS", "SAMPLE", "RULE", "DETECTED");
    for r in &results {
        println!(
            "{:<38} {:<20} {:<8} {}",
            r.name,
            r.rule,
            if r.detected { "yes" } else { "no" },
            if r.ok() { "OK" } else { "MISS" }
        );
        if !r.ok() {
            println!("    note: {}", r.note);
        }
    }
    let detected = results.iter().filter(|r| r.ok()).count();
    println!(
        "\nmutation: {}/{} detected — {}",
        detected,
        results.len(),
        if mutation::passed(&results) { "PASS" } else { "FAIL" }
    );
    mutation::passed(&results)
}

fn print_baseline(root: &Path) -> bool {
    match gate::write_baseline(root) {
        Ok(n) => {
            println!(
                "baseline: recorded {} known issue(s) in .prodgate/baseline.txt",
                n
            );
            true
        }
        Err(e) => {
            eprintln!("baseline: failed to write .prodgate/baseline.txt: {}", e);
            false
        }
    }
}

struct Args {
    command: String,
    root: String,
    json: bool,
}

/// Resolve the --out FILE argument. Defaults to prodgate-report.html in cwd.
/// Parsed directly from argv to avoid widening Args for one command.
fn out_path() -> std::path::PathBuf {
    let argv: Vec<String> = env::args().collect();
    for (i, a) in argv.iter().enumerate() {
        if a == "--out" {
            if let Some(v) = argv.get(i + 1) {
                return std::path::PathBuf::from(v);
            }
        }
        if let Some(v) = a.strip_prefix("--out=") {
            return std::path::PathBuf::from(v);
        }
    }
    std::path::PathBuf::from("prodgate-report.html")
}

fn parse_args() -> Result<Option<Args>, i32> {
    let argv: Vec<String> = env::args().skip(1).collect();

    if argv.iter().any(|a| a == "-h" || a == "--help") {
        help();
        return Ok(None);
    }
    if argv.iter().any(|a| a == "-V" || a == "--version") {
        println!("prodgate {}", VERSION);
        return Ok(None);
    }

    let json = argv.iter().any(|a| a == "--json");
    // Collect positional args on the raw argv, skipping flags and the value
    // that follows --out. Index on raw argv so the skip is unambiguous.
    let mut positional: Vec<&str> = Vec::new();
    let mut skip_next = false;
    for a in &argv {
        if skip_next { skip_next = false; continue; }
        if a == "--out" { skip_next = true; continue; }
        if a.starts_with("--out=") || a.starts_with('-') { continue; }
        positional.push(a);
    }

    // No positional command: preserve the historical default of `all`.
    let command = positional.first().copied().unwrap_or("all").to_string();
    let valid = ["gate", "history", "prune", "replay", "baseline", "cases", "mutate", "all", "prompt", "efficacy", "regression", "report", "artifacts"];
    if !valid.contains(&command.as_str()) {
        eprintln!("unknown command: {} (try --help)", command);
        return Err(2);
    }
    let root_at = if positional.is_empty() { 0 } else { 1 };
    if positional.len() > root_at + 1 {
        eprintln!("too many arguments (try --help)");
        return Err(2);
    }
    let root = positional.get(root_at).copied().unwrap_or(".").to_string();

    Ok(Some(Args {
        command,
        root,
        json,
    }))
}

/// Emit JSON and return its `passed` field. The exit code stays consistent
/// with the report so scripts can use either signal.
fn emit_json(doc: prodgate::json::Json) -> bool {
    let passed = doc
        .get("passed")
        .map(|v| matches!(v, prodgate::json::Json::Bool(true)))
        .unwrap_or(false);
    println!("{}", doc.stringify());
    passed
}

fn main() {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(code) => std::process::exit(code),
    };
    let root = Path::new(&args.root);

    let ok = if args.json {
        match args.command.as_str() {
            "gate" => emit_json(report::gate_json_recording(root)),
            "history" => emit_json(report::history_json(root)),
            "prune" => match prodgate::history::prune(root) {
                Ok(removed) => emit_json(report::prune_json(removed)),
                Err(e) => {
                    eprintln!("prune: failed to rewrite .prodgate/history.jsonl: {}", e);
                    false
                }
            },
            "replay" => emit_json(report::replay_json(root)),
            "cases" => emit_json(report::cases_json(root)),
            "mutate" => emit_json(report::mutation_json()),
            "all" => emit_json(report::all_json(root)),
            "baseline" => {
                // A write operation, but emits a report so scripts can chain
                // it without switching output modes.
                match gate::write_baseline(root) {
                    Ok(recorded) => emit_json(prodgate::json::Json::Obj(vec![
                        ("tool".into(), prodgate::json::Json::Str("prodgate".into())),
                        (
                            "command".into(),
                            prodgate::json::Json::Str("baseline".into()),
                        ),
                        ("passed".into(), prodgate::json::Json::Bool(true)),
                        (
                            "recorded".into(),
                            prodgate::json::Json::Num(recorded as f64),
                        ),
                    ])),
                    Err(e) => {
                        eprintln!("baseline: failed to write: {}", e);
                        false
                    }
                }
            }
            "efficacy" => emit_json(prodgate::efficacy::to_json(&prodgate::efficacy::run(
                Path::new(env!("CARGO_MANIFEST_DIR")),
            ))),
            "artifacts" => emit_json(prodgate::artifacts::to_json(&prodgate::artifacts::check_root(root))),
            "regression" => emit_json(prodgate::regression::to_json(&prodgate::regression::run())),
            "report" => {
                // HTML is a human artifact; --json on report emits metadata
                // about the generated file rather than the document itself.
                let doc = report::gate_json(root);
                let snaps = prodgate::history::load(root);
                let html = prodgate::html_report::render_with_history(&doc, &snaps);
                let out = out_path();
                match std::fs::write(&out, html) {
                    Ok(()) => emit_json(prodgate::json::Json::Obj(vec![
                        ("tool".into(), prodgate::json::Json::Str("prodgate".into())),
                        ("command".into(), prodgate::json::Json::Str("report".into())),
                        ("passed".into(), prodgate::json::Json::Bool(true)),
                        ("out".into(), prodgate::json::Json::Str(out.display().to_string())),
                    ])),
                    Err(e) => { eprintln!("report: failed to write {}: {}", out.display(), e); false }
                }
            }
            "prompt" => {
                use prodgate::json::Json;
                let cfg = gate::load_config(root);
                let cases_root = Path::new(env!("CARGO_MANIFEST_DIR"));
                let sections = prodgate::prompt::sections(cases_root, &cfg.surface);
                let doc = Json::Obj(vec![
                    ("tool".into(), Json::Str("prodgate".into())),
                    ("command".into(), Json::Str("prompt".into())),
                    ("passed".into(), Json::Bool(true)),
                    ("rules".into(), Json::Arr(sections)),
                ]);
                emit_json(doc)
            }
            _ => unreachable!(),
        }
    } else {
        match args.command.as_str() {
            "gate" => print_gate(root),
            "history" => print_history(root),
            "prune" => print_prune(root),
            "replay" => print_replay(root),
            "baseline" => print_baseline(root),
            "cases" => print_cases(root),
            "mutate" => print_mutation(),
            "report" => {
                let doc = report::gate_json(root);
                let snaps = prodgate::history::load(root);
                let html = prodgate::html_report::render_with_history(&doc, &snaps);
                let size = html.len();
                let out = out_path();
                match std::fs::write(&out, html) {
                    Ok(()) => {
                        println!("report: wrote {} ({} bytes, self-contained)", out.display(), size);
                        true
                    }
                    Err(e) => { eprintln!("report: failed to write {}: {}", out.display(), e); false }
                }
            }
            "artifacts" => print_artifacts(root),
            "regression" => {
                let r = prodgate::regression::run();
                println!("snapshots            : {}", r.snapshots);
                println!("files scanned        : {}", r.files_scanned);
                println!("false positives      : {}", r.false_positives.len());
                for fp in &r.false_positives { println!("  FP: {}", fp); }
                println!("regression: {}", if r.passed { "PASS" } else { "FAIL" });
                r.passed
            }
            "efficacy" => {
                let r = prodgate::efficacy::run(Path::new(env!("CARGO_MANIFEST_DIR")));
                println!("AI code samples      : {}", r.samples);
                println!("with findings        : {}", r.samples_with_findings);
                println!("baseline FAILs       : {}", r.baseline_findings);
                println!("rules hit            : {}", r.rules_hit.len());
                println!("prompt-covered       : {}/{}", r.covered, r.rules_hit.len());
                println!("counterfactual ok    : {}/{}", r.counterfactual_ok, r.rules_hit.len());
                for g in &r.gaps { println!("  GAP: {}", g); }
                println!("\nscope: information sufficiency only - does NOT measure AI behavior change");
                println!("efficacy: {}", if r.passed { "PASS" } else { "FAIL" });
                r.passed
            }
            "prompt" => {
                let cfg = gate::load_config(root);
                let cases_root = Path::new(env!("CARGO_MANIFEST_DIR"));
                print!("{}", prodgate::prompt::render(cases_root, &cfg.surface));
                true
            }
            "all" => {
                // Every guard runs — no early exit — so one failing check
                // cannot hide the state of the others.
                let a = print_replay(root);
                println!();
                let m = print_mutation();
                println!();
                let b = print_gate(root);
                println!();
                let c = print_cases(root);
                println!();
                let r = print_regression();
                println!();
                let art = print_artifacts(root);
                println!();
                print_efficacy_summary();
                // A baseline can flip FAIL to PASS without changing code.
                // Always show how many issues are frozen, and fail loudly if
                // the project has quietly accumulated a backlog.
                let frozen = gate::load_baseline(root).len();
                let limit = gate::MAX_BASELINE;
                println!(
                    "baseline frozen        : {}/{} ({} {})",
                    frozen,
                    limit,
                    if frozen > limit { "OVER LIMIT" } else { "within limit" },
                    if frozen > limit { "— gate bypass suspected" } else { "" }
                );
                a && m && b && c && r && art && frozen <= limit
            }
            _ => unreachable!(),
        }
    };

    if !ok {
        std::process::exit(1);
    }
}
