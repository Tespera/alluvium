//! e2e: replay 旧 session 能重做。
//!
//! 用户可见断言：给定一个已归档过的 session id，`alluvium replay <id>` 能重新蒸馏并
//! 在现有 topic 页上反映新 prompt 的效果（updated 时间 + body 变化）。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn replay_specific_session_redistills() {
    // TODO
}
