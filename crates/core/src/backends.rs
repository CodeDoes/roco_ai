//! HTTP model backends (OpenAI-compatible chat completions).
//!
//! THESE ARE OPTIONAL — the local-first philosophy means RWKV and other local
//! models are the backbone. API backends are supplements for tasks that need
//! more capability than local hardware can provide.
//!
//! Both NVIDIA's free developer API and Kilo-AI expose OpenAI-compatible
//! `/chat/completions` endpoints, so they share one client. API keys come from
//! the environment; everything is gated behind the `http-backends` cargo
//! feature so the core orchestration layer stays network-free.
//!
//! Enable with: `cargo build --features http-backends`
//!
//! ```bash
//! export NVAPI_KEY=...        # NVIDIA free API (build.nvidia.com)
//! export KILO_API_KEY=...     # Kilo-AI
//! # optional: KILO_BASE_URL / KILO_MODEL to override defaults
//! ```

use crate::engine::{
    BoxFuture, CompletionRequest, CompletionResponse, EngineError, ModelBackend,
};
use crate::engine::MockBackend;

// ===================================================================
// HTTP backends — only compiled with `http-backends` feature
// ===================================================================

#[cfg(feature = "http-backends")]
mod http {
    use std::time::{Duration, Instant};

    use serde_json::{json, Value};
    use tracing::{error, info, warn};

    use crate::engine::{
        CompletionRequest, CompletionResponse, EngineError, TokenUsage,
    };

    /// Shared OpenAI-compatible chat-completions client.
    pub struct OpenAICompat {
        pub client: reqwest::Client,
        pub base_url: String,
        pub api_key: String,
        pub model: String,
        pub json_mode: bool,
        pub reasoning_effort: Option<String>,
    }

    impl OpenAICompat {
        pub fn new(
            base_url: impl Into<String>,
            api_key: impl Into<String>,
            model: impl Into<String>,
        ) -> Self {
            Self {
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(90))
                    .build()
                    .expect("failed to build HTTP client"),
                base_url: base_url.into(),
                api_key: api_key.into(),
                model: model.into(),
                json_mode: false,
                reasoning_effort: None,
            }
        }

        pub fn with_json_mode(mut self, on: bool) -> Self {
            self.json_mode = on;
            self
        }

        pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
            self.reasoning_effort = Some(effort.into());
            self
        }

        pub fn build_body(&self, req: &CompletionRequest) -> Value {
            let mut body = json!({
                "model": self.model,
                "messages": [
                    { "role": "system", "content": req.system },
                    { "role": "user", "content": req.prompt }
                ],
                "temperature": req.temperature,
                "max_tokens": req.max_tokens,
            });
            if self.json_mode {
                body["response_format"] = json!({ "type": "json_object" });
            }
            if let Some(effort) = &self.reasoning_effort {
                body["reasoning_effort"] = json!(effort);
            }
            body
        }

        pub async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, EngineError> {
            let body = self.build_body(req);
            let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
            info!(
                model = %self.model,
                json_mode = self.json_mode,
                url = %url,
                max_tokens = req.max_tokens,
                "http request"
            );
            let started = Instant::now();
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await;
            match resp {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        let text = resp.text().await.unwrap_or_default();
                        warn!(
                            model = %self.model,
                            status = %status,
                            ms = started.elapsed().as_millis(),
                            body = %text.chars().take(500).collect::<String>(),
                            "http error"
                        );
                        return Err(EngineError::Backend(format!("HTTP {status}: {text}")));
                    }
                    let value: Value = resp
                        .json()
                        .await
                        .map_err(|e| EngineError::Backend(format!("invalid JSON response: {e}")))?;
                    let text = value
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                        .ok_or_else(|| {
                            EngineError::Backend("missing choices[0].message.content".into())
                        })?
                        .to_string();
                    let usage = parse_usage(&value);
                    let parsed = serde_json::from_str(&text).ok();
                    info!(
                        model = %self.model,
                        status = %status,
                        ms = started.elapsed().as_millis(),
                        prompt_tokens = usage.prompt_tokens,
                        completion_tokens = usage.completion_tokens,
                        snippet = %text.chars().take(200).collect::<String>(),
                        "http ok"
                    );
                    Ok(CompletionResponse { text, usage, parsed })
                }
                Err(e) => {
                    error!(
                        model = %self.model,
                        url = %url,
                        ms = started.elapsed().as_millis(),
                        error = %e,
                        "request failed"
                    );
                    Err(EngineError::Backend(format!("request failed: {e}")))
                }
            }
        }
    }

    pub fn parse_usage(v: &Value) -> TokenUsage {
        let u = v.get("usage");
        TokenUsage {
            prompt_tokens: u.and_then(|x| x.get("prompt_tokens")).and_then(|x| x.as_u64()).unwrap_or(0) as usize,
            completion_tokens: u.and_then(|x| x.get("completion_tokens")).and_then(|x| x.as_u64()).unwrap_or(0) as usize,
        }
    }
}

