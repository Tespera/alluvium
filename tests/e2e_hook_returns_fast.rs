//! e2e: Stop hook 不卡 Claude Code。
//!
//! 用户可见断言：`alluvium archive --detached` 读取 stdin payload，
//! fork 后台 worker，立刻返回。父进程墙钟时间应远小于一次蒸馏耗时。
//! 这里阈值放在 1.5 秒（足以覆盖 cold-start cargo CI），核心是断言它**不会**
//! 等待蒸馏完成（蒸馏 fake 后端读文件是即时的，但真后端要数秒到分钟）。

mod common;

use common::TestEnv;
use std::io::Write;
use std::time::Instant;

#[test]
fn stop_hook_detached_returns_fast() {
    let env = TestEnv::new("e2e-fast-001");
    env.write_fake_response(r#"{"title":"unused","tags":[],"extracted":[]}"#);

    let payload = env.hook_payload_json("Stop");

    let mut child = env
        .alluvium_std()
        .args(["archive", "--detached"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn alluvium archive --detached");

    // Feed the hook payload on stdin (the binary reads it before forking).
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .expect("write payload");

    let start = Instant::now();
    let status = child.wait().expect("wait for parent to exit");
    let elapsed = start.elapsed();

    assert!(status.success(), "parent must exit cleanly");
    assert!(
        elapsed.as_millis() < 1500,
        "Stop hook parent took {elapsed:?}; should return <1.5 s (the detached worker continues in background)"
    );
}
