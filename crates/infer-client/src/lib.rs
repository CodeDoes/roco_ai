//! `RemoteBackend` — a [`ModelBackend`] client for the RoCo **singleton
//! inference API server**.
//!
//! The inference API server is the single owner of the local model (it is a
//! proxy for the hardware it runs on — e.g. the RWKV backend). Every other
//! surface (the HTTP server's story routes, the Zed/VS Code LSP, the CLI
//! REPL) talks to it through this client instead of loading its own model.
//!
//! The wire protocol is the server's OpenAI-compatible `POST /v1/completions`
//! endpoint. Requests are the serializable subset of [`CompletionRequest`]
//! (`on_token` / `bnf_mask` are skipped by `#[serde(skip)]`). When a request
//! carries an `on_token` callback the client sets `stream: true` and consumes
//! the SSE stream, invoking the callback with each emitted delta.
//!
//! The endpoint is OpenAI-compatible, so this same client can target any
//! OpenAI-style `/v1/completions` server by pointing `base_url` at it. Extra
//! request headers (e.g. auth forwarding) are supported via `extra_headers`.

use std::collections::HashMap;

use base64::Engine;
use futures::future::BoxFuture;
use roco_engine::{
    CompletionRequest, CompletionResponse, EngineError, ModelBackend, TokenUsage,
};

/// Default base URL for the singleton inference API server.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080";

/// A [`ModelBackend`] that forwards to a remote inference API server over HTTP.
pub struct RemoteBackend {
    base_url: String,
    client: reqwest::Client,
    extra_headers: HashMap<String, String>,
    name: String,
}

impl RemoteBackend {
    /// Build a client from an explicit base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_headers(base_url, HashMap::new())
    }

    /// Build a client with extra request headers (auth, forwarding, …).
    pub fn with_headers(
        base_url: impl Into<String>,
        extra_headers: HashMap<String, String>,
    ) -> Self {
        let base = base_url.into();
        let base = if base.ends_with('/') {
            base.trim_end_matches('/').to_string()
        } else {
            base
        };
        Self {
            base_url: base,
            client: reqwest::Client::new(),
            extra_headers,
            name: "remote".to_string(),
        }
    }

    /// Build a client, resolving the base URL from `ROCO_API_URL` and any
    /// extra headers from `ROCO_API_HEADERS` (a JSON object of string→string).
    pub fn from_env() -> Self {
        let base = std::env::var("ROCO_API_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let mut headers = HashMap::new();
        if let Ok(raw) = std::env::var("ROCO_API_HEADERS") {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&raw) {
                headers = map;
            }
        }
        Self::with_headers(base, headers)
    }
}

impl ModelBackend for RemoteBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn vocab_bytes(&self) -> Option<Vec<Vec<u8>>> {
        let url = format!("{}/vocab", self.base_url);
        let client = self.client.clone();
        let extra_headers = self.extra_headers.clone();
        // Synchronous fetch (vocab is needed before building a grammar mask).
        let rt = match tokio::runtime::Handle::try_current() {
            Ok(h) => h,
            Err(_) => return None,
        };
        rt.block_on(async move {
            let mut req = client.get(&url);
            for (k, v) in &extra_headers {
                req = req.header(k, v);
            }
            let resp = req.send().await.ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let body: serde_json::Value = resp.json().await.ok()?;
            let arr = body.get("vocab")?.as_array()?;
            let mut vocab = Vec::with_capacity(arr.len());
            for item in arr {
                let b64 = item.as_str()?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
                vocab.push(bytes);
            }
            Some(vocab)
        })
    }

    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let base_url = self.base_url.clone();
        let client = self.client.clone();
        let extra_headers = self.extra_headers.clone();
        Box::pin(async move { remote_complete(&client, &base_url, &extra_headers, req).await })
    }
}

/// Serialized request shape sent to the remote `/v1/completions` endpoint.
/// It is the OpenAI-compatible subset the server's route accepts.
#[derive(serde::Serialize)]
struct WireRequest {
    prompt: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    system: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grammar: Option<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    prefill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preserve_state: Option<bool>,
}

