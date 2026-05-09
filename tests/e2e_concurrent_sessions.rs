//! e2e: 两个 session 同时结束不写坏文件。
//!
//! 用户可见断言：并行触发两次 `alluvium archive` 命中同一个 topic 页，最终页面内容
//! 是完整 well-formed markdown（frontmatter 不缺、不重叠）。文件锁保证序列化。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn concurrent_archives_serialize_writes() {
    // TODO
}
