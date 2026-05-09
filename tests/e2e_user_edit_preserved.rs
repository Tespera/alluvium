//! e2e: Obsidian 里的手改不被覆盖。
//!
//! 用户可见断言：用户在某个 topic 页里手动加了一段（Alluvium 没生成过的内容），
//! 下次 archive 命中这个页时，merger 应保留用户那段；只重写 Alluvium 上次自己写的部分。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn user_handwritten_section_survives_merge() {
    // TODO
}
