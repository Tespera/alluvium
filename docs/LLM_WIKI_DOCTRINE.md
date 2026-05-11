# Alluvium · LLM Wiki 原则（项目圣经）

> 这份文档是 Alluvium 的产品宪法。
> 任何代码改动如果与这里的原则冲突，**改的是代码，不是原则**。
> 当 AI 觉得"实现这样更简单 / 当前架构装不下"时，请重读本文，再决定是不是要妥协。

本文的论点全部来自 [Karpathy 的 LLM Wiki gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)（2026-04 发布）。Karpathy 是发明者，我们是实现者；要做的是**忠实落地**他的模式，不是抄目录结构换个内核。

---

## 第一原则：Wiki 是 *compounding artifact*，不是 session 的并集

Karpathy 原文：

> "Instead of just retrieving from raw documents at query time, the LLM **incrementally builds and maintains a persistent wiki** ... The knowledge is compiled once and then *kept current*, not re-derived on every query."
>
> "the wiki is a **persistent, compounding artifact**."

**实操含义**：每次新 source 进来，wiki 应该**长进去**而不是**长出去**。新内容应该 *update existing pages*，不是 *append yet another page*。

**Alluvium 的合规性测试**：
- 同一主题（"Karpathy wiki 模式"）在 vault 里**只能有一个文件**。
- 一次 archive 应该 *touch* 10-15 个已有 page（更新它们），同时**只**新建少数真正未覆盖的 topic。
- 看 vault 一眼能知道知识结构，**不**应该看到 5 个 slug 各异、内容雷同的文件。

---

## 第二原则：Ingest 时 LLM **必须**看到 existing wiki

Karpathy 原文：

> "When you add a new source, the LLM doesn't just index it for later retrieval. **It reads it, extracts the key information, and integrates it into the existing wiki** — updating entity pages, revising topic summaries, **noting where new data contradicts old claims**, strengthening or challenging the evolving synthesis."
>
> "A single source might **touch 10-15 wiki pages**."

**实操含义**：Ingest 阶段的 LLM prompt **必须**喂入现有 wiki 的索引（至少 slug + title + 一行摘要）。LLM 看不到现有 wiki，就不可能"integrate into the existing wiki"——它只会从零生 fact list 然后追加。

**反模式（我们曾经这样做）**：
- distill prompt 只喂 transcript
- LLM 输出 slug 是凭空产生的（每次都重新发明）
- downstream 靠 slug 完全相同 / fuzzy 字符串相似度来去重——这都是事后挽救
- 结果：用户 vault 里 `claude-cli-后端-默认-api-key-fallback` / `claude-cli默认后端优于api-key` / `claude-cli-backend-default-over-api-key` 三个独立文件指同一观点

**正确做法**：
1. archive 启动时先扫 `<vault>/Alluvium/wiki/{concepts,entities}/` 收集所有 (slug, title, summary)
2. 渲染 distill prompt 时把这份索引注入 user template
3. system prompt 明确指令：
   - "If the transcript discusses something already covered by an existing topic, **use that topic's slug** and write a `body_markdown` that *updates / extends* it."
   - "Only create a new slug when the topic is genuinely uncovered."
4. 输出仍是 ExtractedFact 列表，但 page_slug 大概率匹配现有的——下游 merger 走 update path

---

## 第三原则：维护工具是必备的，不是 nice-to-have

Karpathy 列举了三种核心操作：Ingest（前面已讲）、Query（不在 Alluvium 范围）、**Lint**。在我们的实现里，"lint" 这个角色被拆成 3 个互补的工具，因为单一 lint 命令同时管不好三种不同的退化模式：

| Karpathy 说的 | Alluvium 工具 | 干什么 |
|---|---|---|
| "contradictions between pages / orphan pages" | `alluvium lint [--apply]` | 找近似重复的 topic pair，LLM 决策合并 |
| "stale claims that newer sources have superseded" + episode 类不该在 wiki | `alluvium audit [--apply]` | 扫每页判 keep / move-to-log / delete |
| "rewriting fragmented topic pages tighter" | `alluvium rewrite <slug> / --all` | LLM 把 episodic 框架重写成 timeless prose；多 fact-block 用 `alluvium consolidate` |

**三个工具加起来才是 Karpathy 说的完整 lint**。少任何一个都会让 wiki 在一周内退化：
- 没 `lint`：跨语言/拼写不同的同主题页累积成平行宇宙
- 没 `audit`：日记式"今天我做了 X"页淹没真知识
- 没 `rewrite`：真知识被锁死在一次性会话框架里，未来读不出 timeless 教训



Karpathy 原文：

> "**Lint.** Periodically, ask the LLM to health-check the wiki. Look for: contradictions between pages, stale claims that newer sources have superseded, **orphan pages with no inbound links**, important concepts mentioned but lacking their own page, missing cross-references, data gaps that could be filled with a web search."

**实操含义**：Wiki 必然会熵增——即使 ingest 阶段 LLM 知道现有 wiki，跨语言 / 跨表达 / 时间漂移仍会让重复偷溜进来。Lint **必须**存在，作为反熵机制。

**不是 v0.2/v1.0 才做**——只要 ingest 在跑，lint 就必须存在。否则用户 vault 第一周就开始烂。

**v0.2 扩展**：矛盾检测、orphan page 提示、缺失 cross-reference 检测、向量化 dedup（替代当前 lint 的多轴 Jaccard 启发式）。

---

## 第四原则：log.md 和 index.md 都是给 LLM 看的，不只给人看

Karpathy 原文：

> "**index.md** is content-oriented. It's a catalog of everything in the wiki — each page listed with a link, a one-line summary, and optionally metadata like date or source count. ... When answering a query, **the LLM reads the index first to find relevant pages, then drills into them**."
>
> "**log.md** is chronological. It's an append-only record of what happened and when — ingests, queries, lint passes."

