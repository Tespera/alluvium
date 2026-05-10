//! Concrete LLM backend implementations.
//!
//! Each submodule implements the [`super::backend::LlmBackend`] trait for
//! one provider. The selection logic lives in
//! [`super::backend_selection`]; cli/archive constructs whichever backend
//! the user's config (or auto-detection) indicates.

pub mod anthropic;
pub mod claude_cli;
pub mod fake;
pub mod gemini;
pub mod openai_compatible;
