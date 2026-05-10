//! e2e: Obsidian 里的手改不被覆盖。
//!
//! 用户可见断言：用户在某个 topic 页里手动加了一段（在 Alluvium 标记块外面），
//! 下次 archive 命中这个页时，merger 应保留用户那段；只重写 Alluvium 自己之前写的部分。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Polishing Alluvium",
  "tags": ["alluvium"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "alluvium-design",
      "page_title": "Alluvium Design",
      "summary": "Alluvium tightens the merger to keep user edits outside markers.",
      "body_markdown": "Updated body — Alluvium owns this block, and only this block.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.95
    }
  ]
}"#;

const USER_HANDWRITTEN_PARAGRAPH: &str =
    "## My personal notes\n\nThis paragraph was written by hand and should survive any number of\nautomated merges. If you see this comment disappear, the merger has\nstomped on user content.\n";

#[test]
fn user_handwritten_section_survives_merge() {
    let env = TestEnv::new("e2e-userdit-001");

    let topic_rel = "wiki/concepts/alluvium-design.md";

    // Pre-existing page: an Alluvium fact block (which will be replaced)
    // PLUS a user-handwritten section *outside* the markers (which must
    // survive untouched).
    let pre_existing = format!(
        "\
---
title: Alluvium Design
type: concept
created: 2026-05-01
updated: 2026-05-01
tags:
  - alluvium
---

# Alluvium Design

<!-- alluvium:fact id=deadbeef -->
Original Alluvium-generated body.
<!-- alluvium:end -->

{USER_HANDWRITTEN_PARAGRAPH}"
    );
    env.write_existing_page(topic_rel, &pre_existing);

    env.write_fake_response(FAKE_RESPONSE);

    env.alluvium()
        .args(["archive", "--session", &env.session_id])
        .assert()
        .success();

    let body = std::fs::read_to_string(env.alluvium_subdir().join(topic_rel)).unwrap();

    // The user paragraph (every line of it) must still be there.
    for line in USER_HANDWRITTEN_PARAGRAPH.lines() {
        if line.trim().is_empty() {
            continue;
        }
        assert!(
            body.contains(line),
            "user-handwritten line was lost in merge: {line:?}\nfull body:\n{body}"
        );
    }

    // And the new Alluvium-owned block is present (old one may still be
    // there per merger semantics — different fact_id means append-not-replace,
    // and the user's paragraph is what we care about preserving).
    assert!(
        body.contains("Alluvium owns this block"),
        "Alluvium fact block should hold the new body; got:\n{body}"
    );
}