/// Run a completion against the remote inference API server.
async fn remote_complete(
    client: &reqwest::Client,
    base_url: &str,
    extra_headers: &HashMap<String, String>,
    req: CompletionRequest,
) -> Result<CompletionResponse, EngineError> {
    let stream = req.on_token.is_some();
    let wire = WireRequest {
        prompt: req.prompt.clone(),
        system: req.system.clone(),
        temperature: Some(req.temperature),
        max_tokens: Some(req.max_tokens),
        thinking: if req.thinking { Some(true) } else { None },
        grammar: req.grammar.clone(),
        stream,
        prefill: req.prefill.clone(),
        session: req.session.clone(),
        preserve_state: if req.preserve_state { Some(true) } else { None },
    };

    let url = format!("{base_url}/v1/completions");
    let mut builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&wire);
    for (k, v) in extra_headers {
        builder = builder.header(k, v);
    }

    let resp = builder
        .send()
        .await
        .map_err(|e| EngineError::Backend(format!("inference API request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_string());
        return Err(EngineError::Backend(format!(
            "inference API error {status}: {body}"
        )));
    }

    if stream {
        // SSE stream: accumulate deltas, invoke on_token for each text chunk.
        let mut stream = resp.bytes_stream();
        let mut full = String::new();
        let mut prompt_tokens = 0usize;
        let mut completion_tokens = 0usize;
        let on_token = req.on_token;

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| EngineError::Backend(format!("inference API stream error: {e}")))?;
            let text = String::from_utf8_lossy(&chunk);
            for line in text.split('\n') {
                let line = line.trim();
                if !line.starts_with("data:") {
                    continue;
                }
                let data = line.trim_start_matches("data:").trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let value: serde_json::Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                // OpenAI-style delta: choices[0].text
                if let Some(delta) = value.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("text")).and_then(|t| t.as_str()) {
                    if !delta.is_empty() {
                        full.push_str(delta);
                        completion_tokens += 1;
                        if let Some(cb) = &on_token {
                            cb(delta);
                        }
                    }
                }
                // Usage, if the server sends it on a closing event.
                if let Some(u) = value.get("usage") {
                    if let Some(p) = u.get("prompt_tokens").and_then(|v| v.as_u64()) {
                        prompt_tokens = p as usize;
                    }
                    if let Some(c) = u.get("completion_tokens").and_then(|v| v.as_u64()) {
                        completion_tokens = c as usize;
                    }
                }
            }
        }

        if prompt_tokens == 0 {
            prompt_tokens = req.estimated_prompt_tokens;
        }
        return Ok(CompletionResponse {
            text: full,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
            },
            parsed: None,
            think_trace: None,
        });
    }

    // Non-streaming: parse the OpenAI-compatible response envelope.
    let value: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| EngineError::Backend(format!("inference API decode failed: {e}")))?;

    let text = value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| EngineError::Backend("inference API: missing choices[0].text".into()))?
        .to_string();

    let (prompt_tokens, completion_tokens) = match value.get("usage") {
        Some(u) => (
            u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        ),
        None => (req.estimated_prompt_tokens, 0),
    };

    Ok(CompletionResponse {
        text,
        usage: TokenUsage {
            prompt_tokens,
            completion_tokens,
        },
        parsed: None,
        think_trace: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_slash_is_trimmed() {
        let b = RemoteBackend::new("http://localhost:8080/");
        assert_eq!(b.base_url, "http://localhost:8080");
    }

    #[test]
    fn default_base_url_constant() {
        let b = RemoteBackend::new(DEFAULT_BASE_URL);
        assert_eq!(b.base_url, "http://127.0.0.1:8080");
    }

    #[test]
    fn extra_headers_collected() {
        let mut h = HashMap::new();
        h.insert("X-Token".to_string(), "abc".to_string());
        let b = RemoteBackend::with_headers("http://x", h);
        assert_eq!(b.extra_headers.get("X-Token").unwrap(), "abc");
    }
}
