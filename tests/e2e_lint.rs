//! e2e: `alluvium lint --apply` 真合并近似 topic。
//!
//! 用户可见断言：vault 里有两个 slug 互为近似（编辑距离低）的 topic 页。
//! 跑 `alluvium lint --apply`，假 LLM 返回 MERGE 决策。结果：
//! - winning slug 的页面 body 替换为合并后内容
//! - losing slug 的文件被删除
//! - log.md 多了一条 `## [date] lint | merged X → Y` 记录
//!
//! 这是 [LLM_WIKI_DOCTRINE] 原则 3 的最小实现。

mod common;

use common::TestEnv;

const LINT_DECISION: &str = r#"{
  "decision": "merge",
  "winning_slug": "atomic-file-write",
  "merged_body": "**Atomic file write** is implemented as: write to `path.tmp`, `fsync`, `rename(path.tmp, path)`, then `fsync(parent_dir)` so the directory entry survives a crash. POSIX guarantees `rename` atomicity within one filesystem.",
  "reason": "both pages describe the same POSIX atomic-write idiom"
}"#;

const LINT_KEEP: &str = r#"{
  "decision": "keep",
  "winning_slug": null,
  "merged_body": null,
  "reason": "different topics despite slug similarity"
}"#;

#[test]
fn lint_apply_merges_near_duplicates() {
    let env = TestEnv::new("e2e-lint-apply");

    // Two near-duplicate slugs: `atomic-file-write` vs `atomic-write`.
    // Title similarity is high — should fire the candidate threshold.
    env.write_existing_page(
        "wiki/concepts/atomic-file-write.md",
        "---\ntitle: Atomic File Write\n---\n\n# Atomic File Write\n\n<!-- alluvium:fact id=AAAAAAAA -->\nWrite to tmp + rename. POSIX rename is atomic.\n<!-- alluvium:end -->\n",
    );
    env.write_existing_page(
        "wiki/concepts/atomic-write.md",
        "---\ntitle: Atomic Write\n---\n\n# Atomic Write\n\n<!-- alluvium:fact id=BBBBBBBB -->\nDon't forget to fsync the parent dir or the dirent may not survive a crash.\n<!-- alluvium:end -->\n",
    );

    env.write_fake_response(LINT_DECISION);

    env.alluvium().args(["lint", "--apply"]).assert().success();

    let winner = env
        .alluvium_subdir()
        .join("wiki")
        .join("concepts")
        .join("atomic-file-write.md");
    let loser = env
        .alluvium_subdir()
        .join("wiki")
        .join("concepts")
        .join("atomic-write.md");

    assert!(winner.exists(), "winning page must remain");
    assert!(!loser.exists(), "losing page must be deleted");

    let body = std::fs::read_to_string(&winner).unwrap();
    assert!(
        body.contains("**Atomic file write**"),
        "merged body should be in winner page; got:\n{body}"
    );
    assert!(
        body.contains("fsync(parent_dir)"),
        "merged content should preserve loser's unique fact:\n{body}"
    );
    // Exactly one fact-block (the merged one).
    let opens = body.matches("<!-- alluvium:fact id=").count();
    assert_eq!(opens, 1, "merged page should hold exactly one fact-block");

    // log.md should record the lint action.
    let log = env.alluvium_subdir().join("wiki").join("log.md");
    let log_body = std::fs::read_to_string(&log).expect("log.md should exist after lint");
    assert!(
        log_body.contains("lint | merged"),
        "log.md should record the lint action; got:\n{log_body}"
    );
    assert!(
        log_body.contains("atomic-write"),
        "log.md should mention the loser slug"
    );
}

#[test]
fn lint_dry_run_does_not_modify_files() {
    let env = TestEnv::new("e2e-lint-dryrun");
    env.write_existing_page(
        "wiki/concepts/atomic-file-write.md",
        "---\ntitle: Atomic File Write\n---\n\n# Atomic File Write\n\n<!-- alluvium:fact id=AAAAAAAA -->\nbody A\n<!-- alluvium:end -->\n",
    );
    env.write_existing_page(
        "wiki/concepts/atomic-write.md",
        "---\ntitle: Atomic Write\n---\n\n# Atomic Write\n\n<!-- alluvium:fact id=BBBBBBBB -->\nbody B\n<!-- alluvium:end -->\n",
    );
    env.write_fake_response(LINT_DECISION);

    let before_a = std::fs::read_to_string(
        env.alluvium_subdir()
            .join("wiki/concepts/atomic-file-write.md"),
    )
    .unwrap();
    let before_b =
        std::fs::read_to_string(env.alluvium_subdir().join("wiki/concepts/atomic-write.md"))
            .unwrap();

    let assert = env.alluvium().arg("lint").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("would merge") || stdout.contains("Dry-run"),
        "dry-run output should clearly say no files were touched; got:\n{stdout}"
    );

    let after_a = std::fs::read_to_string(
        env.alluvium_subdir()
            .join("wiki/concepts/atomic-file-write.md"),
    )
    .unwrap();
    let after_b =
        std::fs::read_to_string(env.alluvium_subdir().join("wiki/concepts/atomic-write.md"))
            .unwrap();

    assert_eq!(before_a, after_a, "dry-run must not modify file A");
    assert_eq!(before_b, after_b, "dry-run must not modify file B");
}

#[test]
fn lint_keep_decision_leaves_both_pages_alone() {
    let env = TestEnv::new("e2e-lint-keep");
    env.write_existing_page(
        "wiki/concepts/atomic-file-write.md",
        "---\ntitle: Atomic File Write\n---\n\n# Atomic File Write\n\n<!-- alluvium:fact id=AAAAAAAA -->\nbody A\n<!-- alluvium:end -->\n",
    );
    env.write_existing_page(
        "wiki/concepts/atomic-write.md",
        "---\ntitle: Atomic Write\n---\n\n# Atomic Write\n\n<!-- alluvium:fact id=BBBBBBBB -->\nbody B\n<!-- alluvium:end -->\n",
    );
    env.write_fake_response(LINT_KEEP);

    env.alluvium().args(["lint", "--apply"]).assert().success();

    // Both files survive even with --apply, because the LLM said KEEP.
    assert!(env
        .alluvium_subdir()
        .join("wiki/concepts/atomic-file-write.md")
        .exists());
    assert!(env
        .alluvium_subdir()
        .join("wiki/concepts/atomic-write.md")
        .exists());
}
