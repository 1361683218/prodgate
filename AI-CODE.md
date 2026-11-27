# 强 AI 写的代码会犯这些错吗？这样做有意义吗？

这是个该用数据回答的问题，不是靠直觉。这轮做了一次诚实的自测：**让 AI（写这个项目的同一个模型）在不被告知规则的情况下，按普通用户的口吻写两个服务，然后用 prodgate 扫自己写的代码。**

## 测试方法

给 AI 的任务就是两句普通需求，不提 prodgate、不提规则：

1. "帮我写个 Express API，带用户注册和登录"
2. "写个 FastAPI 服务处理支付 webhook"

AI 自然产出了 35 行代码。关键片段：

```js
// api/src/server.js
app.use(cors());
const JWT_SECRET = process.env.JWT_SECRET || 'dev-secret-key-change-me';
```

```python
# webhook/src/main.py
WEBHOOK_SECRET = "whsec_example_key_replace_this"
```

## 结果

第一次扫描：

| 问题 | AI 写了什么 | 当时规则 |
|---|---|---|
| 硬编码密钥 fallback | `JWT_SECRET = env \|\| 'dev-secret...'` | ✅ 抓到（第 12 轮刚修） |
| CORS 库默认 wildcard | `app.use(cors())` | ❌ 漏检 |
| 占位符密钥直接部署 | `WEBHOOK_SECRET = "whsec_example_..."` | ❌ 漏检 |

修完规则后重扫，**三个全部抓到**：

```
API:    cors.wildcard@src/server.js:6
        secrets.hardcoded@src/server.js:9
Webhook: secrets.hardcoded@src/main.py:5
```

## 第二批：更多场景

又让 AI 写了 4 个不同领域的服务（认证、数据库、文件上传、日志），共 60 行：

| 场景 | AI 写了什么 | 结果 |
|---|---|---|
| auth | `SECRET = "my-super-secret-jwt-key-12345"` | ✅ 抓到 |
| db | `postgresql://admin:password123@localhost/prod` | ❌ 当时漏检，已修 |
| upload | 无认证文件上传 | ❌ 超出 7 条规则范围 |
| logging | 只有 `logger.error`，无集中错误追踪 | 全 PASS（假绿灯） |

连接串漏检的原因很典型：变量名是 `engine`，不是凭证键，所以"键名 → 值"的规则看不见。修法是加一条**值格式信号**——`scheme://user:password@host` 本身就是凭证泄露，不管变量叫什么。只匹配数据库连接串，跳过 https 基本认证和占位符密码（`password`、`${DB_PASS}`）。注意 `password123` 这类真实密码不能被当占位符跳过，所以占位符判断用精确相等而非子串包含。修完后 db 场景也抓到了。

## 假绿灯问题

logging 服务连集中错误追踪都没有，7 条规则却全报 PASS——因为声明型规则没看到 `prodgate: error_tracking=off` 就默认通过。这和第 8 轮修的"扫 0 文件报 PASS"是同类问题，只是更隐蔽。

修法不是把"没信号"改成 FAIL（静态扫推断不了运行时，所有项目都会挂），而是加 `UNCONFIRMED` 判定：声明型规则没看到标记时不再说 PASS，明确说"未确认"——不阻塞构建，但报告里看得见。信号型规则（cors/secrets）没信号仍是真 PASS，因为它们确实扫了代码没发现危险写法。

## 回答那个问题

**强 AI 会犯这些错吗？会，而且犯得自然。**

写这 35 行代码时我知道全部 7 条规则——这是最严苛的条件。我依然写了裸 `cors()`（因为 express 官方示例就这么写）、写了带 fallback 的密钥（因为"开发时先用默认值"是常见写法）、写了占位符密钥（因为"TODO 换成真实值"是标准做法）。这不是疏忽，是**强模型在"先让代码跑起来"压力下的默认行为**。

**这样做有意义吗？分规则看，答案不同。**

| 规则类型 | AI 会不会犯 | 工具有没有用 |
|---|---|---|
| 信号型：硬编码凭证、CORS wildcard | 会 | **有用**——这次 3 个问题都是真风险，且修完后都能抓到 |
| 声明型：5 条 `key=off` 规则 | 不会 | **基本没用**——AI 不知道 prodgate 语法，真实项目没人写 |

这是必须诚实说的：**7 条规则里有 5 条对 AI 生成代码不产生任何检出**。它们服务的是"团队主动声明我已故意关闭某项防护"这个场景，不是扫描场景。把整体检出率包装成"7 条规则全覆盖"会误导人。

**所以真正的价值不在规则数量，而在两件事：**

1. **抓 AI 的默认行为**：`cors()`、env fallback、占位符密钥——这些是模型反复会写的，且每个都有真实的部署风险（CORS 默认放开、生产环境用公开密钥、占位符直接上线）。
2. **案例库 + mutation 的验证闭环**：这次两个漏检不是靠读代码想出来的，是靠"AI 自己写一段代码然后扫"发现的。这比手工构造样本更接近真实使用。

## 这次迭代修了什么

- `cors()` 裸调用检测：expressjs/cors 默认 `origin: '*'`，没有 `*` 字面量可找，改为检测调用本身。明确配置 `cors({origin: ...})` 和 `require('cors')` 不误报。
- 明确凭证键名（exact match）放宽值判断：`WEBHOOK_SECRET = "whsec_example_..."` 这类占位符值不需要含数字——AI 常写这种格式并且真的部署。后缀式键名（`SOME_KEY`）保持严格。
- 加了一条误报防护：`ConfigAttribute("SECRET_KEY")` 这种全大写配置键名引用不是凭证值。这是在修占位符误报时发现 flask 框架代码被误伤后补的。

三个 AI 场景都进 mutation 样本（现在 10/10）和 `tests/real_world.rs`。

## 追加：按"AI 代码安全"方向补 SQL 注入

用户的方向建议是让 AI 写出的代码达到安全。盘点后发现 7 条规则里只有 3 条真正是安全规则，而 AI 实测里最严重的漏检是 SQL 注入：`conn.execute(text(f"SELECT * FROM users WHERE id = {user_id}"))`。OWASP 第一，AI 极常写，规则库一条都没有。

加了 `sql.injection` 信号型规则。过程中真实项目连出误报（`Text("...")` 匹配 `text(`、跨行窗口把 UI 代码当 SQL），最终收紧到极窄：只认同一行 f-string 里同时含 SQL 动词和插值。**代价是漏掉跨行和多段拼接的注入**——这是有意的取舍，误报比漏报更伤害工具的可用性。

诚实结论：加了这条之后，8 条规则里安全相关的是 4 条（cors/secrets/rls/sql），另外 4 条（rate/idempotency/tracking/backup）是可观测性、一致性和可靠性。这个工具是"AI 代码的生产就绪门禁"，不是完整的静态安全扫描器——SQL 注入只覆盖了 f-string 的一种形式，XSS、命令注入、路径遍历都没有。

如果你的问题是"有了强 AI 还需要这个工具吗"——**对信号型规则，需要**。强 AI 写代码的默认行为和资深工程师的审慎习惯之间有稳定差距，这个差距可以被静态规则可靠地抓住。

如果你的问题是"7 条规则是不是都有用"——**不是**。5 条声明型规则对 AI 代码无效，它们解决的是另一个问题（团队显式记录取舍）。这个边界不承认，工具就是在自欺。
