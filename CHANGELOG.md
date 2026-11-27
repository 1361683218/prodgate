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

## 第 9 轮：多语言 Web 验证 + CORS 漏检修复

按 BENCHMARK 的下一步扫了三个真实 Web 框架：expressjs/express（JS）、pallets/flask（Python）、gin-gonic/gin（Go），共 129 文件，surface=web。4 条 Web 规则首次被真实评估，全部无信号（这三个核心库都没有 CORS 实现，符合预期）。

关键收获是做了受控注入实验：往真实项目里注入已知危险的 CORS 代码。注入 Express 的 `res.setHeader('Access-Control-Allow-Origin', '*')` 后规则没报——真漏检。根因和上一轮 secrets 误报同构：规则对"值怎么和键关联"的假设太窄，只认 `:` 和 `=`，不认函数调用的逗号传参。

修法不是继续列举分隔符，而是换判断维度：找带引号的 `'*'` 作为值（排除 `=== '*'` 比较），要求 origin 关键词在同一或相邻代码行。修完三种语言的真实写法全部检出（express:85、flask:1690、gin:863），恢复后回到 PASS。

方法论结论写进 BENCHMARK：只扫真实项目会给你虚假的安全感，必须做受控注入才能发现漏检。以后每条规则都要双向验证。

新增 3 个回归测试（real_world.rs 从 7 到 10）。全套 37 测试通过，clippy 干净，自举 PASS。

## 第 10 轮：mutate 产品化

判断：先不做 Autofix。静态扫描推断不了运行时行为，基于不完整信号自动改代码会引入"改坏代码"这个新失败模式，风险大于价值。

改做更有价值的事——把前两轮最大的方法论收获（受控注入验证）从 BENCHMARK.md 的文字变成可执行的能力。新增 `src/mutation.rs`：6 个已知坏样本，每个放进模拟真实框架目录结构的临时项目（`lib/express.js`、`src/flask/app.py`、`gin.go`），走完整 gate 流程（配置加载 → 文件收集 → adapter → 规则），验证对应规则报 FAIL。

这补上了 replay 的盲区：replay 直接调规则函数，跳过配置/收集/适配这些步骤。规则正确但项目没扫到文件，replay 照样过。两个样本来自真实框架写法，一个来自 expressjs/cors PR #39。

接入 CLI（`mutate` 子命令，文本+JSON）、report、MCP（第 5 个工具 `prodgate_mutate`），并纳入 `all` 自举判据——现在 replay、mutation、gate 三项都要过。

做了端到端负向验证：临时把 cors_wildcard 改成永远返回 None，mutate 立刻 4 个 MISS、整体 FAIL（2/6）；恢复后 6/6。证明它不是摆设。CI 因为跑 `all` 自动覆盖 mutate，无需改配置。

新增 tests/mutation.rs 5 个测试（含"空集合不算 PASS"和"一个 MISS 就整体失败"）。全套 42 测试通过，clippy 干净，自举 PASS。

## 第 11 轮：补齐案例库 + anchored provenance

用户要求补上 4 条声明型规则空着的案例来源。先做判断：这 4 条规则检测 `prodgate: key=off`，真实开源项目不会写这种标记，找不到"真实 commit 显式 opt-out"。强行套用会把案例的 good/bad 做成伪造的，破坏案例库"对规则说真话"的原则。

改成找"防护缺失造成真实事故"的 commit 作为风险锚点，逐个核实 diff：
- rate.limit — express-rate-limit@34729c8b（异步限流错误绕过中间件，限流失效开放）
- error.tracking — sentry-javascript@563e333a（Remix 渲染错误不触发 window.onerror）
- webhook.idempotency — stripe-node@de7a7b72（重试 POST 无幂等键重复扣款）
- backup — postgres@ebd092bc（pg_dump 外键顺序导致备份无效）

但案例的 bad 侧必须用规则能检测的 opt-out 语法，那不是上游代码。现有的 provenance 只有 upstream/synthetic 两档，真实依据只能塞在 note 自由文本里，无法核验。加了第三档 `anchored`：source/source_url 真实可核验 + expression=synthetic 表示样本表达层是合成的。验证器据此区分 upstream（真 commit 真代码）和 anchored（真 commit 合成代码），trusted_count 改为计算所有 citable（upstream + anchored）。

补齐案例时又逼出一个 adapter 缺口：backup 案例用 `.sh` 样本，Language::Unknown 没有注释 token，声明检测不到。加了 Language::Shell 支持 .sh/.bash/.zsh 和 `#` 注释。这是案例库第二次逼出工具缺陷（第一次是第 5 轮的 CORS 规则）。

