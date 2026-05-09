# Architecture

## 模块树

```
src/
├── main.rs                        # CLI 入口
├── lib.rs                         # 让 tests/ 能引用
│
├── cli/                           # 子命令分发
│   ├── init.rs                    #   alluvium init       （向导 + 装 plugin）
│   ├── archive.rs                 #   alluvium archive    （Stop hook 调它）
│   ├── replay.rs                  #   alluvium replay     <id|--since 7d|--all>
│   ├── status.rs                  #   alluvium status     （看最近归档）
│   ├── dry_run.rs                 #   alluvium dry-run    （蒸馏不写盘）
│   ├── consolidate.rs             #   alluvium consolidate （定期重写碎片化的 topic 页）
│   ├── session_start.rs           #   SessionStart hook 入口
│   ├── pre_compact.rs             #   PreCompact hook 入口
│   ├── session_end.rs             #   SessionEnd hook 入口
│   └── uninstall.rs               #   alluvium uninstall
│
├── config/                        # 配置加载/写入
│   ├── mod.rs                     #   config.toml schema（[default] + 预留 [profiles.*]）
│   └── secrets.rs                 #   API key 走系统 Keychain
│
├── hook/                          # hook 工程层
│   ├── plugin_manifest.rs         #   生成/校验 .claude-plugin/plugin.json
│   ├── lock.rs                    #   文件锁（fs2）防并发写坏
│   ├── self_filter.rs             #   自我引用屏蔽（cwd 检测）
│   └── spawn.rs                   #   detached 子进程；hook 立即返回
│
├── transcript/                    # transcript 处理
│   ├── jsonl.rs                   #   流式读 ~/.claude/projects/*.jsonl
│   ├── reconstruct.rs             #   重建对话流，过滤 sub-agent 噪声
│   ├── merge_snapshots.rs         #   合并 PreCompact 快照 + 最终 transcript
│   └── metadata.rs                #   抽 cwd / session_id / start-end / model / tokens
│
├── distiller/                     # LLM 蒸馏层
│   ├── client.rs                  #   Anthropic HTTP（reqwest + serde + SSE）
│   ├── prompt.rs                  #   加载外部 prompt 模板（minijinja 渲染）
│   ├── budget.rs                  #   字段长度 cap（学 cognee 的 4-8KB）
│   └── parser.rs                  #   解析 LLM 输出为结构化 ExtractedFacts
│
├── extraction/                    # 从 transcript 抽 entity / concept / 决定 / 踩坑
│   └── mod.rs
│
├── wiki/                          # 决定哪些 topic 页要被改、各改什么
│   ├── locator.rs                 #   grep + 标题模糊匹配 → 找已有 topic 页
│   └── decide.rs                  #   阈值决策（v0.1 简单匹配，v0.2 加 embeddings）
│
├── vault/                         # 写盘层
│   ├── writer.rs                  #   atomic write（temp file + rename）
│   ├── merger.rs                  #   合并新内容 + 已有页，保留用户手改（diff-based）
│   ├── frontmatter.rs             #   YAML frontmatter 解析 + 合并
│   ├── log_appender.rs            #   往 wiki/log.md 追加一行
│   └── index_updater.rs           #   增量更新 wiki/index.md（只读各页 frontmatter）
│
├── consolidate/                   # alluvium consolidate 实现
│   └── mod.rs
│
└── log/                           # 归档日志
    └── status.rs                  # 给 status 命令读
```

## 数据流（端到端）

```
Claude Code session 进行中
         ↓
   SessionStart hook
         ↓
   alluvium session-start
   写 ~/.cache/alluvium/sessions/<id>/resolved.json
   （记下 vault 路径、是否要跳过、目标 topic 上下文）
         ↓
   ……会话正常进行……
         ↓
  （可能多次）PreCompact hook
         ↓
   alluvium pre-compact
   把当前 transcript 快照存到
   ~/.cache/alluvium/sessions/<id>/snapshots/0001.jsonl
         ↓
  （用户结束）Stop hook
         ↓
   alluvium archive --session <id>
   →  spawn detached 子进程立即返回（hook 不卡 Claude Code）
         ↓
   ┌─ detached 子进程 ─────────────────────────────────┐
   │  1. 加文件锁                                      │
   │  2. self_filter: 检查 cwd 是否 Alluvium 自己     │
   │  3. transcript: merge_snapshots + 重建对话        │
   │  4. distiller: 加载 recipe → 调 Anthropic API     │
   │  5. extraction: 解析为 ExtractedFacts             │
   │     (entities / concepts / decisions / gotchas)   │
   │  6. wiki: 对每条事实，定位目标 topic 页            │
   │  7. vault.merger: 加载已有页 → diff-based 合并    │
   │  8. vault.writer: atomic write 每个 topic 页      │
   │  9. vault.log_appender: log.md 追加一行           │
   │ 10. vault.index_updater: index.md 增量更新        │
   │ 11. log: 归档日志追加记录                         │
   └───────────────────────────────────────────────────┘
         ↓
   SessionEnd hook
         ↓
   alluvium session-end
   清理 ~/.cache/alluvium/sessions/<id>/
```

