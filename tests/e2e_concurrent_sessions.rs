//! e2e: 两个 archive 同时跑不写坏文件。
//!
//! 用户可见断言：spawn 两个 `alluvium archive --session ...` 进程同时跑（命中
//! 同一个 topic 页），两个都成功退出，最终页面是 well-formed markdown
//! （frontmatter 解析成功、marker 不交叠）。文件锁保证序列化。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Concurrent session",
  "tags": ["alluvium"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "concurrent-topic",
      "page_title": "Concurrent Topic",
      "summary": "Two archive workers race to write this same page.",
      "body_markdown": "Either order should produce a well-formed page.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.9
    }
  ]
}"#;

#[test]
fn concurrent_archives_serialize_writes() {
    let env = TestEnv::new("e2e-concur-A");
    env.add_transcript("e2e-concur-B");
    env.write_fake_response(FAKE_RESPONSE);

    // Spawn two archive workers in parallel against different sessions
    // that produce the SAME slug — they will contend on the topic page,
    // and the file lock must serialize them.
    let mut child_a = env
        .alluvium_std()
        .args(["archive", "--session", "e2e-concur-A"])
        .spawn()
        .expect("spawn archive A");
    let mut child_b = env
        .alluvium_std()
        .args(["archive", "--session", "e2e-concur-B"])
        .spawn()
        .expect("spawn archive B");

    let status_a = child_a.wait().expect("wait A");
    let status_b = child_b.wait().expect("wait B");
    assert!(status_a.success(), "archive A failed");
    assert!(status_b.success(), "archive B failed");

    let topic = env
        .alluvium_subdir()
        .join("wiki")
        .join("concepts")
        .join("concurrent-topic.md");
    assert!(
        topic.exists(),
        "topic page must exist after concurrent runs"
    );

    let body = std::fs::read_to_string(&topic).unwrap();

    // Frontmatter sanity: starts with `---\n`, has a closing `---\n`.
    assert!(
        body.starts_with("---\n"),
        "frontmatter prefix missing — page may be corrupted:\n{body}"
    );
    assert!(
        body.matches("---\n").count() >= 2,
        "frontmatter not closed — page may be corrupted:\n{body}"
    );

    // Marker block discipline: `<!-- alluvium:fact id=... -->` matches
    // `<!-- alluvium:end -->` count.
    let opens = body.matches("<!-- alluvium:fact id=").count();
    let closes = body.matches("<!-- alluvium:end -->").count();
    assert_eq!(
        opens, closes,
        "marker open/close count mismatch ({opens} opens, {closes} closes); page corrupted:\n{body}"
    );
    assert!(opens >= 1, "expected at least one fact block; got:\n{body}");
}
