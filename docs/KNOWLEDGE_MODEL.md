# Knowledge Model · Karpathy LLM Wiki 在 Alluvium 里的实现

## 思想来源

Andrej Karpathy 2026-04 提出的 [LLM Wiki 模式](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)。核心反转：**LLM 不是搜索引擎**，是**图书馆员**——把原始材料编译成一份不断重写的 wiki，read 时查的是已经编译好的 wiki，不是原档。

> "Sessions stay as immutable raws; knowledge consolidates upward."

Alluvium 严格按这个模式组织 vault。

## Vault 物理布局

```
<your-vault>/Alluvium/
│
├── raw/                              # 不可变层（Layer 1）
│   └── sessions/
│       └── 2026-05-09T14-23_a1b2c3.md   # 一次 session 一份 transcript 副本
│                                          # 永不被 Alluvium 重写；用户可手动归档
│
├── wiki/                             # 维护层（Layer 2，LLM 不断重写）
│   ├── index.md                      # 主题分类 hub（按域分组列所有 topic 页）
│   ├── log.md                        # append-only 时间索引（一行一 session）
│   ├── overview.md                   # 高层执行摘要（手动维护或 consolidate 时重写）
│   │
│   ├── concepts/                     # 概念 / 模式 / 技术
│   │   ├── claude-code-hooks.md
│   │   ├── rust-cargo-workspaces.md
│   │   └── obsidian-frontmatter-merge.md
│   │
│   ├── entities/                     # 项目 / 工具 / 库 / 人 / 组织
│   │   ├── alluvium.md
│   │   ├── cognee.md
│   │   └── obsidian-second-brain.md
│   │
│   └── sources/                      # 单 source 摘要（可选层）
│       └── 2026-05-09_alluvium-design.md
│
└── CLAUDE.md                         # vault schema（不是仓库根的那份！）
```

**关键区分**：

- `raw/` 是**档案**——transcript 原始内容，Alluvium 写一次就不再动。用户想"那次 session 具体说了啥"还能查到。
- `wiki/` 是**知识**——entity 和 concept 的当前最佳理解，每次新 session 后被 merge 修订。
- `sources/` 是**可选层**——给"喜欢按 session 看"的用户保留一份摘要页。**v0.1 默认不开启**，用户在 config 里 opt-in。

## Topic 页：concept vs entity 怎么分

| | concept（概念） | entity（实体） |
|---|---|---|
| 定义 | 一种**思想 / 模式 / 技术 / 方法** | 一个**具体存在的事物**（项目、工具、人、库、组织） |
| 例子 | `karpathy-llm-wiki.md`、`atomic-file-write.md`、`prompt-caching.md` | `alluvium.md`、`claude-code.md`、`andrej-karpathy.md` |
| 测试问句 | "这是个**做法**还是**做这个的人/物**？" | 同左 |
| 默认数量 | 多（一个 session 通常更新 3-8 个 concept） | 少（一个 session 通常涉及 1-3 个 entity） |

**借鉴 Karpathy 原话**：分类边界本来就模糊，"intentionally abstract... will depend on your domain"。Alluvium 的 distill prompt 让 LLM 自己判断，但**给定一个固定的 schema**（见下）让分类落地。

## 单个 Topic 页的标准结构

每个 `concepts/*.md` 和 `entities/*.md` 长这样：

```markdown
---
title: Claude Code Hooks
type: concept                  # concept | entity
tags: [claude-code, automation, hooks]
created: 2026-04-15
updated: 2026-05-09
sources:                       # 这个页面的内容来自哪些 session
  - "[[../raw/sessions/2026-04-15T10-30_xxx]]"
  - "[[../raw/sessions/2026-05-09T14-23_a1b2]]"
relations:                     # 关系类型化（不是裸 wikilink）
  used-by: ["[[alluvium]]", "[[cognee]]"]
  related: ["[[claude-code-plugins]]"]
  supersedes: []
---

# Claude Code Hooks

## TL;DR
Claude Code 暴露的事件钩子机制，用于在 session 生命周期中插入外部脚本。

## 核心机制
（LLM 维护的正文，可能跨多个 session 累积）

## 已知踩坑
- 同步 hook 阻塞 UX（→ detached spawn 模式）
- ……

## 链接到这个页面的 source 摘要（可选）
- [[../sources/2026-04-15_alluvium-hook-design]] — 首次设计四 hook 架构
- [[../sources/2026-05-09_alluvium-design]] — 引入 PreCompact 快照
```

### Frontmatter Schema 强约定