7 条规则现在全部有案例：2 upstream、4 anchored、1 synthetic，0 invalid。新增 tests/cases.rs 2 个测试（全覆盖 + anchored 判定）。CLI/MCP/JSON 的 "trusted upstream" 改为 "source-backed"，JSON 字段 trusted_upstream → source_backed。全套 44 个测试通过，clippy 干净，自举 PASS。

## 第 12 轮：最后一个案例升级 + secrets 规则修复

把 secrets.hardcoded/c001 从 synthetic 升级成 upstream，案例库 7/7 全部可核验。找到 securevault@3db9bd2f（security: remove hardcoded fallback secrets），good/bad 双侧都逐字来自 server/config/config.js 的 diff。

升级过程中真实样本再次逼出规则缺陷：bad 侧 `JWT_SECRET: process.env.JWT_SECRET || 'securevault_default_secret_change_me'` 规则报 PASS。两个原因叠加：
1. assignment_eq 只认 `=`，不认对象属性冒号 `:`；
2. 第 8 轮为压误报加的"值必须含数字"限制，恰好排除了这个纯字母的真实 fallback 值。

修复：assignment_eq 兼容 `:`；rhs 找 `||` / `??` 之后的引号字面量；fallback 模式本身就是强信号（生产环境在环境变量缺失时真的会用这个值），放宽"必须含数字"要求，只要求含字母。普通赋值仍保持原限制，避免误报回归。

验证：真实 commit 的 good/bad 正确区分，10 个既有 real_world 测试无回归，express/flask/gin 三个真实框架仍零误报，自举三项 PASS。新增 2 个真实世界回归（fallback 检出 + env-only 不误报）。

案例库现在 3 upstream + 4 anchored，0 synthetic，0 invalid。测试数 46。注释里找到的 CHAT_KEY 因不在标准凭证键名列表而未检出——这是规则已知的键名白名单边界，不是 bug。

## 第 13 轮：从案例延伸，扩展凭证键名识别

从刚升级的 secrets/c001 案例继续：同一个 securevault commit 有两个 hunk（JWT_SECRET 和 CHAT_KEY），c001 只覆盖了前者。确认 CHAT_KEY fallback 规则报 PASS——键名不在精确白名单里。这是案例库第 4 次逼出规则缺陷。

结构性修复：把"精确词白名单"改成"精确词 + 后缀模式"。`_key` / `_secret` / `_token` / `_password` / `_passwd` 结尾的标识符都算凭证键，但要求下划线前缀和至少 3 个字符（排除 MONKEY 这类非凭证词）。safe 列表加 `public_`（公钥不是秘密）。这不是加特例，而是把键名识别从列举改成模式。

用同一 commit 的第二个 hunk 建 c002 案例（upstream，逐字来自 diff），案例库 8/8 全部 source-backed。mutation 端到端样本从 6 增到 8，新增两个 fallback 场景。

验证：3 个真实凭证键全检出，MONKEY/public_key/test_key 不误报，express/flask/gin 零误报，craft-libs 3492 文件零误报，自举三项 PASS。新增 2 个真实世界回归。放宽 tests/cases.rs 里写死的数量断言——以后加案例不用再改测试。全套 48 测试、clippy 干净。

## 第 14 轮：回答"强 AI 会不会犯这些错"

用户提出根本性质疑：强 AI 写的代码会犯这些错吗，这样做有意义吗。用实测回答而不是信念：让写这个项目的同一个模型，在不告知规则的情况下按普通需求写 35 行 Web 服务代码，然后扫自己写的代码。

结果：AI 自然写出了三个问题——裸 `cors()`（库默认 wildcard）、env fallback 密钥、占位符 webhook 密钥。第一个当时被抓到，后两个漏检。这直接证明了方向有价值（AI 确实会犯）同时也暴露了覆盖不足。

修两个漏检：
1. 裸 `cors()` 调用检测：没有 `*` 字面量可找，改为识别调用本身；`require('cors')` / `cors({origin: ...})` / `cors(opts)` 不误报。中途踩了一个坑：after 是 `);` 不是 `;`，第一次修完仍漏检，debug 打印定位后改为"只由闭合括号/分号/空白组成"。
2. exact 凭证键名放宽值判断：`WEBHOOK_SECRET = "whsec_example_..."` 不需要含数字，因为 AI 常写这种占位符并真的部署。后缀式键名保持严格。放宽后 flask 的 `ConfigAttribute("SECRET_KEY")` 被误伤，加了全大写配置键名引用的排除。

重扫 AI 代码三个问题全部检出，三个真实框架零误报，48→51 测试全过。三个 AI 场景进 mutation（8→10）和 real_world 回归。

写了 `AI-CODE.md` 诚实记录：信号型规则对 AI 代码有用（这次 3 个真问题），但 5 条声明型规则对 AI 完全无效（AI 不知道 `key=off` 语法）。不承认这个边界就是自欺。

