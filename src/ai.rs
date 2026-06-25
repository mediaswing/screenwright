//! Optional AI-assisted writing-prompt generation.
//!
//! The user brings their own account: either an Anthropic (Claude) or an
//! OpenAI (ChatGPT) API key. Nothing here runs unless the user explicitly asks
//! for a prompt and has supplied a key, so the rest of the app stays fully
//! usable offline.
//!
//! Rust has no official Anthropic/OpenAI SDK, so we talk to the REST endpoints
//! directly over HTTPS (`ureq`). Each provider has a slightly different request
//! and response shape; [`generate`] hides that behind one call.

use serde_json::{json, Value};

/// Which AI backend to call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Anthropic Claude — `POST /v1/messages`.
    Anthropic,
    /// OpenAI ChatGPT — `POST /v1/chat/completions`.
    OpenAi,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "Claude (Anthropic)",
            Provider::OpenAi => "ChatGPT (OpenAI)",
        }
    }

    /// The default, most-capable model for this provider.
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-opus-4-8",
            Provider::OpenAi => "gpt-4o",
        }
    }

    /// Environment variable conventionally holding this provider's key.
    pub fn env_var(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
        }
    }
}

/// Everything needed to make one request.
#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub api_key: String,
    pub model: String,
}

const SYSTEM_PROMPT: &str = "You are a generative muse for screenwriters. \
When asked, produce a single vivid, original screenplay writing prompt: a \
brief premise with a strong central character, a clear dramatic conflict, and \
an evocative setting that could open a scene. Keep it to 3-5 sentences. Do not \
include commentary, headings, or options — just the prompt itself.";

/// Build the user-facing instruction from an optional topic/seed.
fn user_message(topic: &str) -> String {
    let topic = topic.trim();
    if topic.is_empty() {
        "Give me a fresh screenplay writing prompt.".to_string()
    } else {
        format!("Give me a screenplay writing prompt inspired by: {topic}")
    }
}

/// Call the configured provider and return the generated prompt text.
///
/// This blocks on the network, so callers should run it off the UI thread.
pub fn generate(config: &Config, topic: &str) -> Result<String, String> {
    if config.api_key.trim().is_empty() {
        return Err(format!(
            "No API key. Set {} or paste a key in the panel.",
            config.provider.env_var()
        ));
    }
    match config.provider {
        Provider::Anthropic => call_anthropic(config, topic),
        Provider::OpenAi => call_openai(config, topic),
    }
}

fn call_anthropic(config: &Config, topic: &str) -> Result<String, String> {
    let body = json!({
        "model": config.model,
        "max_tokens": 400,
        "system": SYSTEM_PROMPT,
        "messages": [{ "role": "user", "content": user_message(topic) }],
    });

    let val = post_json(
        "https://api.anthropic.com/v1/messages",
        &[
            ("x-api-key", config.api_key.as_str()),
            ("anthropic-version", "2023-06-01"),
        ],
        body,
    )?;

    // Success shape: { "content": [ { "type": "text", "text": "..." }, ... ] }
    val["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "Claude returned no text content.".to_string())
}

fn call_openai(config: &Config, topic: &str) -> Result<String, String> {
    let body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_message(topic) },
        ],
    });

    let auth = format!("Bearer {}", config.api_key);
    let val = post_json(
        "https://api.openai.com/v1/chat/completions",
        &[("authorization", auth.as_str())],
        body,
    )?;

    // Success shape: { "choices": [ { "message": { "content": "..." } } ] }
    val["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "ChatGPT returned no message content.".to_string())
}

/// POST `body` as JSON with the given extra headers and parse the JSON reply.
///
/// HTTP error statuses are not treated as transport errors here so that the
/// provider's own error message (which lives in the response body) can be
/// surfaced to the user.
fn post_json(url: &str, headers: &[(&str, &str)], body: Value) -> Result<Value, String> {
    let mut req = ureq::post(url)
        .config()
        .http_status_as_error(false)
        .build()
        .header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }

    let mut resp = req
        .send_json(&body)
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let val: Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("Could not parse the response: {e}"))?;

    if !status.is_success() {
        // Both providers nest a human-readable message under `error.message`.
        let detail = val["error"]["message"]
            .as_str()
            .unwrap_or("unknown error");
        return Err(format!("API error ({}): {detail}", status.as_u16()));
    }
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_handles_empty_and_topic() {
        assert!(user_message("   ").contains("fresh"));
        assert!(user_message("a heist on Mars").contains("heist on Mars"));
    }

    #[test]
    fn missing_key_is_reported_without_network() {
        let cfg = Config {
            provider: Provider::Anthropic,
            api_key: "  ".into(),
            model: "claude-opus-4-8".into(),
        };
        let err = generate(&cfg, "noir").unwrap_err();
        assert!(err.contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn provider_defaults() {
        assert_eq!(Provider::Anthropic.default_model(), "claude-opus-4-8");
        assert_eq!(Provider::OpenAi.env_var(), "OPENAI_API_KEY");
    }
}
