# Architectural Decisions (ADR Log)

每条决策都是讨论过、有原因的。**改动前先读 why**——大概率你（AI）漏看了某个权衡，否则不会想推翻它。

格式：每条 ADR 给出 status、context、decision、why、alternatives、when to revisit。

---

## ADR-001: 实现语言 = Rust

**Status**: Accepted (2026-05-09)

**Context**: 候选语言 Python / TypeScript / Go / Rust。用户是纯 VibeCoder，不写不读代码，所有实现走 AI。

**Decision**: 用 Rust。

**Why**:

1. **类型系统 = AI 自带质检员**：Rust 在编译期就拦截大半 bug，AI 在 commit 前已拿到 compiler 反馈。Python 的运行时报错对非读码用户是黑盒。
2. **单二进制分发**：`brew install alluvium` 即可，避免 Python pyenv 版本撞车导致用户朋友装不上的死亡之吻。
3. **长期稳定性**：Rust 项目两年后仍能跑，Python 受依赖弃坑影响大。
4. **AI 写 Rust 已够用**（2026 中），borrow checker / lifetime 错误 Claude 一次能修对。
5. **Prompt 迭代速度劣势可消除**：把 prompt 模板做成 `prompts/*.toml` 外部文件，不需要重新编译。

**Alternatives considered**:

- **Python**：Anthropic SDK 最成熟、prompt 迭代最快，但分发烂、运行时报错对 VibeCoder 不友好。**初期推荐过，被覆盖**。
- **TypeScript**：跟 Claude Code 同栈，但 npm 依赖地狱在 OSS 分发上比 pipx 还麻烦。
- **Go**：单二进制 + 性能 OK，但 Anthropic Go SDK 比 Rust 还薄、生态趋势上 AI infra 在向 Rust 偏。

**When to revisit**: 如果 v0.2 之后蒸馏 prompt 频繁迭代到不耐受、或者发现 `anthropic-rs` 社区库长期维护停滞。

---

## ADR-002: License = MIT/Apache-2.0 双协议

**Status**: Accepted (2026-05-09)

**Context**: 同赛道 8 个项目里 6 个 MIT、1 个 0BSD、1 个无 LICENSE（疏忽）。用户决定走 Rust 生态惯例。

**Decision**: 双协议 MIT + Apache-2.0，用户三选一。

**Why**:

1. Rust 社区主流（rustc / cargo / 大多数 crates 都是这个组合）。
2. Apache-2.0 防专利诉讼，大公司用更安心。
3. MIT 兼容广泛、轻量。
4. Day-1 第一个 commit 就放 LICENSE 文件——避免成为下一个 Roasbeef/obsidian-claude-code（无 LICENSE 等于 all-rights-reserved）。

**Alternatives considered**:

- **纯 MIT**：跟赛道一致、心智成本低。我（AI）原本推荐这个，被用户覆盖。理由也合理——双协议跟 Rust 生态对齐更重要。
- **0BSD**：过于激进，连署名都不要，PKM 工具不需要这种姿态。
- **AGPL**：会劝退用户，不适合个人 PKM。

**When to revisit**: 不轻易换协议（开源项目换协议是 breaking change，已贡献过的人需要重新同意）。

---

## ADR-003: 知识组织 = Karpathy LLM Wiki，不是 session 日志

**Status**: Accepted (2026-05-09)

**Context**: 最初设计是"每 session 一份 .md，topic resolver 决定 new vs update"，但本质仍是 session-shaped 笔记，只是带去重。

**Decision**: 严格按 Karpathy LLM Wiki 模式：transcript → `raw/sessions/`（不可变档案），抽出的事实 → `wiki/concepts/` 和 `wiki/entities/` 的 topic 页（合并修订）。session 不直接变成最终笔记。

**Why**:

1. **匹配用户原始需求**："不是完全 100% 保留 Claude Code 的输出结果，而是把它整理成精华文档"+"如果后续有更新则对文档进行更新"——这是 wiki 模式，不是 journal。
2. **Alluvium 这个名字本身就是这个 metaphor**：transcript 是上游的水，wiki 是下游沉积层。
3. **避免 "journal wearing wiki's clothes"**：纯 session-per-note 即使带去重，仍然是日志，知识没真正按主题沉淀。
4. **差异化卖点**：8 个同赛道项目没一个完整实现 Karpathy 模式 + 自动触发——这是 Alluvium 的独占生态位。