## 第 15 轮：第二批 AI 代码 + 假绿灯修复

继续 AI 自测：又写 4 个领域服务（认证/数据库/上传/日志）。auth 的硬编码 JWT secret 抓到；db 的 `postgresql://admin:password123@localhost/prod` 漏检——变量名是 engine，键名规则看不见。upload 的无认证文件上传超出规则范围，logging 的无集中错误追踪则触发了声明型规则的假绿灯。

修两个问题：

1. 连接串明文密码检测：`scheme://user:password@host` 是值格式信号，不依赖键名。限定数据库连接串（postgres/mysql/mongodb 等），跳过 https 基本认证。踩坑：占位符判断最初用子串包含，把 `password123` 当占位符跳过了；改成精确相等后正确。真实框架和自举零回归。

2. UNCONFIRMED 判定：声明型规则（5 条）没看到 opt-out 时不再报 PASS，改为 UNCONFIRMED——"没人声明"不等于"防护到位"。不阻塞构建（passed() 仍只看 FAIL），但报告里明确可见。信号型规则保持 PASS。这是第 8 轮"扫 0 文件假绿灯"的同类修复。

新增 tests/unconfirmed.rs 4 个测试 + real_world 2 个连接串测试。57 测试全过，clippy 干净，自举 PASS，mutation 10/10，cases 8/8。README 加了六种 verdict 的说明表，AI-CODE.md 补了第二批数据和假绿灯分析。

## 第 16 轮：按"AI 代码安全"方向加 SQL 注入规则

用户建议方向：让 AI 写出的代码达到安全。盘点发现 7 条规则里只有 3 条真正是安全（cors/secrets/rls），而 AI 代码实测里最严重的漏检是 SQL 注入——OWASP 第一，AI 极常写 `execute(f"SELECT ... {uid}")`，规则完全没有。

新增信号型规则 `sql.injection`。过程中反复误报，连修多次：
- `Text("...")` 小写后匹配 `text(`，加标识符词边界
- 跨行窗口把 UI 代码拼进 SQL 判断，改为极窄的单行 f-string 检测
- 越界 panic（找不到闭合引号时索引超界），加边界检查

关键结构性 bug 是 secrets 的 exact 关键词用子串匹配：`names_bad_token` 含 "token" 被当强凭证键，`DEFAULT_PASSWORD = "VelvetSweatshop"`（Excel 公开默认密码）被当泄露。改成标识符边界匹配 + `_file`/`default_` 排除。还修了 fallback 判断——`||` 出现在字符串之后是布尔或，不是 fallback；只检查引号之前的部分。

代价（诚实记录）：收紧后的 SQL 规则只认同一行里 f-string 同时含 SQL 动词和插值。真实世界的 SQL 注入常跨行或拆成多段，会漏。宁可漏一些变体，也不能在每次扫描都误报。案例用 Griddy@d24419ce（AI agent 修的真实 SQL 注入 commit）做 anchored，样本压缩成规则能检测的单行形式，note 里说明。

4 个 Rust 项目 + 3 个 Web 框架零误报。AI 代码 db 场景现在同时抓到连接串密码和 SQL 注入。mutation 10→11，案例 8→9，全套 59 测试、clippy 干净、自举 PASS。

## 第 17 轮：修复建议（从"发现问题"到"告诉 AI 怎么改"）

方向判断：继续加安全规则的边际收益在下降（每加一条都要经历多轮误报修复）。对"让 AI 写的代码达到安全"这个目标，比多一条规则更实用的是让每条 FAIL 带一句可直接照做的修复指令——不自动改代码，风险低。

Finding 加 `suggestion: Option<&'static str>`，11 个构造点全部补齐。6 条信号型 FAIL 有具体建议（cors 换显式 origin、凭证改用环境变量并轮换、DB URI 密码移出连接串、SQL 改参数化查询、配置错误改 roots），声明型/UNCONFIRMED 为 None（需要人判断）。CLI 文本、JSON、MCP 三处输出都带上了，同一份文案保证一致。

写测试时发现一个已有 bug：`PREFIX_PARTS` 里的 "AKIA" 是大写，但 `prefixed()` 接收的是小写值——AWS key 前缀从来没生效过。改成小写 `akia`。

新增 tests/suggestions.rs 3 个测试（每条信号 FAIL 必须有建议、无信号不能有建议、JSON 必须序列化该字段）。62 测试全过，craft-libs 零误报，clippy 干净，自举 PASS。README 说明 `suggestion` 字段。

## 第 18 轮：事前预防——给 AI 的编码规范

