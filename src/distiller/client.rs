//! Anthropic HTTP client.
//!
//! Plain `reqwest` + `serde` against `api.anthropic.com/v1/messages`.
//! No third-party SDK — keeps the dependency surface minimal and avoids
//! tracking an immature crate.

use anyhow::Result;

pub struct AnthropicClient;

impl AnthropicClient {
    pub fn new(_api_key: String) -> Self {
        Self
    }

    pub async fn messages(&self, _request: serde_json::Value) -> Result<serde_json::Value> {
        anyhow::bail!("distiller::client::messages: not yet implemented (scaffold v0.1)")
    }
}