| 字段 | 类型 | 必需 | 说明 |
|---|---|---|---|
| `title` | string | ✓ | 人读标题 |
| `type` | enum | ✓ | `concept` \| `entity` \| `source` \| `meta` |
| `tags` | array | ✓ | 主题标签，给 Obsidian graph 用 |
| `created` | date | ✓ | 首次创建日期 |
| `updated` | date | ✓ | 最后修订日期（每次 merge 后更新） |
| `sources` | array | ✓ | wikilink 数组，指向 `raw/sessions/*` |
| `relations.uses` | array | ⚪ | "本页面用到的东西" |
| `relations.used-by` | array | ⚪ | "用到本页面的东西" |
| `relations.related` | array | ⚪ | 相关 |
| `relations.supersedes` | array | ⚪ | 替代了哪些旧概念 |

**为什么要 typed relations**：Karpathy 原版用裸 wikilink，丢失了关系语义。三方实现（obsidian-second-brain）已经踩到坑。Alluvium v0.1 就用 typed relations，写 prompt 时让 LLM 输出关系类型。

### YAML 字段命名约定

YAML frontmatter 用 **kebab-case**（`used-by`、`see-also`、`updated-at`）——更 Obsidian / 业界惯例友好。

Rust 内部用 `snake_case`，通过 `#[serde(rename = "used-by")]` 桥接。

**真理来源是 [`templates/frontmatter.yaml.j2`](../templates/frontmatter.yaml.j2)**——该文件长啥样，Rust 必须匹配，反之不行。改 frontmatter 字段时先改模板，再让测试驱动改 Rust 类型。

### 用户手改保留的具体算法

每个 Alluvium 写入的段落用 HTML 注释包裹：

```markdown
<!-- alluvium:fact id=<short-hash> -->
## Section title

Body content.
<!-- alluvium:end -->
```

merge 时只动**带标记的块内**内容；块外的所有内容（用户手写）原样保留。frontmatter 字段属主划分见 [DECISIONS.md ADR-009](DECISIONS.md)。

## `wiki/log.md` 格式

按日期分组的时间索引。**Alluvium 只在尾部追加**，永远不修改任何已存在的行。

用户**也可以**在 log.md 里手写自己的笔记（自由形式段落、链接、记录 Claude Code 之外的事），Alluvium 把所有非自动行视为用户内容、不动。

实际写入规则：

- 自动行格式：`- HH:MM <session title> → [[touched]] [[pages]]`
- 自动行追加到对应日期组（`## YYYY-MM-DD`）末尾；如果该日期组不存在，在文件末尾建一个新日期组
- 不做时间排序（按到达顺序追加）；用户想排序自己改

示例：

```markdown
# Log

## 2026-05-09

- 14:23 alluvium 项目设计 → 触及 [[entities/alluvium]] [[concepts/karpathy-llm-wiki]] [[concepts/claude-code-hooks]]
- 16:01 修了 GoldPrice 的 chart 渲染 bug → 触及 [[entities/goldprice]] [[concepts/echarts-tooltip]]

## 2026-05-08

- 09:15 ……
```

每次 archive 后追加一行：`{HH:MM} {distilled-title} → {touched-pages}`。
查"最近做了啥"走这里。

## `wiki/index.md` 格式

按域分组列**所有 topic 页**的链接：

```markdown
# Index

## Projects (entities)
- [[entities/alluvium]] · Claude Code session auto-archiver
- [[entities/goldprice]] · 黄金价格追踪 macOS app

## Concepts · AI / LLM
- [[concepts/karpathy-llm-wiki]]
- [[concepts/prompt-caching]]
- [[concepts/structured-outputs]]

## Concepts · Rust
- [[concepts/rust-cargo-workspaces]]
- [[concepts/rust-async-runtimes]]

……
```

**Alluvium 写入区**用 HTML 注释标记圈定，用户自己加的内容放在标记之外不动：

```markdown
# Index

<!-- ALLUVIUM-INDEX-START -->
## Projects (entities)
- [[entities/alluvium]] · ...

## Concepts
- ...
<!-- ALLUVIUM-INDEX-END -->

## My notes
（用户手写区，Alluvium 永远不动这里）
```

**增量更新规则**：每次 archive 后，**只读受影响 topic 页的 frontmatter**（不读正文！）拼出索引行；这样就算 vault 里有 5000 个 topic 页也不会爆 LLM 上下文。详细算法见 `src/vault/index_updater.rs`。

## `<vault>/Alluvium/CLAUDE.md` 是什么

