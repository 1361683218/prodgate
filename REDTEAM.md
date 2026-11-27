# 裁判红队审计：AI 能不能让 prodgate 说谎？

2026-11-27。用户要求"把好关"——prodgate 是 AI 自迭代体系的裁判，裁判本身必须抗攻击。
以下是实际执行的攻击实验，不是理论分析。结论基于实测，不是假设。

## 攻击面与结果

| # | 攻击手法 | 结果 | 防线 |
|---|---|---|---|
| 1 | `prodgate baseline` 冻结所有 FAIL | **攻击成立**（已修） | `all` 报冻结数，超 20 条 FAIL；审计头 |
| 2 | `roots` 指向不存在的目录 | 挡住 | no-sources 规则（第 8 轮修） |
| 3 | 删掉一条规则（`all_rules()` 少一项） | 挡住 | mutation + cases 失败 |
| 4 | 规则还在但永远返回 None | 挡住 | replay + mutation + cases 全失败 |
| 5 | 改 `surface` 让规则 NOT-ASSESSED | 部分挡住 | 只在规则有触发样本时暴露；见下 |
| 6 | 改测试/裁判文件本身 | **挡不住** | 靠 git 保护分支 + 人工 review，不是代码能解决的 |

### 攻击 1（已修）

实测：2 个 eval FAIL，AI 跑 `prodgate baseline` → gate PASS，代码没改。
MCP 不暴露 baseline，但有 shell 的 agent 能调 CLI。修复见 `tests/baseline_guard.rs`。

### 攻击 3/4 为什么被挡住

关键在**独立期望集**：
- `mutation.rs` 有自己的 18 个坏样本清单，不依赖 `all_rules()` 的数量
- `cases.rs` 要求 15 条规则每条都有案例
- `fixture.rs` 的 replay 虽然遍历 `all_rules()`（自指），但 mutation/cases 补上了

删规则 → cases 挂（案例找不到规则）；规则失效 → mutation 挂（坏样本检不出）。
前面 37 轮无意中建成了纵深防御——**没有任何单层是充分的，但交叉验证有效**。

### 攻击 5 的诚实说明

`surface=web` 项目改配置成 `cli`，Web 专属规则变 NOT-ASSESSED 不参与判定。这在语义上"合法"（配置声明了项目形态），但 AI 可以靠改配置逃避检查。当前没有 cross-check 配置与代码实际形态的机制。
不过：`all` 会显示 NOT-ASSESSED 数量，且 `regression`/`cases` 用固定 web surface，改配置骗不过它们。风险有限，未修。

### 攻击 6 是不可修的

如果 AI 能改 `tests/*.rs` 或 `.github/workflows/`，它就能重定义"通过"。这不是 prodgate 能防的——
它和 Ralph Loop 实验里 judge.sh 入库是同一类问题（见 `loop/EXPERIMENT.md`）。

**真正的防线在 prodgate 之外**：
- GitHub 保护分支，禁止直接 push 到 master
- CI 在干净 checkout 上跑（AI 的本地改动不能影响 CI 的裁判）
- 敏感文件（tests/、workflows/、loop/judge.sh）人工 review

一句话：**本地 `all` 是给 AI 的自我检查，不是安全边界；CI + git 权限才是**。

## 结论

prodgate 的六道门禁能扛住"改代码/改规则/改配置"这三类自指攻击，靠的是多源交叉验证。
唯一实测成立的 baseline 绕过已补。剩下的"改裁判"类攻击必须靠流程隔离。

给未来维护者的原则：
1. 任何新增的门禁都不要只依赖 `all_rules()` —— 要有独立期望集
2. baseline/prompt/report 这类"信息性"命令不能被 AI 用来改变门禁结果
3. 裁判代码的保护靠 git 和 CI，不靠提示词里写"请不要改"

## 防线现状审计（2026-11-27 14:44）

红队报告写了"防线在 CI + git 权限"，但实际查了仓库：**38 个 commit 全部直推 master，0 个 PR，没有分支保护**。也就是说这道防线目前是空的——攻击 6（改测试/裁判文件）现在完全可行。

代码层补了两样：

1. `.github/CODEOWNERS` — 声明裁判文件（tests/、check.rs、mutation.rs、cases.rs、fixture.rs、judge.sh、workflows/）需要 owner review
2. CI 加 `Flag judge-file changes` 步骤 — 提交改了裁判文件时在 CI 日志打 `::warning::`，可审计

**但这两样都是声明性的，不强制**。要真正生效，需要你在 GitHub 网页上做一次：

```
Settings → Branches → Add branch ruleset for master
  ✓ Require a pull request before merging
  ✓ Require approvals (1)
  ✓ Require review from Code Owners
```

做完之后：AI 直推 master 会被拒；改裁判文件的 PR 没有你 review 不能合。这才是攻击 6 的真防线。

在此之前，记住一句话：**本地 `all` 全绿不代表代码可信，只代表当前这套（可能被改过的）裁判说它没问题**。

## 分支保护启用后的 CI 修复（2026-11-27 14:54）

用户开启分支保护后，CI 连续失败 5 次（exit 101）。本地 macOS 干净 target 全过，但 Ubuntu runner 挂。排查过程本身是个教训：

1. 前两次失败 annotation 只说 "exit 101"，`::warning::` 显示我的 judge 步骤在工作
2. 拆成独立命名步骤 → CI "通过"了，但那是**假绿灯**：`cmd | tee` 返回 tee 的 0，吞掉了真实失败
3. 加 `set -o pipefail` → 真失败重现
4. 让 CI 自报失败输出（`::error::` annotation）→ 定位到 `Full gate` 步骤
5. 失败日志第一行揭示根因：**`cargo run` 有两个二进制（prodgate / prodgate-mcp），CI 的 cargo 要求显式 `--bin`**
6. 加 `--bin prodgate` → CI 全绿

**两个额外教训**：
- 本地能跑 ≠ CI 能跑。本地 cargo 默默选了默认二进制，CI 版本更严格
- `| tee` 不加 pipefail 是假绿灯制造机——这正是本项目存在的意义，结果在自己 CI 上栽了

现在状态：分支保护已开、CODEOWNERS 已声明、CI 全绿、judge 文件变更会告警。红队报告里的攻击 6 防线从"空"变成"实"。

## 第七道门禁自身的边界（2026-11-28 端到端实验发现）

用合并后的完整体系真实跑了一轮 Ralph Loop（全新上下文 AI + artifacts.tsv + 七道门禁），跑通后发现一个语义漏洞：

**artifact 门禁验证"文件存在且非空"，不验证"本阶段真的产出了它"。** AI 可以零产出、把 `required` 产物指向任意已存在文件（README.md、prodgate.conf）拿到 artifacts PASS。实测成立。

为什么在完整体系里危害有限：业务门禁（gate）独立运行——AI 该修的 bug 没修，gate 就 FAIL，`all` 整体 FAIL。假产物声明必须配合"业务门禁本来就能过"才有意义，那通常意味着没什么可攻击的。

但纯产出类阶段（写文档、生成资源）没有 gate 验证，假声明是真问题。

**已加固**：禁止把裁判文件（tests/、.github/workflows/、check.rs、mutation.rs 等、artifacts.tsv 自身）声明为产物——这些文件存在且非空，但作为产物是纯粹的造假。

**仍未解决（诚实标注）**：无法在纯文件层面证明"这个文件是本轮写的"。真正的保证靠 git diff + CODEOWNERS review（产物出现在最近 commit 的变更里）。这是流程层的防线，不是代码层的。
