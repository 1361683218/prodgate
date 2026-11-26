use prodgate::{cases, check::Verdict, fixture, gate};
use std::env;
use std::path::Path;

fn print_gate(root: &Path) -> bool {
    let report = gate::run_gate(root);
    println!("{:<22} {:<14} NOTE", "RULE", "VERDICT");
    for f in &report.findings {
        println!("{:<22} {:<14} {}", f.id, f.verdict.as_str(), f.note);
        if f.verdict != Verdict::Pass && f.evidence != "-" {
            println!("    at {}", f.evidence);
        }
    }
    let baselined = report
        .findings
        .iter()
        .filter(|f| f.verdict == Verdict::Baseline)
        .count();
    println!("\nscanned {} source file(s), {} known debt(s)", report.files_scanned, baselined);
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
    println!("{:<26} {:<12} {:<7} {:<7} STATUS", "CASE", "PROVENANCE", "GOOD", "BAD");
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
        "\n{} case(s), {} trusted upstream, {} invalid",
        results.len(),
        trusted,
        results.iter().filter(|r| !r.valid()).count()
    );
    println!("cases: {}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn print_baseline(root: &Path) -> bool {
    let n = gate::write_baseline(root);
    println!("baseline: recorded {} known debt(s) in .prodgate/baseline.txt", n);
    true
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("all");
    let root = Path::new(args.get(2).map(String::as_str).unwrap_or("."));

    let ok = match cmd {
        "gate" => print_gate(root),
        "replay" => print_replay(root),
        "baseline" => print_baseline(root),
        "cases" => print_cases(root),
        "all" => {
            let a = print_replay(root);
            println!();
            let b = print_gate(root);
            a && b
        }
        _ => {
            eprintln!("usage: prodgate <gate|replay|baseline|cases|all> [root]");
            std::process::exit(2);
        }
    };

    if !ok {
        std::process::exit(1);
    }
}