**Alternatives considered**:

- **Session-log + dedup**（最初的 v0.1 草稿）：体量小、上线快，但卖点弱、跟 obsidian-second-brain 高度重合。
- **混合模式**（session-log 默认，wiki 可选）：增加复杂度、混淆产品形态。

**Trade-offs accepted**:

- 模块复杂度更高（多了 extraction / wiki / log_appender / index_updater 模块）。
- 用户首次见到归档结果时不再是"一篇笔记出现"，而是"几个 topic 页被改了"。需要 `alluvium status` 把这件事讲清楚。
- 需要主动对治 append-only drift（见 ADR-007）。

**When to revisit**: 不会回退。append-only drift 处理失败的话改善对治策略，不是改回 session-log。

---

## ADR-004: Hook 安装方式 = Claude Code Plugin

**Status**: Accepted (2026-05-09)

**Context**: 候选方式：(a) `alluvium init` 直接编辑用户的 `~/.claude/settings.json` 注入 hook；(b) 打包成 Claude Code Plugin（`.claude-plugin/plugin.json`），用 `claude plugin install` 注册。

**Decision**: Plugin 形式（b）。

**Why**:

1. **安装/卸载干净**：Plugin 注册/注销是原子操作；改 settings.json 注入容易留残渣。
2. **不污染用户 settings.json**：用户可能跟其他工具混用，不该被 Alluvium 占字段。
3. **多 plugin 不打架**：Claude Code 的 plugin 机制处理多个 plugin 的 hook 合并；手编辑 settings.json 容易冲突。
4. **借鉴 cognee-integrations**——他们已验证这个 UX 路径可行。

**Alternatives considered**: 手编辑 settings.json，被覆盖。

**When to revisit**: 如果 Claude Code 的 plugin 机制有重大变更或下线（不太可能短期发生）。

---

## ADR-005: Profile 数量 = v0.1 单 profile

**Status**: Accepted (2026-05-09)

**Context**: 多 profile 支持（`--profile work` / `--profile personal`）有真实需求（多 vault 用户），但增加 v0.1 复杂度。

**Decision**: v0.1 仅单 profile。**配置文件结构预留 `[profiles.*]` 表**，未来加多 profile 不算 breaking change。

**Why**:

1. **大多数用户单 vault**——v0.1 体量优先。
2. **多 profile 涉及面广**：config schema、hook 注入逻辑、所有子命令的 `--profile` 参数、profile 切换逻辑——全部要改。
3. **配置结构预留是 0 成本的**——`config.toml` 里 `[default]` 就是默认 profile，将来 `[profiles.work]` 是叠加。

**Alternatives considered**: v0.1 即支持多 profile，被覆盖（过度设计）。

**When to revisit**: v0.2，或者用户社区出现明确多 profile 诉求时。

---

## ADR-006: Topic 去重策略 = 简单匹配（v0.1）

**Status**: Accepted (2026-05-09)

**Context**: 给 ExtractedFact 找对应 topic 页（"决定新建 or 更新已有"）需要某种相似度匹配。候选：(a) grep + 标题模糊匹配；(b) embeddings + 向量相似度。

**Decision**: v0.1 用 (a) 简单匹配。

**Why**:

1. **简单匹配的失败模式可控**：偶尔生成重复 topic 页，用户能直观看到，手动合并即可。LLM 后续也能做 consolidate。
2. **embeddings 引入新依赖**：需要运行本地 embedding model（ollama / sentence-transformers）或调外部 API。Rust 生态里搞 embeddings 还不够顺。
3. **冷启动问题**：vault 初期没什么内容，embeddings 没有比 grep 更准的优势。
4. **可控性**：grep + 标题匹配规则用户能理解、能调；embeddings 是黑盒。

**Alternatives considered**: v0.1 即用 embeddings，被覆盖。

**When to revisit**: v0.2，当 vault 累积到几百页 topic、grep 误匹配/漏匹配明显出现时。

---

## ADR-007: PreCompact Hook 必须包含

**Status**: Accepted (2026-05-09)

