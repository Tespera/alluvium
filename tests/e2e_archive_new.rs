//! e2e: 新会话能被归档成笔记。
//!
//! 用户可见断言：跑完 `alluvium archive --session <id>` 后，vault 的
//! `wiki/concepts/` 或 `wiki/entities/` 下应至少出现一份新的 .md 文件，
//! `wiki/log.md` 应被追加一行，`raw/sessions/` 下应留有一份原档副本。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Designing Alluvium",
  "tags": ["alluvium", "design"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "alluvium-design",
      "page_title": "Alluvium Design",
      "summary": "Alluvium auto-archives Claude Code sessions to an Obsidian wiki.",
      "body_markdown": "Alluvium watches Claude Code sessions end and distills the transcript into knowledge that lands in topic pages.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.95
    }
  ]
}"#;

#[test]
fn archive_new_session_creates_topic_pages() {
    let env = TestEnv::new("e2e-new-001");
    env.write_fake_response(FAKE_RESPONSE);

    env.alluvium()
        .args(["archive", "--session", &env.session_id])
        .assert()
        .success();

    let alluvium_root = env.alluvium_subdir();

    let topic = alluvium_root
        .join("wiki")
        .join("concepts")
        .join("alluvium-design.md");
    assert!(topic.exists(), "expected topic page at {}", topic.display());
    let topic_body = std::fs::read_to_string(&topic).unwrap();
    assert!(
        topic_body.contains("Alluvium Design"),
        "topic page must contain the page_title"
    );
    assert!(
        topic_body.contains("<!-- alluvium:fact id="),
        "topic page must wrap the fact body in markers"
    );

    let log = alluvium_root.join("wiki").join("log.md");
    assert!(log.exists(), "log.md should be created on first archive");
    let log_body = std::fs::read_to_string(&log).unwrap();
    assert!(
        log_body.contains("Designing Alluvium"),
        "log.md should mention the session title; got:\n{log_body}"
    );

    let raw_dir = alluvium_root.join("raw").join("sessions");
    let raw_count = std::fs::read_dir(&raw_dir).unwrap().count();
    assert!(
        raw_count >= 1,
        "raw/sessions/ should hold at least one transcript copy (got {raw_count})"
    );
}