## 模块边界（职责分离）

| 模块 | **做** | **不做** |
|---|---|---|
| `cli/` | 解析参数、调度其他模块 | 业务逻辑 |
| `config/` | 读写 config.toml、Keychain 取 API key | 决定默认值之外的策略 |
| `hook/` | hook 的工程模式（lock / spawn / filter / plugin manifest） | 触发后的归档逻辑 |
| `transcript/` | JSONL → 结构化 ConversationData | LLM 调用、写盘 |
| `distiller/` | 调 Anthropic API、解析输出 | transcript 解析、决定写哪 |
| `extraction/` | distill 输出 → ExtractedFacts | 决定写哪个文件 |
| `wiki/` | 给每条 fact 决定目标 topic 页（new / existing） | 实际写盘 |
| `vault/` | 写 .md（atomic / merge / frontmatter / log / index） | LLM 调用、决定写哪 |
| `consolidate/` | 用 LLM 重写碎片化 topic 页 | 单次 archive 流程 |
| `log/` | 归档日志写入 + status 子命令读 | 用户配置 |

## 运行时状态文件

```
~/.config/alluvium/
└── config.toml                           # 主配置（用户可编辑）

~/.cache/alluvium/                         # 临时状态，可随便删
└── sessions/<session-id>/
    ├── resolved.json                     # SessionStart 写的元信息
    └── snapshots/0001.jsonl              # PreCompact 快照（多次累积）

~/.local/share/alluvium/                   # 持久数据
├── log/archive.jsonl                     # 每次归档一条记录
└── debug/<session-id>/                   # --debug 时落各阶段中间产物 JSON
    ├── 01-transcript-reconstructed.json
    ├── 02-distill-input.json
    ├── 03-distill-output.json
    ├── 04-extracted-facts.json
    └── 05-vault-writes.json
```

## 数据格式（模块间中间产物）

定义在 `src/lib.rs` 公开类型，所有跨模块边界的数据都用 serde 序列化：

- `ConversationData` — transcript 解析后
- `DistillerInput` — 喂给 LLM 的结构
- `DistillerOutput` — LLM 返回的原始结构
- `ExtractedFacts` — 抽完事实后的列表（每条带类型 + 目标 topic 候选）
- `VaultWritePlan` — 准备执行的写盘动作（哪些文件、各写什么 diff）
- `VaultWriteResult` — 实际执行的结果（带行号变化）

`--debug` 时每步落 JSON，用户用 Obsidian 直接打开看，出问题贴给 Claude 定位。

## 测试策略

`tests/` 下全是 e2e 集成测试，**测试名 = 用户可见的断言**：

```
tests/
├── e2e_archive_new.rs              ✓ 新会话能被归档成笔记
├── e2e_archive_update.rs           ✓ 已有主题会被更新而非重建
├── e2e_user_edit_preserved.rs      ✓ Obsidian 里的手改不被覆盖
├── e2e_hook_idempotent.rs          ✓ 重复装 plugin 不会重复注入
├── e2e_dry_run.rs                  ✓ dry-run 不写盘
├── e2e_replay.rs                   ✓ replay 旧 session 能重做
├── e2e_replay_bulk.rs              ✓ replay --since 7d 批量回填
├── e2e_pre_compact.rs              ✓ compact 前内容能被快照、不丢
├── e2e_concurrent_sessions.rs      ✓ 两个 session 同时结束不写坏文件
├── e2e_self_reference.rs           ✓ Alluvium 开发目录的会话不被归档
└── e2e_hook_returns_fast.rs        ✓ hook 在 100ms 内返回
```

单元测试放在 `src/<module>/` 下的 `#[cfg(test)] mod tests`，覆盖纯函数（解析器、frontmatter merge、self_filter 判定等）。
