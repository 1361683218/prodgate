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
source = "owner/repo@<sha>"        # 必填；合成案例写 "synthetic"
source_url = "https://github.com/..." # 真实来源必填且必须 https
contributor = "你的名字"             # 必填
```

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

输出里 `PROVENANCE` 列分三档：
- `upstream` — 真实 commit，可核验，计入 trusted_count
- `synthetic` — 诚实标注的合成案例，允许存在但不冒充真实
- `UNVERIFIED` — 来源缺失/伪造，验证失败

## 当前案例

| 案例 | 来源 | 意义 |
|---|---|---|
| `cors.wildcard/c001` | expressjs/cors@a14f3bca (PR #39) | 真实的 wildcard origin bug 修复，一行 diff |
| `rls/c001` | supabase/supabase@49bda396 (PR #48394) | 真实的 RLS 安全 footgun：建表忘了 enable，任意登录用户可读写授权表 |
| `secrets.hardcoded/c001` | synthetic | 合成，来源待补 |

后 4 条规则（rate.limit / error.tracking / webhook.idempotency / backup）
的案例来源还空着——这就是最需要贡献的地方。找到真实的安全修复 commit，
按上面的格式提交一个目录就行。
