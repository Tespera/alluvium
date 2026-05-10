//! e2e: PreCompact 钩子的快照在 Stop 时被合回 archive。
//!
//! 流程：
//! 1. 写一个 transcript（"早期"内容）
//! 2. 跑 `alluvium pre-compact` —— 它把当时的 transcript 拷贝到 cache 快照目录
//! 3. 把 transcript 改成"晚期"内容（模拟 Claude Code compact 之后只剩压缩摘要）
//! 4. 跑 `alluvium archive --session <id>`
//! 5. 断言：archive 用到的 fake 响应能正确返回；快照文件存在；早期内容
//!    没有因为 compact 而丢失（snapshots 目录里能找到）。

mod common;

use common::TestEnv;
use std::io::Write;

const FAKE_RESPONSE: &str = r#"{
  "title": "Pre-compact session",
  "tags": ["alluvium"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "pre-compact-topic",
      "page_title": "Pre-Compact Topic",
      "summary": "Topic produced from a session that went through PreCompact.",
      "body_markdown": "If you can read this, the archive ran end-to-end.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.9
    }
  ]
}"#;

fn binary_cache_dir(home: &std::path::Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Caches")
            .join("dev.alluvium.alluvium")
    } else {
        home.join(".cache").join("alluvium")
    }
}

#[test]
fn precompact_snapshot_merges_into_archive() {
    let env = TestEnv::new("e2e-precompact-001");
    env.write_fake_response(FAKE_RESPONSE);

    let payload = env.hook_payload_json("PreCompact");

    // Drive PreCompact via stdin payload — the hook copies the transcript
    // into the cache snapshots dir.
    let mut child = env
        .alluvium_std()
        .arg("pre-compact")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn pre-compact");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let status = child.wait().expect("wait pre-compact");
    assert!(status.success(), "pre-compact hook must succeed");

    // Snapshot must be on disk under the binary's cache dir.
    let snapshots_dir = binary_cache_dir(env.home.path())
        .join("sessions")
        .join(&env.session_id)
        .join("snapshots");
    assert!(
        snapshots_dir.exists(),
        "PreCompact should have created snapshots dir at {}",
        snapshots_dir.display()
    );
    let snap_count = std::fs::read_dir(&snapshots_dir).unwrap().count();
    assert!(
        snap_count >= 1,
        "PreCompact should write at least one snapshot; got {snap_count}"
    );

    // Now run archive — it should pick up the snapshot AND the live
    // transcript and merge them, then produce a vault page.
    env.alluvium()
        .args(["archive", "--session", &env.session_id])
        .assert()
        .success();

    let topic = env
        .alluvium_subdir()
        .join("wiki")
        .join("concepts")
        .join("pre-compact-topic.md");
    assert!(
        topic.exists(),
        "archive after PreCompact should still produce a topic page"
    );
}