// ===================================================================
// NVIDIA backend
// ===================================================================

#[cfg(feature = "http-backends")]
pub struct NvidiaBackend {
    inner: http::OpenAICompat,
}

#[cfg(feature = "http-backends")]
impl NvidiaBackend {
    pub const DEFAULT_BASE_URL: &'static str = "https://integrate.api.nvidia.com/v1";
    // minimax-m3 is the only reliably free model on build.nvidia.com as of
    // 2026-07-12. The others rotate in and out of free tier or have rate
    // limits that make them unreliable for agentic workflows.
    //
    // To discover currently available free models:
    //   curl -s 'https://integrate.api.nvidia.com/v1/models' | jq '.data[] | select(.id | test("free|community")) | .id'
    //
    // NVIDIA's free tier: https://build.nvidia.com/explore/discover
    pub const DEFAULT_MODEL: &'static str = "minimaxai/minimax-m3";
    pub const MODELS: &'static [&'static str] = &[
        "minimaxai/minimax-m3",         // ✅ reliable free, good for reasoning
        // The following are sometimes free but not guaranteed:
        "qwen/qwen3-next-80b-a3b-instruct",
        "nvidia/nemotron-3-super-120b-a12b",
        "z-ai/glm-5.2",
        // Check for newly added free models periodically:
        //   curl -s 'https://integrate.api.nvidia.com/v1/models' | jq '.data[].id'
    ];

    pub fn from_env() -> Result<Self, EngineError> {
        let key = std::env::var("NVIDIA_API_KEY")
            .or_else(|_| std::env::var("NVAPI_KEY"))
            .map_err(|_| EngineError::Backend("NVIDIA_API_KEY (or NVAPI_KEY) not set".into()))?;
        let model = std::env::var("NV_MODEL").unwrap_or_else(|_| Self::DEFAULT_MODEL.into());
        Ok(Self::new(key, Some(model)))
    }

    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let inner = http::OpenAICompat::new(
            Self::DEFAULT_BASE_URL,
            api_key,
            model.unwrap_or_else(|| Self::DEFAULT_MODEL.into()),
        ).with_json_mode(true);
        Self { inner }
    }
}

#[cfg(feature = "http-backends")]
impl ModelBackend for NvidiaBackend {
    fn name(&self) -> &str { "nvidia" }
    fn supports_constrained_decoding(&self) -> bool { self.inner.json_mode }
    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move { self.inner.complete(&req).await })
    }
}

// ===================================================================
// Kilo-AI backend
// ===================================================================

#[cfg(feature = "http-backends")]
pub struct KiloBackend {
    inner: http::OpenAICompat,
}

#[cfg(feature = "http-backends")]
impl KiloBackend {
    pub const DEFAULT_BASE_URL: &'static str = "https://api.kilo.ai/api/gateway";
    pub const DEFAULT_MODEL: &'static str = "tencent/hy3:free";

    pub fn from_env() -> Result<Self, EngineError> {
        let key = std::env::var("KILO_API_KEY")
            .map_err(|_| EngineError::Backend("KILO_API_KEY not set".into()))?;
        let base = std::env::var("KILO_BASE_URL").unwrap_or_else(|_| Self::DEFAULT_BASE_URL.into());
        let model = std::env::var("KILO_MODEL").unwrap_or_else(|_| Self::DEFAULT_MODEL.into());
        Ok(Self::new(base, key, model))
    }

    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            inner: http::OpenAICompat::new(base_url, api_key, model)
                .with_reasoning_effort("medium"),
        }
    }
}

