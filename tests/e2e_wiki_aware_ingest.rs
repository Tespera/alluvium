//! e2e: distill prompt is wiki-aware.
//!
//! 用户可见断言：当 vault 已经有一些 topic 页时，archive 调 LLM 之前
//! 会把现有 topic 索引（slug + title + summary）注入 prompt。这是
//! [LLM_WIKI_DOCTRINE] 原则 2 的核心机制——LLM 必须看到现有 wiki，
//! 才能 *update* 已有页而不是 *mint* 平行重复。
//!
//! 怎么验证：用 `--debug` 跑 archive，读 `<data>/debug/<sid>/rendered_prompt.json`，
//! 断言 user prompt 里包含我们预置的 topic 的 slug 和 title。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Wiki-aware test session",
  "tags": [],
  "extracted": []
}"#;

fn binary_data_dir(home: &std::path::Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("dev.alluvium.alluvium")
    } else {
        home.join(".local").join("share").join("alluvium")
    }
}

#[test]
fn distill_prompt_includes_existing_wiki_topics() {
    let env = TestEnv::new("e2e-wiki-aware-001");

    // Pre-seed two topic pages.
    env.write_existing_page(
        "wiki/concepts/atomic-file-write.md",
        "---\ntitle: Atomic File Write\n---\n\n# Atomic File Write\n\n<!-- alluvium:fact id=11111111 -->\nWrite to tmp, fsync, rename. POSIX guarantees atomicity within one filesystem.\n<!-- alluvium:end -->\n",
    );
    env.write_existing_page(
        "wiki/entities/alluvium.md",
        "---\ntitle: Alluvium\n---\n\n# Alluvium\n\n<!-- alluvium:fact id=22222222 -->\nA Claude Code session auto-archiver written in Rust.\n<!-- alluvium:end -->\n",
    );

    env.write_fake_response(FAKE_RESPONSE);

    env.alluvium()
        .arg("--debug")
        .args(["archive", "--session", &env.session_id])
        .assert()
        .success();

    // archive --debug dumps each pipeline stage to data_dir/debug/<sid>/.
    // We inspect rendered_prompt.json to verify the existing-topics
    // index made it into the user message.
    let dump = binary_data_dir(env.home.path())
        .join("debug")
        .join(&env.session_id)
        .join("rendered_prompt.json");
    assert!(dump.exists(), "expected debug dump at {}", dump.display());
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dump).unwrap()).unwrap();
    let user_prompt = json
        .get("user")
        .and_then(|v| v.as_str())
        .expect("rendered_prompt.json should have `user` field");

    assert!(
        user_prompt.contains("Existing topics in the user's wiki"),
        "user prompt missing the wiki-aware preamble:\n{user_prompt}"
    );
    assert!(
        user_prompt.contains("`atomic-file-write`"),
        "existing concept slug not surfaced in prompt:\n{user_prompt}"
    );
    assert!(
        user_prompt.contains("Atomic File Write"),
        "existing concept title not surfaced in prompt:\n{user_prompt}"
    );
    assert!(
        user_prompt.contains("`alluvium`"),
        "existing entity slug not surfaced in prompt:\n{user_prompt}"
    );
}

#[test]
fn distill_prompt_says_empty_when_vault_is_fresh() {
    let env = TestEnv::new("e2e-wiki-aware-fresh");
    env.write_fake_response(FAKE_RESPONSE);

    env.alluvium()
        .arg("--debug")
        .args(["archive", "--session", &env.session_id])
        .assert()
        .success();

    let dump = binary_data_dir(env.home.path())
        .join("debug")
        .join(&env.session_id)
        .join("rendered_prompt.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dump).unwrap()).unwrap();
    let user_prompt = json.get("user").and_then(|v| v.as_str()).unwrap();
    assert!(
        user_prompt.contains("vault is empty") || user_prompt.contains("first session"),
        "fresh-vault preamble missing:\n{user_prompt}"
    );
}
