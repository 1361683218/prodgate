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

这不是 bug，是设计取舍：静态扫描推断不了运行时行为，prodgate 不假装能推断。但这意味着一个只靠声明型规则的项目，实际检出能力很弱。目前真正有检出力的是两条信号型规则（cors.wildcard、secrets.hardcoded）加上递归配置。

**secrets 规则刻意牺牲了召回率。** 为了压掉误报，现在要求值紧凑且含字母数字混合（或带已知前缀）。一些真实但不合这个形状的凭证会漏掉。补召回应该靠案例库积累真实样本，而不是继续放宽规则——那会重新引入误报。

**基准规模还小。** 四个项目都是 Rust，且都是同一个作者/生态。下一步应该扫不同语言、不同形态（尤其 surface=web）的项目，验证那 4 条 Web 规则。

## 怎么复现

```bash
cargo build --offline
cargo test --offline                    # 34 个测试，含 7 个真实世界回归
cargo run --offline -- gate --json <项目路径>
```

被扫项目需要临时放一个 `prodgate.conf` 声明 roots / surface / recursive。基准脚本没有写成命令，因为不同项目的布局判断仍需人工确认——把这个判断自动化本身就是另一轮工作。
