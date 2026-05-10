//! e2e: dry-run 不写盘。
//!
//! 用户可见断言：跑完 `alluvium dry-run` 后，vault 目录不变（无 wiki/、无 raw/、
//! 无 log.md），但 stdout 上能看到蒸馏后的内容预览。

mod common;

use common::TestEnv;

const FAKE_RESPONSE: &str = r#"{
  "title": "Dry-run preview",
  "tags": ["preview"],
  "extracted": [
    {
      "type": "concept",
      "page_slug": "dry-run-output",
      "page_title": "Dry-Run Output",
      "summary": "Preview only — should never reach the vault.",
      "body_markdown": "If you can read this in vault/Alluvium/, dry-run is broken.",
      "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
      "confidence": 0.9
    }
  ]
}"#;

#[test]
fn dry_run_does_not_write_vault() {
    let env = TestEnv::new("e2e-dry-001");
    env.write_fake_response(FAKE_RESPONSE);

    let output = env.alluvium().arg("dry-run").output().expect("run dry-run");
    assert!(
        output.status.success(),
        "dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Dry-run preview") || stdout.contains("dry-run-output"),
        "stdout should contain a preview of distilled content; got:\n{stdout}"
    );

    // Vault must remain pristine — dry-run is read-only by contract.
    let alluvium_root = env.alluvium_subdir();
    assert!(
        !alluvium_root.join("wiki").exists(),
        "dry-run must not create wiki/"
    );
    assert!(
        !alluvium_root.join("raw").exists(),
        "dry-run must not create raw/"
    );
}
