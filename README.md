# prodgate — 生产就绪门禁的最小内核

方向①「生产就绪 Gates」的第一个自举点。Rust，std-only，离线可编译。

## 为什么是这个内核

数据源：Legit.Show 对 1,723 个开源 AI/MCP/开发工具项目的扫描——
95% 未声明错误追踪、82% 未声明限流、66% webhook 无幂等、85% 无行级安全、
100% 存在宽开放 CORS。开发者工具类别 98% 有缺口。

这些是"资深工程师靠反射补上、模型几乎从不主动补"的不可见控制。
内核只做一件事：**把反射变成可执行的门禁**。

## 自举判据（已达成）

```
$ ./target/debug/prodgate all .
replay: PASS     # 7/7 规则都能区分 good / bad 样本
gate:   PASS     # 工具用自己扫自己的仓库，无 FAIL
exit=0
```

两个判据必须同时成立。只有 gate 过、回放没过，说明规则是假的；
只有回放过、gate 没过，说明规则 misconfigure。

## 三条不可妥协的设计

1. **规则自带判定证据。** 每条规则有一个 `(good, bad)` fixture 对。
   `replay` 命令反复跑它们。规则不能区分好坏 = BROKEN，无论文档写得多漂亮。
   这就是护城河的种子：规则库 = 可回放的判定案例库。

2. **NOT-ASSESSED 是一等公民。** 不适用的规则显式标注原因
   （`rule targets web surface; declared surface is 'cli'`），绝不静默跳过。
   沉默的跳过无法通过 review。

3. **规则不能命中自己。** `RATE_KEY` / `RATE_OFF` 拆成常量拼接，
   避免规则源码里出现它要搜的字面量，导致自举时自撞。

## 当前规则集（7 条，7/7 回放有效）

| 规则 | 检测 | surface |
|---|---|---|
| `cors.wildcard` | 通配 CORS origin | web |
| `secrets.hardcoded` | 硬编码凭证字面量 | 全适用 |
| `rate.limit` | 显式关闭限流 | web |
| `error.tracking` | 显式关闭错误追踪 | 全适用 |
| `webhook.idempotency` | 显式关闭 webhook 幂等 | web |
| `rls` | 显式关闭行级安全 | web |
| `backup` | 显式关闭备份 | 全适用 |

规则分两类写法：
- **信号型**（cors / secrets）：在源码里找真实的不安全字面量。
- **声明型**（其余 5 条）：找 `prodgate: <key>=off` 这样的显式 opt-out，
  而不是猜"有没有实现"。静态扫描无法推断运行时行为，所以只认声明。

## 迭代路线（每个 PR 必须维持两个自举判据）

**第 2 轮 — 补齐 7 项控制 ✅ 已完成**
已加 `error.tracking`、`webhook.idempotency`、`rls`、`backup`。
每条都附 good/bad fixture 对并在 gate 里声明适用 surface。
回放 7/7，自举 gate PASS。另做了负向测试：构造一个故意含全部 7 个
缺口的项目，门禁 7/7 命中并 exit 1——证明规则不是只会放行自己。

**第 3 轮 — 跨语言 ✅ 已完成**
新增 `src/adapter.rs`：`Language` 识别 + `SourceFile::declaration_lines()`。
规则签名从 `(path, src)` 换成 `SourceFile`，7 条规则一行语义没改。
覆盖 Rust / Python / JS / TS / Go。

关键区分：规则分"信号型"和"声明型"后，adapter 才真正有用——
- 信号型（cors / secrets）扫**全部行**，因为字符串里的 `*` 也会真被当 header 用
- 声明型（5 条 opt-out）只读**注释行**，字符串里的 `rls=off` 不是操作者的决定

验证：`tests/adapter.rs` 3 个测试——同一条 rls 规则不改一行，
在 Python / TypeScript / Go 三种语言的样本上都能命中；字符串里的
`rls=off` 不误报。多语言负向测试：py/ts/go 各一个 opt-out，门禁
3/3 命中并给出正确文件名行号，exit 1。`cargo test` 6/6，clippy 干净。

**第 4 轮 — baseline 棘轮 ✅ 已完成**
新增 `baseline` 子命令：把当前 FAIL 集按 `rule|evidence` fingerprint
冻结到 `.prodgate/baseline.txt`。gate 时匹配到的降级为 BASELINE
（报告但不阻塞），新增的仍 FAIL 阻塞。

这解决了老项目接入的冷启动问题：带着一身欠债也能用门禁，
只要不新增就好。重跑 `baseline` 会收缩到当前真实欠债数——
修一条就少一条，棘轮只能往一个方向转。

