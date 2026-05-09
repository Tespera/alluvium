//! Load `prompts/*.toml` and render a backend-agnostic prompt.
//!
//! Output is a [`super::backend::RenderedPrompt`] (system + user + model +
//! max_tokens) which any [`super::backend::LlmBackend`] can consume.
//!
//! ## Resolution flow
//!
//! 1. Read `prompts_dir/distill.toml` as the **base** template.
//! 2. If `recipe_name != "default"`, also read
//!    `prompts_dir/recipes/<recipe_name>.toml` and apply its overrides.
//! 3. Render the user_template via minijinja with the prepared transcript.

use anyhow::{Context, Result};
use minijinja::{context, Environment};
use serde::Deserialize;
use std::path::Path;

use super::backend::RenderedPrompt;
use super::budget::{self, ByteCaps};
use super::DistillerInput;

/// A fully-resolved prompt template — the in-memory shape after merging
/// base + recipe.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub model: Option<String>,
    pub max_tokens: u32,
    pub byte_caps: ByteCaps,
    pub max_facts_per_session: Option<u32>,
    pub max_body_chars: Option<u32>,
    pub system: String,
    pub user_template: String,
}

#[derive(Debug, Deserialize)]
struct BaseFile {
    meta: BaseMeta,
    byte_caps: ByteCaps,
    prompt: PromptSection,
}

#[derive(Debug, Deserialize)]
struct BaseMeta {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default = "default_max_tokens")]
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct PromptSection {
    system: String,
    user_template: String,
}

fn default_max_tokens() -> u32 {
    4096
}

#[derive(Debug, Deserialize)]
struct RecipeFile {
    meta: RecipeMeta,
    #[serde(default)]
    overrides: RecipeOverrides,
    #[serde(default)]
    prompt_overrides: RecipePromptOverrides,
}

#[derive(Debug, Deserialize)]
struct RecipeMeta {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    description: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    inherits: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    max_tokens: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct RecipeOverrides {
    #[serde(default)]
    max_facts_per_session: Option<u32>,
    #[serde(default)]
    max_body_chars: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct RecipePromptOverrides {
    #[serde(default)]
    extra_system: Option<String>,
}

/// Load and resolve a recipe. `recipe_name == "default"` skips recipe lookup.
pub fn load(recipe_name: &str, prompts_dir: &Path) -> Result<PromptTemplate> {
    let base_path = prompts_dir.join("distill.toml");
    let base_text = std::fs::read_to_string(&base_path)
        .with_context(|| format!("reading base prompt {}", base_path.display()))?;
    let base: BaseFile = toml::from_str(&base_text)
        .with_context(|| format!("parsing base prompt {}", base_path.display()))?;

    let mut tpl = PromptTemplate {
        model: base.meta.model,
        max_tokens: base.meta.max_tokens,
        byte_caps: base.byte_caps,
        max_facts_per_session: None,
        max_body_chars: None,
        system: base.prompt.system,
        user_template: base.prompt.user_template,
    };

    if recipe_name != "default" {
        let recipe_path = prompts_dir
            .join("recipes")
            .join(format!("{recipe_name}.toml"));
        let recipe_text = std::fs::read_to_string(&recipe_path)
            .with_context(|| format!("reading recipe {}", recipe_path.display()))?;
        let recipe: RecipeFile = toml::from_str(&recipe_text)
            .with_context(|| format!("parsing recipe {}", recipe_path.display()))?;

        if let Some(model) = recipe.meta.model {
            tpl.model = Some(model);
        }
        if let Some(mt) = recipe.meta.max_tokens {
            tpl.max_tokens = mt;
        }
        tpl.max_facts_per_session = recipe.overrides.max_facts_per_session;
        tpl.max_body_chars = recipe.overrides.max_body_chars;
        if let Some(extra) = recipe.prompt_overrides.extra_system {
            tpl.system.push_str(&extra);
        }
    }

    Ok(tpl)
}

/// Render the template into a backend-agnostic [`RenderedPrompt`].
pub fn render(tpl: &PromptTemplate, input: &DistillerInput) -> Result<RenderedPrompt> {
    let prepared = prepare_for_template(input, &tpl.byte_caps);

    let mut env = Environment::new();
    env.add_template("user", &tpl.user_template)
        .context("compiling user_template (minijinja)")?;
    let template = env
        .get_template("user")
        .expect("template was just added; lookup must succeed");

    let user_message = template
        .render(context! {
            transcript => prepared,
            max_facts => tpl.max_facts_per_session,
            max_body_chars => tpl.max_body_chars,
        })
        .context("rendering user_template (minijinja)")?;

    Ok(RenderedPrompt {
        system: tpl.system.clone(),
        user: user_message,
        model: tpl.model.clone(),
        max_tokens: tpl.max_tokens,
    })
}

fn prepare_for_template(input: &DistillerInput, caps: &ByteCaps) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = input
        .conversation
        .messages
        .iter()
        .map(|m| {
            let cap = if m.role == "assistant" {
                caps.assistant_message
            } else {
                caps.user_message
            };
            let truncated_content = budget::truncate(&m.content, cap);

            let tool_calls: Vec<serde_json::Value> = m
                .tool_calls
                .iter()
                .map(|tc| {
                    let input_str =
                        serde_json::to_string(&tc.input).unwrap_or_else(|_| String::from("{}"));
                    let input_capped = budget::truncate(&input_str, caps.tool_use);
                    let output = tc
                        .output
                        .as_ref()
                        .map(|o| budget::truncate(o, caps.tool_result));
                    serde_json::json!({
                        "name": tc.name,
                        "input": input_capped,
                        "output": output,
                    })
                })
                .collect();

            serde_json::json!({
                "role": m.role,
                "content": truncated_content,
                "tool_calls": tool_calls,
            })
        })
        .collect();

