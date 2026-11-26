//! Rule replay. Each rule owns a (good, bad) fixture pair. A rule that cannot
//! tell them apart is broken, regardless of how fluent its documentation is.

use crate::adapter::SourceFile;
use crate::check::{all_rules, RuleCtx, Verdict};
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct ReplayResult {
    pub rule: &'static str,
    pub good: Verdict,
    pub bad: Verdict,
}

impl ReplayResult {
    pub fn ok(&self) -> bool {
        self.good == Verdict::Pass && self.bad == Verdict::Fail
    }
}

fn first_file(dir: &Path) -> Option<SourceFile> {
    let entry = fs::read_dir(dir).ok()?.flatten().next()?;
    let path = entry.path();
    let name = path.file_name()?.to_string_lossy().to_string();
    let src = fs::read_to_string(&path).ok()?;
    Some(SourceFile::new(name, src))
}

pub fn replay(root: &Path) -> Vec<ReplayResult> {
    let base = root.join("tests").join("fixtures");
    let ctx = RuleCtx { surface: "web" };

    all_rules()
        .into_iter()
        .map(|rule| {
            let dir = base.join(rule.id);
            let good = first_file(&dir.join("good"));
            let bad = first_file(&dir.join("bad"));

            let verdict_of = |maybe: Option<SourceFile>| {
                maybe
                    .map(|file| {
                        (rule.run)(&file, &ctx)
                            .map(|f| f.verdict)
                            .unwrap_or(Verdict::Pass)
                    })
                    .unwrap_or(Verdict::NotAssessed)
            };

            ReplayResult {
                rule: rule.id,
                good: verdict_of(good),
                bad: verdict_of(bad),
            }
        })
        .collect()
}