前 17 轮都是事后检测。对"让 AI 写出的代码达到安全"这个目标，写代码时就不犯错比写完再改更有效。新增 `prompt` 命令和 MCP `prodgate_prompt` tool：从规则库 + 通过验证的案例库生成一份 AI 可直接遵守的编码规范，每条规则带真实 commit 的 bad/good 代码、修复指令、来源。

设计要点：内容从 `all_rules()` 和 `cases::validate()` 动态生成，不手写第二份文档——规则和案例库变了 prompt 自动跟着变，不会漂移。声明型规则额外说明"主动加上控制，不要静默省略；有意关闭要显式写 prodgate: key=off"。

prompt 的 suggestion 通过跑案例的 bad 样本从 rule 函数取，保证和 gate 输出同一份文案。CLI 文本 + JSON 结构化 + MCP 三通道。

新增 tests/prompt.rs 3 测试。65 测试全过，clippy 待验，自举 PASS。

## 第 19 轮：prompt 按项目形态裁剪

第 18 轮的 prompt 是通用规范——CLI 项目也收到 CORS/webhook/RLS 的完整代码示例，浪费 AI 注意力。这轮按 `prodgate.conf` 的 surface 裁剪：不适用规则压成一行 NOT APPLICABLE，适用的保留完整 bad/good/修复。

关键设计：把 gate 里硬编码的 rule→surface 判定抽成公开函数 `applies_to_surface()`，gate 和 prompt 共用。测试断言两者一致——被 gate 标 NOT-ASSESSED 的规则绝不会在 prompt 里被当成可执行建议。避免两处判定漂移。

路径分离：prompt 内容来自 prodgate 自带案例库（CARGO_MANIFEST_DIR），surface 来自目标项目（CLI 的 root / MCP 的可选 root）。MCP tool schema 加了可选 root 参数。CLI 文本、JSON applicable 字段、MCP 三通道一致。

新增 4 测试（cli/web 差异、JSON applicable、与 gate 判定一致）。69 测试全过，自举 PASS。

## 第 20 轮：批量加 7 条 AI 安全规则（按用户要求增加代码量）

第三批 AI 实测的 7 个安全场景（eval 注入、不安全反序列化、弱密码哈希、JWT none、路径遍历、shell 注入、凭证日志）现行规则 0 检出。一次性加 7 条信号型规则，新文件 src/rules_security.rs 承载。

误报处理：真实项目连出 4 个误报（Rust 的 eval 函数、app_fingerprint 里的 print 子串、MS-OFFCRYPTO 格式要求的 md5、普通方法调用）。结构性修复：标识符边界匹配 has_call()、按 Language 限定 eval 只对 JS/TS、NOSONAR/格式标记抑制、bare md5 调用不算密码哈希。近邻污名 nearby_taint() 补上 deser/path 的跨行漏检。最终 4 个真实项目零误报，7 个 AI 场景全检出。

7 组 fixture + 7 个 synthetic 案例（AI 实测样本，诚实标注不冒充 commit）+ 7 个 mutation 样本。案例库 16 个：9 source-backed + 7 synthetic。放宽 cases 测试——synthetic 是一等公民，但必须 source=synthetic 且无 URL。

自举悖论：规则的 FIX 常量字符串含危险模式，被自己的规则扫到；mutation.rs 的注入样本同理。skip_path 加 mutation/rules_security。mutation 11→18，replay 8→15。71 测试全过，clippy 待验，自举 PASS。

## 第 21 轮：文档一致性 + 更大规模验证

第 20 轮批量加 7 条规则后，文档严重漂移：README 还写"7 条规则、8 个案例、信号型三条"，AI-CODE.md 的边界结论停留在 8 条规则时代。这轮把"当前状态"描述全部更新到 15 条规则 / 16 案例 / 10 条信号型，AI-CODE.md 的历史叙述保留但加了"当前状态"置顶说明（历史是历史，不能篡改）。

扩大验证到全部 7 个真实项目（4 Rust + express/flask/gin），15 条规则全量扫描零误报。prompt 自动包含全部 15 条规则（因为它从 all_rules() 动态生成）。

结构性修复：新增 tests/docs.rs，从 README/BENCHMARK/AI-CODE 提取规则数与案例数，和 all_rules()/案例库实际数量对比。以后加规则不同步更新文档，测试直接挂——不再靠人记。文档里的数字改成阿拉伯数字以便机器读取。

76 测试全过，自举 PASS。

## 第 22 轮：一次报告所有 finding

发现缺陷：同一规则在同一文件有多个问题时只报第一个——eval 出现 2 次只报第 1 行。AI 修完第一个再扫才冒出第二个，挤牙膏式体验。

