//! Detection rules. Each rule is a pure function over a normalized SourceFile.
//! No external crates: we deliberately keep this buildable offline.

use crate::adapter::SourceFile;
/// Concrete, copy-pasteable fixes shared by CLI and MCP output.
const FIX_BARE_CORS: &str = "Replace bare cors() with an explicit origin, e.g. cors({ origin: 'https://your-app.example' }). If the API is genuinely public, document that decision.";
const FIX_CORS_LITERAL: &str = "Replace the '*' origin with the exact allowed origin(s); never combine '*' with credentials.";
const FIX_DB_URI: &str = "Move the password out of the URI: read it from an environment variable and build the connection string at runtime. Rotate the exposed value.";
const FIX_SECRET_LITERAL: &str = "Load the credential from an environment variable or secret manager and fail fast if missing. Remove the literal from source and rotate the exposed value.";
const FIX_SQL: &str = "Use a parameterized query: execute(\"SELECT ... WHERE id = ?\", (user_id,)). Never interpolate values into SQL text.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    /// A known issue recorded in the baseline: reported but does not block.
    Baseline,
    /// Declaration rule saw no opt-out marker. This is NOT proof that the
    /// control exists — only that nobody declared anything. Non-blocking,
    /// but must not be reported as a clean PASS.
    Unconfirmed,
    NotAssessed,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Baseline => "BASELINE",
            Verdict::Unconfirmed => "UNCONFIRMED",
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
    /// Concrete fix guidance for an AI agent or developer. None when the
    /// finding needs human judgment (e.g. a declaration rule opt-out).
    pub suggestion: Option<&'static str>,
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
// Lowercase: `prefixed()` receives a lowercased value.
const PREFIX_PARTS: [&str; 5] = ["sk", "ghp", "gho", "xoxb", "akia"];
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

    // A bare `cors()` middleware call is the single most common AI-generated
    // CORS mistake: expressjs/cors defaults origin to '*'. There is no '*'
    // literal to find — the danger is in the library's default. Detect the
    // call itself, but not `require('cors')` (an import) or `cors({origin: ...})`
    // (an explicit configuration).
    for (no, line) in &code {
        let lower = line.to_lowercase();
        if lower.contains("require(") || lower.contains("import ") {
            continue;
        }
        for token in ["cors()", "cors( )"] {
            if let Some(pos) = lower.find(token) {
                let after = lower[pos + token.len()..].trim_start();
                // Bare call: only closing parens / semicolon / whitespace after.
                // `cors({ origin: ... })` has `{`, so it is an explicit config.
                let bare = after.chars().all(|c| matches!(c, ')' | ';' | ' ' | '\t'));
                if bare {
                    return Some(Finding {
                        id: "cors.wildcard",
                        verdict: Verdict::Fail,
                        evidence: format!("{}:{}", file.path, no),
                        note: "cors() middleware uses library default wildcard origin".into(),
                        suggestion: Some(FIX_BARE_CORS),
                    });
                }
            }
        }
    }

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
                suggestion: Some(FIX_CORS_LITERAL),
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
                suggestion: Some(FIX_CORS_LITERAL),
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
                suggestion: Some(FIX_CORS_LITERAL),
            });
        }
    }
    None
}

