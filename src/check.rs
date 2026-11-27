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

// Provider prefixes are assembled at runtime so this rules file does not
// contain the exact strings it searches for. Self-gate must not trip on its
// own source, and push protection must not flag the rule as a real secret.
const PREFIX_PARTS: [&str; 5] = ["sk", "ghp", "gho", "xoxb", "AKIA"];
const PREFIX_SUFFIXES: [&str; 5] = ["_live_", "_", "_", "-", ""];
fn prefixed(value: &str) -> bool {
    PREFIX_PARTS
        .iter()
        .zip(PREFIX_SUFFIXES.iter())
        .any(|(head, tail)| {
            let mut full = String::from(*head);
            full.push_str(tail);
            value.starts_with(&full)
        })
}

/// Signal rule: inspects every line. Wildcard CORS is dangerous even in a
/// string, because strings get used as header values.
fn cors_wildcard(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    // How the origin key and the wildcard are connected varies a lot in real
    // code: `origin: '*'`, `origin = '*'`, or `setHeader('...Origin', '*')`
    // where they are function arguments separated by a comma. Matching one
    // specific operator misses real configurations. Instead: find a quoted
    // '*' used as a VALUE (not the object of a comparison), and require an
    // origin keyword in the same or adjacent code line.
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

    let has_origin = |line: &str| {
        has_any(line, &["cors", "alloworigin", "allow_origin", "origin"])
    };

    // A quoted '*' that is being compared (`=== '*'`) is a read, not a
    // configuration. Everything else (after `:` `=` or `,` or inside a
    // string literal like "Origin: *") can be a value.
    let wildcard_as_value = |line: &str| -> bool {
        for needle in ["'*'", "\"*\""] {
            let mut from = 0;
            while let Some(rel) = line[from..].find(needle) {
                let pos = from + rel;
                let before = line[..pos].trim_end();
                let is_comparison = before.ends_with("==")
                    || before.ends_with("===")
                    || before.ends_with("!=")
                    || before.ends_with("!==")
                    || before.ends_with(">=")
                    || before.ends_with("<=");
                if !is_comparison {
                    return true;
                }
                from = pos + needle.len();
            }
        }
        // Bare `Origin: *` without quotes, as in header strings.
        let lower = line.to_lowercase();
        for (i, _) in lower.match_indices('*') {
            let before = lower[..i].trim_end();
            if before.ends_with(':') || before.ends_with('=') {
                return true;
            }
        }
        false
    };

    for w in code.windows(2) {
        let (first_no, first) = w[0];
        let (second_no, second) = w[1];
        // Same line: origin keyword and wildcard value together.
        if has_origin(first) && wildcard_as_value(first) {
            return Some(Finding {
                id: "cors.wildcard",
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, first_no),
                note: "wildcard assigned as CORS origin".into(),
            });
        }
        // Adjacent lines: key on one line, wildcard value on the next.
        if (has_origin(first) && wildcard_as_value(second))
            || (has_origin(second) && wildcard_as_value(first))
        {
            return Some(Finding {
                id: "cors.wildcard",
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, first_no),
                note: "wildcard assigned as CORS origin".into(),
            });
        }
        // Last line has no window partner; check it alone below.
        let _ = second_no;
    }
    if let Some((no, line)) = code.last() {
        if has_origin(line) && wildcard_as_value(line) {
            return Some(Finding {
                id: "cors.wildcard",
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, no),
                note: "wildcard assigned as CORS origin".into(),
            });
        }
    }
    None
}

/// Is the assignment target a credential key? Exact words plus a suffix
/// pattern (`_key`, `_secret`, ...). The underscore prefix matters: `MONKEY`
/// ends in "key" but is not a credential, while `chat_key` and `CHAT_AES_KEY`
/// are. The suffix must have at least 3 identifier chars before it.
fn credential_key(lhs_lower: &str, exact: &[&str], safe: &[&str]) -> bool {
    if has_any(lhs_lower, safe) {
        return false;
    }
    if has_any(lhs_lower, exact) {
        return true;
    }
    let trimmed = lhs_lower.trim_end();
    ["_key", "_secret", "_token", "_password", "_passwd"]
        .iter()
        .any(|suffix| trimmed.ends_with(suffix) && trimmed.len() > suffix.len() + 2)
}

