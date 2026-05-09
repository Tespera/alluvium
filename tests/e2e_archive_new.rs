//! e2e: 新会话能被归档成笔记。
//!
//! 用户可见断言：跑完 `alluvium archive` 后，vault 的 `wiki/concepts/` 或
//! `wiki/entities/` 下应至少出现一份新的 .md 文件，并且 `wiki/log.md` 应被追加一行。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn archive_new_session_creates_topic_pages() {
    // TODO: 用 tempfile + 真实 transcript fixture 跑端到端。
}
