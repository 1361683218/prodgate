# 任务：为 prodgate 实现一个新命令

你是接手一个项目的 AI。**你没有之前的对话历史**——你所有的上下文都在这份文件和 `loop/progress.md` 里。这是刻意设计：状态在磁盘，不在模型记忆。

## 工作目录

`/Users/wujing/WorkBuddy/2026-11-25-14-05-11/prodgate`

先 `cd` 到那里。这是一个 Rust 项目，零外部依赖，离线可构建。

## 规则（必须遵守）

1. **一次只做一件事**：读 progress.md，找到第一个 `- [ ]` 任务，只完成它，然后退出。不要一次做多个任务。
2. **不许改裁判**：`loop/judge.sh` 和 `src/check.rs` 的规则定义部分不可修改。改裁判 = 作弊。
3. **必须自验**：完成代码后运行 `./loop/judge.sh .`，确认输出 `JUDGE: PASS`。失败就继续修，直到通过。
4. **记录进度**：完成后在 progress.md 里把该任务打勾 `- [x]`，并在"轮次记录"追加一行（做了什么、裁判结果）。
5. **提交**：`git add -A && git commit -m "..."` 每个任务一个 commit。
6. **完成信号**：只有当 progress.md 里**所有任务都是 `- [x]`** 且裁判 PASS 时，最后输出一行 `COMPLETE`。否则输出 `NEXT`。

## 风格约定（从项目现有代码推断，不要发明新风格）

- 每个功能模块是 `src/<name>.rs`，在 `src/lib.rs` 注册 `pub mod <name>;`
- CLI 命令在 `src/main.rs` 的 `valid` 列表 + JSON 分支 + 文本分支
- JSON 用 `crate::json::Json`，API 见 `src/json.rs`
- 测试在 `tests/<name>.rs`
- 所有文档数字不写死（会漂移），用测试守卫
- 中文注释解释"为什么"，代码保持简洁

## 当前任务清单

见 `loop/progress.md`。只读第一个未勾选项。

## 开始

1. `cd` 到工作目录
2. 读 `loop/progress.md`
3. 找到第一个 `- [ ]` 任务
4. 用 `Read`/`Grep`/`Glob` 探索相关代码（不要靠猜）
5. 实现 + 测试 + 裁判
6. 更新 progress.md + commit
7. 输出 `NEXT` 或 `COMPLETE`
