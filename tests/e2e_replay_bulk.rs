//! e2e: replay --since 处理多个 session。
//!
//! 用户可见断言：vault 里有两个 transcript，跑 `alluvium replay --since 7d`
//! 后两个都被归档（每个产生一个 topic 页 + log.md 各一行）。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Bulk replay session",
  "tags": ["replay"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "bulk-replay-topic",
      "page_title": "Bulk Replay Topic",
      "summary": "A shared topic across replayed sessions.",
      "body_markdown": "All replayed sessions land on this same page.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.9
    }
  ]
}"#;

#[test]
fn replay_since_processes_recent_sessions() {
    let env = TestEnv::new("e2e-bulk-A");
    env.add_transcript("e2e-bulk-B");
    env.write_fake_response(FAKE_RESPONSE);

    env.alluvium()
        .args(["replay", "--since", "7d"])
        .assert()
        .success();

    // Both sessions should have hit the topic page (same slug → merge).
    let topic = env
        .alluvium_subdir()
        .join("wiki")
        .join("concepts")
        .join("bulk-replay-topic.md");
    assert!(
        topic.exists(),
        "bulk replay should create the shared topic page"
    );

    // log.md should have entries from both sessions (two lines).
    let log = env.alluvium_subdir().join("wiki").join("log.md");
    let log_body = std::fs::read_to_string(&log).expect("log.md must exist");
    let session_lines = log_body
        .lines()
        .filter(|l| l.contains("Bulk replay session"))
        .count();
    assert!(
        session_lines >= 2,
        "log.md should hold one line per session (≥2); got:\n{log_body}"
    );

    // Two raw transcript copies in raw/sessions/.
    let raw_dir = env.alluvium_subdir().join("raw").join("sessions");
    let raw_count = std::fs::read_dir(&raw_dir).unwrap().count();
    assert!(
        raw_count >= 2,
        "raw/sessions/ should hold one file per session (≥2); got {raw_count}"
    );
}
