# 自动循环进度（唯一事实源）

> AI 每轮只读第一个 `- [ ]`，完成后打勾并 commit。所有项打勾 + 裁判 PASS 才 COMPLETE。

## 任务清单

- [x] 实现 `prodgate history` 命令：读取 `.prodgate/history.jsonl`，文本模式输出表格（时间、文件数、FAIL/PASS 数、passed），JSON 模式输出 `history_json` 结构。接入 `src/main.rs` 的 valid 列表与两个分支。在 `src/report.rs` 加 `history_json(root)` 函数（参考 gate_json 的风格）。
- [x] 为 history 命令加测试 `tests/history_cli.rs`：验证有历史/无历史两种情况下的 JSON 结构（用 `history::record` 生成测试数据，不手写 JSON 文件）。

## 轮次记录

- 第 1 轮：新增 report::history_json(root)（tool/command/passed/count/history，复用 history::to_json）；main.rs 接入 valid 列表、JSON 分支与文本表格（零依赖 UTC 时间格式化），history 只读不追加快照。裁判 JUDGE: PASS。
- 第 2 轮：新增 tests/history_cli.rs 两个测试——无历史时 JSON 结构（count=0、history 空数组）与有历史时（用 history::record 生成两条快照，校验各字段并确认读取不追加快照）。裁判 JUDGE: PASS。
- 第 3 轮：新增 history::prune(root)→usize（30 天窗口外快照删除，始终保留最新一条，破损行按 record 语义丢弃，幂等）；main.rs 接入 valid 列表与文本/JSON 分支，report.rs 加 prune_json(removed)；artifacts.tsv 声明产物 src/history.rs。裁判 JUDGE: PASS（七道门禁全绿）。

## 第二轮实验任务（合并后端到端，2026-11-28）

- [x] 给 history 模块加一个 `prune` 能力：在 `src/history.rs` 里新增 `pub fn prune(root: &Path) -> std::io::Result<usize>`，删除超过 30 天时间窗口的快照，保留最新一条，返回删除条数。CLI 加 `prodgate prune` 命令（文本+JSON 分支），只做这一件事。产物声明到 artifacts.tsv。