fingerprint 故意不含 note 文本：改写描述不算新增欠债。

验证：tests/baseline.rs 2 个测试（旧欠债不阻塞+新增阻塞、欠债消失后
baseline 收缩为 0）。端到端演示看到 rls 旧欠债标 BASELINE、新加的
backup 立刻 FAIL。`cargo test` 8/8，clippy 干净，自举 PASS。

**第 5 轮 — 案例库产品化 ✅ 已完成**
新增 `src/cases.rs` 和 `cases/` 案例库。每个案例是带 `case.toml`
来源元数据的 good/bad 目录对，`cases` 子命令验证来源可信度 + 回放。

已录入 2 个真实上游案例：
- `expressjs/cors@a14f3bca` — PR #39 的 wildcard origin bug，一行 diff
- `supabase/supabase@49bda396` — PR #48394 的 RLS footgun（建表忘了
  enable，任意登录用户可读写授权表）

验证器拒绝无来源案例、拒绝合成案例冒充真实（source_url 必须留空）。
贡献指南见 `CASES.md`。

**这轮最重要的收获**：真实案例逼出了两个合成 fixture 没覆盖的规则 bug——
真实代码把 origin key 和 wildcard 写在两行；修复后代码里
`options.origin === '*'` 是比较而非赋值，规则误报。两个都修了。
这就是案例库的价值：它对规则说真话，玩具样本做不到。

验证：`cargo test` 12/12、clippy 干净、cases 3 个 0 invalid（2 个 upstream）、
回放 PASS、自举 PASS。**五轮迭代全部完成，内核稳定。**

## 接入层

MCP 不是产品，是接入方式。内核 `run_gate` / `replay` 形态无关：
- **CLI ✅** — 退出码 0/1 直接挂 CI
- **MCP ✅** — stdio / JSON-RPC，让 AI 写代码时实时自查
- **git hook** — pre-commit 本地检查（未做）

MCP 与 CLI 共用同一个内核，没有任何检测逻辑在 MCP 层重写。

## MCP Server

编译后得到第二个二进制 `prodgate-mcp`。在 MCP 客户端配置里注册：

```json
{
  "mcpServers": {
    "prodgate": {
      "command": "/绝对路径/prodgate/target/debug/prodgate-mcp"
    }
  }
}
```

四个工具，全部只读：

| 工具 | 何时调用 | 作用 |
|---|---|---|
| `prodgate_rules` | **写代码之前** | 列出 7 条控制，前馈——让 agent 一开始就知道要加什么 |
| `prodgate_gate` | 写完代码之后 | 扫描项目，每条规则给 PASS/FAIL/NOT-ASSESSED + 证据行号 |
| `prodgate_replay` | 改过规则之后 | 验证规则仍能区分 good/bad 样本 |
| `prodgate_cases` | 加过案例之后 | 验证案例库来源与回放 |

**两个刻意的设计决定：**

1. **不暴露 `baseline`。** 冻结欠债是人的决定，不是 agent 的决定。
   让 agent 能调 baseline，等于给它一个把 FAIL 变成 PASS 的橡皮图章。
2. **不存在的路径返回错误，不返回 PASS。** `run_gate` 扫 0 个文件时会全 PASS；
   如果 agent 拼错路径就会误报成功。MCP 层在任何扫描前先验证 root 是存在的目录。

FAIL 时工具返回 `isError: true`，让 agent 明确知道需要修复；NOT-ASSESSED 不算错误。

## 用法

```bash
cargo build --offline                    # 同时产出 prodgate 和 prodgate-mcp
./target/debug/prodgate replay           # 规则有效性
./target/debug/prodgate gate             # 扫当前仓库（baseline 已应用）
./target/debug/prodgate baseline         # 冻结当前 FAIL 为已知欠债
./target/debug/prodgate cases            # 验证案例库来源与回放
./target/debug/prodgate all              # 回放+门禁，自举验证
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | ./target/debug/prodgate-mcp          # MCP 冒烟测试
```

扫别的项目：`./target/debug/prodgate gate /path/to/project`
（被扫项目需要 `prodgate.conf` 声明 roots 和 surface；缺失则用默认值）。

## 边界

- 只做静态信号检测，不推断运行时行为。不适用的规则标 NOT-ASSESSED。
- 不做"这行是不是 AI 写的"分类器——那是死路，见方向②的笔记。
- 当前是最小内核，不是成品。它的价值是证明"用自己验证自己"这条路径可走。
