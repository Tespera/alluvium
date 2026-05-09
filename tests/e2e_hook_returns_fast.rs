//! e2e: hook 在 100ms 内返回，不卡 Claude Code。
//!
//! 用户可见断言：测量 `alluvium archive --session <id>` 的 wall-clock 退出时间应 < 100ms。
//! 真正的工作在 detached 子进程里，hook 立即返回。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn stop_hook_returns_within_100ms() {
    // TODO
}