    serde_json::json!({
        "session_id": input.conversation.session_id,
        "metadata": &input.conversation.metadata,
        "messages": messages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::metadata::SessionMetadata;
    use crate::transcript::{ConversationData, Message};
    use std::io::Write;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    fn minimal_base_toml() -> &'static str {
        r#"
[meta]
name = "default"

[byte_caps]
tool_use = 4096
tool_result = 8192
user_message = 8192
assistant_message = 16384

[prompt]
system = "BASE SYSTEM"
user_template = "USER {{ transcript.session_id }}"
"#
    }

    fn sample_input() -> DistillerInput {
        DistillerInput {
            conversation: ConversationData {
                session_id: "sid-123".into(),
                messages: vec![Message {
                    role: "user".into(),
                    content: "hello".into(),
                    tool_calls: vec![],
                }],
                metadata: SessionMetadata {
                    session_id: "sid-123".into(),
                    cwd: None,
                    started_at: None,
                    ended_at: None,
                    model: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    event_count: 1,
                },
            },
            recipe_name: "default".into(),
        }
    }

    #[test]
    fn load_default_yields_no_model_override() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), minimal_base_toml());
        let tpl = load("default", dir.path()).unwrap();
        assert!(tpl.model.is_none());
        assert_eq!(tpl.max_tokens, 4096);
        assert_eq!(tpl.system, "BASE SYSTEM");
    }

    #[test]
    fn render_produces_rendered_prompt_struct() {
        let tpl = PromptTemplate {
            model: Some("claude-haiku-4-5".into()),
            max_tokens: 1024,
            byte_caps: ByteCaps::default(),
            max_facts_per_session: None,
            max_body_chars: None,
            system: "SYS".into(),
            user_template: "Session: {{ transcript.session_id }}".into(),
        };
        let p = render(&tpl, &sample_input()).unwrap();
        assert_eq!(p.system, "SYS");
        assert_eq!(p.model.as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(p.max_tokens, 1024);
        assert!(p.user.contains("sid-123"));
    }

    #[test]
    fn render_passes_max_facts_to_template() {
        let tpl = PromptTemplate {
            model: None,
            max_tokens: 1024,
            byte_caps: ByteCaps::default(),
            max_facts_per_session: Some(7),
            max_body_chars: None,
            system: "".into(),
            user_template: "{% if max_facts %}MAX={{ max_facts }}{% endif %}".into(),
        };
        let p = render(&tpl, &sample_input()).unwrap();
        assert!(p.user.contains("MAX=7"));
    }

    #[test]
    fn render_truncates_long_user_message() {
        let tpl = PromptTemplate {
            model: None,
            max_tokens: 1024,
            byte_caps: ByteCaps {
                user_message: 20,
                ..ByteCaps::default()
            },
            max_facts_per_session: None,
            max_body_chars: None,
            system: "".into(),
            user_template: "{{ transcript.messages[0].content }}".into(),
        };
        let mut input = sample_input();
        input.conversation.messages[0].content = "X".repeat(1000);
        let p = render(&tpl, &input).unwrap();
        assert!(p.user.contains("truncated"));
    }

    #[test]
    fn load_recipe_appends_extra_system() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), minimal_base_toml());
        write_file(
            &dir.path().join("recipes/min.toml"),
            r#"
[meta]
name = "min"
inherits = "default"

[overrides]
max_facts_per_session = 5

[prompt_overrides]
extra_system = "\n\nMINIMAL"
"#,
        );
        let tpl = load("min", dir.path()).unwrap();
        assert!(tpl.system.contains("BASE SYSTEM"));
        assert!(tpl.system.contains("MINIMAL"));
        assert_eq!(tpl.max_facts_per_session, Some(5));
    }

    /// Smoke: shipped prompts/distill.toml loads cleanly under the new
    /// (model-optional) deserialize path.
    #[test]
    fn real_repo_prompts_load_cleanly() {
        let prompts_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts");
        if !prompts_dir.exists() {
            return;
        }
        let _default = load("default", &prompts_dir).expect("default loads");
        for recipe in &["minimalist", "dev-journal", "verbose"] {
            let _t = load(recipe, &prompts_dir)
                .unwrap_or_else(|e| panic!("recipe {recipe} should load, got {e:#}"));
        }
    }
}
