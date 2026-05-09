//! YAML frontmatter parse + merge.
//!
//! Uses `gray_matter` for parsing; renders via the `templates/frontmatter.yaml.j2`
//! template.

use anyhow::Result;

pub fn parse(_markdown: &str) -> Result<(serde_yaml::Value, String)> {
    anyhow::bail!("vault::frontmatter::parse: not yet implemented (scaffold v0.1)")
}

pub fn render(_data: &serde_yaml::Value) -> Result<String> {
    anyhow::bail!("vault::frontmatter::render: not yet implemented (scaffold v0.1)")
}
