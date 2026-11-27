# 增量机制调研报告：九个项目怎么做"少跑"

调研时间：2026-11-27。范围：工作区 4 个项目 + 外部 5 个知名开源项目。
所有结论都有具体配置证据；抓不到或规模太小的，明确标注，不硬找。

## 核心结论

**增量不是"跑得快"，是"少跑"。** 缓存（rust-cache/sccache）属于"跑得快"，九个项目里只有 tokio 和 deno 认真做了缓存，而且 deno 自己把它归为编译优化而非测试增量。真正的增量来自四种机制，见下面的范式表。

一个反直觉的发现：**GitHub 原生 `on.push.paths` 被成熟项目集体回避**。rust-analyzer、tokio、ripgrep、cargo、deno——五个都没有用路径触发器。原因是 merge queue 和必需检查（required checks）：被跳过的 workflow 会让必需检查悬空，PR 永远合不了。它们的替代方案是 job 级 `if:` 门控，产生 `skipped` 结果，配合 `conclusion` job 把 skipped 当成功处理。

---

## 四种增量范式

### A · 声明式依赖图 — 改动算出影响范围

| 项目 | 机制 | 声明位置 |
|---|---|---|
| craft-libs | `[package.metadata.craft] reads` 声明跨 crate 读的文件，自动算传递闭包 | **Cargo 包元数据**（随 crate 版本化） |
| deno | `tools/check_deno_core_changes.js` 列 `libs/core/**` 等目录前缀，`git diff` 命中才跑 core 测试 | CI 脚本常量 |
| rust-analyzer | `dorny/paths-filter` 声明 `editors/code/** → typescript job` | CI 配置 `filters:` |

**craft-libs 的优势**：映射下沉到包管理器层，可传递（A 读 B、B 读 C ⇒ 改 C 影响 A），改一处即可。
**deno/rust-analyzer 的局限**：扁平目录前缀匹配，无传递依赖推导；改依赖范围要同时改代码和 CI 两处。deno 的目录清单甚至在两个文件里重复。

 craft-libs 的证据（`crates/pdf/Cargo.toml:15`）：
```toml
[package.metadata.craft]
reads = [
    "crates/cms/profiles/craft-coated-fogra39.icc",
    "crates/fonts-build/tests/fixture/subset/Tiny-glyf.ttf",
]
```
deno 的证据（`tools/check_deno_core_changes.js`）：
```js
const DENO_CORE_PACKAGE_DIRS = ["libs/core", "libs/ops", "libs/serde_v8", ...];
const denoCoreChanged = changedFiles.some((file) =>
  DENO_CORE_PACKAGE_DIRS.some((dir) => file.startsWith(dir + "/")) ||
  file === "Cargo.lock" || file === "Cargo.toml");
```

### B · 拓扑闸门 — 便宜失败就不启动昂贵

| 项目 | 机制 |
|---|---|
| tokio | 4 个廉价 job（`clippy`/`fmt`/`docs`/`minrust`）汇总成 `basics`（内容 `exit 0`），~30 个昂贵 job 全部 `needs: basics`。红 PR 省下整个下游矩阵 |
| rust-analyzer | `cancel-if-matrix-failed` job：矩阵任一格失败，主动调 GitHub API 取消其余并行 job |
| craft-anything | 昂贵的 FreeBSD/Windows-ARM64 检查降到 nightly + paths 触发；便宜检查每次跑 |

tokio 的证据（`.github/workflows/ci.yml`）：
```yaml
basics:
  needs: [clippy, fmt, docs, minrust]
  steps: [{ run: exit 0 }]
# 其后几乎每个昂贵 job：
  needs: basics
```
rust-analyzer 的证据：
```yaml
cancel-if-matrix-failed:
  needs: rust
  if: always()
  # jq 检查任一非 success/skipped → curl POST /actions/runs/${RUN_ID}/cancel
```

### C · 维度切分 — 只跑代表性格

| 项目 | 机制 |
|---|---|
| deno | `test_crate × shard_index` 矩阵，`CI_SHARD_INDEX/TOTAL` 内部分片；release 测试 PR 上默认跳过，需 `ci-full` 标签 |
| tokio | `cargo hack test --each-feature`；feature powerset 用 `exclude` 矩阵裁剪 |
| cargo | `cargo test -p cargo --test testsuite -- fix::` 只跑 fix 前缀；基准只测一个（其余下载要 10 分钟） |
| ripgrep | cross 目标上跳过 PCRE2 测试（qemu 下每个集成测试跑两遍，运行时近翻倍） |

ripgrep 的证据（`.github/workflows/ci.yml` 注释）：
> "These tests should actually work, but they almost double the runtime. Every integration test spins up qemu ... so we run tests without PCRE2 (with cross)."

这是最贴合"少跑"精神的一类：**基于信号等价性剪枝**——一个代表性环境验证过就认定其他环境冗余。这是工程判断，不是形式化保证。

