//! Load `prompts/*.toml` and render the user message via minijinja.
//!
//! ## Resolution flow
//!
//! 1. Read `prompts_dir/distill.toml` as the **base** template.
//! 2. If `recipe_name != "default"`, also read
//!    `prompts_dir/recipes/<recipe_name>.toml` and apply its overrides:
//!    - `[overrides]` table sets `max_facts_per_session` / `max_body_chars`
//!    - `[prompt_overrides].extra_system` is **appended** to base system prompt
//!    - `[meta].model` (if present) overrides base model
//! 3. The resulting [`PromptTemplate`] is what [`render`] consumes.
//!
//! ## Render
//!
//! [`render`] applies [`super::budget::ByteCaps`] truncation to each transcript
//! field, then renders the user_template through minijinja with the prepared
//! data. It returns a JSON value ready to POST as the body of an Anthropic
//! Messages API request.

use anyhow::{Context, Result};
use minijinja::{context, Environment};
use serde::Deserialize;
use std::path::Path;

use super::budget::{self, ByteCaps};
use super::DistillerInput;

/// A fully-resolved prompt template.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub model: String,
    pub max_tokens: u32,
    pub byte_caps: ByteCaps,
    pub max_facts_per_session: Option<u32>,
    pub max_body_chars: Option<u32>,
    pub system: String,
    pub user_template: String,
}

// ────────────── on-disk file shapes ──────────────

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
    model: String,
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

// ────────────── load + render ──────────────

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
            tpl.model = model;
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

/// Render the template into an Anthropic Messages API request body.
pub fn render(tpl: &PromptTemplate, input: &DistillerInput) -> Result<serde_json::Value> {
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

    Ok(serde_json::json!({
        "model": tpl.model,
        "max_tokens": tpl.max_tokens,
        "system": tpl.system,
        "messages": [
            {"role": "user", "content": user_message}
        ]
    }))
}

