//! End-to-end mutation validation.
//!
//! `replay` checks a rule against its own toy fixture by calling the rule
//! function directly. That misses an entire class of failures: the rule may
//! be correct but the project never scans the file, the adapter may reject
//! the language, or a neighboring rule may interfere.
//!
//! Mutation validation injects a known-bad sample into a temporary project
//! that mirrors a real project's layout, then runs the FULL gate pipeline
//! (config load -> source collection -> adapter -> rules) and checks that the
//! expected rule reports FAIL. A rule that passes replay but misses mutation
//! is not actually protecting anything.
//!
//! Samples come from real frameworks (expressjs/express, pallets/flask,
//! gin-gonic/gin) and from a real upstream PR (expressjs/cors #39).

use crate::check::Verdict;
use crate::gate;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// A known-bad snippet placed at a path that mirrors a real project.
pub struct MutationCase {
    pub name: &'static str,
    pub rule: &'static str,
    pub surface: &'static str,
    /// Relative path inside the temporary project, preserving real layout.
    pub path: &'static str,
    pub source: &'static str,
}

/// Samples that must be caught end-to-end. Each one corresponds to a pattern
/// found or injected during the real-project benchmark (see BENCHMARK.md).
pub fn cases() -> Vec<MutationCase> {
    vec![
        MutationCase {
            name: "express setHeader wildcard",
            rule: "cors.wildcard",
            surface: "web",
            path: "lib/express.js",
            source: "app.use(function(req, res, next) {\n  res.setHeader('Access-Control-Allow-Origin', '*');\n  next();\n});\n",
        },
        MutationCase {
            name: "flask headers assignment wildcard",
            rule: "cors.wildcard",
            surface: "web",
            path: "src/flask/app.py",
            source: "response.headers['Access-Control-Allow-Origin'] = '*'\n",
        },
        MutationCase {
            name: "gin Header call wildcard",
            rule: "cors.wildcard",
            surface: "web",
            path: "gin.go",
            source: "func injectedCors() {\n    c.Header(\"Access-Control-Allow-Origin\", \"*\")\n}\n",
        },
        MutationCase {
            name: "expressjs/cors PR #39 two-line",
            rule: "cors.wildcard",
            surface: "web",
            path: "lib/index.js",
            source: "key: 'Access-Control-Allow-Origin'\nvalue: '*'\n",
        },
        MutationCase {
            name: "hardcoded api key in production",
            rule: "secrets.hardcoded",
            surface: "cli",
            path: "src/config.rs",
            source: "let api_key = \"FAKE_sample_51H8xQ2eZvKYlo2Cabcdefgh\";\n",
        },
        MutationCase {
            name: "hardcoded env fallback (securevault)",
            rule: "secrets.hardcoded",
            surface: "cli",
            path: "server/config/config.js",
            source: "  JWT_SECRET: process.env.JWT_SECRET || 'securevault_default_secret_change_me',\n",
        },
        MutationCase {
            name: "hardcoded fallback with custom key name",
            rule: "secrets.hardcoded",
            surface: "cli",
            path: "server/controllers/chatController.js",
            source: "const CHAT_KEY = process.env.CHAT_AES_KEY || 'SecureVaultChatKey2024!';\n",
        },
        MutationCase {
            name: "explicit rls opt-out",
            rule: "rls",
            surface: "web",
            path: "src/db.rs",
            source: "// prodgate: rls=off\n",
        },
    ]
}

#[derive(Debug, Clone)]
pub struct MutationResult {
    pub name: &'static str,
    pub rule: &'static str,
    pub detected: bool,
    pub evidence: String,
    pub note: String,
}

impl MutationResult {
    pub fn ok(&self) -> bool {
        self.detected
    }
}

fn scratch_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "prodgate-mutation-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Inject one case into a fresh temporary project and run the full gate.
fn inject_and_gate(case: &MutationCase) -> MutationResult {
    let dir = scratch_root();
    let _ = fs::remove_dir_all(&dir);

    // Derive the scan root from the sample path's first component so the
    // layout matches how the real project is configured (lib/, src/, .).
    let first = case.path.split('/').next().unwrap_or("src");
    let roots = if case.path.contains('/') { first } else { "." };
    let config = format!(
        "roots={}\nsurface={}\nrecursive=true\n",
        roots, case.surface
    );
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join("prodgate.conf"), config);

    let target = dir.join(case.path);
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let written = fs::write(&target, case.source).is_ok();

    let report = if written {
        Some(gate::run_gate(&dir))
    } else {
        None
    };
    let _ = fs::remove_dir_all(&dir);

    match report {
        None => MutationResult {
            name: case.name,
            rule: case.rule,
            detected: false,
            evidence: "-".into(),
            note: "failed to write sample file".into(),
        },
        Some(report) => {
            let finding = report
                .findings
                .iter()
                .find(|f| f.id == case.rule && f.verdict == Verdict::Fail);
            match finding {
                Some(f) => MutationResult {
                    name: case.name,
                    rule: case.rule,
                    detected: true,
                    evidence: f.evidence.clone(),
                    note: f.note.clone(),
                },
                None => MutationResult {
                    name: case.name,
                    rule: case.rule,
                    detected: false,
                    evidence: "-".into(),
                    note: format!(
                        "expected {} to FAIL; report: {}",
                        case.rule,
                        report
                            .findings
                            .iter()
                            .filter(|f| f.verdict == Verdict::Fail)
                            .map(|f| f.id)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                },
            }
        }
    }
}

pub fn run_mutations() -> Vec<MutationResult> {
    cases().iter().map(inject_and_gate).collect()
}

pub fn passed(results: &[MutationResult]) -> bool {
    !results.is_empty() && results.iter().all(|r| r.ok())
}
