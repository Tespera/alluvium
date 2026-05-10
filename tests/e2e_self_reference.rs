//! e2e: Alluvium 自己开发目录的会话不被归档。
//!
//! 用户可见断言：当 archive 的 cwd 落在 `skip_paths` 内时，应早返回，
//! 不写任何 vault 文件。

mod common;

use common::TestEnv;

#[test]
fn self_referent_session_is_skipped() {
    let env = TestEnv::new("e2e-self-001");

    // Skip dir lives inside HOME so it is auto-cleaned with the tempdir.
    let skip_dir = env.home.path().join("alluvium-dev");
    std::fs::create_dir_all(&skip_dir).unwrap();
    env.set_skip_paths(&[&skip_dir]);

    // Fake response is irrelevant — archive must skip BEFORE the LLM call.
    env.write_fake_response(r#"{"title":"unused","tags":[],"extracted":[]}"#);

    env.alluvium()
        .args(["archive", "--session", &env.session_id])
        // Manual mode reads cwd via current_dir(); putting the binary's
        // cwd inside the skip path triggers the self-filter.
        .current_dir(&skip_dir)
        .assert()
        .success();

    // Vault must remain pristine — no wiki/, no log.md, no raw/.
    let alluvium_root = env.alluvium_subdir();
    assert!(
        !alluvium_root.join("wiki").exists(),
        "skipped session must not create wiki/; got tree at {}",
        alluvium_root.display()
    );
    assert!(
        !alluvium_root.join("raw").exists(),
        "skipped session must not create raw/"
    );
}
