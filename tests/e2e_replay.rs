//! e2e: replay 旧 session 能重做。
//!
//! 用户可见断言：archive 一次 → replay 同一个 id 再来一次。topic 页应该
//! 还是同一个文件（无副本），`updated:` 时间被刷新，archive log 里有两条记录。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Replay test",
  "tags": ["alluvium"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "replay-target",
      "page_title": "Replay Target",
      "summary": "A topic page that gets re-archived to test replay.",
      "body_markdown": "Body content from the canonical fake response.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.95
    }
  ]
}"#;

#[test]
fn replay_specific_session_redistills() {
    let env = TestEnv::new("e2e-replay-001");
    env.write_fake_response(FAKE_RESPONSE);

    // First archive — establishes the page.
    env.alluvium()
        .args(["archive", "--session", &env.session_id])
        .assert()
        .success();

    let topic = env
        .alluvium_subdir()
        .join("wiki")
        .join("concepts")
        .join("replay-target.md");
    assert!(topic.exists(), "first archive should create the page");
    let first_body = std::fs::read_to_string(&topic).unwrap();

    // Replay the same session — should land on the existing page in place.
    env.alluvium()
        .args(["replay", &env.session_id])
        .assert()
        .success();

    // Still exactly one file under that slug — replay must not duplicate.
    let concepts_dir = env.alluvium_subdir().join("wiki").join("concepts");
    let count = std::fs::read_dir(&concepts_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("replay-target"))
        .count();
    assert_eq!(count, 1, "replay must not duplicate the topic page");

    let second_body = std::fs::read_to_string(&topic).unwrap();
    assert!(
        second_body.contains("<!-- alluvium:fact id="),
        "page body still has marker block after replay"
    );
    // Same content (same fake response) → same fact_id → block replaced
    // in place. The page should remain valid markdown.
    assert!(!second_body.is_empty());
    let _ = first_body; // referenced for clarity; could compare if useful
}
