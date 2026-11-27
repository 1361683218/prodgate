# Ralph Loop 壳 — 让 AI 自己迭代 prodgate

这是把"用户每轮说继续"替换成自动循环的试验壳。核心原则：

- **状态在磁盘，不在模型记忆**：每轮启动全新 AI 进程，只读 PROMPT.md + progress.md
- **裁判独立**：judge.sh 只调 `prodgate all`，AI 无权改裁判
- **完成要可验证**：所有任务打勾 **且** 裁判 PASS，才输出 COMPLETE

## 文件

| 文件 | 作用 | 谁能改 |
|---|---|---|
| `PROMPT.md` | 给 AI 的自包含指令（无对话历史依赖） | 人 |
| `progress.md` | 任务清单 + 每轮记录（唯一事实源） | AI |
| `judge.sh` | 裁判：跑 prodgate all，返回退出码 | 人（AI 不许改） |
| `run.sh` | bash 循环：调 agent CLI，检测 COMPLETE | 人 |

## 用法

```bash
# 需要本机装有支持 -p 打印模式的 agent CLI（claude / codex 等）
AGENT_CMD="claude -p" MAX_ITERATIONS=20 ./loop/run.sh

# 或用本环境自带的 Agent 工具手动跑一轮 —— 见 loop/EXPERIMENT.md
```

## 为什么这样设计

来自 Ralph Loop（Geoffrey Huntley）的最小形式：`while :; do cat PROMPT.md | agent; done`。
但加了两样关键东西：

1. **可验证裁判**：Ralph 原始形式靠"任务清单打勾"，我们额外要求 `prodgate all` 六道门禁全过——防止 AI 自己说"做完了"
2. **裁判权隔离**：AI 可以改源码、改测试、改 progress.md，但不能改 judge.sh（人持有）——这防的是 DGM 那个"Agent 删掉检测自己作弊的标记"的失败模式