### D · 内容寻址 — 相同内容天然幂等

| 项目 | 机制 |
|---|---|
| ledger-rs | 证据 ID = `ev_<sha256(canonical_json)>`，排除 `summary`/`subject.name` 等展示字段；`save_evidence` 用 `if !path.exists()` 跳过重复写 |
| ledger-ui | 规模小干脆不做增量，UI 全量 `refresh()`，靠 ledger-rs 下层幂等 |
| prodgate | baseline fingerprint = `rule\|path\|fnv1a64(行内容)`，行号移动不失配——同源思路 |

ledger-rs 的证据（`src/evidence.rs:199`、`src/project.rs:122`）：
```rust
format!("ev_{}", sha256_hex(canonical_json(&v).as_bytes()))
// save_evidence:
if !path.exists() { write_atomic(&path, text.as_bytes())?; }
```

craft-anything 还有个**域级增量刷新**值得记：`crates/engine/tests/project_incremental_refresh.rs` 用完整文件字节摘要匹配才复用已验证证据，并用指针地址断言复用。ROADMAP 记录"仅靠 mtime/长度不够，必须完整字节摘要"——这正好呼应 ledger-rs 的内容哈希设计。

---

## 没有增量机制的项目

| 项目 | 原因 |
|---|---|
| ledger-rs | 单 crate 14 模块，无 CI、无 `.github/`。规模不值得 |
| ledger-ui | 单 crate，无 CI。靠下层 ledger-rs 幂等 |
| prodgate | 单 crate，但 baseline 与 history 用了内容寻址与 append-only 思路 |

诚实原则：规模小的项目不上增量是正确决策，不是缺陷。

---

## 缓存：属于"跑得快"，不属于"少跑"

- tokio：`Swatinem/rust-cache@v2`，带 macOS 的 `cache-bin` workaround
- deno：自研 `actions/cache`（cacheVersion 127），共享 `./target` + `mtime_cache` 恢复文件时间戳 + thinlto 缓存 700MB；main 每次强制存新缓存
- rust-analyzer / cargo / ripgrep / craft-anything：**刻意不缓存**或缓存不可见。rust-analyzer 的 rust-cache 被注释掉，全局 `CARGO_INCREMENTAL: 0`

craft-anything 的 `docs/pitfalls.md` 记了一个教训：`target/debug/incremental` 曾达 46GB，建议检查时 `CARGO_INCREMENTAL=0` 并定期删除。共享缓存清理一次删掉 11.6GiB。**增量的阴暗面是磁盘膨胀**——这是"少跑"之外的真实成本。

---

## 对 prodgate 的启示

已落地的：
- baseline 内容寻址（范式 D）
- history append-only（"只加新的，不重算过去"）
- 六道门禁统一在 `all`，CI 一步强制（范式 B 的简化版——小项目不需要拓扑闸门）

实测后的适用性判断（2026-11-27）：

| 范式 | 是否落地 | 判定依据 |
|---|---|---|
| A 声明式依赖图 | **不做** | 13 秒扫 3492 文件已够快；增量会引入"漏扫改动范围"的正确性风险，收益为负 |
| B 拓扑闸门 | **不做** | 实测六道关卡最慢 0.33s（replay），全流程 <0.5s。tokio 的 basics 闸门是为省掉 miri/asan 等 30 个昂贵 job，prodgate 没有昂贵关卡，分层是伪需求 |
| C 维度切分 | 已有 | mutation 按规则切片，15 条规则各自独立验证 |
| D 内容寻址 | 已落地 | baseline fingerprint 用内容哈希；history 追加式快照 |

调研的副产品：从 craft-libs 的"定时全量保缓存不过期"和 ledger 的内容生命周期得到启发，发现 history 原先只有 500 条上限没有时间维度（每天扫=500天，每秒扫=8分钟）。已加 30 天时间窗口 + 条数双裁剪，这是本次调研唯一真正落地到 prodgate 的改动。
- 范式 C：mutation 已经按规则切片，不需要矩阵分片。
- 缓存：无外部依赖是项目原则，不引入 rust-cache 类工具。

---

## 证据来源

本地项目：`reverse-engineering/{craft-libs,craft-anything,ledger-rs,ledger-ui}` 的 Cargo.toml、.github/workflows/*.yml、docs/pitfalls.md、src/*.rs。

外部项目（raw.githubusercontent.com 真实配置）：
- `rust-lang/rust-analyzer` — `.github/workflows/ci.yaml`
- `tokio-rs/tokio` — `.github/workflows/ci.yml`
- `BurntSushi/ripgrep` — `.github/workflows/ci.yml`
- `rust-lang/cargo` — `.github/workflows/main.yml`
- `denoland/deno` — `.github/workflows/ci.generated.yml`、`ci.ts`、`tools/check_deno_core_changes.js`

注：tokio 与 deno 的 CI 文件首次抓取被截断，对关键片段做了二次窄范围抓取补齐；未复查部分不做结论。
