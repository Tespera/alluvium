# Hook 架构

## 4 个 Hook 的契约

Alluvium 安装为 Claude Code Plugin，在 `.claude-plugin/plugin.json` 里声明 4 个 hook。**少一个都不行**——下文解释每一个为什么不能省。

```json
{
  "hooks": [
    { "event": "SessionStart", "command": "alluvium session-start" },
    { "event": "PreCompact",   "command": "alluvium pre-compact" },
    { "event": "Stop",         "command": "alluvium archive" },
    { "event": "SessionEnd",   "command": "alluvium session-end" }
  ]
}
```

## Hook 接收上下文的方式（关键）

**Claude Code hook 通过 stdin 给命令传一个 JSON payload**——不是环境变量、不是命令行参数。

每个 hook 命令收到的 stdin 长这样：

```json
{
  "session_id": "abc123",
  "transcript_path": "/path/to/transcript.jsonl",
  "cwd": "/current/working/directory",
  "permission_mode": "default",
  "hook_event_name": "Stop"
}
```

所有 hook 事件（SessionStart / PreCompact / Stop / SessionEnd / UserPromptSubmit / PostToolUse）的 stdin schema 相同；用 `hook_event_name` 字段区分。

**可用的环境变量只有路径相关的**（不是 session 上下文）：
- `$CLAUDE_PROJECT_DIR` — 项目根
- `$CLAUDE_PLUGIN_ROOT` — plugin 安装目录
- `$CLAUDE_PLUGIN_DATA` — plugin 持久数据目录

**`$SESSION_ID` 不存在**——这是早期设计的误解，已修正。详见 ADR-011 in [DECISIONS.md](DECISIONS.md)。

实现：见 [`src/hook/payload.rs`](../src/hook/payload.rs) 的 `HookPayload` 结构 + `read_from_stdin()`。每个 hook 子命令的入口先调它解析 payload，再走自己的逻辑。

## SessionStart

**何时触发**：Claude Code 会话开始时（用户起一次新对话）。

**Alluvium 做的事**：

1. 读 ``<config>`/config.toml` 决定本次 session 的 vault、profile、recipe
2. 检查 cwd 是否触发 self-filter（开发 Alluvium 自己时跳过）
3. 写 ``<cache>`/sessions/<session-id>/resolved.json`：
   ```json
   {
     "session_id": "...",
     "started_at": "2026-05-09T14:23:00Z",
     "cwd": "/Users/eric/...",
     "skip_reason": null,
     "vault_path": "/Users/eric/Documents/MyVault",
     "alluvium_subdir": "Alluvium",
     "recipe": "dev-journal"
   }
   ```

**为什么需要**：解决"hook 之间共享上下文"的问题。后续 PreCompact / Stop / SessionEnd 都直接读这份 `resolved.json`，不用重新解析配置、不用重新走 self-filter 判定。借鉴自 cognee-integrations。

**hook 应在 50ms 内返回**——只读配置 + 写一个小 JSON。

## PreCompact

**何时触发**：Claude Code 准备做 context compaction 时（即上下文压缩）。可能在一次 session 中触发**多次**。

**Alluvium 做的事**：

1. 读 `resolved.json` 拿 session id
2. 抓当前的 transcript JSONL（截至此刻）
3. 复制一份到 ``<cache>`/sessions/<id>/snapshots/{N:04d}.jsonl`（N 递增）

**为什么不能省**：长 session 中途 compaction 后，原始细节会被压缩成摘要。如果只挂 Stop hook，等 session 结束时 transcript 里只剩压缩版，蒸馏出来的笔记**丢内容**。snapshot 把 compact 之前的状态留下来，archive 时再合并。

**借鉴自 cognee-integrations 的 `pre-compact.py`**——他们用同样思路构建 "Memory Anchor"。

**hook 应在 100ms 内返回**——只是文件复制操作。

## Stop

**何时触发**：用户结束 Claude Code 会话（Ctrl+C / 关窗口 / quit 命令）。

**Alluvium 做的事**：

```rust
// 关键：detached spawn，hook 立即返回
let _ = Command::new("alluvium")
    .args(["archive", "--session", &session_id])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .process_group(0)              // 脱离 hook 的进程组
    .spawn()?;
// 不 wait，让子进程托孤给 init
```

子进程做的事（在后台跑 5-30 秒）：

