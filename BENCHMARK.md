# 真实项目扫描基准

这份记录回答一个问题：prodgate 在真实代码上到底有没有用。之前它只扫过自己的玩具样本，那些样本是自己写的，规则对它们负责不等于对真实代码负责。

## 扫描结果

用工作区现成的四个 Rust 项目做基准。每个项目临时写入 `prodgate.conf`（扫完删除，不改项目源码），使用递归模式覆盖 Cargo workspace 布局。

| 项目 | 源码文件 | 结果 | FAIL |
|---|---|---|---|
| craft-anything | 829 | PASS | 0 |
| craft-libs | 3,490 | PASS | 0 |
| ledger-rs | 12 | PASS | 0 |
| ledger-ui | 2 | PASS | 0 |

合计 4,333 个文件，零 FAIL。四个项目的 surface 都是 cli，所以 4 条 Web 规则全部报告 NOT-ASSESSED，不参与判定。

命令形态：

```ini
roots=crates          # 或 src
surface=cli
recursive=true
```

## 这轮抓出的真问题

真实项目暴露了三个玩具样本从没覆盖的缺陷。前两个是工具本身的 bug，第三个是规则的误报。

### 1. 扫 0 个文件却报 PASS

`craft-anything` 是 Cargo workspace，源码在 `crates/*/src`，而默认 `roots=src` 在根目录不存在。结果是 0 个文件被扫描，但工具报告 PASS——一个彻底的假绿灯。

修法：扫到 0 个源码文件时，产出合成的 `config.no-sources` FAIL。这个 finding 不能被 baseline 冻结，因为配置错误永远不该被当成"已知问题"。

### 2. 递归模式缺失

就算发现了问题，用户也没法扫 workspace 布局。加了 `recursive=true` 配置，递归遍历时跳过 target、node_modules、.git 等目录。开启后 craft-anything 从 0 个文件变成 829 个。

### 3. secrets 规则连续四类误报

这是最值得记的一段。规则原本的逻辑是"某行包含凭证关键词 + 等号右侧是长字符串"。在真实项目里连续误报：

| 误报 | 来源 | 原因 |
|---|---|---|
| `let window_only = "...2048 tokens."` | craft-anything `agent.rs:2302` | 整行扫描，右侧文本里有 "tokens" |
| `let secret = "my secret/value"` | craft-anything `standin/tests.rs:264` | 键名像凭证，值是测试占位符 |
| `let secret = "privatevalue"` | craft-anything `regression_har_metadata.rs:6` | 纯字母长字符串被当成 key |
| `let secret = "[2001:DB8::BEEF]"` | craft-anything `regression_har_normalized_metadata.rs:12` | IPv6 测试常量 |
| `let token = "0123456789abcdef..."` | craft-libs `server.rs:880` | `#[test]` 函数内联在生产文件里 |
| doc 注释 `?a=1&amp;b=2` | craft-libs `html/src/lib.rs:23` | URL 查询串里的 `=` 被当成赋值 |

我连续加了五轮启发式（值不含空格、必须字母数字混合、已知前缀、跳过 test 路径、跳过 `#[test]` 函数体），每修一个就冒出下一个。到第五次我停下来做了判断：**继续加规则是打地鼠，不是解决问题。**

最终的三项结构性修复：

1. **只把赋值号左侧当键名**，右侧文本里的词不算。
2. **只认字符串字面量之外的 `=`**，URL 查询串和文档注释里的等号不算。
3. **测试代码整体跳过**——既包括 test 形状的路径，也包括 `#[test]` / `#[cfg(test)]` 函数体（用花括号深度追踪）。Rust 允许测试内联在生产文件里，只看路径挡不住。

修完后四个项目 4,333 个文件全部 clean，而生产代码里的真凭证仍能抓到。这七类误报都写进了 `tests/real_world.rs` 做回归测试。

## 诚实的局限

**声明型规则在真实项目上基本不触发。** 5 条声明型规则找的是 `prodgate: key=off` 这样的显式关闭标记，真实项目不会写这种标记。所以它们报 PASS 主要是因为"没看见信号"，不是因为"确认防护到位"。

这不是 bug，是设计取舍：静态扫描推断不了运行时行为，prodgate 不假装能推断。但这意味着一个只靠声明型规则的项目，实际检出能力很弱。目前真正有检出力的是十条信号型规则：cors.wildcard、secrets.hardcoded、sql.injection，加上 7 条 AI 高频安全规则（eval.injection、unsafe.deserialization、weak.crypto、jwt.algorithm、path.traversal、shell.injection、credential.logging）。这十条扫真实代码里的危险写法，在 7 个真实项目（4 个 Rust + express/flask/gin）上零误报。

