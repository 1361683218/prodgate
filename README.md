# prodgate

检查代码库里那些容易被漏掉的生产防护：限流、错误追踪、webhook 幂等、行级安全、通配符 CORS、硬编码凭证、备份声明。

它是一个 Rust 命令行工具，std-only，离线可编译。也可以作为 MCP server 运行，让 AI 写完代码后立刻自查。

## 解决什么问题

举个例子。让 AI 写一个支付 webhook，它交出的代码能跑、类型检查过、单测也过，但不会主动加幂等键。支付网关重试一次，用户就被扣两次款。同样地，它通常不会加错误追踪，也不会加限流。

这些不算 bug。lint 不管它们，单测抓不到它们，人工 review 在代码量翻倍之后也很难逐条核对。

prodgate 把这几项做成一份可执行的清单。它扫源码，看两项中的哪一项成立：防护被显式关掉了，或者项目声明这项不适用。扫描结果带文件名和行号，退出码 0 或 1，可以直接挂在 CI 上。

## 快速开始

```bash
cargo build --offline
./target/debug/prodgate all .
```

`all` 做两件事：先回放规则（检查每条规则还能不能区分好坏样本），再扫描当前仓库（`prodgate/` 自己也要过这一关）。两个都通过，退出码是 0。

单独运行的命令：

```bash
./target/debug/prodgate replay           # 检查规则本身有没有坏
./target/debug/prodgate gate             # 扫描当前仓库
./target/debug/prodgate cases            # 检查案例库
./target/debug/prodgate mutate           # 注入已知坏样本，验证规则端到端能检出
./target/debug/prodgate baseline         # 把当前 FAIL 记为已知问题
./target/debug/prodgate --help           # 所有命令和参数
./target/debug/prodgate --version        # 版本号
```

任何命令加 `--json` 会输出结构化报告，方便脚本和工具读取：

```bash
./target/debug/prodgate gate --json .
```

```json
{
  "tool": "prodgate",
  "command": "gate",
  "passed": true,
  "files_scanned": 10,
  "known_issues": 0,
  "findings": [
    { "rule": "rls", "verdict": "FAIL", "evidence": "src/db.rs:1", "note": "...", "suggestion": "..." }
  ]
}
```

`verdict` 有六种，区分得很细：

| verdict | 含义 | 阻塞构建 |
|---|---|---|
| `PASS` | 信号型规则扫了代码，没发现危险写法 | 否 |
| `FAIL` | 发现问题 | **是** |
| `BASELINE` | 已知问题，记录在 baseline 里 | 否 |
| `UNCONFIRMED` | 声明型规则没看到 opt-out 标记——**不代表防护到位**，只代表没人声明 | 否 |
| `NOT-ASSESSED` | 规则不适用于当前 surface（如 Web 规则对 CLI 项目） | 否 |
| `config.no-sources` | 扫描到 0 个源码文件，配置错误 | **是** |

`UNCONFIRMED` 是刻意设计的：静态扫描推断不了运行时，没人写 `prodgate: rate_limit=off` 不等于限流真的存在。把它报成 PASS 是假绿灯。

每个 FAIL 都带 `suggestion`——一句可以直接照做的修复指令。CLI 显示为 `fix:`，JSON 和 MCP 里是同文字段。这样 AI 拿到报告就知道怎么改，不用猜规则想要什么。UNCONFIRMED 和 PASS 没有建议（它们不需要动作）。

`replay`、`cases`、`all` 的 JSON 也有同样的 `tool` / `command` / `passed` 三个顶层字段。脚本只读 `passed` 就行，不用解析文本。退出码仍然和 `passed` 一致：通过是 0，失败是 1。

扫别的项目时加上路径：`./target/debug/prodgate gate /path/to/project`。被扫的项目可以用 `prodgate.conf` 声明扫描哪些目录、属于什么形态：

```ini
roots=src,api
surface=web
```

`surface=web` 会启用 5 条只对 Web 服务有意义的规则。没有配置文件时用默认值（`src`、cli）。

Cargo workspace 这类源码分散在 `crates/*/src` 的项目，加一行 `recursive=true` 递归扫描。扫描到 0 个源码文件会报 FAIL（规则是 `config.no-sources`），不会给你一个假的 PASS。这个判定不能被 baseline 冻结。

## 事前预防：给 AI 的编码规范

`gate` 是事后检查。要让 AI 在写代码时就别犯错，先加载规范：

```bash
./target/debug/prodgate prompt        # 完整编码指南（Markdown）
./target/debug/prodgate prompt --json # 结构化版本，供工具消费
```

输出从**规则库 + 通过验证的案例库**生成，每条规则带真实 commit 里的坏写法、好写法、修复指令和来源。规则和案例库变了，提示词自动跟着变——不会出现"文档说一套、gate 查另一套"。

