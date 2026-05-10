//! e2e: plugin 清单文件的 hook 列表不重复。
//!
//! Alluvium 不动用户的 settings.json — plugin 安装走 Claude Code 自己的
//! `claude plugin install <path>` 命令。Alluvium 这边能控的、应该测试的
//! 是仓库内的 `.claude-plugin/plugin.json` 是不是 well-formed 而且每个
//! 4 个 hook 都恰好声明一次（避免 PR 误把 hook 复制粘贴成两份）。
//!
//! 这个 hook 列表是发布到 marketplace 的"事实"，比 idempotency 本身更值得守。

use serde_json::Value;
use std::path::PathBuf;

fn manifest_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("plugins/alluvium/.claude-plugin/plugin.json"),
        PathBuf::from(".claude-plugin/plugin.json"),
    ]
}

#[test]
fn install_plugin_is_idempotent() {
    for manifest_path in manifest_paths() {
        let content = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
        let manifest: Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("parse {}: {e}", manifest_path.display()));

        let hooks = manifest
            .get("hooks")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{} has no hooks object", manifest_path.display()));

        for event in ["SessionStart", "PreCompact", "Stop", "SessionEnd"] {
            let arr = hooks
                .get(event)
                .and_then(Value::as_array)
                .unwrap_or_else(|| {
                    panic!("{} missing hooks.{event} array", manifest_path.display())
                });
            assert_eq!(
                arr.len(),
                1,
                "{}: hooks.{event} should have exactly 1 entry, got {}",
                manifest_path.display(),
                arr.len()
            );

            // Each entry is `{ "hooks": [{type, command}] }`.
            let inner = arr[0]
                .get("hooks")
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("hooks.{event}[0].hooks missing"));
            assert_eq!(
                inner.len(),
                1,
                "{}: hooks.{event}[0].hooks should have exactly 1 command, got {}",
                manifest_path.display(),
                inner.len()
            );
        }
    }
}
