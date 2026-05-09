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