评估后否决了"改 15 条规则签名为 Vec<Finding>"的方案——会动摇 replay/fixture/cases 全部契约，风险大于收益。改用 gate 层屏蔽重扫：规则报出 finding 后清空该行内容（保留行号），重跑同一规则，直到无新 finding。15 条规则零改动，所有契约不变，行号保持正确。加 50 条上限防病态循环。

验证：eval×3 报 3 处（行 1/2/3），跨 2 文件报 2 处，cors 多行窗口不重复报，无信号规则仍只报 1 条 PASS，60 条 eval 触发上限且证据无重复。新增 tests/multi_finding.rs 4 测试。80 测试全过，自举 PASS。

## 第 23 轮：baseline 改为内容寻址

地基缺陷：baseline 的 fingerprint 是 `rule|path:LINE`，绑定行号。实测在文件顶部加一行 import，原来 baselined 的 2 个 eval 问题变成 1 个 FAIL——旧问题"复活"，baseline 静默失效。这对采用 baseline 渐进接入的团队是致命的：任何正常的代码编辑都可能导致门禁突然变红。

改成内容寻址：`rule|path|fnv1a64(规范化行内容)`。行号移动不影响匹配，改动那行代码本身才生成新 fingerprint（正确拦截）。FNV-1a 自实现，无外部依赖。旧格式仍可读取（升级不破坏已有 baseline），新写入一律新格式。读文件失败时 fallback 到旧格式（config.no-sources 这类无行号的 finding）。

验证：加两行后 2 个问题仍被识别为 BASELINE 且 passed=True；改动问题代码后该处重新 FAIL。新增 3 个 baseline 测试。83 测试全过，自举 PASS。

## 第 24 轮：声明型判定改为规则元数据

地基隐患：gate.rs 硬编码 DECLARATION_RULES 清单（5 条）决定"无信号时报 UNCONFIRMED 还是 PASS"，和 check.rs 里真正的声明型规则分离。新增声明型规则若忘了同步清单，会静默报 PASS——假绿灯。tests/docs.rs 也有同样的重复清单。

给 Rule 加 kind: RuleKind（Signal/Declaration）元数据，gate 和 docs 测试全从它读，硬编码清单删除。新增 tests/rule_kind.rs 3 测试：每条规则的 kind 与无信号时的实际 verdict 一致（标错就挂）、两种 kind 都用到了、声明型不把字符串里的 opt-out 当声明。

86 测试全过，自举 PASS。

## 第 25 轮：surface 判定收进规则元数据

最后一处重复硬编码：gate.rs 的 applies_to_surface 写死 4 条 web 规则，tests/prompt.rs 两处重列，README 写"4 条 Web 规则"。新增 web-only 规则忘同步，prompt 会在 CLI 项目里把它当可执行建议——误导 AI。

给 Rule 加 web_only: bool，applies_to_surface 改为从 Rule 读，RuleBlock 也直接存该字段（避免反复查 all_rules）。tests/prompt.rs 从元数据推导 web 规则清单，README 不再写死数字。

至此"单一事实源"收敛完成：规则类型（kind）、适用 surface（web_only）、判定位置、文档数字、baseline 身份、案例来源全部单一来源 + 测试守卫。86 测试全过，clippy 干净，自举 PASS。

## 第 26 轮：闭环实验框架（用户选"先2"）

用户先前问"这样做有意义么"，这轮建立可复现的实验回答能回答的那部分。诚实前提：环境无法现场生成 AI 代码，所以验证的是信息充分性而非行为改变——报告里每一处都标注 scope。

归档三批 AI 亲写代码（13 个样本）到 benchmark/ai-code/，不再依赖 /tmp。新增 src/efficacy.rs 三步验证：基线扫描（13 FAIL / 10 规则）、prompt 覆盖（10/10 完整 bad/good/fix）、反事实（prompt 教的好写法 10/10 不触发对应规则）。结论：遵守 prompt 足以避免所有观察到的错误——信息充分性成立，行为改变未测。

CLI prodgate efficacy（文本+JSON）+ MCP prodgate_efficacy tool。新增 tests/efficacy.rs 5 测试。91 测试全过。

## 第 27 轮：真实世界回归固化（用户选"再1"）

前几轮的"7 个真实项目零误报"全是手动验证。把每一次误报修复固化成快照：benchmark/regression/ 里 10 个真实形态代码片段——Rust 普通 eval 方法、app_fingerprint 里的 print、MS-OFFCRYPTO 要求的 SHA-1、参数化 SQL、环境变量读密钥、secure_filename 路径、subprocess 列表、JSON.parse、显式 CORS、Go 普通 eval 函数。

新增 src/regression.rs 直接对快照跑全部规则，确认零 FAIL。CLI prodgate regression（文本+JSON）+ MCP prodgate_regression tool。新增 tests/regression.rs 5 测试，包括"守护守护本身"——断言已知 FP 形态仍在快照里，防有人靠删快照让测试通过；以及快照非空、扩展名可识别（防 vacuous pass）。