**Context**: 最初设计只挂 Stop hook。研究 cognee-integrations 后发现长 session 的 context compaction 会丢内容，需要 PreCompact 时快照。

**Decision**: 4 个 hook（SessionStart / PreCompact / Stop / SessionEnd）齐挂，**PreCompact 必须有**。

**Why**:

- Claude Code 长 session 中途自动触发 compaction，把对话压缩成摘要。
- 如果只 Stop hook 时读 transcript，拿到的是已压缩的版本，蒸馏质量大降。
- PreCompact 时快照原始 transcript 到 cache，archive 时合并快照 + 最终 transcript。

**Alternatives considered**: 只 Stop hook，被覆盖。

**When to revisit**: 如果 Claude Code 改变 compaction 行为（比如保留原始 transcript 不覆盖）。

---

## ADR-008: Hook 立即返回 = Detached Spawn

**Status**: Accepted (2026-05-09)

**Context**: 蒸馏要 5-30 秒（LLM 调用 + 多个 topic 页写盘）。如果 Stop hook 同步等完成，Claude Code 关窗口体验很糟。

**Decision**: Stop hook 内 spawn 一个 detached 子进程，hook 100ms 内返回。子进程托孤给 init（macOS 上是 launchd）。

**Why**:

1. **用户体验**：关 Claude Code 时不能感觉卡。
2. **生命周期解耦**：archive 子进程独立于 Claude Code 进程，Claude Code 关掉子进程继续跑。
3. **借鉴 cognee-integrations 的 `_spawn_detached_sync()` 模式**——已验证。

**Trade-offs accepted**:

- 用户关 Claude Code 时不知道 archive 是否成功——通过 `alluvium status` 查询，或 `~/.local/share/alluvium/log/archive.jsonl`。
- 系统重启会杀掉跑中的子进程——需要 archive 子进程支持 resume 或允许丢失最新一次。**v0.1 接受丢失**（用户重启时本来就要损失工作中的 session），v0.2 加 resume 支持。

**Alternatives considered**: 同步等待，被覆盖。

**When to revisit**: 如果 v0.2 引入持久化任务队列。

---

## ADR-009: 用户手改保留 = HTML 注释段标记

**Status**: Accepted (2026-05-09)

**Context**: 硬约束 #5（[CLAUDE.md](../CLAUDE.md)）要求 Alluvium merge 时只覆盖自己上次写的部分，保留用户在 Obsidian 里的手改。"diff-based merge" 是一个口号，初稿没落地到具体算法。

**Decision**: Alluvium 写入的每个段落用 HTML 注释标记包裹：

```markdown
<!-- alluvium:fact id=<short-hash> -->
## Section title

Body content.
<!-- alluvium:end -->
```

merge 时算法：

1. 解析现有页面，提取所有 `alluvium:fact` 块及其 id
2. 对新 ExtractedFacts 中的每个 fact：
   - 计算 fact id（page_slug + summary 的稳定哈希前 8 位）
   - 如果 id 已存在，**只替换那个块的内容**
   - 如果不存在，追加块到页面末尾
3. **块外的所有内容（用户手写）原样保留**
4. frontmatter 单独 merge：用户加的字段保留；Alluvium 拥有的字段（`updated`、`sources`、`relations.*`）覆盖。Rust 端用 const set 防止漂移

**Why HTML 注释而非 frontmatter section hash**:

- HTML 注释在 Obsidian 渲染时不可见，不污染阅读
- grep 友好，定位边界不依赖 frontmatter parser
- 用户在 Vim/VSCode 里编辑能看到块边界，知道 "动这里会被覆盖"

**Trade-offs accepted**:

- 用户手动删除注释标记后，merge 时该段会被当成 "用户手写" 而保留 + 追加新版本（重复）。需要在 [docs/CUSTOMIZING_PROMPTS.md](CUSTOMIZING_PROMPTS.md) 警告
- frontmatter 字段属主划分需要单独维护清单

**Alternatives considered**:

- 三方 diff（cache 存上次写的快照）：cache 丢就崩，恢复路径复杂
- frontmatter section hash：解析层依赖太重，failure mode 难调试
- 整段重写 + 用户改后停手（"标记已被人编辑"）：知识不再增长，弃

