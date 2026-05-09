//! Hook engineering layer.
//!
//! Concerns shared by all four Claude Code hooks:
//!
//! - [`plugin_manifest`] — generate / validate `.claude-plugin/plugin.json`
//! - [`lock`] — file-lock so concurrent archives don't collide
//! - [`self_filter`] — refuse to archive sessions whose cwd is the Alluvium
//!   dev tree (avoid recursion)
//! - [`spawn`] — detach a child so the hook returns within 100ms

pub mod lock;
pub mod plugin_manifest;
pub mod self_filter;
pub mod spawn;
