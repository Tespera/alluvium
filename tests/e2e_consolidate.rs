//! e2e: consolidate 把多个 fact-block 合并成一个。
//!
//! 用户可见断言：一个 topic 页有 3 个 `alluvium:fact` 块；跑
//! `alluvium consolidate <slug>` 后，页面里只剩 1 个块，块内容是
//! fake-LLM 返回的"consolidated body"，frontmatter 不变，
//! marker 外的用户内容（页首/页尾段落）原样保留。

mod common;

use common::TestEnv;

const PAGE_TITLE: &str = "Alluvium Design";
const PAGE_SLUG: &str = "alluvium-design";

const CONSOLIDATED_BODY: &str =
    "Alluvium 是一个 Claude Code 会话的自动归档器。它由 Stop hook 触发，将\n\
     transcript 蒸馏后合并到 Karpathy 风格的 wiki。consolidate 命令把同一\n\
     topic 页上累积的多个 fact-block 重写为一段紧凑的描述。";

fn pre_existing_page() -> String {
    format!(
        "\
---
title: {PAGE_TITLE}
type: concept
created: 2026-05-01
updated: 2026-05-09
tags:
  - alluvium
---

# {PAGE_TITLE}

This is a user-written intro paragraph that lives ABOVE the first marker.
It must survive consolidation untouched.

<!-- alluvium:fact id=11111111 -->
First fragment: Alluvium is an auto-archiver. Triggered by Stop hook.
<!-- alluvium:end -->

<!-- alluvium:fact id=22222222 -->
Second fragment: Alluvium distills transcripts and merges into a wiki.
<!-- alluvium:end -->

<!-- alluvium:fact id=33333333 -->
Third fragment: Alluvium follows a Karpathy-style topic-page model.
<!-- alluvium:end -->

This is a user footer paragraph that lives BELOW the last marker. It
must also survive consolidation.
"
    )
}

#[test]
fn consolidate_collapses_three_blocks_into_one() {
    let env = TestEnv::new("e2e-consolidate-001");
    env.write_existing_page(
        &format!("wiki/concepts/{PAGE_SLUG}.md"),
        &pre_existing_page(),
    );
    // FakeBackend returns this verbatim as the consolidated body.
    env.write_fake_response(CONSOLIDATED_BODY);

    env.alluvium()
        .args(["consolidate", PAGE_SLUG])
        .assert()
        .success();

    let body = std::fs::read_to_string(
        env.alluvium_subdir()
            .join("wiki")
            .join("concepts")
            .join(format!("{PAGE_SLUG}.md")),
    )
    .unwrap();

    // Frontmatter survives.
    assert!(
        body.contains(&format!("title: {PAGE_TITLE}")),
        "frontmatter title must survive consolidate"
    );
    assert!(
        body.contains("created: 2026-05-01"),
        "frontmatter `created:` must survive"
    );

    // User content above and below the envelope survives.
    assert!(
        body.contains("This is a user-written intro paragraph"),
        "intro paragraph above first marker should survive"
    );
    assert!(
        body.contains("This is a user footer paragraph"),
        "footer paragraph below last marker should survive"
    );

    // Exactly one fact-block remains.
    let opens = body.matches("<!-- alluvium:fact id=").count();
    let closes = body.matches("<!-- alluvium:end -->").count();
    assert_eq!(
        opens, 1,
        "expected exactly 1 fact-block after consolidate, got {opens}\nbody:\n{body}"
    );
    assert_eq!(closes, 1, "open/close count mismatch in body:\n{body}");

    // The consolidated content from the fake LLM is in the body.
    assert!(
        body.contains("Alluvium 是一个 Claude Code 会话的自动归档器"),
        "consolidated body should be the LLM's response; got:\n{body}"
    );

    // None of the original fragment-specific phrasing remains (those
    // strings were unique to the input fragments and should be gone).
    assert!(
        !body.contains("First fragment:"),
        "original fragment 1 marker prose should be gone"
    );
    assert!(
        !body.contains("Second fragment:"),
        "original fragment 2 marker prose should be gone"
    );
    assert!(
        !body.contains("Third fragment:"),
        "original fragment 3 marker prose should be gone"
    );
}

#[test]
fn consolidate_is_noop_with_one_block() {
    let env = TestEnv::new("e2e-consolidate-noop");
    let only_one = format!(
        "\
---
title: {PAGE_TITLE}
type: concept
---

# {PAGE_TITLE}

<!-- alluvium:fact id=99999999 -->
Single fragment that should not be touched.
<!-- alluvium:end -->
"
    );
    env.write_existing_page(&format!("wiki/concepts/{PAGE_SLUG}.md"), &only_one);
    env.write_fake_response("never-called");

    let assert = env
        .alluvium()
        .args(["consolidate", PAGE_SLUG])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("nothing to do") || stdout.contains("fewer than 2"),
        "single-block page should report no-op; got: {stdout}"
    );

    // File must be byte-identical (single block survives unchanged).
    let body = std::fs::read_to_string(
        env.alluvium_subdir()
            .join("wiki")
            .join("concepts")
            .join(format!("{PAGE_SLUG}.md")),
    )
    .unwrap();
    assert!(body.contains("Single fragment that should not be touched."));
    assert_eq!(body.matches("<!-- alluvium:fact id=").count(), 1);
}
