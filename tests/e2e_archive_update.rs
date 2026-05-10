//! e2e: 已有主题会被更新而非重建。
//!
//! 用户可见断言：当 ExtractedFact 的 page_slug 已存在于 vault 中时，merger 应当
//! 修改那一份现有的页面，而不是新建一份带后缀的。归档前后只能有 *一个*
//! 同 slug 的文件，且 `updated:` 字段被刷新。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Refining Alluvium Design",
  "tags": ["alluvium"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "alluvium-design",
      "page_title": "Alluvium Design",
      "summary": "Alluvium adds a stricter byte cap on transcript chunks.",
      "body_markdown": "Following review, Alluvium now caps each transcript field at 4-8 KB before passing to the distiller.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.9
    }
  ]
}"#;

#[test]
fn archive_existing_topic_updates_in_place() {
    let env = TestEnv::new("e2e-update-001");

    let concepts_dir = env.alluvium_subdir().join("wiki").join("concepts");
    std::fs::create_dir_all(&concepts_dir).unwrap();
    let topic_rel = "wiki/concepts/alluvium-design.md";

    // Per ADR-014: fact_id = sha256[:4]("<slug>:<normalized_summary>"). When
    // the new archive's (slug, summary) matches the pre-existing block's id,
    // the merger REPLACES the block in place rather than appending. We
    // pre-seed the page with a block whose id matches what the new fact
    // will hash to — so we can assert the old body is gone.
    let new_summary = "Alluvium adds a stricter byte cap on transcript chunks.";
    let expected_id = alluvium::vault::merger::fact_id("alluvium-design", new_summary);

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

<!-- alluvium:fact id={expected_id} -->
Earlier draft body.
<!-- alluvium:end -->
"
    );
    env.write_existing_page(topic_rel, &pre_existing);

    env.write_fake_response(FAKE_RESPONSE);

    env.alluvium()
        .args(["archive", "--session", &env.session_id])
        .assert()
        .success();

    // Still only one file with this slug — no duplicate suffixes.
    let entries: Vec<_> = std::fs::read_dir(&concepts_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("alluvium-design"))
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "exactly one alluvium-design.md should exist; got {entries:?}"
    );

    let body = std::fs::read_to_string(env.alluvium_subdir().join(topic_rel)).unwrap();
    assert!(
        body.contains("4-8 KB"),
        "page body should contain the new fact's content; got:\n{body}"
    );
    assert!(
        !body.contains("Earlier draft body."),
        "the previous fact block should have been replaced; got:\n{body}"
    );
    assert!(
        body.contains("updated: 2026-05-10"),
        "`updated:` should be refreshed to today; got:\n{body}"
    );

    // And there must be exactly ONE marker block with this id — replace,
    // not append.
    let marker = format!("<!-- alluvium:fact id={expected_id} -->");
    let occurrences = body.matches(marker.as_str()).count();
    assert_eq!(
        occurrences, 1,
        "exactly one marker block with id={expected_id} expected; got {occurrences}\nbody:\n{body}"
    );
}