1. 加文件锁 ``<cache>`/lock`（防两个 session 同时归档撞同一个 vault 文件）
2. 检查 `resolved.json.skip_reason`，如果非空（self-filter 命中）直接 exit
3. 调 `transcript::merge_snapshots` 合并 PreCompact 快照 + 最终 transcript JSONL
4. 调 `distiller::run` 蒸馏（带 byte cap，按 recipe）
5. 调 `extraction::run` 抽 ExtractedFacts
6. 调 `wiki::locate` 给每条 fact 找目标 topic 页
7. 调 `vault::merger::merge_each` 合并写盘（atomic）
8. 调 `vault::log_appender::append` 追加到 log.md
9. 调 `vault::index_updater::update_incremental` 更新 index.md
10. 调 `log::record` 写归档日志

**为什么必须 detach**：

- Claude Code 关窗口时，所有 hook 同步执行。hook 卡 5-30 秒等 LLM 回复 → 用户感觉"Claude Code 卡死"，会强杀。
- detached 后子进程托孤给 init（macOS 是 launchd），跟 Claude Code 进程生命周期解耦。Claude Code 可以马上关，子进程在后台跑完。

**hook 应在 100ms 内返回**——只是 spawn + close。`tests/e2e_hook_returns_fast.rs` 验这一点。

**借鉴自 cognee-integrations 的 `_spawn_detached_sync()` 模式**。

## SessionEnd

**何时触发**：Claude Code 会话彻底结束（在 Stop 之后）。

**Alluvium 做的事**：

1. 读 `resolved.json`
2. 删除 ``<cache>`/sessions/<id>/` 整个目录

**为什么需要**：清理快照临时文件。如果不清理，长期累积会占盘。

**注意**：SessionEnd 时，Stop 触发的 detached archive 子进程**可能还在跑**——它们独立读各自的输入文件（transcript JSONL 在 `~/.claude/projects/`，不在 cache 里），不冲突。但 archive 子进程**不依赖 cache 目录里的快照**——快照在 archive 流程开头就已经被读到内存。

**hook 应在 50ms 内返回**——只是 `rm -rf`。

## 工程模式（贯穿所有 hook）

### 1. 文件锁防 race

两个 session 几乎同时结束 → 两个 detached `alluvium archive` 子进程并发跑 → 可能同时改同一个 topic 页。

用 `fs2::FileExt::lock_exclusive` 在 ``<cache>`/lock` 上加锁。后到的等前一个完成。

实现：`src/hook/lock.rs`。

### 2. 自我引用屏蔽

如果你（用户）正在用 Claude Code 开发 Alluvium 自己，那次 session 不应该被归档（否则会形成"Alluvium 归档了关于改 Alluvium 的对话"的奇怪递归）。

**判定**：SessionStart 时检查 cwd——如果 cwd 是 Alluvium 仓库的开发目录（`/Volumes/Work/VibeCoding/alluvium` 及其子目录），写 `resolved.json` 时 `skip_reason = "self-development"`。后续所有 hook 看到 `skip_reason` 非空就 no-op。

**配置**：用户可在 `config.toml` 里添加自己的"屏蔽路径列表"——比如他的其他 dotfiles 仓库、敏感工作目录。

借鉴自 cognee-integrations 屏蔽 `cognee` 关键词的思路。

### 3. Plugin 打包 vs 手编辑 settings.json

**用 plugin**：`.claude-plugin/plugin.json` 声明 hook，用户运行：

```bash
$ alluvium init        # 配 vault / API key / recipe
$ alluvium plugin install  # 等价于 `claude plugin install <path>`，自动注册
```

**不要手改用户的 `~/.claude/settings.json`**——那种方式安装/卸载难干净，容易跟其他工具打架。

借鉴自 cognee-integrations。

### 4. byte cap

蒸馏时一个工具调用的 `return_value` 可能是几 MB（比如 `Read` 一个大文件）。直接塞 prompt 会爆 context 也会贵。

每个字段过 cap：`{ "tool_use": 4096, "tool_result": 8192, ... }`。超出截断 + 标记 `[truncated, original was N bytes]`。

cap 值放 `prompts/distill.toml` 顶部，用户可调。

借鉴自 cognee-integrations。

## hook 性能要求

|  | 必须 | 测试 |
|---|---|---|
| SessionStart | < 50ms | 目测 + log timing |
| PreCompact | < 100ms | 目测 |
| Stop | < 100ms | `tests/e2e_hook_returns_fast.rs` |
| SessionEnd | < 50ms | 目测 |

慢于这个用户会感觉到 Claude Code 卡顿，是不可接受的。