96 测试全过，clippy 待验，自举 PASS。

## 第 28 轮：all 统一为完整门禁

哨兵散落问题：efficacy 和 regression 建成后没进 CI——CI 只跑 `all`（replay+mutation+gate）和 `cases`。改规则重新引入误报，regression 会挂但 CI 不知道。

把 cases、regression 纳入 `all`（文本版和 all_json），efficacy 作为信息性报告展示但不阻塞（实验指标不该让构建变红）。文本版所有关卡都执行完再汇总，不让一个失败掩盖其他状态。CI workflow 从两步门禁简化成一步 `all`。

加测试断言 all_json 必须聚合全部六项，且五个 gating 守卫的 passed 都为 true——以后加哨兵忘接进 all，测试立刻挂。96 测试全过，自举 PASS。

## 第 29 轮：修 symlink 循环与重复扫描

运行时故障模式：walk_dir 用 path.is_dir() 判断目录，会跟随符号链接。实测一个指向祖先的 symlink 让 2 个真实文件被扫成 64 个——重复扫描浪费时间、产生重复 finding、深层文件系统上可能栈溢出。

修法：symlink_metadata（不跟随链接）识别符号链接并跳过；visited 集合用 canonicalize 后的路径防别名重复。实测回到 2 个文件，正常深层递归仍工作。

新增 tests/symlink.rs 4 测试：指向祖先的链接、指向文件的链接、正常深递归、含空格路径。100 测试全过。

## 第 30 轮：收尾交付（用户选"收尾交付"）

用户问"接下来干什么"，判断工具已到自洽终点，建议收尾而非继续扩张，用户确认。

两步：
1. 真实项目实测：craft-libs 3492 个源文件、13 秒扫描、零 FAIL；ledger-rs（web 形态）12 文件零 FAIL。
2. 新增 DELIVERY.md —— 一页纸的决策者视角交付摘要：一句话定位、解决什么、四层验证凭什么信、边界（不是完整 SAST / 5 条声明型对 AI 无效 / efficacy 只测信息充分性 / baseline 是逃生舱）、怎么用、项目现状。

写文档时踩到自己立的规矩：硬编码行数/commit 数会漂移（写 5513 行，加个测试就变 5457；写 28 commit，提交一次就变）。最终去掉这些脆弱数字，只保留有测试守卫的稳定数字（15 规则、18/18、16/16、3492 文件）。新增 docs 测试守 DELIVERY 的关键数字。

101 测试全过，clippy 干净，六道门禁全 PASS。项目到此收尾。

## 第 31 轮：自包含 HTML 可视化报告（用户"嫌 prodgate 太小"）

用户说嫌 prodgate 太小。明确立场：不灌行数（那只会把工具变成垃圾场），但扩真实产品能力。选了缺的一块——终端和 JSON 之外，没有能拿给人看的报告。

新增 src/html_report.rs：把 gate JSON 渲染成单文件 HTML，内联 CSS/JS，无 CDN，离线可打开。含统计卡、状态分布条、规则详情卡、修复建议块、前端筛选。所有嵌入的源码文本走 escape_html() 转义——被扫项目里的 `<script>` 不会变成报告里的活脚本。

CLI 加 report 命令（--out FILE / --out=FILE），零外部依赖的有参命令——因 --out 的值会被误当 positional root，重写了参数解析用 skip_next 标记跳过。

新增 tests/html_report.rs 5 测试：转义覆盖、自包含性（无 link/http/@import）、恶意源码转义、统计与修复展示、空 findings。106 测试全过，clippy 干净。

这是"做大"的正确方式：真实能力增长（~370 行报告模块 + 测试），不是灌水。

## 第 32 轮：扫描历史趋势（用户要看增量，顺带产品化）

用户想看别人的项目怎么做增量。研究了 craft-libs（143万行 Rust workspace）：四层收窄——crate 物理拆分、每 crate 独立 workflow 的 paths 过滤、[package.metadata.craft] reads 声明跨 crate 依赖自动算影响范围、定时全量兜底保缓存不过期。核心洞察：增量不是跑得快，是少跑——靠结构拆分让影响范围可计算。

股东理念用在产品上：新增 src/history.rs，每次 gate 往 .prodgate/history.jsonl 追加一条快照（时间/文件数/各 verdict 计数/passed）。append-only 不覆盖，500 条上限，损坏行跳过。HTML 报告读历史画内联 SVG 折线图（FAIL 随时间变化，绿点 PASS 红点 FAIL）。

