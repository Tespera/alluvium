//! Load and render prompt templates from `prompts/*.toml` and `prompts/recipes/*.toml`.
//!
//! Uses `minijinja` for the user_template field. Recipe files inherit from
//! the default and override specific keys.

use anyhow::Result;

pub fn render(_recipe_name: &str, _input: &super::DistillerInput) -> Result<String> {
    anyhow::bail!("distiller::prompt::render: not yet implemented (scaffold v0.1)")
}
