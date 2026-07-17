use zed_extension_api::{self as zed, Command, LanguageServerCommand};

const API_BASE: &str = "http://localhost:3000";

struct RoCoExtension;

impl zed::Extension for RoCoExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<Command, String> {
        // We don't use a language server, but we need to implement this
        Err("No language server".to_string())
    }

    fn completions(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        _buffer: &zed::Buffer,
        _position: zed::PointUtf16,
    ) -> Result<Vec<zed::Completion>, String> {
        // TODO: Get suggestions from API
        Ok(vec![])
    }
}

zed::register_extension!(RoCoExtension);

/// Make an API request
async fn api_request(path: &str, body: Option<&str>) -> Result<String, String> {
    let url = format!("{}{}", API_BASE, path);

    let client = reqwest::Client::new();
    let mut request = client.get(&url);

    if let Some(body) = body {
        request = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    Ok(text)
}

/// Generate a chapter
async fn generate_chapter(content: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "content": content,
    });

    let result = api_request("/chapters/generate", Some(&body.to_string())).await?;
    let response: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| format!("Failed to parse response: {}", e))?;

    response["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in response".to_string())
}

/// Continue writing
async fn continue_writing(text: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "text": text,
    });

    let result = api_request("/continue", Some(&body.to_string())).await?;
    let response: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| format!("Failed to parse response: {}", e))?;

    response["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No text in response".to_string())
}

/// Get suggestions
async fn get_suggestions(text: &str) -> Result<Vec<String>, String> {
    let body = serde_json::json!({
        "text": text,
    });

    let result = api_request("/suggestions", Some(&body.to_string())).await?;
    let response: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| format!("Failed to parse response: {}", e))?;

    let suggestions = response["suggestions"]
        .as_array()
        .ok_or_else(|| "No suggestions in response".to_string())?;

    Ok(suggestions
        .iter()
        .filter_map(|s| s["text"].as_str().map(|s| s.to_string()))
        .collect())
}
