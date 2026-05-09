//! Pre-prompt byte caps.
//!
//! Truncates per-field content (tool_use args, tool_result output, individual
//! messages) before they go into the prompt. Pattern borrowed from
//! cognee-integrations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ByteCaps {
    pub tool_use: usize,
    pub tool_result: usize,
    pub user_message: usize,
    pub assistant_message: usize,
}

impl Default for ByteCaps {
    fn default() -> Self {
        Self {
            tool_use: 4096,
            tool_result: 8192,
            user_message: 8192,
            assistant_message: 16384,
        }
    }
}

pub fn truncate(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        s.to_owned()
    } else {
        format!(
            "{}\n[... truncated, original was {} bytes]",
            &s[..cap],
            s.len()
        )
    }
}
