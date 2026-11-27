# 案例库贡献指南

案例库是这个项目唯一的护城河。规则代码任何人都能抄，
从真实项目里攒出来的「好版本 / 坏版本」对照抄不走。

**贡献者只提交案例，不改规则代码。**

## 为什么这样设计

合成 fixture 只能证明规则对自己写的玩具样本有效。
真实案例会逼出规则没覆盖的写法——本项目就踩过两次：

1. `expressjs/cors` 的真实代码把 origin key 和 wildcard value 写在**两行**，
   合成 fixture 写在一行，规则漏检。
2. 修复后的代码里有 `options.origin === '*'`（**比较**不是赋值），
   规则把检查也当成配置，误报。

两个 bug 都是引入真实案例后才暴露的。**案例库的价值在于它对规则说真话。**

## 案例格式

```
cases/<rule-id>/<case-id>/
  case.toml      # 元数据，来源必须可核验
  good/<任意源码文件>
  bad/<任意源码文件>
```

`case.toml` 字段：

```toml
id = "cors.wildcard/c001"          # 必填
rule = "cors.wildcard"             # 必填，必须匹配已有规则 id
title = "一句话说明这个案例是什么"   # 必填
source = "owner/repo@<sha>"        # 必填；无来源写 "synthetic"
source_url = "https://github.com/..." # 真实来源必填且必须 https
contributor = "你的名字"             # 必填
expression = "synthetic"           # 样本表达层是合成的（见下方 anchored）
```

### source 和 expression 的区别

两个字段回答不同的问题：

- `source` / `source_url`：这个案例**为什么重要**有没有可核验的真实依据。
- `expression`：`good/` 和 `bad/` 里的**代码文本**是不是逐字来自那个 commit。

四档组合：

| provenance | source | expression | 含义 |
|---|---|---|---|
| `upstream` | 真实 commit | `real`（默认） | 真 commit，真代码 |
| `anchored` | 真实 commit | `synthetic` | 真 commit 证明风险，代码文本是我们按 prodgate 语法写的 |
| `synthetic` | `synthetic` | — | 无真实来源，纯合成，不许带 URL |
| `UNVERIFIED` | 缺失/形状错 | — | 拒绝，验证失败 |

`anchored` 是给声明型规则用的。像 `prodgate: rate_limit=off` 这种 opt-out 标记，真实开源项目不会写——它不可能出现在上游 commit 里。但"限流失效会导致什么"有真实 commit 可查。这类案例用真实 commit 锚定风险，用合成文本表达规则，不能冒充 `upstream`。

## 三条铁律

1. **来源不能编。** `validate` 会检查 source 形状、source_url 是否为 https
   且指向同一 repo。无来源或形状不对 → `UNVERIFIED`，案例被拒。
2. **合成必须标合成。** 标 `synthetic` 的案例 source_url 必须留空，
   不许拿一个看起来真的 URL 冒充。验证器会检查这个约束。
3. **good 必须 PASS，bad 必须 FAIL。** 分不开就是无效案例，
   跟规则写得好不好无关。

## 验证

```bash
./target/debug/prodgate cases .     # 验证全部案例
cargo test                          # 案例库是测试门禁的一部分
```

输出里 `PROVENANCE` 列分四档：
- `upstream` — 真实 commit + 真实代码，可核验
- `anchored` — 真实 commit 锚定风险，代码文本是合成的；可核验，计入 source-backed
- `synthetic` — 无真实来源，诚实标注，允许存在但不冒充真实
- `UNVERIFIED` — 来源缺失/伪造，验证失败

## 当前案例

7 条规则全部有案例，全部可核验：3 个 upstream、4 个 anchored。

| 案例 | provenance | 真实依据 |
|---|---|---|
| `cors.wildcard/c001` | upstream | expressjs/cors@a14f3bca (PR #39)，wildcard origin bug 的一行修复 |
| `rls/c001` | upstream | supabase/supabase@49bda396 (PR #48394)，建表忘 enable RLS，任意登录用户可读写授权表 |
| `rate.limit/c001` | anchored | express-rate-limit@34729c8b，异步限流处理器错误绕过错误中间件，限流失效开放 |
| `error.tracking/c001` | anchored | sentry-javascript@563e333a，Remix 渲染错误不触发 window.onerror / unhandledrejection |
| `webhook.idempotency/c001` | anchored | stripe-node@de7a7b72，重试 POST 没有幂等键会重复扣款 |
| `backup/c001` | anchored | postgres@ebd092bc，pg_dump 忽略扩展配置表外键顺序导致备份无效 |
| `secrets.hardcoded/c001` | upstream | securevault@3db9bd2f，移除 JWT_SECRET 硬编码 fallback，生产环境可能因环境变量缺失而使用公开密钥 |

案例库已全部有真实来源。新案例按上面的格式提交一个目录就行。