**实操含义**：index.md 不是装饰——它是 LLM 在 ingest / query / lint 时**先读**的导航文件。所以它必须：
- 每行一个 topic：`[[concepts/foo]] — one-line summary`
- 摘要要够具体，让 LLM 看一行就能判断"我手上这个 fact 属不属于这个 topic"
- ingest 时由 LLM 维护，不是机械拼接 frontmatter

**当前 Alluvium 的 index.md** 是 `vault::index_updater` 拼 frontmatter 出来的——可以接受作为初始版本，但**必须确保 LLM 在 ingest prompt 里看得到等价信息**（即"existing topics 列表"）。

log.md 的作用：
- 给 lint 看历史决策（"上周已经 lint 过这两条，决定保留分立"）
- 给用户看时间线
- Karpathy 建议格式：`## [YYYY-MM-DD] <op> | <title>`，可 grep。我们当前 `- HH:MM <title> → [[pages]]` 风格语义等价但不严格 grep-friendly——可接受，不阻塞核心修复。

---

## 第五原则：Alluvium **故意**与 Karpathy 模式分歧的几处（要透明声明）

不是所有偏离都是 bug。以下分歧是 **deliberate**：

### 1. Source ≠ 文章 / PDF，而是 Claude Code session transcript

Karpathy 原文设想用户 *drop article* / *drop PDF* / *drop chapter*。Alluvium 的 source 是 transcript——内容更碎、更对话、更工程上下文密集。这导致：
- 一个 transcript 可能包含多个独立 topic（normal）
- 一个 transcript 也可能整段都是 episode 类（"今天 PR #90 被 revert 了"）—— 应该被过滤掉，不进 wiki，只进 log.md
- transcript 平均长度 50-500KB（远大于一篇文章），需要 byte-cap 截断（已实现）

### 2. Ingest 由 hook 自动触发，不是用户手动

Karpathy 原文："I have the LLM agent open on one side and Obsidian open on the other. The LLM makes edits based on our conversation, and I browse the results in real time."

Alluvium 反过来：用户用 Claude Code 写代码，session 一结束 hook 自动归档，**用户事后**在 Obsidian 里看结果。

**代价**：失去了 Karpathy 工作流里"用户和 LLM 实时讨论 ingest 决策"的环节。**补偿措施**：
- distill prompt 必须更严格（约束 LLM 的产出，因为没人实时纠错）
- lint 必须更主动（兜底 ingest 阶段的偶尔走偏）

### 3. 用户保留 marker 外的编辑权

Karpathy 原文："The LLM owns this layer entirely. ... You read it; the LLM writes it."

我们故意打破这一条：用 HTML marker `<!-- alluvium:fact id=X -->` 把 LLM 拥有的内容圈出来，marker 外是用户的——LLM 不会动。

**理由**：纯粹 "LLM owns 100%" 在交互式工作流里 OK，但 Alluvium 是被动归档——用户事后想加个人评注 / 修一处 LLM 错误，需要一个安全的"用户领地"。marker 模型让两边共存。

### 4. Query 不在 Alluvium 范围

Karpathy 原文有 Query 操作（用户问 wiki 问题）。Alluvium 不做这个——用户用 mcp-obsidian / Claude Code's file search / Obsidian 自己搜。

**理由**：Alluvium 是单向归档工具，反向查询是另一个产品的事。把范围拉小才能做精。

---

## 合规性评分卡（持续维护）

每次发版前用这张表打分：

| 原则 | 是否合规 | 证据 |
|---|---|---|
| 1. Compounding artifact（无重复） | ☐ | 用 `lint --dry-run` 看建议合并对数 |
| 2. Ingest 看现有 wiki | ☐ | 检查 distill prompt 的 user_template 是否注入 `existing_topics` |
| 3. Lint 存在且能用 | ☐ | `alluvium lint` 命令 + e2e 覆盖 |
| 4. index.md 信息足够 LLM 用 | ☐ | 抽查一行能否判断"这个新 fact 属不属于" |
| 5. Episode 不混入 wiki | ☐ | 抽查 5 个最新 concept 文件，看是否都是 durable knowledge |

未全部 ☑ 不发版。"v0.1.0 alpha 拿出去试水"不是借口——发不能用的东西只会让用户拉黑产品。

---

## 反模式列表（看到 AI 在写这些就要叫停）

1. ❌ distill prompt 不喂 existing wiki，直接让 LLM 出 fact list（违反原则 2）
2. ❌ "v0.2 再加 lint" —— 没有 lint 的 wiki 第一周就烂（违反原则 3）
3. ❌ 把 episode 类（"X 发生了 / 当前状态是 Y"）当 fact 写进 concepts/（违反原则 5.1）
4. ❌ 跨语言 slug 飘任其发生（"foo" / "foo-bar" / "Foo Bar" 共存）— 这是原则 1 的具体失败（违反原则 1+2）
5. ❌ index.md 只是 frontmatter 拼接，没有 summary 让 LLM 用（违反原则 4）
6. ❌ 把 LLM 的产出**只**写到 fact-block 里，不更新 index.md / log.md / 对侧引用（违反原则 1：Karpathy 说"single source touches 10-15 pages"）

---

## 致 AI 的一段话

读到这里，回去看你即将提交的代码。它符合上面 5 个原则吗？不符合就改。

如果觉得"原则 X 实现起来太复杂，先简化一下"——停。先和用户确认这个简化是否在 V01_SCOPE.md 里被允许（OUT 清单）。如果不在 OUT 清单，那就是必须做的——找到最小可行实现，做下去。

不要发不能用的东西。Alpha 不是借口。
