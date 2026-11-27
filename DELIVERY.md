# prodgate — 交付摘要

> 一页纸讲清这个工具是什么、凭什么信它、边界在哪。
> 细节见 `README.md`（用法）、`AI-CODE.md`（实验）、`BENCHMARK.md`（基准）、`CASES.md`（案例库）、`CHANGELOG.md`（开发记录）。

## 一句话

prodgate 是一个**给 AI 写代码用的生产就绪门禁**：写代码前给它一份规范，写完扫一遍，发现问题直接给出可照做的修复指令。它不是安全审计工具。

## 它解决什么

一次真实观察：让强 AI 在不知道规则的情况下写 13 段代码，它稳定地漏掉同一类东西——裸 `cors()`、带 fallback 的密钥、f-string 拼 SQL。这不是疏忽，是"先让代码跑起来"压力下的默认行为。资深工程师靠肌肉记忆补上的防护，模型不会自动补。

prodgate 把这类"看不见的防护"变成两样东西：

1. **事前** — `prodgate prompt` 输出一份编码规范，每条规则带真实 commit 里的坏写法、好写法、修复指令，按项目形态裁剪（CLI 项目不会收到 CORS 示例）
2. **事后** — `prodgate gate` 扫描代码，每个 FAIL 都带一句可直接照做的 `fix:`，一次报出所有位置（不是修一个才冒出下一个）

两者共用同一套规则和修复文案，不会说两套话。第三个出口是 `prodgate report --out report.html`——生成一个自包含的 HTML 仪表板（内联 CSS/JS，无 CDN），浏览器直接打开就能看规则通过率、FAIL 详情和修复建议，拿去给人演示不用解释命令行输出。

15 条规则：10 条信号型（扫真实危险写法） + 5 条声明型（记录团队主动关闭某项防护的决定）。

## 凭什么信它

四层自我验证，统一在 `prodgate all` 一条命令里，CI 强制：

| 关卡 | 验证什么 | 当前结果 |
|---|---|---|
| replay | 每条规则能区分好坏样本 | 15/15 |
| mutation | 端到端能检出注入的坏样本 | 18/18 |
| cases | 案例库来源真实、回放正确 | 16/16，0 invalid |
| regression | 真实形态代码零误报（10 个快照） | 0 false positive |
| artifacts | 阶段产物在磁盘上（协议 §4 的机械执行） | 无 manifest 时 PASS |
| gate | 工具扫自己 | PASS |

另有 `efficacy` 实验（信息性，不阻塞）：13 个 AI 亲写代码样本触发 13 个 finding，涉及 10 种规则；prompt 对这 10 种规则覆盖 10/10，且教的好写法 10/10 不触发对应规则。

**真实项目实测**：`craft-libs` 3492 个源文件，13 秒扫描，零 FAIL；`ledger-rs`（web 形态）12 个文件，零 FAIL。

## 边界（必须说清）

- **不是完整 SAST**。XSS、CSRF、SSRF、依赖漏洞、命令注入的其他变体都没有。信号型规则为了压误报刻意收窄，会漏掉一些变体。
- **5 条声明型规则对 AI 代码无检出**。它们服务的是"团队显式记录取舍"，不是扫描场景。AI 不知道 `prodgate: key=off` 语法。
- **efficacy 只验证信息充分性**，不证明 AI 加载 prompt 后会真的照做——这个环境无法生成新代码来测行为改变。要回答那一半，需要另做对照实验。
- **baseline 是逃生舱，也是受控入口**。它把已知问题冻结为 BASELINE（非阻塞），MCP 刻意不暴露；CLI 有，但 `all` 会报冻结数量，超 20 条直接 FAIL——有 shell 的 AI 用它伪造全绿会被拦。

这些边界不是缺陷隐瞒，是工具的能力范围。承认它，工具才可信。

## 怎么用

```bash
cargo build --release

./target/release/prodgate prompt   > AI_CODING_RULES.md  # 写代码前喂给 AI
./target/release/prodgate gate /path/to/project          # 写完扫描
./target/release/prodgate report /path/to/project --out report.html  # 可视化报告
./target/release/prodgate all                             # 完整门禁（CI 用一条就够）
```

MCP 里对应 `prodgate_prompt`、`prodgate_gate`、`prodgate_efficacy`、`prodgate_regression` 等工具。推荐工作流：AI 开始写代码前调 `prompt`，写完调 `gate`，有 FAIL 按 `fix:` 修后重扫。

被扫项目用 `prodgate.conf` 声明形态：

```ini
roots=src,api
surface=web        # 或 cli，决定是否检查 CORS/限流/幂等/RLS
recursive=true     # Cargo workspace 等分散源码
```

## 项目现状

纯 Rust 实现（src + tests），离线可构建（无外部 crate 依赖），全套测试与 clippy 在 CI 强制。目前是一个**自洽、有测试守着、能用的工具**——不是一个正在扩张的项目。
