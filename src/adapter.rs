//! Language adapters. Rules operate on a normalized `SourceFile`, so the
//! same rule semantics work across languages without editing the rules.
//!
//! The real job here is not "which extensions exist" — it is deciding which
//! lines count as a *declaration*. A string literal containing `rls=off` is
//! not an opt-out; a comment line is. Signal rules still see every line.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Unknown,
}

impl Language {
    pub fn for_path(path: &str) -> Self {
        let ext = path
            .rsplit_once('.')
            .map(|(_, e)| e)
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "rs" => Language::Rust,
            "py" => Language::Python,
            "js" | "jsx" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "go" => Language::Go,
            _ => Language::Unknown,
        }
    }

    pub fn is_source(self) -> bool {
        self != Language::Unknown
    }

    fn comment_tokens(self) -> &'static [&'static str] {
        match self {
            Language::Python => &["#"],
            Language::Rust | Language::JavaScript | Language::TypeScript | Language::Go => &["//"],
            Language::Unknown => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub lang: Language,
    pub lines: Vec<String>,
}

impl SourceFile {
    pub fn new(path: String, src: String) -> Self {
        let lang = Language::for_path(&path);
        let lines = src.lines().map(str::to_string).collect();
        Self { path, lang, lines }
    }

    /// Declaration lines, with the comment marker removed. Declaration-style
    /// rules read only these; an opt-out buried in a string is not a decision.
    pub fn declaration_lines(&self) -> Vec<(usize, &str)> {
        let tokens = self.lang.comment_tokens();
        let mut out = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(tok) = tokens.iter().find(|t| trimmed.starts_with(**t)) {
                out.push((i + 1, trimmed[tok.len()..].trim()));
            }
        }
        out
    }
}
