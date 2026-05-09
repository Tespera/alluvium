# Customizing Prompts (面向终端用户)

> ⚠️ 占位文档。Alluvium v0.1 scaffold 完成、第一份 prompt 模板写好后，本文档会被替换为完整的用户指南。

## 计划写什么

1. **prompt 文件在哪、长啥样**——`prompts/distill.toml` / `prompts/merge.toml` / `prompts/classify.toml` / `prompts/recipes/*.toml` 的位置和结构
2. **怎么加自己的 recipe**——复制现有 recipe 改名、调字段、在 `config.toml` 里引用
3. **怎么调蒸馏的"详略程度"**——展示 minimalist / dev-journal / verbose 三份 recipe 的关键差异
4. **怎么改 frontmatter 字段**——`templates/frontmatter.yaml.j2` 的可改区
5. **怎么改 entity vs concept 分类规则**——distill prompt 里的判定段落
6. **改完怎么测**——`alluvium dry-run` 用最近一个 session 试效果，不写盘
7. **改坏了怎么恢复**——`alluvium reset-prompts` 恢复默认（待实现）
8. **prompt-only 修改 vs 模板修改 vs 配置修改的边界**——什么情况改哪个文件

## 现状

prompt 模板未实现，本文档暂为占位。

**触发填充本文档的条件**（任一满足即必须填）：

- `src/distiller/prompt.rs` 实现完成（用户能 `alluvium dry-run` 看到真实蒸馏输出）
- `prompts/distill.toml` 的 `[prompt]` 段被填实
- `alluvium init` 实现完成（用户开始能装 Alluvium）

填充本文档的 AI 在**同一个 PR/commit 里同时改本文件**，不要拖到下个 PR——本文档落后会让用户开始用却找不到怎么改 prompt，劝退。

CI 不强制检查，但 reviewer 在 merge 前应核对：当上述任一文件被实质性改动时，本文档也有对应更新。

## 注意：HTML 注释段标记

实现 prompt 加载逻辑后，本文档**必须**警告用户：每个 topic 页里 Alluvium 写入的部分由 HTML 注释包裹（`<!-- alluvium:fact id=... -->...<!-- alluvium:end -->`）。**手动删除注释标记**会导致下次 archive 时该段被当成"用户手写"，新版本会被追加在末尾——造成内容重复。详见 [DECISIONS.md ADR-009](DECISIONS.md)。