`prompt` 会读目标项目的 `prodgate.conf`，按 `surface` 裁剪：CLI 项目不会收到 CORS/RLS 这些完整示例，只看到一行 "NOT APPLICABLE"。这样 AI 的注意力留在真正相关的控制上。判定逻辑和 gate 共用同一个函数——被 gate 标为 NOT-ASSESSED 的规则，绝不会在 prompt 里被当成可执行建议。

MCP 里对应 `prodgate_prompt` tool（可选 `root` 参数，默认 cli）。推荐的工作流：AI 开始写代码前调用它，写完调用 `prodgate_gate` 验证。

## 它检查什么

| 规则 | 检测什么 | 适用 |
|---|---|---|
| `cors.wildcard` | 把 CORS origin 设为 `*` | web |
| `secrets.hardcoded` | 凭证键被赋了长字面量，或连接串里带明文密码 | 全部 |
| `sql.injection` | SQL 用 f-string 插值拼出后传给 execute | 全部 |
| `rate.limit` | 显式关闭限流 | web |
| `error.tracking` | 显式关闭错误追踪 | 全部 |
| `webhook.idempotency` | 显式关闭 webhook 幂等 | web |
| `rls` | 显式关闭行级安全 | web |
| `backup` | 显式关闭备份 | 全部 |
| `eval.injection` | eval/exec 处理请求数据 | 全部 |
| `unsafe.deserialization` | 反序列化不可信输入 | 全部 |
| `weak.crypto` | 用 MD5/SHA1 存密码 | 全部 |
| `jwt.algorithm` | JWT 校验接受 none 算法 | 全部 |
| `path.traversal` | 用户输入拼进文件路径 | 全部 |
| `shell.injection` | shell=True 拼插值命令 | 全部 |
| `credential.logging` | 凭证写进日志 | 全部 |

规则有两种写法，区别在它们看什么：

**信号型**有 10 条：`cors.wildcard`、`secrets.hardcoded`、`sql.injection`，以及 7 条 AI 高频安全规则（`eval.injection`、`unsafe.deserialization`、`weak.crypto`、`jwt.algorithm`、`path.traversal`、`shell.injection`、`credential.logging`）。它们扫全部代码行，发现危险写法就报 FAIL 并给出修复建议。

另外五条是**声明型**，只在注释里找 `prodgate: <key>=off`。这里有个取舍：静态扫描没法判断一个服务在运行时到底有没有限流，所以 prodgate 不去猜，只认人明确写下的关闭声明。代码字符串里的 `rls=off` 不算，那不是决定。

不适用的规则不会被静默跳过。比如扫一个 CLI 项目时，4 条 Web 规则会显示 `NOT-ASSESSED`，并写明原因。这样看报告的人知道哪些没查，不会以为是查过没问题。

支持 Rust、Python、JavaScript、TypeScript、Go。加语言只需要改 `src/adapter.rs`，规则本身不用动。

## 接入 CI

`gate` 和 `all` 在失败时返回退出码 1。直接放到流水线的检查步骤里就行。

老项目如果已经有一堆已知问题，先跑一次 `baseline`：

```bash
./target/debug/prodgate baseline
```

它会把当前的 FAIL 按 `rule|evidence` 写进 `.prodgate/baseline.txt`。之后再跑 `gate`，这些旧问题显示为 `BASELINE`，报告但不阻塞；新出现的问题仍然是 FAIL，会拦住构建。重跑一次 `baseline` 会把名单收缩到当前真实问题，所以修一条就少一条。

## 接入 AI（MCP）

编译后会得到第二个二进制 `prodgate-mcp`。在 MCP 客户端里注册：

```json
{
  "mcpServers": {
    "prodgate": {
      "command": "/绝对路径/prodgate/target/debug/prodgate-mcp"
    }
  }
}
```

六个工具，都是只读的：

| 工具 | 什么时候用 | 返回什么 |
|---|---|---|
| `prodgate_rules` | 写代码之前 | 列出 15 条规则，让 AI 提前知道要加哪些防护 |
| `prodgate_prompt` | 开始写代码之前 | 完整的编码规范：每条规则带真实好坏样本和修复指令，按 surface 裁剪 |
| `prodgate_gate` | 写完代码之后 | 每条规则的 PASS / FAIL / NOT-ASSESSED，带行号 |
| `prodgate_replay` | 改过规则之后 | 检查规则还能不能区分好坏样本 |
| `prodgate_mutate` | 改过规则之后 | 把真实坏样本注入临时项目，验证端到端能检出 |
| `prodgate_cases` | 加过案例之后 | 检查案例的来源和回放结果 |

