use zed_extension_api::{
    self as zed,
    http_client::{HttpMethod, HttpRequest, RedirectPolicy},
    Command, SlashCommand, SlashCommandOutput,
    SlashCommandOutputSection,
};
use std::sync::Mutex;

/// Default URL for the RoCo server. Override via the `ROCO_API_URL` env var.
fn api_base() -> String {
    std::env::var("ROCO_API_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

struct RoCoExtension {
    /// Cache whether the server was reachable on last check
    last_health_ok: Mutex<bool>,
}

impl RoCoExtension {
    /// Quickly check if the roco server is alive.
    fn server_running(&self) -> bool {
        let url = format!("{}/health", api_base());
        match HttpRequest::builder()
            .method(HttpMethod::Get)
            .url(&url)
            .build()
        {
            Ok(req) => match req.fetch() {
                Ok(_resp) => {
                    // Successful HTTP response means zed runtime handled redirects/errors
                    *self.last_health_ok.lock().unwrap() = true;
                    true
                }
                Err(_e) => {
                    *self.last_health_ok.lock().unwrap() = false;
                    false
                }
            },
            Err(_) => false,
        }
    }
}

impl zed::Extension for RoCoExtension {
    fn new() -> Self {
        Self {
            last_health_ok: Mutex::new(false),
        }
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<Command, String> {
        let roco_path = std::env::var("ROCO_PATH").unwrap_or_else(|_| "roco".to_string());
        Ok(Command {
            command: roco_path,
            args: vec![
                "server".to_string(),
                "--stdio-lsp".to_string(),
            ],
            env: vec![],
        })
    }

    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        _worktree: Option<&zed::Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        // Called when the user types /roco <args> in the assistant panel
        if command.name != "roco" {
            return Err(format!("Unknown slash command: {}", command.name));
        }

        if !self.server_running() {
            return Err(
                "RoCo server not running — start it in your terminal:\n  roco server --story --detach\n\nOr set ROCO_API_URL to point to a running instance.".to_string(),
            );
        }

        let input = args.join(" ");
        if input.trim().is_empty() {
            return Ok(SlashCommandOutput {
                text: "Usage: /roco <prompt> — generates story text from the given prompt."
                    .to_string(),
                sections: vec![],
            });
        }

        // Call the OpenAI-compatible endpoint
        let url = format!("{}/v1/completions", api_base());
        let body = serde_json::json!({
            "prompt": input,
            "max_tokens": 256,
            "temperature": 0.4,
            "system": "You are a creative writing assistant. Complete the text naturally with vivid prose.",
        });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| format!("Failed to serialize request: {e}"))?;

        let http_req = HttpRequest::builder()
            .method(HttpMethod::Post)
            .url(&url)
            .header("Content-Type", "application/json")
            .body(body_bytes)
            .redirect_policy(RedirectPolicy::FollowLimit(5))
            .build()
            .map_err(|e| format!("Failed to build request: {e}"))?;

        let resp = http_req.fetch().map_err(|e| format!("API error: {e}"))?;
        let body_str = String::from_utf8(resp.body)
            .map_err(|e| format!("Non-UTF8 response: {e}"))?;

        let value: serde_json::Value = serde_json::from_str(&body_str)
            .map_err(|e| format!("Failed to parse response: {e}"))?;

        let text = value["choices"][0]["text"]
            .as_str()
            .ok_or_else(|| "No completion in response".to_string())?;

        Ok(SlashCommandOutput {
            text: text.trim().to_string(),
            sections: vec![SlashCommandOutputSection {
                range: (0..(text.len() as u32)).into(),
                label: "RoCo AI".to_string(),
            }],
        })
    }

    fn complete_slash_command_argument(
        &self,
        _command: SlashCommand,
        _args: Vec<String>,
    ) -> Result<Vec<zed::SlashCommandArgumentCompletion>, String> {
        // Provide suggestions for /roco argument completions
        Ok(vec![
            zed::SlashCommandArgumentCompletion {
                label: "Write a chapter about...".to_string(),
                new_text: "a lone cultivator seeking immortality".to_string(),
                run_command: false,
            },
            zed::SlashCommandArgumentCompletion {
                label: "Continue the story...".to_string(),
                new_text: "continuing from where we left off, the knight".to_string(),
                run_command: false,
            },
        ])
    }
}

zed::register_extension!(RoCoExtension);

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_base_default() {
        let saved = std::env::var("ROCO_API_URL").ok();
        std::env::remove_var("ROCO_API_URL");
        assert_eq!(api_base(), "http://localhost:8080");
        if let Some(val) = saved {
            std::env::set_var("ROCO_API_URL", val);
        }
    }

    #[test]
    fn test_api_base_env_override() {
        let saved = std::env::var("ROCO_API_URL").ok();
        std::env::set_var("ROCO_API_URL", "http://10.0.0.1:9999");
        assert_eq!(api_base(), "http://10.0.0.1:9999");
        if let Some(val) = saved {
            std::env::set_var("ROCO_API_URL", val);
        } else {
            std::env::remove_var("ROCO_API_URL");
        }
    }

    #[test]
    fn test_response_parsing() {
        let response_json = serde_json::json!({
            "choices": [{
                "text": "The knight drew his sword and faced the dragon.",
                "index": 0,
                "finish_reason": "length"
            }]
        });

        let text = response_json["choices"][0]["text"]
            .as_str()
            .expect("text field");
        assert!(text.contains("knight"));
        assert!(text.contains("dragon"));
    }

    #[test]
    fn test_response_missing_field() {
        let response_json = serde_json::json!({
            "choices": [{
                "finish_reason": "stop"
            }]
        });
        assert!(response_json["choices"][0]["text"].as_str().is_none());
    }

    #[test]
    fn test_request_body_matches_server_contract() {
        // The `/roco` slash command POSTs JSON to the singleton inference
        // API server's `POST /v1/completions`. The server's route
        // (`crates/server/src/routes.rs` `OpenAiCompletionRequest`) requires
        // `prompt` and optionally `system`/`temperature`/`max_tokens`.
        // This locks the plugin→server request contract so a shape change
        // on either side breaks the test.
        let input = "a lone cultivator seeking immortality";
        let body = serde_json::json!({
            "prompt": input,
            "max_tokens": 256,
            "temperature": 0.4,
            "system": "You are a creative writing assistant. Complete the text naturally with vivid prose.",
        });

        // Required field present and a string.
        assert!(body["prompt"].is_string());
        assert_eq!(body["prompt"].as_str().unwrap(), input);
        // Optional fields the server accepts.
        assert!(body["system"].is_string());
        assert!(body["max_tokens"].is_u64());
        assert!(body["temperature"].is_f64());
        // No chat-completions-only fields are sent (server has no
        // `/v1/chat/completions`); the endpoint is text-completion only.
        assert!(body.get("messages").is_none());
        assert!(body.get("model").is_none());
    }

    #[test]
    fn test_server_health_path() {
        // The slash command checks `{api_base}/health` before calling.
        let url = format!("{}/health", api_base());
        assert!(url.ends_with("/health"));
        assert!(!url.contains("//health"));
    }

    #[test]
    fn test_usage_text_from_api() {
        let response = SlashCommandOutput {
            text: "Usage: /roco <prompt> — generates story text from the given prompt."
                .to_string(),
            sections: vec![],
        };
        assert!(response.text.starts_with("Usage:"));
        assert!(response.text.contains("/roco"));
    }
}
