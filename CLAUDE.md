# Alluvium · 项目宪法

> 你（AI）每次进入这个仓库，**先读这份文件**。这里是项目的核心约束。任何与本文件冲突的实现都是错的——遇到分歧请向用户提问，**不要自行决定推翻这里的约定**。

## 一句话

Alluvium 是一个 Claude Code session 自动归档器：每次 Claude Code 会话结束后，被动地把对话蒸馏成"精华笔记"，按 Karpathy 的 LLM Wiki 模式合并进用户的 Obsidian vault。

## 卖点（README slogan）

> **You don't save your sessions. Your sessions save themselves.**

跟同赛道 8 个 obsidian 类项目（claudesidian / claudian / claude-obsidian / obsidian-second-brain 等）的核心差异：

- 别人是**主动唤起**（slash command / 侧边栏 / MCP 按需调用）；Alluvium 是**被动归档**（hook 触发，用户无感）。
- 别人多数存原始对话或简单摘要；Alluvium **蒸馏后合并到 topic 页**，不留 session 日志。
- 别人多数是 Obsidian 插件或 Claude 内嵌助手；Alluvium 是**外部 Rust 二进制 + Claude Code Plugin**。

如果 AI 在写"侧边栏"、"slash command"、"per-session note 文件"这类东西，**停下来重读本文**——多半跑偏了。

## 知识组织模型（核心，详见 [docs/KNOWLEDGE_MODEL.md](docs/KNOWLEDGE_MODEL.md)）

vault 内三层：

```
<vault>/Alluvium/
├── raw/sessions/        # 不可变原档：每次 session 一份 transcript 副本
├── wiki/
│   ├── index.md         # 主题分类 hub
│   ├── log.md           # append-only 时间线（一行一 session）
│   ├── overview.md
│   ├── concepts/        # 概念、模式、技术（topic 页）
│   ├── entities/        # 项目、工具、人、库（topic 页）
│   └── sources/         # 单 session 摘要（可选层）
└── CLAUDE.md            # vault 内的 schema 文件（注意：这个 CLAUDE.md ≠ 仓库根的这份）
```

**核心原则**：知识沉淀到 topic 页（concepts/ 和 entities/），session **不直接变成笔记**。session 的最终去处是 `raw/sessions/` 作为不可变原档，知识被抽出后向上汇总进 wiki。

## Hook 架构（4 个，详见 [docs/HOOKS.md](docs/HOOKS.md)）

```
SessionStart  →  写 ~/.cache/alluvium/sessions/<id>/resolved.json
PreCompact    →  快照当前 transcript 防 compact 后丢
Stop          →  spawn detached 子进程，hook 立即返回（不卡 Claude Code）
SessionEnd    →  清理临时文件
```

**PreCompact 不能省**——长 session 中途 compaction 会丢原始细节。
**detached spawn 不能省**——蒸馏要 5-30 秒，hook 必须 100ms 内返回。

## 硬约束（不可违反）

1. **用户是纯 VibeCoder**，不写也不读代码。任何代码层选项都要翻译成"对用户可见的影响"才呈现。
2. **prompt / config / 中间产物必须是外部文件**（toml / yaml / md / json），用户能直接编辑、能用 Obsidian 打开看。
3. **测试是用户的眼睛**——功能正确性靠 `cargo test` 通过/失败回答。测试名要表达用户可见的断言（参考 `tests/e2e_*.rs` 命名）。
4. **单二进制分发**——`cargo build --release` 出来的二进制独立运行，不依赖 Python / Node 等环境。
5. **保留用户手改**——用户可能在 Obsidian 里手动改过任何 wiki 页。merge 时只能覆盖 Alluvium 上次写的部分，必须 diff。
6. **自我引用屏蔽**——cwd 在 Alluvium 自己开发目录的 session 不归档（避免递归）。

## 反模式（看到 AI 在写这些，就是跑偏了）

- ❌ 给一个 session 单独建一份 .md 作为最终笔记（**应该合并到 topic 页**）
- ❌ 在 Stop hook 里同步等 LLM 调用完成（**应该 detach**）
- ❌ 让用户手编辑 `~/.claude/settings.json` 注入 hook（**用 `.claude-plugin/plugin.json`**）
- ❌ 蒸馏时把整段 transcript 无截断塞 prompt（**每字段要有 byte cap，借鉴 cognee 的 4-8KB**）
- ❌ "顺手"加 v0.2+ 才该做的功能（multi-profile / embeddings dedup / scheduled consolidate / proactive discovery）。看 [docs/V01_SCOPE.md](docs/V01_SCOPE.md)。
- ❌ 把 `index.md` 全量重写（**增量更新**：抽各 topic 页 frontmatter 拼，不读正文，避免上下文爆炸）
- ❌ 主动新建 dataset / profile / vault layout 变体（**v0.1 单 profile 单 vault**，配置文件结构留好门即可）

## v0.1 范围

详见 [docs/V01_SCOPE.md](docs/V01_SCOPE.md)。

## 决策记录

详见 [docs/DECISIONS.md](docs/DECISIONS.md)。**改动任何已经做出的架构决策前**，先读那里的 ADR，理解 why——不要破坏已经达成的共识。

## 致谢与差异化

详见 [docs/PRIOR_ART.md](docs/PRIOR_ART.md)。

## 文档地图

```
CLAUDE.md                        ← 你在读这个（项目宪法）
README.md                        公开 README（English）
docs/
  ARCHITECTURE.md                模块图 + 数据流
  KNOWLEDGE_MODEL.md             Karpathy wiki 在 vault 里的具体长法
  HOOKS.md                       4 个 hook 的契约 + 工程模式
  DECISIONS.md                   ADR 决策记录（共 8 条）
  V01_SCOPE.md                   v0.1 IN / OUT 清单
  PRIOR_ART.md                   致谢 + 8+1 同类项目对比
  CUSTOMIZING_PROMPTS.md         面向终端用户（占位）
```

## 致 AI 的一段话

用户是纯 VibeCoder，**你（AI）需要替他守住产品方向和工程质量**。当你不确定时：
- 先读这份 CLAUDE.md
- 再读 docs/ 里相关的那一份
- 仍不清楚就问用户，**不要靠"合理推测"自己定**

写代码前先解释你打算做什么、改了哪几个模块、哪些行为会变。用户没有读代码 review 的能力，所以**你给他看的不是 diff，是行为变化的文字描述**。
