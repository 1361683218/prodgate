//! Additional signal rules for AI-generated security mistakes.
//!
//! Each rule is deliberately narrow. A false positive on every scan is far
//! worse than a missed variant: the gate must stay clean on real frameworks.
//! All rules skip test-shaped paths and follow the same SourceFile contract
//! as the core rules in `check.rs`.

use crate::adapter::SourceFile;
use crate::check::{Finding, RuleCtx, Verdict};

const FIX_EVAL: &str = "Never pass request data to eval/exec. Parse the value explicitly (e.g. JSON.parse, int()) or use a safe expression evaluator with a whitelist.";
const FIX_DESER: &str = "Never unpickle/deserialize untrusted input. Use a data-only format such as JSON with schema validation.";
const FIX_CRYPTO: &str = "Use a password KDF (argon2/bcrypt/scrypt) with a per-user salt. MD5/SHA1 are fast hashes and are not password hashing.";
const FIX_JWT: &str = "Pin the expected algorithm explicitly (e.g. algorithms=['HS256']) and never accept 'none'. Use the correct key type for that algorithm.";
const FIX_PATH: &str = "Resolve and validate the final path against an allowlisted base directory; reject '..' and absolute paths. Use the framework's safe file-sending helper.";
const FIX_SHELL: &str = "Avoid shell=True with interpolated input. Pass an argument list (subprocess.run([...], shell=False)) so values are never parsed as shell syntax.";
const FIX_LOG: &str = "Never log credentials, tokens or passwords — not even at debug level. Log a user id or a redacted marker instead.";

fn skip_path(file: &SourceFile) -> bool {
    let p = file.path.to_lowercase();
    ["test", "spec", "fixture", "mock", "example", "mutation", "rules_security"]
        .iter()
        .any(|seg| p.contains(seg))
}


