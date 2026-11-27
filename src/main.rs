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
    println!("  replay     Check that every rule separates good from bad fixtures");
    println!("  cases      Validate the contribution case library");
    println!("  mutate     Inject known-bad samples into real project layouts");
    println!("  baseline   Record current FAILs as known issues");
    println!("  all        Replay + mutate + gate (self-bootstrap check, the default)");
    println!("  prompt     Emit AI coding rules with real good/bad examples (feed-forward)");
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
    let positional: Vec<&str> = argv
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();

    // No positional command: preserve the historical default of `all`.
    let command = positional.first().copied().unwrap_or("all").to_string();
    let valid = ["gate", "replay", "baseline", "cases", "mutate", "all", "prompt"];
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
            "gate" => emit_json(report::gate_json(root)),
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
            "prompt" => {
                use prodgate::json::Json;
                let sections = prodgate::prompt::sections(root);
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
            "replay" => print_replay(root),
            "baseline" => print_baseline(root),
            "cases" => print_cases(root),
            "mutate" => print_mutation(),
            "prompt" => {
                print!("{}", prodgate::prompt::render(root));
                true
            }
            "all" => {
                let a = print_replay(root);
                println!();
                let m = print_mutation();
                println!();
                let b = print_gate(root);
                a && m && b
            }
            _ => unreachable!(),
        }
    };

    if !ok {
        std::process::exit(1);
    }
}
