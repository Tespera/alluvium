//! Fake LLM backend for end-to-end tests.
//!
//! Reads a canned response file from `$ALLUVIUM_FAKE_LLM_RESPONSE_PATH` and
//! returns its contents as the LLM's text output. Activated by selection.rs
//! whenever that env var is non-empty — so tests just point the env var at
//! a JSON fixture and the rest of the pipeline runs untouched.
//!
//! This backend is intentionally undocumented for end users: it is a test
//! seam, not a feature. A user who *did* set the env var would presumably
//! want the same behavior, so leaking through to production is harmless.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;

use crate::distiller::backend::{LlmBackend, LlmResponse, RenderedPrompt};

#[derive(Debug, Clone)]
pub struct FakeBackend {
    response_path: PathBuf,
}

impl FakeBackend {
    pub fn new(response_path: PathBuf) -> Self {
        Self { response_path }
    }
}

#[async_trait]
impl LlmBackend for FakeBackend {
    async fn complete(&self, _prompt: &RenderedPrompt) -> Result<LlmResponse> {
        let text = std::fs::read_to_string(&self.response_path).with_context(|| {
            format!(
                "FakeBackend: reading canned response from {}",
                self.response_path.display()
            )
        })?;
        Ok(LlmResponse { text, usage: None })
    }

    fn kind(&self) -> &'static str {
        "fake"
    }
}