/// Detect `db_scheme://user:password@host` inline credentials. Returns a
/// finding note when found. Only data-store schemes are checked — HTTPS URLs
/// with basic-auth (`https://user:pass@host`) are often fixtures and are not
/// server-side secrets. Placeholder passwords are also skipped.
fn uri_with_password(line: &str) -> Option<&'static str> {
    let schemes = [
        "postgres://", "postgresql://", "mysql://", "mongodb://",
        "mongodb+srv://", "redis://", "mssql://", "oracle://", "amqp://",
    ];
    let lower = line.to_lowercase();
    for scheme in schemes {
        let mut from = 0;
        while let Some(rel) = lower[from..].find(scheme) {
            let start = from + rel + scheme.len();
            let rest = &lower[start..];
            // Credentials are `user:password@` before the host.
            if let Some(at) = rest.find('@') {
                let userinfo = &rest[..at];
                if let Some(colon) = userinfo.find(':') {
                    let password = &userinfo[colon + 1..];
                    let placeholder = [
                        "password", "passwd", "xxx", "your_", "example",
                        "test", "changeme", "todo", "<", "$", "{",
                    ];
                    // Only exact placeholder words, not substrings: real
                    // passwords like "password123" must still be caught.
                    let is_placeholder = placeholder.contains(&password)
                        || password.starts_with("${");
                    if !password.is_empty() && !is_placeholder && password.len() >= 3 {
                        return Some("database connection URI contains inline password");
                    }
                }
            }
            from = start;
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
    if exact_keyword(lhs_lower, exact) {
        return true;
    }
    let trimmed = lhs_lower.trim_end();
    ["_key", "_secret", "_token", "_password", "_passwd"]
        .iter()
        .any(|suffix| trimmed.ends_with(suffix) && trimmed.len() > suffix.len() + 2)
}

/// Exact keyword with identifier boundaries: `secret` matches `secret` and
/// `jwt_secret`, but not `secretary` or `names_bad_token` (where "token" is
/// only part of a longer compound name describing something else).
fn exact_keyword(lhs: &str, exact: &[&str]) -> bool {
    let trimmed = lhs.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    exact.iter().any(|word| {
        trimmed == *word
            || trimmed.ends_with(&format!("_{}", word))
            || trimmed.starts_with(&format!("{}_", word))
    })
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
    let safe = ["example", "placeholder", "your_", "test_", "dummy", "redact", "fixture", "public_", "_file", "default_"];

    for (i, line) in file.lines.iter().enumerate() {
        if in_test[i] {
            continue;
        }

        // A DB connection URI with an inline password is a credential leak
        // regardless of the variable name (`engine = create_engine("...")`).
        // This is a VALUE-format signal, checked before key-name logic.
        if let Some(note) = uri_with_password(line) {
            return Some(Finding {
                id: "secrets.hardcoded",
                verdict: Verdict::Fail,
                evidence: format!("{}:{}", file.path, i + 1),
                note: note.into(),
                suggestion: Some(FIX_DB_URI),
            });
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

        // A function call on the RHS means the string inside it is an
        // ARGUMENT, not the assigned value:
        //   names_bad_token = last.eq_ignore_ascii_case("invalid_token")
        // The literal is a comparison word, not a credential. Keep the
        // exception for `process.env.X || "literal"` (a real fallback).
        let rhs_lower = rhs.to_lowercase();
        let first_q_pos = rhs_lower.find(['"', '\'']);
        let before_value = first_q_pos.map(|p| &rhs_lower[..p]).unwrap_or(&rhs_lower);
        let has_fallback = before_value.contains("||") || before_value.contains("??");
        if !has_fallback {
            let call = rhs_lower.find('(').unwrap_or(usize::MAX);
            let quote = rhs_lower.find(['"', '\'']).unwrap_or(usize::MAX);
            if call < quote && call > 0 {
                let ident = rhs_lower[..call].trim_end();
                if ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
                    continue;
                }
            }
        }

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
        // An ALL_CAPS_WITH_UNDERSCORES value is a config KEY NAME (e.g.
        // ConfigAttribute("SECRET_KEY")), not a credential value. Framework
        // code is full of these; treating them as secrets is a false positive.
        let is_config_key_name = value.chars().all(|c| c.is_ascii_uppercase() || c == '_');
        let has_digit = value.chars().any(|c| c.is_ascii_digit());
        let has_alpha = value.chars().any(|c| c.is_ascii_alphabetic());
        let prefixed_value = prefixed(&value_lower);
        // An env-var fallback (`process.env.X || "literal"`) is a strong
        // signal on its own: the literal is what production actually uses
        // when the variable is missing. It need not contain digits — real
        // fixes remove values like "securevault_default_secret_change_me".
        let is_fallback = rhs[..quote_pos].contains("||") || rhs[..quote_pos].contains("??");
        // An exact credential word on the left (`WEBHOOK_SECRET`, `API_KEY`)
        // is stronger evidence than a suffix match: AI models commonly write
        // placeholder values like "whsec_example_key_replace_this" into these
        // and forget to replace them. Such a value need not contain digits.
        // Suffix-only keys (`SOME_KEY`) keep the stricter rule.
        let strong_key = exact_keyword(&lhs_lower, &keys);
        let suspicious = if is_fallback || strong_key {
            has_alpha
        } else {
            prefixed_value || (has_digit && has_alpha)
        };
        if end_quote >= 8 && compact && !is_config_key_name && suspicious {
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
                suggestion: Some(FIX_SECRET_LITERAL),
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
    // A `#[cfg(test)] mod tests {` block is conventionally at the end of a
    // Rust file. Brace counting is unreliable across complex real-world code
    // (string interpolation, macros, closures), so once such a module opens,
    // mark every remaining line as test code. This is conservative: it may
    // skip a few non-test lines after the module, but never misses a test.
    let mut cfg_test_module = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[test]") || trimmed.starts_with("#[cfg(test)]") {
            pending_attr = true;
        }
        if cfg_test_module || test_depth.is_some() {
            mask[i] = true;
        }
        if pending_attr && trimmed.starts_with("fn ") {
            test_depth = Some(depth + 1);
            mask[i] = true;
            pending_attr = false;
        }
        if pending_attr && (trimmed.starts_with("mod ") || trimmed.starts_with("pub mod ")) {
            cfg_test_module = true;
            mask[i] = true;
            pending_attr = false;
        }

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
                suggestion: None,
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

/// Signal rule: SQL built by string interpolation and passed to an
/// execution function. Parameterized queries are the safe pattern and must
/// never be flagged. This is the single most common injection class in
/// AI-generated data-access code.
fn sql_injection(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    let path_lower = file.path.to_lowercase();
    if ["test", "spec", "fixture", "mock"]
        .iter()
        .any(|seg| path_lower.contains(seg))
    {
        return None;
    }
    let in_test = test_line_mask(&file.lines);

    let exec_fns = [
        "execute", "executemany", "executescript", "raw(", "query(", "query_raw",
        "text(",
    ];

    // Strict signal: a single quoted string that contains BOTH an SQL verb
    // and an interpolation `{expr}`. Deliberately narrow — broader patterns
    // (`.format()`, `+` concatenation, multi-line windows) produced false
    // positives on real frameworks and UI code. Missing those forms is an
    // accepted tradeoff: a false positive on every scan is worse than a
    // missed variant.
    for (i, line) in file.lines.iter().enumerate() {
        if in_test[i] {
            continue;
        }
        let lower = line.to_lowercase();
        let trimmed = lower.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let has_exec = exec_fns.iter().any(|f| {
            let mut from = 0;
            while let Some(rel) = lower[from..].find(f) {
                let pos = from + rel;
                let boundary = pos == 0
                    || (!lower.as_bytes()[pos - 1].is_ascii_alphanumeric()
                        && lower.as_bytes()[pos - 1] != b'_');
                if boundary {
                    return true;
                }
                from = pos + 1;
            }
            false
        });
        if !has_exec {
            continue;
        }
        // Find f-string literals: f"..." or f'...'
        for marker in ["f\"", "f'"] {
            let mut from = 0;
            while from < lower.len() {
                let rel = match lower[from..].find(marker) {
                    Some(r) => r,
                    None => break,
                };
                let start = from + rel + 2;
                if start > lower.len() {
                    break;
                }
                let quote = marker.as_bytes()[1] as char;
                let tail = &lower[start..];
                let end = tail.find(quote).unwrap_or(tail.len());
                let literal = &tail[..end];
                let sql_verb = ["select ", "insert ", "update ", "delete ",
                    "drop ", "alter ", "truncate ", "where "];
                if literal.contains('{') && literal.contains('}')
                    && sql_verb.iter().any(|v| literal.contains(v))
                {
                    return Some(Finding {
                        id: "sql.injection",
                        verdict: Verdict::Fail,
                        evidence: format!("{}:{}", file.path, i + 1),
                        note: "SQL f-string built with interpolation passed to execute".into(),
                        suggestion: Some(FIX_SQL),
                    });
                }
                if start + end >= lower.len() {
                    break;
                }
                from = start + end + 1;
            }
        }
    }
    None
}

pub fn all_rules() -> Vec<Rule> {
    vec![
        Rule { id: "cors.wildcard", description: "No wildcard CORS origin", run: cors_wildcard },
        Rule { id: "secrets.hardcoded", description: "No hardcoded credential literals", run: secrets_hardcoded },
        Rule { id: "sql.injection", description: "No SQL built from string interpolation", run: sql_injection },
        Rule { id: "rate.limit", description: "Rate limiting not explicitly disabled", run: rate_limit_off },
        Rule { id: "error.tracking", description: "Error tracking not explicitly disabled", run: error_tracking_off },
        Rule { id: "webhook.idempotency", description: "Webhook idempotency not explicitly disabled", run: webhook_idempotency_off },
        Rule { id: "rls", description: "Row level security not explicitly disabled", run: rls_off },
        Rule { id: "backup", description: "Backup not explicitly disabled", run: backup_off },
        Rule { id: "eval.injection", description: "No eval/exec on request data", run: crate::rules_security::eval_injection },
        Rule { id: "unsafe.deserialization", description: "No unsafe deserialization of untrusted input", run: crate::rules_security::unsafe_deserialization },
        Rule { id: "weak.crypto", description: "No fast hash for password storage", run: crate::rules_security::weak_crypto },
        Rule { id: "jwt.algorithm", description: "JWT verification must not accept none", run: crate::rules_security::jwt_algorithm },
        Rule { id: "path.traversal", description: "No user-controlled value in filesystem path", run: crate::rules_security::path_traversal },
        Rule { id: "shell.injection", description: "No shell=True with interpolated command", run: crate::rules_security::shell_injection },
        Rule { id: "credential.logging", description: "No credentials written to logs", run: crate::rules_security::credential_logging },
    ]
}
