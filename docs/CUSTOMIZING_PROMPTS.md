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

prompt 模板尚未实现。AI（你）在 scaffold 完成、写第一个 prompt 时，**回来填这份文档**，给用户一份能照着改的指南。