/// Does `line` contain `needle` as a whole identifier (not a substring of a
/// longer name)? `print(` must not match `app_fingerprint(`.
fn has_call(line: &str, needle: &str) -> bool {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(needle) {
        let pos = from + rel;
        let before_ok = pos == 0 || {
            let c = bytes[pos - 1];
            !(c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
        };
        if before_ok { return true; }
        from = pos + 1;
    }
    false
}


/// Is there request/user input in the few lines above? AI-written handlers
/// commonly read the value on one line and use it on the next.
fn nearby_taint(lines: &[String], idx: usize) -> bool {
    let start = idx.saturating_sub(4);
    lines[start..=idx].iter().any(|l| {
        let u = l.to_lowercase();
        u.contains("request.") || u.contains("req.") || u.contains("req,")
            || u.contains("user_input") || u.contains("untrusted")
            || u.contains("form[") || u.contains("args.get")
            || u.contains("body.") || u.contains("cookies.get")
    })
}

/// Is there a suppression marker (NOSONAR / format-required comment) near
/// this line? Real framework code sometimes must use a legacy primitive for
/// wire-format compatibility; that is not a security decision.
fn suppressed(lines: &[String], idx: usize) -> bool {
    let start = idx.saturating_sub(3);
    lines[start..=idx].iter().any(|l| {
        let u = l.to_uppercase();
        u.contains("NOSONAR") || u.contains("REQUIRED BY THE")
            || u.contains("FILE FORMAT") || u.contains("PART OF THE")
    })
}

fn fail(id: &'static str, file: &SourceFile, line: usize, note: &str, fix: &'static str) -> Finding {
    Finding {
        id,
        verdict: Verdict::Fail,
        evidence: format!("{}:{}", file.path, line),
        note: note.into(),
        suggestion: Some(fix),
    }
}

/// eval/exec called with request/user-controlled data.
pub fn eval_injection(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    if skip_path(file) { return None; }
    // `eval` is a normal function name in Rust/Go; the injection risk is a
    // JavaScript/TypeScript runtime feature.
    if !matches!(file.lang, crate::adapter::Language::JavaScript
        | crate::adapter::Language::TypeScript) { return None; }
    for (i, line) in file.lines.iter().enumerate() {
        let low = line.to_lowercase();
        let t = low.trim_start();
        if t.starts_with("//") || t.starts_with('#') { continue; }
        if suppressed(&file.lines, i) { continue; }
        let dangerous = ["eval(", "exec(", "new function("];
        if !dangerous.iter().any(|d| has_call(&low, d)) { continue; }
        let tainted = ["req.", "request.", "input(", "params", "body", "query",
            "form", "args", "user_", "userinput", "untrusted"];
        if tainted.iter().any(|v| low.contains(v)) {
            return Some(fail("eval.injection", file, i + 1,
                "eval/exec called with request-controlled data", FIX_EVAL));
        }
    }
    None
}

/// Unsafe deserialization of external input.
pub fn unsafe_deserialization(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    if skip_path(file) { return None; }
    for (i, line) in file.lines.iter().enumerate() {
        let low = line.to_lowercase();
        let t = low.trim_start();
        if t.starts_with("//") || t.starts_with('#') { continue; }
        let loads = ["pickle.loads", "pickle.load", "yaml.load(", "marshal.loads",
            "unserialize("];
        if !loads.iter().any(|l| low.contains(l)) { continue; }
        // yaml.load with SafeLoader is fine; pickle of a local file is fine.
        if low.contains("safeloader") || low.contains("csafeloader") { continue; }
        let external = ["request", "cookie", "header", "body", "form", "param",
            "socket", "recv", "upload", "user_", "input("];
        if external.iter().any(|v| low.contains(v)) || nearby_taint(&file.lines, i) {
            return Some(fail("unsafe.deserialization", file, i + 1,
                "untrusted data passed to unsafe deserializer", FIX_DESER));
        }
    }
    None
}

/// Fast hash used for password storage.
pub fn weak_crypto(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    if skip_path(file) { return None; }
    for (i, line) in file.lines.iter().enumerate() {
        let low = line.to_lowercase();
        let t = low.trim_start();
        if t.starts_with("//") || t.starts_with('#') { continue; }
        if suppressed(&file.lines, i) { continue; }
        // A bare `md5(x)` call is a normal helper in Rust/Go code (file
        // formats, checksums). Only flag explicit crypto-library APIs, or
        // a clearly password-named hash construction.
        let explicit = ["hashlib.md5", "hashlib.sha1", "digest('md5')",
            "createhash('md5')", "md5(password", "sha1(password"];
        if !explicit.iter().any(|w| low.contains(w)) { continue; }
        // Only when it is password-related on this or the function signature.
        let context = file.lines.iter().take(i + 1).rev().take(4)
            .map(|s| s.to_lowercase()).collect::<Vec<_>>().join(" ");
        if context.contains("password") || low.contains("password") || low.contains("passwd") {
            return Some(fail("weak.crypto", file, i + 1,
                "fast hash used where a password KDF is required", FIX_CRYPTO));
        }
    }
    None
}

/// JWT verification accepting the `none` algorithm.
pub fn jwt_algorithm(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    if skip_path(file) { return None; }
    for (i, line) in file.lines.iter().enumerate() {
        let low = line.to_lowercase();
        let t = low.trim_start();
        if t.starts_with("//") || t.starts_with('#') { continue; }
        if !low.contains("jwt.verify") && !low.contains("jwt.decode") { continue; }
        if low.contains("'none'") || low.contains("\"none\"")
            || low.contains("algorithms: []") || low.contains("verify: false") {
            return Some(fail("jwt.algorithm", file, i + 1,
                "JWT verification accepts the none algorithm", FIX_JWT));
        }
    }
    None
}

/// File path built from user input without visible validation.
pub fn path_traversal(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    if skip_path(file) { return None; }
    for (i, line) in file.lines.iter().enumerate() {
        let low = line.to_lowercase();
        let t = low.trim_start();
        if t.starts_with("//") || t.starts_with('#') { continue; }
        let file_op = ["send_file(", "open(", "readfile(", "read_file(", "path.join(",
            "filepath.join("];
        if !file_op.iter().any(|f| low.contains(f)) { continue; }
        // Interpolation or concatenation of request data into a path.
        let interpolated = low.contains("f\"") || low.contains("f'") || low.contains("+ ");
        if !interpolated { continue; }
        let tainted_path = low.contains("request") || low.contains("req.")
            || low.contains("param") || low.contains("args") || low.contains("query")
            || nearby_taint(&file.lines, i);
        if !tainted_path { continue; }
        let context = file.lines.iter().take(i + 1).rev().take(3)
            .map(|s| s.to_lowercase()).collect::<Vec<_>>().join(" ");
        // A validation call nearby means the author handled it.
        if context.contains("resolve(") || low.contains("safe_join")
            || low.contains("secure_filename") || low.contains("startswith(base") {
            continue;
        }
        return Some(fail("path.traversal", file, i + 1,
            "user-controlled value interpolated into a filesystem path", FIX_PATH));
    }
    None
}

/// subprocess with shell=True and interpolated input.
pub fn shell_injection(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    if skip_path(file) { return None; }
    for (i, line) in file.lines.iter().enumerate() {
        let low = line.to_lowercase();
        let t = low.trim_start();
        if t.starts_with("//") || t.starts_with('#') { continue; }
        if !low.contains("shell=true") { continue; }
        let call = ["subprocess.call", "subprocess.run", "subprocess.popen",
            "os.system", "exec("];
        if !call.iter().any(|c| low.contains(c)) { continue; }
        // A literal command with no interpolation is not injection.
        let interp = low.contains("f\"") || low.contains("f'") || low.contains("+ ")
            || low.contains("% ") || low.contains("format(");
        if interp {
            return Some(fail("shell.injection", file, i + 1,
                "shell=True with an interpolated command", FIX_SHELL));
        }
    }
    None
}

/// Credentials written to logs.
pub fn credential_logging(file: &SourceFile, _ctx: &RuleCtx<'_>) -> Option<Finding> {
    if skip_path(file) { return None; }
    for (i, line) in file.lines.iter().enumerate() {
        let low = line.to_lowercase();
        let t = low.trim_start();
        if t.starts_with("//") || t.starts_with('#') { continue; }
        if suppressed(&file.lines, i) { continue; }
        let log_call = ["logger.debug", "logger.info", "logger.warn", "logger.error",
            "logging.debug", "logging.info", "logging.warning", "logging.error",
            "console.log", "console.debug"];
        if !log_call.iter().any(|l| has_call(&low, l)) { continue; }
        let secret = ["password", "passwd", "secret", "token", "api_key", "apikey",
            "credential", "authorization"];
        if !secret.iter().any(|s| low.contains(s)) { continue; }
        // A literal key name (e.g. logging.info("token missing")) is not a leak.
        if !(low.contains("f\"") || low.contains("f'") || low.contains("{")
            || low.contains("+ ")) {
            continue;
        }
        return Some(fail("credential.logging", file, i + 1,
            "credential value interpolated into a log statement", FIX_LOG));
    }
    None
}