为避免重复扫描 3492 文件，把 report.rs 的 gate_json 拆成纯 build + gate_json_recording 包装：CLI gate 用 recording 版（记一次），all 和 report 用纯版（不污染历史）。新增 4 个 history 测试。110 测试全过。

## 第 33 轮：增量机制调研报告（用户"多看几个项目怎么做增量"）

用户想多看几个项目怎么做增量。用两个并行 agent 研究：本地 4 个项目（craft-anything/ledger-rs/ledger-ui，craft-libs 上轮已做）+ 外部 5 个（rust-analyzer/tokio/ripgrep/cargo/deno，WebFetch 抓 raw.githubusercontent.com 真实配置）。

结论归纳成四种范式：A 声明式依赖图（craft-libs 的 reads 下沉到 Cargo 元数据可传递；deno/rust-analyzer 的 CI 目录清单扁平无传递）、B 拓扑闸门（tokio basics 四连、rust-analyzer 失败即取消、craft-anything 降频 nightly）、C 维度切分（deno shard、tokio each-feature、cargo 测试名前缀、ripgrep 信号等价剪枝）、D 内容寻址（ledger-rs 的 ev_sha256、prodgate baseline 同源）。

反直觉发现：GitHub 原生 on.push.paths 被五个成熟项目集体回避——merge queue 下必需检查会悬空，改用 job 级 if: skipped + conclusion 聚合。缓存属于"跑得快"不属于"少跑"，rust-analyzer/cargo/ripgrep 刻意不缓存；craft-anything 记录 incremental 目录曾达 46GB——增量的阴暗面是磁盘膨胀。

报告写入 INCREMENTAL-RESEARCH.md，带具体配置证据与 URL 来源，被截断未复查的部分明确标注。去掉易漂移的行数字。

## 第 34 轮：history 时间窗口裁剪（调研落地）

用户确认"改 prodgate 本身"。先实测六道关卡耗时：最慢 replay 0.33s，全流程 <0.5s——范式 B 拓扑闸门是伪需求（tokio 的 basics 是为省 miri/asan 等 30 个昂贵 job，prodgate 没有昂贵关卡）。范式 A 增量扫描报告里已论证不做。C/D 已落地。所以只做调研唯一真正适用的发现：

history 原先只有 500 条上限，没有时间维度——每天扫一次留 500 天，每秒扫一次留 8 分钟，趋势图时间尺度不可控。借鉴 craft-libs 定时全量保缓存不过期 + ledger 内容生命周期，加 30 天时间窗口 + 条数双裁剪；过期记录自动丢，最新一条永远保留，损坏行不 pin 文件。不加配置项（避免配置蔓延，与条数上限一样是内部防膨胀策略）。

新增 2 测试（旧快照裁剪、全过期仍保留最新）。112 测试全过。调研报告更新适用性判断表，把"哪些范式不适用及数据依据"写清楚。

## 第 35 轮：文档漂移体检（用户"可以" — 数据驱动体检而非硬找改动）

用户说可以，这次不硬找改动，先做健康体检：git status 干净、六门全 PASS、112 测试。但扫文档数字发现两处当前态漂移：BENCHMARK.md 复现章节写"37 个测试"（实际 112），DELIVERY.md 写"101 个测试"（实际 112）。CHANGELOG 里的历史数字是时间线记录，保留正确。

按项目一贯原则处理：不更新数字，而是去掉脆弱数字。两处都改成"全套测试"，并强化 tests/docs.rs 守卫——当前态文档禁止写死测试数，一次性消灭这类漂移而不是逐个修。新增 benchmark 复现章节守卫。

113 测试全过，clippy 干净，六门全 PASS。体检的价值：发现自己的守卫体系有盲区（只守 README/DELIVERY 关键数字，没守 BENCHMARK 当前态章节），补上。

## 第 36 轮：Ralph Loop 自动循环实验（用户想验证"AI 自己迭代"）

用户想验证 AI 能不能自己调用工具迭代，而不只是每轮手打"继续"。建 loop/ 壳：PROMPT.md（自包含指令）、progress.md（唯一事实源）、judge.sh（独立裁判=prodgate all）、run.sh（bash 循环）。

真跑两轮：两个全新上下文的 agent，只给磁盘文件路径不给对话历史。第1轮实现 prodgate history 命令，第2轮加测试，都自验 PASS，输出 NEXT/COMPLETE。独立验证通过。

三个真实发现：
1. 提示词漏洞——PROMPT 说"不许改裁判"但没说"不许 commit 裁判"，AI 把整个 loop/ 含 judge.sh 一起 commit 了。修复：裁判/指令从 git 移除由人持有，只有 progress.md 入库。印证"裁判权靠文件隔离，不靠提示词拜托"。
2. 产物质量超预期——AI 自己加了"读历史必须只读"的断言，这个约束我没提。
3. 时区细节——UTC 显示而本地+8，裁判测不到没写进去的人类验收点。

