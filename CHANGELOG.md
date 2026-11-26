# 开发记录

这里记各轮迭代做过什么。用户文档在 `README.md`。

## 第 1 轮：最小内核

先做一个能自证的工具：7 条控制里先实现 3 条（cors、secrets、rate.limit），每条带 good/bad fixture 对。`replay` 检查规则有效性，`gate` 让工具扫自己的仓库。两个判据必须同时成立。

第一版写完时 gate 全绿，但 replay 抓出 3 条里 2 条是假的：坏样本写成了函数签名，规则根本不会触发。如果只做门禁不做回放，这就是一个永远返回 PASS 的摆设。修正 fixture 后 3/3 有效。

另一个坑是规则源码里写死了要搜的字面量，扫自己时会自撞。改成常量拼接（`RATE_KEY` + `RATE_OFF`）后解决。

## 第 2 轮：补齐 7 项控制

加了 `error.tracking`、`webhook.idempotency`、`rls`、`backup`，每条都带 fixture 对，并在 gate 里声明适用 surface。回放 7/7。

抽了共享的 `find_opt_out` helper，key 和 `=off` 拆成常量拼接，延续第 1 轮的自举规避。

做了负向测试：构造一个 `surface=web`、故意打开全部 7 个缺口的项目，门禁 7/7 命中并 exit 1。自举 PASS 只证明它不会误伤自己，负向测试才证明它真能抓到东西。

## 第 3 轮：跨语言 adapter

新增 `src/adapter.rs`：`Language` 识别 + `SourceFile::declaration_lines()`。规则签名从 `(path, src)` 换成 `SourceFile`，7 条规则一行语义没改。覆盖 Rust / Python / JS / TS / Go。

这轮能成立的前提是第 2 轮把规则分成了信号型和声明型。信号型扫全部行，因为字符串里的 `*` 也会真被当 header 用；声明型只读注释行，字符串里的 `rls=off` 不是操作者的决定。没有这个区分，adapter 就只是个扩展名列表。

验证：同一条 rls 规则不改一行，在 Python / TypeScript / Go 三种样本上都能命中；多语言负向测试里 py/ts/go 各一个 opt-out，门禁 3/3 命中并给出正确文件名行号。

## 第 4 轮：baseline 棘轮

新增 `baseline` 子命令。把当前 FAIL 集按 `rule|evidence` fingerprint 冻结到 `.prodgate/baseline.txt`，gate 时匹配到的降级为 BASELINE，新增的仍然 FAIL。解决老项目带着已知问题接入的冷启动问题。

fingerprint 故意不含 note 文本，改写描述不算新增问题。重跑 `baseline` 会收缩到当前真实问题数，修一条少一条。

端到端演示：rls 旧问题标 BASELINE，新加的 backup 立刻 FAIL。

## 第 5 轮：案例库

新增 `src/cases.rs` 和 `cases/`。案例是带 `case.toml` 来源元数据的 good/bad 目录对，贡献者只提交案例，不碰规则代码。

Provenance 分三档：upstream（可核验）、synthetic（诚实标注）、UNVERIFIED（拒绝）。验证器拒绝无来源案例，也拒绝合成案例冒充真实。

录入两个真实上游案例：

- `expressjs/cors@a14f3bca` — PR #39 的 wildcard origin bug，一行 diff
- `supabase/supabase@49bda396` — PR #48394 的 RLS footgun：建表忘了 enable，任意登录用户可读写授权表

这轮的收获是真实案例逼出了两个玩具样本没覆盖的规则 bug：真实代码把 origin key 和 wildcard value 写在两行，原规则要求同一行；修复后代码里 `options.origin === '*'` 是比较而非赋值，规则误报。改成跨代码行窗口、并跳过 `==` / `===` / `!=` / `>=` 这类运算符后解决。

## 第 6 轮：MCP server

新增 `src/json.rs`（手写极简 JSON，零依赖，保持 std-only）、`src/mcp.rs`（JSON-RPC 2.0 层）和 `src/bin/prodgate-mcp.rs`。四个只读工具全部调用 CLI 用的同一批内核函数，MCP 层没有重写任何检测逻辑。

两个刻意不做的决定：不暴露 `baseline`，因为冻结已知问题应该是人的决定；不存在的路径返回错误而不是 PASS，因为 `run_gate` 扫 0 个文件时会全 PASS，AI 拼错路径就会误报成功。

新增 7 个协议测试。全套 19 个测试通过。

## 第 7 轮：正常项目基础设施

改用词。`known debt` / "欠债"来自 technical debt，但对部分读者有歧义联想，全部换成 `known issue` / "已知问题"，包括代码注释、CLI 输出和测试名。语义不变。

补 MIT LICENSE 和 GitHub Actions CI。CI 跑编译、测试、clippy、自举门禁、案例验证五步。为配合 `--locked`，把 `Cargo.lock` 从 .gitignore 移除并提交。

CLI 加 `--help`、`--version`、`--json`。JSON 报告由 `src/report.rs` 生成，gate / replay / cases / all 都有统一的 `tool` / `command` / `passed` 顶层字段，脚本只读 `passed` 即可。退出码和 `passed` 保持一致。

修一个真 bug：`write_baseline` 原来用 `let _ = fs::write` 吞掉写失败，返回"记录成功"但实际什么都没保存。改成返回 `Result<usize>`，调用方报告错误并返回失败退出码。加了"写入目标是目录时返回 Err"的测试。

测试里用 PID + 时间戳生成临时目录时出现过 flaky（纳秒精度偶发碰撞），加了原子计数器彻底消除。三次重复运行确认稳定。

全套 24 个测试通过，clippy 干净。

## 第 8 轮：真实项目扫描

这轮从"工具能不能用"出发，扫了工作区四个真实 Rust 项目，而不是继续加功能。结果暴露的问题比前七轮加起来的都多。

发现一：扫 craft-anything 时 0 个文件却报 PASS。它是 Cargo workspace，源码在 `crates/*/src`，默认 `roots=src` 找不到。加了 `config.no-sources` 合成 FAIL，且不能被 baseline 冻结——配置错误永远不该被当成已知问题。之前只在 MCP 层防了"路径不存在"，CLI 层"路径存在但扫到 0 文件"这个漏网之鱼现在补上了。

发现二：加 `recursive=true` 配置支持递归扫描，跳过 target / node_modules / .git 等目录。开启后 craft-anything 从 0 个文件变成 829 个。

发现三：secrets 规则在真实项目里连续六类误报（右侧文本含 tokens、测试占位符、纯字母值、IPv6 常量、内联 #[test] 函数、doc 注释里的 URL 参数）。我连续加了五轮启发式，每修一个冒出下一个，到第五次停下——继续加规则是打地鼠。最后做了三项结构性修复：只认赋值号左侧为键名、只认字符串外的等号、测试代码整体跳过（含 `#[test]` 函数体，用花括号深度追踪）。修完四个项目 4,333 个文件全部 clean，真凭证仍能抓到。

七类误报全部写进 `tests/real_world.rs` 做回归。全套 34 个测试通过。

写了 `BENCHMARK.md` 诚实记录结果和局限：声明型规则在真实项目上基本不触发（因为没有 `key=off` 标记），secrets 规则为压误报牺牲了召回率，基准规模还小且只有 Rust。下一轮应该扫不同语言和 surface=web 的项目。