**注意**：这份 `CLAUDE.md` 跟仓库根的 `CLAUDE.md` 是**完全不同的两份文件**：

| | 仓库根 `CLAUDE.md` | vault 里的 `CLAUDE.md` |
|---|---|---|
| 位置 | `/Volumes/Work/VibeCoding/alluvium/CLAUDE.md` | `<user-vault>/Alluvium/CLAUDE.md` |
| 给谁看 | AI 开发 Alluvium 时 | AI 之后**读用户 vault** 时（比如用 Claude 查旧笔记） |
| 内容 | 项目宪法、约束、反模式 | vault 自身的 schema：什么算 entity、frontmatter 字段、链接约定 |
| 谁写 | 我（设计阶段手写） | Alluvium `init` 时自动生成到用户 vault |

vault 里的 CLAUDE.md 大致内容（init 时生成）：

```markdown
# Alluvium-managed vault schema

This vault is auto-maintained by Alluvium. Knowledge is organized as
Karpathy's LLM Wiki.

## Layout
- `raw/sessions/` — immutable transcript archives, NEVER edit
- `wiki/concepts/` — ideas, patterns, techniques
- `wiki/entities/` — projects, tools, people
- `wiki/log.md` — append-only timeline
- `wiki/index.md` — topic catalog

## Frontmatter schema
Every wiki page has: title, type, tags, created, updated, sources, relations.

## Conventions
- Wikilinks in `relations` field carry semantics (uses, used-by, related, supersedes).
- `raw/sessions/*` is an append-only log; do not modify.
- `wiki/log.md` is append-only; do not edit historical entries.
- `wiki/index.md` is auto-generated; manual edits will be overwritten.

## When you (Claude) operate on this vault
- Search starts at `wiki/index.md` to find relevant topic pages.
- For "what happened recently", read `wiki/log.md`.
- For grounding facts, read `wiki/concepts/` and `wiki/entities/`.
- Treat `raw/sessions/*` as evidence/citation, not primary text.
```

## Append-only Drift 的对策

Karpathy 模式最大的失败模式：**append-only 加新页快、重写老页慢**。topic 页越攒越散、越攒越长，几个月后变成不可读。

Alluvium 的对策：

1. **每次 archive 时 LLM 输出 fact 都过 merger**——不是简单 append 到 topic 页底部，而是让 merger 决定"这条 fact 是否已有"、"是否要替换某段"、"是否要重写整段"。
2. **`alluvium consolidate` 命令**（v0.1 手动触发；v0.2 cron）——LLM 重读单个 topic 页，重写为更紧凑的版本。保留所有事实，去掉冗余。
3. **每个 topic 页的 frontmatter `updated` 字段**驱动 consolidate 的优先级——很久没被修订但被多次 source 引用的页面，优先重写。

## 为什么不存"原始对话"作为最终笔记

这是 Alluvium 与同赛道项目最核心的区别。

如果你（AI）发现自己在写"把 session 的对话内容存为 .md"——**停下**。这不是 Alluvium 的形态。Alluvium 的形态是：

- session 的 transcript → `raw/sessions/<id>.md` （原档，不可变）
- 从 transcript 抽出的**事实** → 合并进 `wiki/concepts/*.md` 和 `wiki/entities/*.md`
- session 的元信息（标题 + 触及页面）→ 一行追加到 `wiki/log.md`

这三件事一起做，才完成一次 archive。

## `wiki/overview.md` 是谁的？

vault 自身的高层执行摘要。

**生命周期**：

- `alluvium init` 时建一个空骨架（标题 + 占位 _"Empty until you write something or run consolidate."_）
- 之后 **Alluvium 不会自动重写它**——用户可以一直留空、可以手写、可以放任何东西
- 用户调 `alluvium consolidate` 时，**如果检测到 vault 里有 ≥10 个 topic pages**，会**询问**用户是否让 LLM 重写 overview.md。只有用户明说 "是" 才动

**Why**：overview.md 是用户对 "我这个 vault 是关于什么的" 的解释权。Alluvium 不抢这个解释权。

## 与 Obsidian 的协作

vault 是普通的 Obsidian vault：

- 文件是 markdown，标签是 `#tag`，链接是 `[[wikilink]]`
- 用户可以手动编辑任何文件——`vault/merger.rs` 必须 diff-based、保留用户改动
- Obsidian graph view 自然展示 wiki 结构（typed relations 在 graph 上是不同颜色边）
- 用户安装 [Local REST API](https://github.com/coddingtonbear/obsidian-local-rest-api) 插件可让其他 AI 工具读 vault；Alluvium 自身不依赖
