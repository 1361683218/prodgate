//! Cross-language adapter test. The SAME rule function must catch an opt-out
//! written in Python, TypeScript and Go, without editing the rule.

use prodgate::adapter::{Language, SourceFile};
use prodgate::check::{all_rules, Verdict};
use std::fs;
use std::path::Path;

fn rule(id: &str) -> prodgate::check::Rule {
    all_rules()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("rule {} not found", id))
}

#[test]
fn language_detection_covers_five_languages() {
    assert_eq!(Language::for_path("a.rs"), Language::Rust);
    assert_eq!(Language::for_path("a.py"), Language::Python);
    assert_eq!(Language::for_path("a.ts"), Language::TypeScript);
    assert_eq!(Language::for_path("a.js"), Language::JavaScript);
    assert_eq!(Language::for_path("a.go"), Language::Go);
    assert_eq!(Language::for_path("a.md"), Language::Unknown);
}

#[test]
fn same_rule_catches_opt_out_in_three_languages() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures-adapter/rls");
    let r = rule("rls");
    let ctx = Default::default();

    for sub in ["python", "typescript", "go"] {
        let dir = base.join(sub);
        let entry = fs::read_dir(&dir)
            .unwrap_or_else(|_| panic!("missing fixture dir {:?}", dir))
            .flatten()
            .next()
            .expect("fixture dir is empty");
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let file = SourceFile::new(name, fs::read_to_string(&path).unwrap());
        let finding = (r.run)(&file, &ctx).unwrap_or_else(|| {
            panic!("rule rls did not fire on {} ({:?})", sub, file.lang)
        });
        assert_eq!(finding.verdict, Verdict::Fail);
    }
}

#[test]
fn string_literal_is_not_a_declaration() {
    // Declaration rules read comment lines only. A string containing the
    // opt-out text must not be treated as an operator decision.
    let file = SourceFile::new(
        "app.rs".into(),
        "const NOTE: &str = \"rls=off is legacy\";\n".into(),
    );
    let r = rule("rls");
    let finding = (r.run)(&file, &Default::default());
    assert!(finding.is_none(), "string literal was mistaken for a declaration");
}
