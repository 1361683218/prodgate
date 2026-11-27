//! Feed-forward guidance: turn the rule library and its real case samples
//! into instructions an AI can follow WHILE writing code.
//!
//! The gate finds problems after the fact. This module exists so the same
//! knowledge can be loaded before generation — an AI that knows the bad
//! patterns up front does not need to be corrected later. Content is derived
//! from `all_rules()` and the validated case library, so prompt and gate
//! cannot drift apart.

use crate::cases;
use crate::check::all_rules;
use std::fs;
use std::path::Path;

fn first_file(dir: &Path) -> Option<String> {
    let entry = fs::read_dir(dir).ok()?.flatten().next()?;
    fs::read_to_string(entry.path()).ok()
}

fn trim_for_prompt(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(6)
        .map(|l| l.trim_end())
        .collect();
    lines.join("\n")
}

/// A single rule's guidance block, built from its metadata plus a real
/// good/bad case pair where available.
struct RuleBlock {
    id: &'static str,
    description: &'static str,
    suggestion: Option<&'static str>,
    bad: Option<String>,
    good: Option<String>,
    source: Option<String>,
}

fn collect_blocks(root: &Path) -> Vec<RuleBlock> {
    let results = cases::validate(root);
    let mut out = Vec::new();
    for rule in all_rules() {
        let case = results
            .iter()
            .find(|r| r.meta.rule == rule.id && r.valid());
        let (bad, good, source) = match case {
            Some(c) => {
                // case id is "<rule>/<case-id>" — derive the directory.
                let dir = root.join("cases").join(rule.id).join(
                    c.meta.id.split_once('/').map(|(_, id)| id).unwrap_or("c001"),
                );
                (
                    first_file(&dir.join("bad")).map(|s| trim_for_prompt(&s)),
                    first_file(&dir.join("good")).map(|s| trim_for_prompt(&s)),
                    if c.provenance.is_citable() {
                        Some(c.meta.source.clone())
                    } else {
                        None
                    },
                )
            }
            None => (None, None, None),
        };
        out.push(RuleBlock {
            id: rule.id,
            description: rule.description,
            suggestion: None,
            bad,
            good,
            source,
        });
    }
    out
}

fn suggestion_for(root: &Path, id: &str) -> Option<&'static str> {
    // Probe the rule on one of its own bad case samples to obtain the exact
    // suggestion text the gate would emit. Keeps prompt and gate in lockstep.
    let results = cases::validate(root);
    let case = results.iter().find(|r| r.meta.rule == id && r.valid())?;
    let dir = root.join("cases").join(id).join(
        case.meta.id.split_once('/').map(|(_, c)| c).unwrap_or("c001"),
    );
    let bad_dir = dir.join("bad");
    let entry = fs::read_dir(bad_dir).ok()?.flatten().next()?;
    let name = entry.path().file_name()?.to_string_lossy().to_string();
    let src = fs::read_to_string(entry.path()).ok()?;
    let file = crate::adapter::SourceFile::new(name, src);
    let rule = all_rules().into_iter().find(|r| r.id == id)?;
    let finding = (rule.run)(&file, &crate::check::RuleCtx { surface: "web" })?;
    finding.suggestion
}

/// Generate the full prompt text.
pub fn render(root: &Path, surface: &str) -> String {
    let mut out = String::new();
    out.push_str("# prodgate coding rules\n\n");
    out.push_str(
        "You are writing code that will be checked by prodgate. Follow these rules while generating code, \\\nnot after. Each rule lists a real bad pattern and its fix. Do not produce the BAD form.\n\n",
    );

    let blocks = collect_blocks(root);
    for (i, mut b) in blocks.into_iter().enumerate() {
        if !crate::gate::applies_to_surface(b.id, surface) {
            out.push_str(&format!(
                "## {}. {} — NOT APPLICABLE (this is a `{}` project)\n\n",
                i + 1, b.id, surface
            ));
            continue;
        }
        b.suggestion = suggestion_for(root, b.id);
        out.push_str(&format!("## {}. {}\n\n", i + 1, b.id));
        out.push_str(&format!("Goal: {}\n\n", b.description));
        if let Some(bad) = &b.bad {
            out.push_str("Bad (do not write this):\n```\n");
            out.push_str(bad);
            out.push_str("\n```\n\n");
        }
        if let Some(good) = &b.good {
            out.push_str("Good:\n```\n");
            out.push_str(good);
            out.push_str("\n```\n\n");
        }
        if let Some(fix) = b.suggestion {
            out.push_str(&format!("Fix: {}\n\n", fix));
        } else {
            out.push_str(
                "This is a declaration control: add the control shown in Good to web-facing code. \\\nIf you deliberately disable it, write `prodgate: <key>=off` as a comment so the decision is explicit. \\\nNever silently omit it.\n\n",
            );
        }
        if let Some(src) = &b.source {
            out.push_str(&format!("Real-world basis: {}\n\n", src));
        }
    }

    out.push_str(
        "## How to check your work\n\n\
         Run `prodgate gate <project-root>` after writing code. If a rule reports `FAIL`, apply the \\\nFix above and re-run. `UNCONFIRMED` means the gate cannot tell whether a control exists — state your \\\nintent explicitly in code comments using `prodgate: <key>=off` if you deliberately disabled it.\n",
    );
    out
}

/// Structured form for `--json` / MCP.
pub fn sections(root: &Path, surface: &str) -> Vec<crate::json::Json> {
    use crate::json::Json;
    let mut blocks = collect_blocks(root);
    for b in &mut blocks {
        b.suggestion = suggestion_for(root, b.id);
    }
    blocks
        .into_iter()
        .map(|b| {
            Json::Obj(vec![
                ("id".into(), Json::Str(b.id.into())),
                (
                    "applicable".into(),
                    Json::Bool(crate::gate::applies_to_surface(b.id, surface)),
                ),
                ("description".into(), Json::Str(b.description.into())),
                (
                    "suggestion".into(),
                    b.suggestion
                        .map(|s| Json::Str(s.to_string()))
                        .unwrap_or(Json::Null),
                ),
                (
                    "bad".into(),
                    b.bad.map(Json::Str).unwrap_or(Json::Null),
                ),
                (
                    "good".into(),
                    b.good.map(Json::Str).unwrap_or(Json::Null),
                ),
                (
                    "source".into(),
                    b.source.map(Json::Str).unwrap_or(Json::Null),
                ),
            ])
        })
        .collect()
}
