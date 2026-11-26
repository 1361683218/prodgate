//! Detection rules. Each rule is a pure function over a normalized SourceFile.
//! No external crates: we deliberately keep this buildable offline.

use crate::adapter::SourceFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    /// A known issue recorded in the baseline: reported but does not block.
    Baseline,
    NotAssessed,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Baseline => "BASELINE",
            Verdict::NotAssessed => "NOT-ASSESSED",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub id: &'static str,
    pub verdict: Verdict,
    pub evidence: String,
    pub note: String,
}

/// Scan context. `surface` is declared by the project in prodgate.conf so a
/// rule can return NOT-ASSESSED instead of guessing from source text.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuleCtx<'a> {
    pub surface: &'a str,
}

/// A rule inspects one normalized file and returns a finding, or None when
/// the file does not carry a signal for that rule.
pub type RuleFn = fn(&SourceFile, &RuleCtx<'_>) -> Option<Finding>;

#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub id: &'static str,
    pub description: &'static str,
    pub run: RuleFn,
}

fn has_any(line: &str, needles: &[&str]) -> bool {
    let lower = line.to_lowercase();
    needles.iter().any(|n| lower.contains(n))
}

/// Signal rule: inspects every line. Wildcard CORS is dangerous even in a
/// string, because strings get used as header values.
fn cors_wildcard(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    // Origin key and wildcard value may be on separate lines in real code
    // (e.g. expressjs/cors: key: 'Access-Control-Allow-Origin' / value: '*').
    // Scan a two-line window; skip comment lines so documentation does not
    // count as configuration.
    // Collect only code lines (comments excluded), keeping their original
    // line numbers, then look for an origin key and a wildcard in adjacent
    // code lines — real code puts them on separate lines.
    let code: Vec<(usize, &str)> = file
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let t = line.trim_start();
            !t.starts_with("//") && !t.starts_with('#')
        })
        .map(|(i, line)| (i + 1, line.as_str()))
        .collect();

    // A wildcard is dangerous when it is ASSIGNED as an origin value, not
    // when it appears in a comparison (options.origin === '*' is a check).
    // Match a quoted '*' immediately after an assignment operator.
    let is_wildcard_assignment = |line: &str| -> bool {
        let lower = line.to_lowercase();
        let bytes = lower.as_bytes();
        for (pos, ch) in bytes.iter().enumerate() {
            if *ch != b':' && *ch != b'=' {
                continue;
            }
            // Skip comparison chains: '===' / '==' / '!=' / '>=' are reads,
            // not assignments. Only a lone '=' or ':' counts.
            if *ch == b'=' {
                if pos > 0 && matches!(bytes[pos - 1], b'=' | b'!' | b'>' | b'<') {
                    continue;
                }
                if bytes.get(pos + 1) == Some(&b'=') {
                    continue;
                }
            }
            let after = lower[pos + 1..].trim_start();
            if after.starts_with("'*'") || after.starts_with("\"*\"") {
                return true;
            }
            // Also catch "Origin: *" inside a string literal.
            if after.starts_with('*')
                && after[1..].trim_start().starts_with(['\'', '"'])
            {
                return true;
            }
        }
        false
    };

    for w in code.windows(2) {
        let window = format!("{} {}", w[0].1, w[1].1);
        if !has_any(&window, &["cors", "alloworigin", "allow_origin", "origin"]) {
            continue;
        }
        if is_wildcard_assignment(w[0].1) || is_wildcard_assignment(w[1].1) {
            return Some(Finding {
                id: "cors.wildcard",
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, w[0].0),
                note: "wildcard assigned as CORS origin".into(),
            });
        }
    }
    None
}

/// Signal rule: inspects every line. Hardcoded secret = long literal assigned
/// to a credential-looking key.
fn secrets_hardcoded(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    let keys = ["api_key", "apikey", "secret", "token", "password", "passwd", "access_key"];
    let safe = ["example", "placeholder", "your_", "test_", "dummy", "redact", "fixture"];
    for (i, line) in file.lines.iter().enumerate() {
        let lower = line.to_lowercase();
        if !has_any(line, &keys) || has_any(line, &safe) {
            continue;
        }
        let eq = match lower.find('=') { Some(p) => p, None => continue };
        let rhs = lower[eq + 1..].trim_start();
        let quote = match rhs.chars().next() { Some(c @ ('"' | '\'')) => c, _ => continue };
        let literal = rhs.trim_start_matches(quote);
        let end = literal.find(quote).unwrap_or(0);
        if end >= 8 {
            return Some(Finding {
                id: "secrets.hardcoded",
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, i + 1),
                note: "credential-looking key assigned a long literal".into(),
            });
        }
    }
    None
}

/// Declaration rule: reads only comment lines via the language adapter.
/// An opt-out in a string literal is not a decision and must not trip it.
fn find_opt_out(file: &SourceFile, id: &'static str, needle: &str, note: &str) -> Option<Finding> {
    for (line_no, text) in file.declaration_lines() {
        if text.to_lowercase().contains(needle) {
            return Some(Finding {
                id,
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, line_no),
                note: note.into(),
            });
        }
    }
    None
}

// Keys are split from their suffix so this rules file never contains the
// exact string a rule searches for. Self-gate must not trip on its own rules.
const RATE_KEY: &str = "rate_limit";
const ERROR_KEY: &str = "error_tracking";
const WEBHOOK_KEY: &str = "webhook_idempotency";
const RLS_KEY: &str = "rls";
const BACKUP_KEY: &str = "backup";
const DISABLED_VALUE: &str = "=off";

fn rate_limit_off(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    find_opt_out(file, "rate.limit", &format!("{}{}", RATE_KEY, DISABLED_VALUE), "rate limiting explicitly disabled")
}

fn error_tracking_off(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    find_opt_out(file, "error.tracking", &format!("{}{}", ERROR_KEY, DISABLED_VALUE), "error tracking explicitly disabled")
}

fn webhook_idempotency_off(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    find_opt_out(file, "webhook.idempotency", &format!("{}{}", WEBHOOK_KEY, DISABLED_VALUE), "webhook idempotency explicitly disabled")
}

fn rls_off(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    find_opt_out(file, "rls", &format!("{}{}", RLS_KEY, DISABLED_VALUE), "row level security explicitly disabled")
}

fn backup_off(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    find_opt_out(file, "backup", &format!("{}{}", BACKUP_KEY, DISABLED_VALUE), "backup explicitly disabled")
}

pub fn all_rules() -> Vec<Rule> {
    vec![
        Rule { id: "cors.wildcard", description: "No wildcard CORS origin", run: cors_wildcard },
        Rule { id: "secrets.hardcoded", description: "No hardcoded credential literals", run: secrets_hardcoded },
        Rule { id: "rate.limit", description: "Rate limiting not explicitly disabled", run: rate_limit_off },
        Rule { id: "error.tracking", description: "Error tracking not explicitly disabled", run: error_tracking_off },
        Rule { id: "webhook.idempotency", description: "Webhook idempotency not explicitly disabled", run: webhook_idempotency_off },
        Rule { id: "rls", description: "Row level security not explicitly disabled", run: rls_off },
        Rule { id: "backup", description: "Backup not explicitly disabled", run: backup_off },
    ]
}
