# v0.1 Scope · IN / OUT

**这份文件是 AI 防跑偏的最重要清单之一**。看到自己想加 OUT 列表里的东西时，**停下**。

---

## v0.1 IN（必须做的）

### 核心流程
- [x] Claude Code Plugin 包装（`.claude-plugin/plugin.json`）
- [x] 4 个 hook（SessionStart / PreCompact / Stop / SessionEnd）
- [x] Detached spawn 模式让 Stop hook 100ms 内返回
- [x] 文件锁防并发归档
- [x] 自我引用屏蔽（cwd 检查）

### Transcript 处理
- [x] 流式读 `~/.claude/projects/*.jsonl`
- [x] 重建对话流，过滤 sub-agent 噪声
- [x] 合并 PreCompact 快照 + 最终 transcript

### 蒸馏
- [x] 调 Anthropic API（reqwest + serde + SSE）
- [x] 外部 prompt 模板（`prompts/*.toml`）
- [x] minijinja 模板渲染
- [x] byte cap（`tool_use` / `tool_result` 字段截断）
- [x] 3 份 recipe（`minimalist` / `dev-journal` / `verbose`）

### 知识组织（Karpathy wiki）
- [x] vault 内三层布局（`raw/` / `wiki/` / 在 vault 内的 `CLAUDE.md`）
- [x] entity 与 concept topic 页
- [x] frontmatter schema（含 typed `relations`）
- [x] `wiki/log.md` append-only 时间索引
- [x] `wiki/index.md` 增量更新（不全量重写）

### 写盘
- [x] Atomic write（temp file + rename）
- [x] Frontmatter parse + merge（保留用户手改）
- [x] Diff-based merger（覆盖 Alluvium 上次写的部分，保留其他）

### CLI 命令
- [x] `alluvium init`（向导：vault 路径 / API key / 选 recipe / 装 plugin）
- [x] `alluvium archive --session <id>`（Stop hook 调）
- [x] `alluvium replay <id|--since 7d|--all>`（重做旧 session）
- [x] `alluvium status`（最近 N 次归档摘要）
- [x] `alluvium dry-run`（蒸馏不写盘）
- [x] `alluvium consolidate`（**手动**触发 LLM 重写碎片化 topic 页）
- [x] `alluvium uninstall`

### Topic 匹配
- [x] grep + 标题模糊匹配（决定新建 or 更新）

### 配置 & 安全
- [x] `~/.config/alluvium/config.toml`（默认 profile，预留 `[profiles.*]`）
- [x] API key 走系统 Keychain（macOS Keychain via `keyring` crate）

### 调试可视化
- [x] `--debug` 模式落各阶段中间产物 JSON 到 `~/.local/share/alluvium/debug/`

### 测试
- [x] 11 个 e2e 测试（命名表达用户可见断言）
- [x] CI 跑 cargo test + clippy + fmt

### 分发
- [x] 单二进制 release build
- [x] Homebrew formula（`extra/Formula/alluvium.rb`，tap 仓库待开）

### 文档
- [x] CLAUDE.md（项目宪法）
- [x] README.md
- [x] docs/* 全套
- [x] 用户友好的 customizing guide（之后）

---

## v0.1 OUT（**不要做**，留给 v0.2+）

### Profile 系统
- ❌ 多 profile 支持（`--profile work / personal`）
- ❌ profile 切换、profile-specific 配置覆盖
- ➜ ADR-005，配置结构预留 `[profiles.*]` 即可，**不要**实现切换逻辑

### 跨客户端
- ❌ 监听 Claude Desktop / Cursor / Codex / Hermes 的对话存储
- ❌ 任何 fswatch / 日志监听机制
- ➜ v0.2 加，需要先调研各客户端的 transcript 存储位置

### 智能去重
- ❌ Embeddings-based topic 匹配
- ❌ 向量数据库（Qdrant / sqlite-vec / 任何）
- ➜ v0.2 加，ADR-006

### 调度
- ❌ Cron / launchd plist 自动调度（不算 hook 触发的那种）
- ❌ 后台 daemon 形态
- ❌ 自动定时 consolidate
- ➜ v0.2 加。v0.1 `alluvium consolidate` 是**手动**触发的命令

### Sources 回填
- ❌ `alluvium replay --rebuild-sources`（用户后续打开 `keep_source_summaries = true` 时，给历史 session 补 `wiki/sources/` 摘要页）
- ➜ v0.2

### 双向交互
- ❌ Alluvium 内嵌的 MCP server（让 Claude 能查 vault）
- ❌ Slash command（`/alluvium-recall` 之类）
- ❌ 任何"在 Claude 里使用 Alluvium"的入口
- ➜ Alluvium 是**单向归档**工具，反向查询是另一个产品的事（用户已有 mcp-obsidian 之类的方案）

### 内容增强
- ❌ 自动主题发现（"未命名的 pattern surface"）
- ❌ 跨 topic 自动建议链接
- ❌ 知识图谱可视化
- ❌ 笔记质量评分
- ➜ v0.2 / v0.3 探索

### 多模型
- ❌ 用户切换 distillation model（OpenAI / Gemini / 本地）
- ❌ 多模型 ensemble
- ➜ v0.1 只支持 Anthropic API，简化

### 网络功能
- ❌ 同步到云
- ❌ 多设备共享 vault
- ❌ Web UI / dashboard
- ➜ vault 是用户文件，同步走他自己的 iCloud / Syncthing / Git，Alluvium 不管

### 团队功能
- ❌ 多用户 vault
- ❌ 权限管理
- ❌ 审计日志（archive 日志算最简化的，不算）
- ➜ Alluvium 是个人工具，不进企业语境

### Plugin 机制
- ❌ 让用户给 Alluvium 写自己的 distill plugin
- ❌ 钩子让其他工具介入 archive 流程
- ➜ v0.2 视社区诉求

### 高级 prompt
- ❌ Function calling / tool use 蒸馏
- ❌ Multi-turn distillation（让 LLM 反复确认）
- ❌ Few-shot 学习用户偏好
- ➜ v0.2

### 高级 vault
- ❌ 自动给笔记打 emoji icon
- ❌ Mermaid 图自动生成
- ❌ 时间线/趋势分析
- ➜ 不在产品愿景里

### 兼容
- ❌ Logseq / Roam Research / Notion 兼容
- ❌ Markdown 以外的输出格式
- ➜ Obsidian 是目标平台，不分散精力

---

## 决策原则

当不确定一个功能是 v0.1 IN 还是 OUT 时：

1. 看 [CLAUDE.md](../CLAUDE.md) 的"反模式"列表
2. 看 [DECISIONS.md](DECISIONS.md) 是否有相关 ADR
3. 问"这个功能能不能用 vault 内手动操作 + 已有 CLI 实现"——如果能，就 OUT
4. 仍不确定，**问用户**，不要自己定

**v0.1 的形象是"小而正确"**：跑通 Karpathy wiki + 4 hook + 几个核心 CLI，足够开源。功能堆砌的发布等于没发布——用户装不会用、AI 维护不动。
