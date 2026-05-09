//! e2e: compact 前内容能被快照、不丢。
//!
//! 用户可见断言：模拟一次 PreCompact 钩子调用 + 之后一次 Stop 钩子调用；archive
//! 阶段消费的 transcript 应同时包含快照里的早期内容和最终 JSONL 里的后期内容。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn precompact_snapshot_merges_into_archive() {
    // TODO
}
