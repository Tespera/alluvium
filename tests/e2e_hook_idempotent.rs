//! e2e: 重复装 plugin 不会重复注入。
//!
//! 用户可见断言：连跑两次 `alluvium init`（或 plugin install），`.claude-plugin/plugin.json`
//! 内容相同，hook 列表不重复。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn install_plugin_is_idempotent() {
    // TODO
}