**When to revisit**: 如果用户反馈 HTML 注释影响他们的工作流（比如 Vim 里看着碍眼），考虑改成 frontmatter section hash。

---

## ADR-010: 运行时路径走 `directories` crate

**Status**: Accepted (2026-05-09)

**Context**: 初稿用 Linux XDG 路径（`~/.cache/`、`~/.local/share/`、`~/.config/`）当跨平台默认。在 macOS 上不符合系统惯例（应该是 `~/Library/...`）；Windows 上完全错。

**Decision**: 用 [`directories`](https://crates.io/crates/directories) crate 的 `ProjectDirs::from("dev", "alluvium", "alluvium")` 解析所有运行时路径。每平台映射见 [ARCHITECTURE.md § 运行时状态文件](ARCHITECTURE.md#运行时状态文件)。

**Why**:

1. macOS 用户：`~/Library/Caches/` 自动遵守 Time Machine 不备份的惯例
2. Windows 用户：未来 v0.2 支持 Windows 时不用迁移
3. 实现成本零——crate 一行解析

**Alternatives considered**:

- 自己写 OS 检测 + 路径分支：维护负担、易错
- 跨平台一律 XDG：macOS 用户体验差
- 只支持 macOS：放弃 Linux / Windows 用户群

**When to revisit**: 不会回退。

---

## ADR-011: Hook payload 走 stdin JSON，不走环境变量

**Status**: Accepted (2026-05-09)

**Context**: 初稿 `plugin.json` 里 Stop hook 命令写成 `alluvium archive --session $SESSION_ID`，假设 Claude Code 会展开 `$SESSION_ID` 环境变量。subagent 查 [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks.md) + 真实实现（cognee-integrations）后确认：**`$SESSION_ID` 不存在**。Claude Code 通过 stdin 给 hook 命令传 JSON payload。

**Decision**: 所有 hook 入口命令读 stdin JSON 拿 session 上下文，不依赖任何 session 相关环境变量。

stdin payload schema（所有 hook 事件相同）：

```json
{
  "session_id": "...",
  "transcript_path": "/path/to/.jsonl",
  "cwd": "/path/to/cwd",
  "permission_mode": "default",
  "hook_event_name": "SessionStart" | "PreCompact" | "Stop" | "SessionEnd"
}
```

实现：`src/hook/payload.rs` 的 `HookPayload` struct + `read_from_stdin()`。

**Why**:

1. 这是 Claude Code 官方接口约定；任何 "我猜应该这样" 的设计在第一次跑 hook 时就崩
2. cognee-integrations 真实在用这个 pattern（`payload = json.loads(sys.stdin.read())`），已验证
3. JSON 比环境变量结构化、可扩展（未来加字段不破坏向后兼容）

**Available env vars（只有路径相关）**:

- `$CLAUDE_PROJECT_DIR` — 项目根
- `$CLAUDE_PLUGIN_ROOT` — plugin 安装目录
- `$CLAUDE_PLUGIN_DATA` — plugin 持久数据

这些**不是** session 上下文，是 plugin 框架给的路径常量。

**`alluvium archive` 双模式**:

- **Hook 模式**（无 `--session` 参数）：读 stdin
- **Manual 模式**（`--session <id>`）：从参数取，用于 `replay` 命令内部调用

CLI 用 clap 的 `Option<String>` 表达，运行时根据是否提供切换。

**Alternatives considered**:

- 用 jq 抽 stdin 包一层 shell：增加依赖（jq）+ 多一层进程，没必要
- 让用户在 plugin.json 里写 jq 表达式：把字符串模板暴露给用户编辑，反人类

**When to revisit**: 如果 Claude Code 后续改用其他传递机制（极不可能短期发生）。

---

## ADR-012: Prompt 是事实抽取器，不是笔记生成器

**Status**: Accepted (2026-05-09)

**Context**: distiller prompt 有两种风格——

1. "把 transcript 蒸馏成一篇 markdown 笔记"（LLM 直接产出最终内容）
2. "从 transcript 抽出原子事实清单"（LLM 产出结构化 fact，code 决定排版到 topic 页）

风格 1 更直接，但 merge 时无法精确去重（用户改了一段话怎么定位"对应的旧版本"？）。
风格 2 复杂一点，但每个 fact 有 stable id（[ADR-014](#adr-014)），merge 时定位精准。

**Decision**: 用风格 2。LLM 产出原子事实，每个事实包含 `page_slug`（topic 页定位）、`page_title`、`summary`、`body_markdown`、`relations`、`type` ∈ {entity, concept, decision, gotcha}、`confidence`。code 拼装最终页面。

**Why**:
1. **Merge 精度**：原子 fact + stable id → 精确替换 ADR-009 的 HTML 注释段
2. **跨 session 累积**：同一 topic 页累积多次 session 抽出的 fact，每个 fact 独立可追溯
3. **多 recipe 同 schema**：minimalist / dev-journal / verbose 共享 ExtractedFact 结构，区别在 prompt style guide + 字段长度上限
4. **可测试性**：parser 验 schema、merger 验 id 行为、prompt 单独测渲染——三层独立单元测试

**Recipe 差异**（不在 schema 层，在 prompt style + budget 层）：

| | minimalist | dev-journal（默认） | verbose |
|---|---|---|---|
| 风格指引 | 陈述句、决定/结论 only | 第一人称过去式叙事 | pedagogical 全文 |
| 每 fact 字数上限 | ~400 | ~1200 | ~4000 |
| 每 session 最多 fact 数 | 5 | 12 | 20 |
| 含代码片段 | 否 | 否 | 是 |

**Trade-offs accepted**:
- LLM 产出 JSON 比产出 markdown 略更难（schema 约束 + JSON 转义）。给定 Haiku 4.5 的能力，可接受
- prompt 复杂度提升（要明确说 "你是图书馆员、产 fact 不写笔记"）
- code 端要写 fact → 页面的渲染层

**Alternatives considered**:
- 风格 1（LLM 直接写笔记）：merge 时退化到"全段替换"，丢失用户手改
- structured output via tool use：Anthropic API 有 `tools` 参数能强约束输出，但增加 prompt 工程量；v0.1 用普通 JSON 输出 + parser 容错就够

**When to revisit**: 如果 LLM 产 JSON 出错率 > 5%，考虑切换到 tool-use 强约束。

---

## ADR-013: LLM 输出 JSON Schema

**Status**: Accepted (2026-05-09)

**Context**: ADR-012 决定 prompt 是事实抽取器。但 fact 的具体字段、容错策略、版本兼容需要锁死。

**Decision**: 顶层响应 schema：

```json
{
  "title": "session 简短人读标题",
  "tags": ["3-7 个主题标签"],
  "extracted": [ /* ExtractedFact, ... */ ]
}
```

每个 `ExtractedFact`：

```json
{
  "type": "concept" | "entity" | "decision" | "gotcha",
  "page_slug": "kebab-case-canonical-name",
  "page_title": "Human Title",
  "summary": "1-3 句核心断言",
  "body_markdown": "2-6 段 freeform markdown",
  "relations": {
    "uses": ["other-slug"],
    "used-by": ["other-slug"],
    "related": ["other-slug"],
    "supersedes": []
  },
  "confidence": 0.85
}
```

**字段说明**：
- `type`：决定写到 `wiki/concepts/` 还是 `wiki/entities/`（decision/gotcha 归到最相关的 concept 或 entity 页）
- `page_slug`：dedup key、URL-friendly、wikilink 的 anchor
- `summary`：进 `log.md` 的一句话；merge 时是 fact id 哈希输入
- `body_markdown`：进 topic 页 HTML 注释段内的内容
- `relations`：典型化链接（4 类）
- `confidence`：0.0-1.0，< 0.5 警告 log + 仍写入

**容错策略**：
- 顶层 schema 错（`extracted` 不是数组、JSON 解析失败）→ **整个 archive 失败**，归档日志留错
- `extracted` 数组里某条 fact 字段缺失 → **丢这条**，继续处理其他
- LLM 返回 schema 外字段 → 静默忽略（forward-compat）
- `confidence < 0.5` → 警告但保留

**v0.1 故意省掉的字段**：
- `evidence: [{from_message_index, snippet}]`：好特性但增 token 成本，留 v0.2

**Why 这套 schema**：
1. 每字段都有用途，没有摆设
2. 类型枚举 4 类够用（再细分增加 prompt 决策负担）
3. relations 4 类（uses/used-by/related/supersedes）是 typed-relations 的最小有用集，超越 Karpathy 原版的裸 wikilink

**Alternatives considered**:
- 把 evidence 加进 v0.1：放弃，token 成本不值
- 多枚举 type（如 method / pattern / library）：放弃，4 类已经覆盖 90% case
- 用 tool use 强约束 schema：参见 ADR-012

**When to revisit**: 当用户报告"我想根据某条 fact 反查原 transcript"——届时加 evidence 字段。

---

## ADR-014: fact_id 算法 + frontmatter 三向合并

**Status**: Accepted (2026-05-09)

**Context**: ADR-009 钉了"HTML 注释段标记"的方向，但留了两个细节没定：
1. fact_id 怎么算？
2. frontmatter 字段属主怎么界定，merge 时如何处理用户改动？

**Decision**:

### fact_id

```rust
let normalized = summary
    .to_lowercase()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");
let input: String = format!("{page_slug}:{normalized}").chars().take(80).collect();
let hash = sha256(input.as_bytes());
let id = &hex_lowercase(hash)[..8];
```

- **sha256[:8]**：32 位 entropy，单 vault 几千 facts 时碰撞 ~0.0001%
- **normalize summary**：小写化 + 折叠空白 + 截前 80 字符。让 LLM 微调措辞时仍命中已有 fact
- **why sha256 而非 sha1**：sha1 在密码学语境淘汰；非密码用途也用现代算法是好习惯

### frontmatter 属主 + 三向合并

每个 topic 页 frontmatter 含一个 `_alluvium.last_written` 影子拷贝，merge 时三向 diff：

```yaml
title: Claude Code Hooks
type: concept
tags: [hooks, claude-code, my-custom-tag]   # 用户可见
created: 2026-04-15
updated: 2026-05-09
sources: [...]
relations:
  uses: [...]
_alluvium:
  schema_version: 1
  last_written:                              # Alluvium 上次写入的快照
    tags: [hooks, claude-code]
    relations:
      uses: [...]
    sources: [...]
```

**列表型字段三向合并**（`tags`、`relations.{uses,used-by,related,supersedes}`、`sources`）：

```
S = _alluvium.last_written.{field}      # Alluvium 上次写的
C = 当前文件的 {field}                   # 用户可能改过
A = Alluvium 这次想写的（LLM 抽出）

user_added   = C - S   （用户加的项）
user_removed = S - C   （用户删的项）

merged = (A ∪ user_added) ∖ user_removed
```

**字段属主细则**：

| 字段 | 行为 |
|---|---|
| `tags` | 三向合并 |
| `relations.uses` / `used-by` / `related` / `supersedes` | 三向合并（每子列表独立） |
| `sources` | 三向合并 |
| `created` | 第一次写定，永不改 |
| `updated` | 每次 merge 重写为当前时间 |
| `title` / `type` | Alluvium 覆盖（用户想改用文件名 rename） |
| `_alluvium.*` | Alluvium 全权管理 |
| **任何其他字段**（`priority`、`status`、`aliases`、`user_tags` 等） | **完全保留**，Alluvium 永不动 |

**Why 自带影子（在 frontmatter 里）而非外部 cache**：

- vault 是可移植的（用户用 iCloud / Syncthing 多机同步）；merge 状态应跟 vault 走
- 用户清 `<data>/`、换机器、备份恢复都不影响 merge 正确性
- 透明：用户能看到"Alluvium 记得我上次写了啥"，调试友好

**Trade-off 接受**：
- frontmatter 视觉膨胀 2x。`_` 前缀走 Obsidian 的"内部字段"惯例，properties 面板可折叠
- 加一个字段就要更新 last_written → 写盘体积稍大，可忽略

**Alternatives considered**:
- "只增不减"：体验差，用户删 tag 后下次会被加回来。弃
- 外部 cache file `<data>/last-frontmatter/<slug>.yaml`：vault 不可移植
- 两套 tags 字段（`tags` + `alluvium_tags`）：Obsidian 不识别第二套，graph view 跑偏

**When to revisit**: 如果 frontmatter 视觉膨胀引起用户抱怨，考虑改成外部 cache + 接受不可移植性，或加 frontmatter 折叠功能。