#[cfg(feature = "http-backends")]
impl ModelBackend for KiloBackend {
    fn name(&self) -> &str { "kilo-ai" }
    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move { self.inner.complete(&req).await })
    }
}

// ===================================================================
// Local RWKV placeholder (used when `local-rwkv` feature is NOT enabled)
// ===================================================================

/// Placeholder backend for local RWKV/SSM inference via `web-rwkv`.
/// Returns a clear error until the real backend is wired in.
pub struct LocalRwkvBackend {
    pub model_size: String,
    pub exec_mode: String,
}

impl LocalRwkvBackend {
    pub fn new(model_size: impl Into<String>, exec_mode: impl Into<String>) -> Self {
        Self {
            model_size: model_size.into(),
            exec_mode: exec_mode.into(),
        }
    }
}

impl ModelBackend for LocalRwkvBackend {
    fn name(&self) -> &str { "local-rwkv" }
    fn complete(&self, _req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            Err(EngineError::Backend(
                "local RWKV backend not yet implemented — use Mock, Nvidia, or Kilo. \
                 See the web-rwkv repo (https://github.com/cryscan/web-rwkv) \
                 for the planned integration."
                    .into(),
            ))
        })
    }
}

// ===================================================================
// AnyBackend — dispatches to the configured concrete backend
// ===================================================================

pub enum AnyBackend {
    Mock(MockBackend),
    #[cfg(feature = "http-backends")]
    Nvidia(NvidiaBackend),
    #[cfg(feature = "http-backends")]
    Kilo(KiloBackend),
    LocalRwkv(LocalRwkvBackend),
    #[cfg(feature = "local-rwkv")]
    RwkvBackend(crate::rwkv_backend::RwkvBackend),
}

impl ModelBackend for AnyBackend {
    fn name(&self) -> &str {
        match self {
            AnyBackend::Mock(b) => b.name(),
            #[cfg(feature = "http-backends")]
            AnyBackend::Nvidia(b) => b.name(),
            #[cfg(feature = "http-backends")]
            AnyBackend::Kilo(b) => b.name(),
            AnyBackend::LocalRwkv(b) => b.name(),
            #[cfg(feature = "local-rwkv")]
            AnyBackend::RwkvBackend(b) => b.name(),
        }
    }
    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            match self {
                AnyBackend::Mock(b) => b.complete(req).await,
                #[cfg(feature = "http-backends")]
                AnyBackend::Nvidia(b) => b.complete(req).await,
                #[cfg(feature = "http-backends")]
                AnyBackend::Kilo(b) => b.complete(req).await,
                AnyBackend::LocalRwkv(b) => b.complete(req).await,
                #[cfg(feature = "local-rwkv")]
                AnyBackend::RwkvBackend(b) => b.complete(req).await,
            }
        })
    }
}

#[cfg(all(test, feature = "http-backends"))]
mod tests {
    use super::*;

    #[test]
    fn nvidia_backend_constructs_and_names() {
        let b = NvidiaBackend::new("dummy-key", None);
        assert_eq!(b.name(), "nvidia");
        assert!(b.supports_constrained_decoding());
    }

    #[test]
    fn nvidia_curated_models_present() {
        // minimax-m3 must always be in the list (it's the reliable one)
        assert!(NvidiaBackend::MODELS.contains(&"minimaxai/minimax-m3"));
        // DEFAULT_MODEL should match the most reliable free model
        assert_eq!(NvidiaBackend::DEFAULT_MODEL, "minimaxai/minimax-m3");
    }

    #[test]
    fn kilo_backend_constructs_and_names() {
        let b = KiloBackend::new("https://api.kilo.ai/api/gateway", "dummy-key", "kilo");
        assert_eq!(b.name(), "kilo-ai");
    }

    #[test]
    fn kilo_request_body_has_model_and_reasoning_effort() {
        let b = KiloBackend::new("https://api.kilo.ai/api/gateway", "k", "tencent/hy3:free");
        let req = CompletionRequest {
            system: "sys".into(),
            prompt: "hi".into(),
            output_schema: None,
            grammar: None,
            temperature: 0.2,
            max_tokens: 512,
            estimated_prompt_tokens: 4,
        };
        let body = b.inner.build_body(&req);
        assert_eq!(body["model"], "tencent/hy3:free");
        assert_eq!(body["reasoning_effort"], "medium");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hi");
    }
}
