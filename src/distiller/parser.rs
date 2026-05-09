//! Parse the LLM's JSON response into [`super::DistillerOutput`].
//!
//! Strict schema validation against `prompts/distill.toml`'s
//! `[output_schema]` declaration. Drift between the prompt schema and the
//! parser's expectations is caught here and surfaces as a test failure.

use anyhow::Result;

pub fn parse(_response: serde_json::Value) -> Result<super::DistillerOutput> {
    anyhow::bail!("distiller::parser::parse: not yet implemented (scaffold v0.1)")
}