`baseline` 没有做成工具。冻结已知问题需要人来决定，不能让 AI 自己把 FAIL 变成 PASS。

如果传入的项目路径不存在，工具返回错误，不会返回 PASS。这点很重要：`run_gate` 扫到 0 个文件时本来会全 PASS，AI 拼错路径就会收到一个假的成功信号。

FAIL 时工具设置 `isError: true`，AI 能据此判断需要修复。NOT-ASSESSED 不算错误。

冒烟测试：

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | ./target/debug/prodgate-mcp
```

## 为什么每条规则都带两个例子

每条规则都配了一对 good/bad 样本。改完规则跑 `replay`，能分出好坏才算数。分不出来就报 BROKEN，不管文档写得多漂亮。

这不是形式主义。做第 5 轮时，我从 `expressjs/cors` 和 `supabase/supabase` 的真实 PR 里各取了一个案例，结果暴露出两个自己写的玩具样本从没覆盖的问题：真实代码把 CORS 的 key 和 wildcard 写在两行，原来的规则要求同一行；修复后的代码里 `options.origin === '*'` 是比较而不是赋值，规则误报了。两个都改掉了。

案例库覆盖全部 15 条规则，共 16 个案例：9 个可溯源（4 upstream + 5 anchored，真 commit 支撑）和 7 个 synthetic（来自 AI 实测样本，明确标注不冒充真实 commit）。全部 0 invalid。贡献格式见 `CASES.md`。

## 端到端验证（mutate）

`replay` 有个盲区：它直接调用规则函数，跳过配置加载、文件收集、语言适配这些步骤。规则本身正确，但项目根本没扫到那个文件，replay 照样通过。

`mutate` 补上这一层。它把已知坏样本放进临时项目，保留真实框架的目录结构（`lib/express.js`、`src/flask/app.py`、`gin.go`），然后跑完整的 gate 流程，检查对应规则是否报 FAIL。

```bash
./target/debug/prodgate mutate
```

```
SAMPLE                                 RULE                 DETECTED STATUS
express setHeader wildcard             cors.wildcard        yes      OK
flask headers assignment wildcard      cors.wildcard        yes      OK
gin Header call wildcard               cors.wildcard        yes      OK
expressjs/cors PR #39 two-line         cors.wildcard        yes      OK
hardcoded api key in production        secrets.hardcoded    yes      OK
hardcoded env fallback (securevault)   secrets.hardcoded    yes      OK
hardcoded fallback with custom key name secrets.hardcoded    yes      OK
AI bare cors() default                 cors.wildcard        yes      OK
AI placeholder webhook secret          secrets.hardcoded    yes      OK
explicit rls opt-out                   rls                  yes      OK

mutation: 10/10 detected — PASS
```

这 6 个样本来自真实项目和真实 PR。第 9 轮曾把 Express 的 `setHeader('Access-Control-Allow-Origin', '*')` 注入真实项目，发现规则漏检——`cors.wildcard` 当时只认 `:` 和 `=`，不认函数调用的逗号传参。这类问题 replay 永远发现不了。

`all` 命令同时跑 replay、mutate 和 gate，三项都要通过。MCP 也暴露了 `prodgate_mutate` 工具，AI 改完规则可以直接自查。

## 已知限制

只做静态检查，不推断运行时行为。「这项防护到底实现得对不对」超出了它的能力范围。

它也不判断某行代码是不是 AI 写的。那种分类器在规范统一、用了 formatter 的仓库里误判率很高，不值得做。

现在 15 条规则：10 条信号型（扫真实危险写法）+ 5 条声明型（记录团队取舍）。覆盖 AI 最容易漏的常见风险，但**不是完整的安全审计**——XSS、CSRF、SSRF、命令注入的其他变体、依赖漏洞都没有。信号型规则为了压误报刻意收窄，会漏掉一些变体。

## 参与开发

CI 在每次 push 和 PR 上跑四步：编译、测试、clippy（`-D warnings`）、自举门禁（`all` + `cases`）。本地跑同样的检查：

```bash
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- all .
cargo run --locked -- cases .
```

改规则时，先改 fixture，再改规则，保证 `replay` 仍然能区分好坏样本。还要跑 `mutate`——只过 replay 不过 mutation，说明规则在自己的玩具样本上有效，但扫不到真实代码。加案例时看 `CASES.md`。许可证是 MIT。

## 其他文档

- `AI-CODE.md` — 强 AI 写的代码会不会犯这些错：实测数据与结论
- `BENCHMARK.md` — 在真实项目上的扫描结果，以及发现的局限
- `CASES.md` — 案例库的贡献格式
- `CHANGELOG.md` — 各轮迭代做过什么
