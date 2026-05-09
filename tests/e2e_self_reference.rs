//! e2e: Alluvium 自己开发目录的会话不被归档。
//!
//! 用户可见断言：当 `resolved.json` 的 cwd 在 Alluvium 仓库根之内时，archive 应早返回
//! 并不写任何 vault 文件。归档日志里应留下一条 `skip_reason="self-development"`。

#[test]
#[ignore = "scaffold v0.1: pending implementation"]
fn self_referent_session_is_skipped() {
    // TODO
}