/// Signal rule: inspects every line. Hardcoded secret = long literal assigned
/// to a credential-looking key, in production code only.
fn secrets_hardcoded(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    // Test code is full of credential-shaped literals used as fixtures:
    // `let secret = "..."`, IPv6 strings, prose about tokens. They are not
    // leaked keys. Two kinds of test location are excluded:
    //   1. test-shaped paths (tests/, fixtures/, ...), and
    //   2. #[test] / #[cfg(test)] function bodies, which in Rust live inside
    //      otherwise production files (e.g. server.rs).
    let path_lower = file.path.to_lowercase();
    if ["test", "spec", "fixture", "mock"]
        .iter()
        .any(|seg| path_lower.contains(seg))
    {
        return None;
    }
    let in_test = test_line_mask(&file.lines);

    let keys = ["api_key", "apikey", "secret", "token", "password", "passwd", "access_key"];
    let safe = ["example", "placeholder", "your_", "test_", "dummy", "redact", "fixture", "public_"];

    for (i, line) in file.lines.iter().enumerate() {
        if in_test[i] {
            continue;
        }
        // Find an assignment operator outside strings/comments. Both `=` (let
        // / property assignment) and `:` (object literal property) count —
        // `JWT_SECRET: process.env.JWT_SECRET || "literal"` is a real shape.
        let eq = match assignment_eq(line) { Some(p) => p, None => continue };
        let lhs_lower = line[..eq].to_lowercase();

        // Only the LEFT side of the assignment can be a credential key.
        // Scanning the whole line produces false positives when an ordinary
        // string on the right mentions "tokens" or "password".
        if !credential_key(&lhs_lower, &keys, &safe) {
            continue;
        }
        let rhs = line[eq + 1..].trim_start();

        // Find the first quoted literal on the RHS. It may be a plain value
        // ("literal") or a hardcoded FALLBACK after `||` / `??` /
        // `process.env.X ||` — the fallback is more dangerous because it is
        // actually used when the env var is missing in production.
        let first_quote = rhs.find(['"', '\'']);
        let quote_pos = match first_quote { Some(p) => p, None => continue };
        let quote = rhs.as_bytes()[quote_pos] as char;
        let literal = &rhs[quote_pos + 1..];
        let end_quote = literal.find(quote).unwrap_or(0);
        let value = &literal[..end_quote];
        let value_lower = value.to_lowercase();

        // Real credentials are compact and either mixed (letters + digits)
        // or carry a known provider prefix. Plain words like "privatevalue"
        // are overwhelmingly test fixtures, not keys.
        let compact = !value.is_empty()
            && !value.chars().any(|c| c.is_whitespace() || c == '/' || c == '\\');
        let has_digit = value.chars().any(|c| c.is_ascii_digit());
        let has_alpha = value.chars().any(|c| c.is_ascii_alphabetic());
        let prefixed_value = prefixed(&value_lower);
        // An env-var fallback (`process.env.X || "literal"`) is a strong
        // signal on its own: the literal is what production actually uses
        // when the variable is missing. It need not contain digits — real
        // fixes remove values like "securevault_default_secret_change_me".
        let is_fallback = rhs[..quote_pos].contains("||") || rhs[..quote_pos].contains("??");
        let suspicious = if is_fallback {
            has_alpha
        } else {
            prefixed_value || (has_digit && has_alpha)
        };
        if end_quote >= 8 && compact && suspicious {
            let note = if is_fallback {
                "hardcoded credential fallback in env var expression"
            } else {
                "credential-looking key assigned a long literal"
            };
            return Some(Finding {
                id: "secrets.hardcoded",
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, i + 1),
                note: note.into(),
            });
        }
    }
    None
}

/// Byte index of the first `=` outside string literals and line comments.
/// Returns None when the only `=` signs live inside strings (URLs, examples).
fn assignment_eq(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut in_char = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if escaped { escaped = false; i += 1; continue; }
        if c == b'\\' { escaped = true; i += 1; continue; }
        if c == b'"' && !in_char { in_str = !in_str; i += 1; continue; }
        if c == b'\'' && !in_str { in_char = !in_char; i += 1; continue; }
        if !in_str && !in_char {
            if c == b'=' || c == b':' {
                // Skip comparison operators: ==, ===, !=, >=, <=, =>.
                // A bare ':' is an object-literal property assignment.
                let next = bytes.get(i + 1).copied();
                let prev = if i > 0 { Some(bytes[i - 1]) } else { None };
                if c == b'=' && (next == Some(b'=') || prev == Some(b'!') || prev == Some(b'>')
                    || prev == Some(b'<') || next == Some(b'>')) {
                    i += 1;
                    continue;
                }
                return Some(i);
            }
            // Rust doc comments and line comments: stop scanning, any `=`
            // after here is documentation, not code.
            if c == b'/' && bytes.get(i + 1) == Some(&b'/') { return None; }
        }
        i += 1;
    }
    None
}

/// For each line, report whether it sits inside a Rust #[test] function body.
/// Tracks brace depth from the `fn` that follows the attribute until its
/// closing brace. Attribute lines and the signature line itself are included.
fn test_line_mask(lines: &[String]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut depth: i32 = 0;
    let mut test_depth: Option<i32> = None;
    let mut pending_attr = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[test]") || trimmed.starts_with("#[cfg(test)]") {
            pending_attr = true;
        }
        if test_depth.is_some() {
            mask[i] = true;
        }
        if pending_attr && trimmed.starts_with("fn ") {
            // The opening brace of this signature raises depth to body level.
            test_depth = Some(depth + 1);
            mask[i] = true;
            pending_attr = false;
        }

        // Count braces, ignoring those inside string/char literals roughly by
        // tracking quote state. Good enough for attribute-driven test bodies.
        let mut in_str = false;
        let mut escaped = false;
        for c in line.chars() {
            if escaped { escaped = false; continue; }
            if c == '\\' { escaped = true; continue; }
            if c == '"' { in_str = !in_str; continue; }
            if in_str { continue; }
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if test_depth == Some(depth) {
                        test_depth = None;
                    }
                }
                _ => {}
            }
        }
    }
    mask
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