**信号型规则刻意牺牲了召回率。** 为了压掉误报，规则收得很窄——secrets 要求值紧凑且含字母数字混合（或带已知前缀），eval 只适用于 JS/TS。一些真实但不合这个形状的凭证会漏掉。补召回应该靠案例库积累真实样本，而不是继续放宽规则——那会重新引入误报。

**基准规模还小。** 四个项目都是 Rust，且都是同一个作者/生态。下一步应该扫不同语言、不同形态（尤其 surface=web）的项目，验证那 4 条 Web 规则。

## 怎么复现

```bash
cargo build --offline
cargo test --offline                    # 全套测试，含真实世界回归
cargo run --offline -- gate --json <项目路径>
```

被扫项目需要临时放一个 `prodgate.conf` 声明 roots / surface / recursive。基准脚本没有写成命令，因为不同项目的布局判断仍需人工确认——把这个判断自动化本身就是另一轮工作。

## 第二轮：多语言 Web 项目

第一轮全是 cli surface 的 Rust 项目，4 条 Web 规则没被评估过。这轮浅克隆三个真实 Web 框架，用 `surface=web` 扫描。

| 项目 | 语言 | roots | 文件数 | 结果 |
|---|---|---|---|---|
| expressjs/express | JavaScript | lib | 6 | PASS |
| pallets/flask | Python | src | 24 | PASS |
| gin-gonic/gin | Go | .（递归） | 99 | PASS |

三个核心库都没有 CORS 实现（CORS 是独立中间件），所以"无信号"是正确结果。4 条 Web 规则在这次扫描中确实被评估了（不是 NOT-ASSESSED），只是没检出东西。

### 受控注入实验：找出真漏检

扫描真实项目只能验证"不误报"，验证不了"能检出"。我做了受控实验：往真实项目里注入已知的危险代码，看规则抓不抓得到。

第一次注入 Express 的典型写法：

```js
res.setHeader('Access-Control-Allow-Origin', '*');
```

**规则没报。** 这是个真 bug，玩具样本从没覆盖。根因是规则只认 `origin: '*'` 或 `origin = '*'` 这种赋值，而真实代码是函数调用传参——origin 名和通配符之间是逗号，没有 `=` 或 `:`。

这和上一轮 secrets 的误报是同一类结构问题：**规则对"值怎么和键关联"的假设太窄**。上轮是"扫整行找等号"，这轮是"只认两种运算符"。

修法不再是列举分隔符，而是换个判断维度：找带引号的 `'*'` 作为值出现（排除 `=== '*'` 这类比较），并要求 origin 关键词在同一行或相邻代码行。这样 `origin: '*'`、`origin = '*'`、`setHeader('...Origin', '*')`、`headers['...Origin'] = '*'` 全都能覆盖。

修完重做实验：

| 注入目标 | 真实写法 | 结果 |
|---|---|---|
| express/lib/express.js | `res.setHeader('Access-Control-Allow-Origin', '*')` | FAIL `lib/express.js:85` |
| flask/src/flask/app.py | `response.headers['Access-Control-Allow-Origin'] = '*'` | FAIL `src/flask/app.py:1690` |
| gin/gin.go | `c.Header("Access-Control-Allow-Origin", "*")` | FAIL `gin.go:863` |

三种语言的真实写法全部检出，行号准确。恢复原文件后全部回到 PASS。这也是跨语言 adapter 第一次在正向检出上得到验证（之前只验证过 opt-out 声明的识别）。

新增 3 个回归测试到 `tests/real_world.rs`：函数参数形式、比较不算配置、Python/Go 写法。全套 37 个测试通过。

## 两轮基准的方法论收获

**只扫真实项目会给你虚假的安全感。** 三轮框架全是 PASS，看起来很美好，但它们没有 CORS 代码。真正的验证是受控注入：把已知危险代码放进真实结构里，看规则抓不抓得到。这一步发现了规则最严重的缺陷（漏检），而单纯扫描永远不会发现。

以后每加一条规则，都要做两件事：扫真实项目确认不误报，注入已知坏样本确认能检出。只有一边是不够的。

注入验证现在已经产品化为 `prodgate mutate` 子命令，并纳入 `all` 自举判据和 MCP 工具集。它把下面这些样本放进模拟真实目录结构的临时项目，走完整 gate 流程（`config` → 文件收集 → adapter → 规则），验证规则端到端能检出。`replay` 只直接调用规则函数，两者互补。

做端到端负向验证时，我把 `cors_wildcard` 临时改成永远返回 None，重新编译后 `mutate` 立刻报 4 个 MISS、整体 FAIL（2/6）。恢复规则后回到 6/6。这证明 mutate 不是又一个永远返回 PASS 的摆设。

**连续打补丁时要停下来。** 两轮各出现了一次"修一个冒出下一个"：secrets 连修五次、CORS 从赋值到函数参数。两次的正确解法都不是继续加特例，而是重新审视规则的基本假设。
