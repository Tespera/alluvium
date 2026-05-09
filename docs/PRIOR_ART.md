# Prior Art · 致谢与差异化

Alluvium 站在前人肩膀上。这份文档列出我们参考过的项目，注明它们各自的角色，明确我们**借鉴**了什么、**没借鉴**什么。

赛道很拥挤——Claude + Obsidian 类工具光本文档列的就有 9 个。Alluvium 的差异化卖点在 [CLAUDE.md](../CLAUDE.md)：**hook 触发的被动归档 + Karpathy wiki 模式**——这两条同时具备的项目目前没有。

---

## 直接借鉴（对 Alluvium 设计有具体影响）

### eugeniughelbur/obsidian-second-brain · MIT
**[github.com/eugeniughelbur/obsidian-second-brain](https://github.com/eugeniughelbur/obsidian-second-brain)**

**它是什么**: Claude Code skill，把 vault 当 AI-first 的二脑。31 个 slash command + 调度 agent，明确"更新已有页而非追加"，有 `/obsidian-reconcile` 处理矛盾。

**借鉴**:
- "更新而非追加"的产品哲学——直接对应 Alluvium 的 `vault::merger` 模块
- nightly consolidate 思路——Alluvium v0.1 做手动版 `alluvium consolidate`，v0.2 做调度版
- prompt 让 LLM 同时输出 entity + concept 分类的结构
- Karpathy 模式的延伸（typed relations、多 source 引用）

**没借鉴**:
- 它是手动 `/obsidian-save` 触发，Alluvium 是 hook 自动触发——这是 Alluvium 的差异点

---

### topoteretes/cognee-integrations (Claude Code 子目录) · Apache-2.0
**[github.com/topoteretes/cognee-integrations](https://github.com/topoteretes/cognee-integrations/tree/main/integrations/claude-code)**

**它是什么**: Cognee（知识图谱）的 Claude Code 集成。捕获工具调用 trace + Q&A 喂给 graph DB，session 启动时反向注入相关上下文给 Claude。

**借鉴**（重要）:
- **PreCompact hook 模式**——他们的 `pre-compact.py` 启发了 Alluvium 必须挂 PreCompact 的决策（[ADR-007](DECISIONS.md#adr-007)）
- **Detached spawn 模式**（`_spawn_detached_sync`）—— Alluvium `src/hook/spawn.rs` 的设计原型（[ADR-008](DECISIONS.md#adr-008)）
- **resolved.json 模式**——SessionStart 写一份元信息文件供后续 hook 读取
- **byte cap**（4-8KB per field）—— Alluvium `src/distiller/budget.rs`
- **Plugin 打包**（`.claude-plugin/plugin.json`）—— [ADR-004](DECISIONS.md#adr-004)
- **Sync lock**——文件锁防并发，`src/hook/lock.rs`
- **Self-reference 屏蔽**——他们屏蔽 `cognee` 关键词；Alluvium 类比检查 cwd 路径
- **审计日志**——Alluvium `~/.local/share/alluvium/log/archive.jsonl`

**没借鉴**:
- 他们的目标存储是**graph DB**，给 LLM 读；Alluvium 目标存储是 **markdown vault**，给人读。完全不同的产品。
- 他们的形态是**实时 RAG cache**（每次用户提问注入相关上下文）；Alluvium 是**离线归档**。

---

### AgriciDaniel/claude-obsidian · MIT
**[github.com/AgriciDaniel/claude-obsidian](https://github.com/AgriciDaniel/claude-obsidian)**

**它是什么**: Claude + Obsidian wiki 构建器，明确实现 Karpathy LLM Wiki 模式。`/wiki` `ingest` 等命令，按 source 类型（Website / GitHub / Business / Personal / Research / Book）分模式建 8-15 个 wiki 页。

**借鉴**:
- Karpathy 模式的初步实现示范（验证可行性）
- session start/stop 钩子刷新 cache 的思路（虽然他们只用钩子刷缓存，没做真正的 archive）

**没借鉴**:
- 他们建新页为主，dedupe 是部分的；Alluvium 走"merge 优先"
- 触发是手动 `/wiki`，Alluvium 是自动

---

### Andrej Karpathy · LLM Wiki Gist
**[gist.github.com/karpathy/442a6bf555914893e9891c11519de94f](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)**（2026-04-04 发布）

**它是什么**: 提出 LLM Wiki 模式的源头 gist。三层结构：`raw/`（不可变源）+ `wiki/`（LLM 维护）+ `CLAUDE.md`（schema）。

**借鉴**: 整个知识组织模型（[KNOWLEDGE_MODEL.md](KNOWLEDGE_MODEL.md)）、[ADR-003](DECISIONS.md#adr-003) 决策的思想源头。

**没借鉴**:
- Karpathy 自己强调"小到中等、慢节奏、人工策展"——Alluvium 自动化更激进，所以必须主动对治 append-only drift。
- 原版没有 typed relations、增量 index——Alluvium 强化这两块。

---

## 同赛道对比（没直接借鉴，但定义了 Alluvium 的差异化坐标系）

### heyitsnoah/claudesidian · MIT
**[github.com/heyitsnoah/claudesidian](https://github.com/heyitsnoah/claudesidian)**

npm 脚本集（Firecrawl / Gemini wrapper）+ skills，存原文全文，无蒸馏。手动触发。

**Alluvium 区别**: Rust 单二进制 vs npm；蒸馏 vs 原文；hook 自动 vs 手动。

---

### YishenTu/claudian · MIT
**[github.com/YishenTu/claudian](https://github.com/YishenTu/claudian)**

Obsidian 插件，把 Claude Code 嵌进 vault 当聊天侧边栏。手动唤起。

**Alluvium 区别**: 外部 daemon vs in-vault chat；归档 vs 实时对话；不写 .ts plugin 代码而走 plugin 包装。

---

### iansinnott/obsidian-claude-code-mcp · 0BSD
**[github.com/iansinnott/obsidian-claude-code-mcp](https://github.com/iansinnott/obsidian-claude-code-mcp)**

MCP server 把 vault 暴露给外部 Claude（WebSocket / HTTP / SSE）。纯文件桥。

**Alluvium 区别**: 这俩**互补**，可同时安装——MCP server 让 Claude 能读 Alluvium 写的 wiki，Alluvium 写 wiki 但不暴露查询接口。

---

### ballred/obsidian-claude-pkm · MIT
**[github.com/ballred/obsidian-claude-pkm](https://github.com/ballred/obsidian-claude-pkm)**

Goal-cascade PKM workflow（年→周→日 slash command）+ PostToolUse 自动 commit。操作的是手写 PKM，不读 transcript JSONL。

**Alluvium 区别**: 处理 transcript vs 手写笔记；归档 vs 个人计划。

---

### Roasbeef/obsidian-claude-code · 无 LICENSE ⚠️
**[github.com/Roasbeef/obsidian-claude-code](https://github.com/Roasbeef/obsidian-claude-code)**

Obsidian 侧边栏 + MCP 暴露命令。手动触发。**无 LICENSE 文件**——法律上 all-rights-reserved，无法借鉴或参考代码。

**反面教训**: Day-1 commit 必须包含 LICENSE。Alluvium 已包含 [LICENSE-MIT](../LICENSE-MIT) 和 [LICENSE-APACHE](../LICENSE-APACHE)。

---

### deivid11/obsidian-claude-code-plugin · MIT
**[github.com/deivid11/obsidian-claude-code-plugin](https://github.com/deivid11/obsidian-claude-code-plugin)**

Obsidian 侧边栏面板，按当前 note 跑 Claude Code / OpenCode 改语法 / TOC / diagram。完全手动，per-note 操作。

**Alluvium 区别**: vault 全局归档 vs 单 note 操作。

---

## 未参考但值得了解的相邻产品

- **Hermes (Nous Research)**——本地 daemon AI agent，跨编辑器后端。如果 Alluvium v0.2 做跨客户端归档，会借鉴它的进程模型。
- **OpenClaw**——通过 IM 平台远程指挥 Claude Code 的工具。可作为 Alluvium 触发器（"嘿，把今天那个 session 整理一下"）。
- **Mem.ai / Reflect / Tana**——商业 AI-first 笔记产品。Alluvium 不与之竞争——它们是端到端 SaaS，Alluvium 是开源 CLI 给已有 Obsidian vault 增强。

---

## 致谢声明（README / NOTICE 中复述）

Alluvium owes specific debts to:
- **Andrej Karpathy** for the [LLM Wiki pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) gist.
- **eugeniughelbur** ([obsidian-second-brain](https://github.com/eugeniughelbur/obsidian-second-brain)) for showing how to combine distillation + rewrite-don't-append at the prompt level.
- **topoteretes** ([cognee-integrations](https://github.com/topoteretes/cognee-integrations)) for the four-hook architecture, detached-spawn pattern, and plugin packaging approach.
- **AgriciDaniel** ([claude-obsidian](https://github.com/AgriciDaniel/claude-obsidian)) for the early Karpathy-pattern implementation we studied.

Differences from each are intentional and documented in this file.
