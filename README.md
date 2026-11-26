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
    { "rule": "rls", "verdict": "FAIL", "evidence": "src/db.rs:1", "note": "..." }
  ]
}
```

`replay`、`cases`、`all` 的 JSON 也有同样的 `tool` / `command` / `passed` 三个顶层字段。脚本只读 `passed` 就行，不用解析文本。退出码仍然和 `passed` 一致：通过是 0，失败是 1。

扫别的项目时加上路径：`./target/debug/prodgate gate /path/to/project`。被扫的项目可以用 `prodgate.conf` 声明扫描哪些目录、属于什么形态：

```ini
roots=src,api
surface=web
```

`surface=web` 会启用 4 条只对 Web 服务有意义的规则。没有配置文件时用默认值（`src`、cli）。

Cargo workspace 这类源码分散在 `crates/*/src` 的项目，加一行 `recursive=true` 递归扫描。扫描到 0 个源码文件会报 FAIL（规则是 `config.no-sources`），不会给你一个假的 PASS。这个判定不能被 baseline 冻结。

## 它检查什么

| 规则 | 检测什么 | 适用 |
|---|---|---|
| `cors.wildcard` | 把 CORS origin 设为 `*` | web |
| `secrets.hardcoded` | 凭证键被赋了一个长字面量 | 全部 |
| `rate.limit` | 显式关闭限流 | web |
| `error.tracking` | 显式关闭错误追踪 | 全部 |
| `webhook.idempotency` | 显式关闭 webhook 幂等 | web |
| `rls` | 显式关闭行级安全 | web |
| `backup` | 显式关闭备份 | 全部 |

规则有两种写法，区别在它们看什么：

**信号型**只有两条，`cors.wildcard` 和 `secrets.hardcoded`。它们扫全部代码行。字符串里的 `*` 真的会被当成 header 值用，所以不能跳过。

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

四个工具，都是只读的：

| 工具 | 什么时候用 | 返回什么 |
|---|---|---|
| `prodgate_rules` | 写代码之前 | 列出 7 条规则，让 AI 提前知道要加哪些防护 |
| `prodgate_gate` | 写完代码之后 | 每条规则的 PASS / FAIL / NOT-ASSESSED，带行号 |
| `prodgate_replay` | 改过规则之后 | 检查规则还能不能区分好坏样本 |
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

案例库现在有 3 个案例，其中 2 个带可核验的上游 commit。贡献格式见 `CASES.md`。

## 已知限制

只做静态检查，不推断运行时行为。「这项防护到底实现得对不对」超出了它的能力范围。

它也不判断某行代码是不是 AI 写的。那种分类器在规范统一、用了 formatter 的仓库里误判率很高，不值得做。

现在仍然是小工具。7 条规则覆盖的是最常见、最容易漏的几项，不是完整的安全审计。

## 参与开发

CI 在每次 push 和 PR 上跑四步：编译、测试、clippy（`-D warnings`）、自举门禁（`all` + `cases`）。本地跑同样的检查：

```bash
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- all .
cargo run --locked -- cases .
```

改规则时，先改 fixture，再改规则，保证 `replay` 仍然能区分好坏样本。加案例时看 `CASES.md`。许可证是 MIT。

## 其他文档

- `BENCHMARK.md` — 在真实项目上的扫描结果，以及发现的局限
- `CASES.md` — 案例库的贡献格式
- `CHANGELOG.md` — 各轮迭代做过什么