实验报告 loop/EXPERIMENT.md。115 测试全过，六门全 PASS。

## 第 37 轮：baseline 滥用防线（用户"要把好关"）

用户强调 prodgate 是裁判，要把好关。红队视角审：有完整 shell 权限的 AI 能跑 `prodgate baseline` 把所有 FAIL 冻结成 BASELINE，代码一字未改却全绿——实测 2 个 eval 问题，baseline 后 gate PASS。MCP 没暴露 baseline，但 Ralph Loop 的 agent 有 shell，CLI 就能用。

不删功能（人类渐进接入合法），加三道关：
1. `all` 的 JSON/文本显式报 baseline 数量（baseline_count / baseline_limit）
2. 超 20 条直接 FAIL（少量是渐进接入，大量是绕过门禁）
3. baseline 文件加审计头（recorded_at_unix / count / 20 issue cap）

实测：25 条冻结 → all exit=1，"OVER LIMIT — gate bypass suspected"；0 条 → exit=0。新增 tests/baseline_guard.rs 4 测试（含"gate 被骗但 all 必须拒绝"的红队断言），report 测试加守卫防字段被删。119 测试全过，clippy 干净，六门全 PASS。

## 第 38 轮：裁判红队审计（用户"要把好关"的延续）

用户说要把好关，上一轮堵了 baseline，这轮系统审其余攻击面。真跑攻击实验不靠推理：

- 删一条规则 → mutation + cases 失败，挡住
- 规则还在但永远返回 None → replay/mutation/cases 全失败，挡住
- roots 指向不存在 → no-sources 挡住
- surface 篡改 → 部分挡住（regression/cases/mutation 用固定 web surface，不受项目配置影响）
- 改测试/裁判文件本身 → 挡不住，靠 git 保护分支 + CI 干净 checkout

关键发现：规则篡改之所以被挡，是因为 mutation/cases 有**独立期望集**，不依赖 all_rules()。前面 37 轮无意中建成了纵深防御——没有任何单层充分，但交叉验证有效。

报告写进 REDTEAM.md，诚实标注不可修的攻击面。原则：本地 all 是 AI 的自我检查，不是安全边界；CI + git 权限才是。

## 第 39 轮：防线现状审计（用户"可以"）

延续红队审计，查"防线在 CI + git 权限"是否真实存在。结果：38 commit 全直推 master、0 PR、无分支保护——这道防线是空的。

代码层补了两样声明：.github/CODEOWNERS（裁判文件需 owner review）、CI 加 Flag judge-file changes 步骤（改 tests/check.rs/mutation/cases/fixture/judge.sh/workflows 时打 warning）。但都是声明性不强制，需用户在 GitHub Settings 开分支保护 + require review from Code Owners 才生效。REDTEAM.md 补了具体操作步骤和"直推 master 全绿不代表可信"的警告。

## 第 40 轮：分支保护启用后的 CI 修复

用户开启 GitHub 分支保护后 CI 连续失败 5 次(exit 101)，本地 macOS 全过。排查：annotation 只说 exit 101 拿不到日志（需 admin）→ 拆步骤出现假绿灯（cmd | tee 吞掉真实失败）→ 加 pipefail 重现 → 让 CI 用 ::error:: annotation 自报失败输出 → 根因：项目有两个二进制，cargo run 在 CI 要求显式 --bin。加 --bin prodgate 后 CI 全绿。

教训：本地 cargo 默默选默认二进制，CI 更严格；| tee 不加 pipefail 是假绿灯制造机——本项目存在的意义就是防假绿灯，结果自己 CI 栽了。REDTEAM.md 补完整排查记录。分支保护+CODEOWNERS+CI 全绿，攻击6防线从空变实。

## 第 41 轮：两套合并成一套体系（用户选"合并"）

工作区根目录凌晨的 AGENT-COLLAB-PROTOCOL.md + protocol-check.sh 和下午的 prodgate/Ralph Loop/红队防线是两条独立线。用户选合并。

共同内核：完成不能由 AI 自己说了算。合并方式不是写大文档，而是让 prodgate 成为唯一裁判：新增 src/artifacts.rs 吸收协作协议 §4 的 artifact-on-disk 检查（兼容 protocol-check.sh 的 TSV 格式，not-assessed 无理由=FAIL），纳入 all_json 和文本 all 作为第七道门禁。Ralph Loop 的 judge.sh 包 prodgate all，自动继承不用改。

CLI prodgate artifacts（文本+JSON）+ MCP tool。8 个测试（含 all 继承）。127 测试全过。顶层 AI-WORKFLOW.md 讲清三者怎么咬合，协作协议头部加统一视角指引。
