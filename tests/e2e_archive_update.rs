//! e2e: 已有主题会被更新而非重建。
//!
//! 用户可见断言：当 ExtractedFact 的 page_slug 已存在于 vault 中时，merger 应当
//! 修改那一份现有的页面（行数变化 / `updated:` 字段刷新），而不是新建一份带后缀的。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn archive_existing_topic_updates_in_place() {
    // TODO
}
