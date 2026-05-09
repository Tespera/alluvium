//! e2e: replay --since 7d 能批量回填。
//!
//! 用户可见断言：跑 `alluvium replay --since 7d` 会按时间范围筛出过去 7 天的所有
//! transcript 并依次重新蒸馏，每个产生归档日志一行。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn replay_since_processes_recent_sessions() {
    // TODO
}