/// Apply byte caps to each transcript message field and shape the data
/// minijinja will see. Tool-use inputs are JSON-stringified before capping
/// (LLM doesn't need the structured form once truncation is in play).
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
    use crate::transcript::{ConversationData, Message, ToolCall};
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
model = "claude-haiku-4-5"

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
                messages: vec![
                    Message {
                        role: "user".into(),
                        content: "Hello there".into(),
                        tool_calls: vec![],
                    },
                    Message {
                        role: "assistant".into(),
                        content: "Hi back!".into(),
                        tool_calls: vec![ToolCall {
                            name: "Bash".into(),
                            input: serde_json::json!({"command": "ls"}),
                            output: Some("file1.txt".into()),
                        }],
                    },
                ],
                metadata: SessionMetadata {
                    session_id: "sid-123".into(),
                    cwd: None,
                    started_at: None,
                    ended_at: None,
                    model: Some("claude-opus-4-7".into()),
                    input_tokens: 0,
                    output_tokens: 0,
                    event_count: 2,
                },
            },
            recipe_name: "default".into(),
        }
    }

    // ────────────── load ──────────────

    #[test]
    fn load_default_reads_base_only() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), minimal_base_toml());

        let tpl = load("default", dir.path()).unwrap();
        assert_eq!(tpl.model, "claude-haiku-4-5");
        assert_eq!(tpl.max_tokens, 4096);
        assert_eq!(tpl.system, "BASE SYSTEM");
        assert!(tpl.max_facts_per_session.is_none());
        assert!(tpl.max_body_chars.is_none());
    }

    #[test]
    fn load_recipe_appends_extra_system_to_base() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), minimal_base_toml());
        write_file(
            &dir.path().join("recipes/minim.toml"),
            r#"
[meta]
name = "minim"
inherits = "default"

[overrides]
max_facts_per_session = 5

[prompt_overrides]
extra_system = "\n\nMINIMAL OVERRIDE"
"#,
        );

        let tpl = load("minim", dir.path()).unwrap();
        assert!(tpl.system.starts_with("BASE SYSTEM"));
        assert!(tpl.system.contains("MINIMAL OVERRIDE"));
        assert_eq!(tpl.max_facts_per_session, Some(5));
    }

    #[test]
    fn load_recipe_can_override_model() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), minimal_base_toml());
        write_file(
            &dir.path().join("recipes/sonnet.toml"),
            r#"
[meta]
name = "sonnet"
model = "claude-sonnet-4-6"
"#,
        );

        let tpl = load("sonnet", dir.path()).unwrap();
        assert_eq!(tpl.model, "claude-sonnet-4-6");
    }

    #[test]
    fn load_recipe_missing_overrides_table_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), minimal_base_toml());
        write_file(
            &dir.path().join("recipes/bare.toml"),
            r#"
[meta]
name = "bare"
"#,
        );
        let tpl = load("bare", dir.path()).unwrap();
        assert!(tpl.max_facts_per_session.is_none());
    }

    #[test]
    fn load_missing_base_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = load("default", dir.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("reading base prompt"), "got: {msg}");
    }

    #[test]
    fn load_missing_recipe_errors() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), minimal_base_toml());
        let err = load("nonexistent", dir.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("reading recipe"), "got: {msg}");
    }

    #[test]
    fn load_malformed_toml_errors() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("distill.toml"), "this is = = not = toml");
        let err = load("default", dir.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("parsing base prompt"), "got: {msg}");
    }

    // ────────────── render ──────────────

    #[test]
    fn render_produces_anthropic_request_shape() {
        let tpl = PromptTemplate {
            model: "claude-haiku-4-5".into(),
            max_tokens: 1024,
            byte_caps: ByteCaps::default(),
            max_facts_per_session: None,
            max_body_chars: None,
            system: "SYS".into(),
            user_template: "Session: {{ transcript.session_id }}".into(),
        };
        let req = render(&tpl, &sample_input()).unwrap();
        assert_eq!(req["model"], "claude-haiku-4-5");
        assert_eq!(req["max_tokens"], 1024);
        assert_eq!(req["system"], "SYS");
        assert_eq!(req["messages"][0]["role"], "user");
        let user_content = req["messages"][0]["content"].as_str().unwrap();
        assert!(user_content.contains("sid-123"));
    }

    #[test]
    fn render_passes_max_facts_to_template() {
        let tpl = PromptTemplate {
            model: "x".into(),
            max_tokens: 1024,
            byte_caps: ByteCaps::default(),
            max_facts_per_session: Some(7),
            max_body_chars: None,
            system: "".into(),
            user_template: "{% if max_facts %}MAX={{ max_facts }}{% endif %}".into(),
        };
        let req = render(&tpl, &sample_input()).unwrap();
        let user = req["messages"][0]["content"].as_str().unwrap();
        assert!(user.contains("MAX=7"));
    }

    #[test]
    fn render_passes_messages_to_template() {
        let tpl = PromptTemplate {
            model: "x".into(),
            max_tokens: 1024,
            byte_caps: ByteCaps::default(),
            max_facts_per_session: None,
            max_body_chars: None,
            system: "".into(),
            user_template:
                "Count: {{ transcript.messages | length }} | First role: {{ transcript.messages[0].role }}".into(),
        };
        let req = render(&tpl, &sample_input()).unwrap();
        let user = req["messages"][0]["content"].as_str().unwrap();
        assert!(user.contains("Count: 2"));
        assert!(user.contains("First role: user"));
    }

    #[test]
    fn render_truncates_long_user_message() {
        let tpl = PromptTemplate {
            model: "x".into(),
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
        let req = render(&tpl, &input).unwrap();
        let user = req["messages"][0]["content"].as_str().unwrap();
        assert!(user.contains("truncated"));
    }

    #[test]
    fn render_includes_tool_calls_in_template() {
        let tpl = PromptTemplate {
            model: "x".into(),
            max_tokens: 1024,
            byte_caps: ByteCaps::default(),
            max_facts_per_session: None,
            max_body_chars: None,
            system: "".into(),
            user_template:
                "TC: {{ transcript.messages[1].tool_calls[0].name }} OUT: {{ transcript.messages[1].tool_calls[0].output }}".into(),
        };
        let req = render(&tpl, &sample_input()).unwrap();
        let user = req["messages"][0]["content"].as_str().unwrap();
        assert!(user.contains("TC: Bash"));
        assert!(user.contains("OUT: file1.txt"));
    }

    #[test]
    fn render_template_compile_error_surfaces_clearly() {
        let tpl = PromptTemplate {
            model: "x".into(),
            max_tokens: 1024,
            byte_caps: ByteCaps::default(),
            max_facts_per_session: None,
            max_body_chars: None,
            system: "".into(),
            user_template: "{% this is broken jinja".into(),
        };
        let err = render(&tpl, &sample_input()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("compiling user_template") || msg.contains("user_template"),
            "got: {msg}"
        );
    }

    // ────────────── real prompts/ files ──────────────

    /// Smoke test: the actual repo prompts/distill.toml + each recipe must
    /// load without error. Catches schema drift between the toml and our
    /// deserialize structs.
    #[test]
    fn real_repo_prompts_load_cleanly() {
        let prompts_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts");
        if !prompts_dir.exists() {
            return; // unusual, but don't fail the suite
        }
        let _default = load("default", &prompts_dir).expect("default loads");
        for recipe in &["minimalist", "dev-journal", "verbose"] {
            let _t = load(recipe, &prompts_dir)
                .unwrap_or_else(|e| panic!("recipe {recipe} should load, got {e:#}"));
        }
    }
}
