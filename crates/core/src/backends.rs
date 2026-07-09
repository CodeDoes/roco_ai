//! HTTP model backends (OpenAI-compatible chat completions).
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

use std::env;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tracing::{error, info, warn};

use crate::engine::{
    BoxFuture, CompletionRequest, CompletionResponse, EngineError, ModelBackend, TokenUsage,
};
use crate::engine::MockBackend;

/// Shared OpenAI-compatible chat-completions client.
pub struct OpenAICompat {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    /// When true, request `response_format: { "type": "json_object" }` so the
    /// model emits JSON (closest thing to constrained decoding over HTTP).
    json_mode: bool,
    /// Optional reasoning effort (e.g. "low"|"medium"|"high"), forwarded as the
    /// OpenAI-compatible `reasoning_effort` field. Undocumented by Kilo but passed
    /// through to reasoning models (e.g. `tencent/hy3`).
    reasoning_effort: Option<String>,
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

    /// Build the OpenAI-compatible request body from a `CompletionRequest`.
    /// Exposed for testing the exact wire format (model, reasoning effort, etc.).
    fn build_body(&self, req: &CompletionRequest) -> Value {
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

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, EngineError> {
        let body = self.build_body(req);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        info!(
            model = %self.model,
            json_mode = self.json_mode,
            url = %url,
            max_tokens = req.max_tokens,
            "nvidia http request"
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
                        "nvidia http error"
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
                    "nvidia http ok"
                );
                Ok(CompletionResponse {
                    text,
                    usage,
                    parsed,
                })
            }
            Err(e) => {
                error!(
                    model = %self.model,
                    url = %url,
                    ms = started.elapsed().as_millis(),
                    error = %e,
                    "nvidia request failed"
                );
                Err(EngineError::Backend(format!("request failed: {e}")))
            }
        }
    }
}

fn parse_usage(v: &Value) -> TokenUsage {
    let u = v.get("usage");
    TokenUsage {
        prompt_tokens: u
            .and_then(|x| x.get("prompt_tokens"))
            .and_then(|x| x.as_u64())
            .unwrap_or(0) as usize,
        completion_tokens: u
            .and_then(|x| x.get("completion_tokens"))
            .and_then(|x| x.as_u64())
            .unwrap_or(0) as usize,
    }
}

// ---------------------------------------------------------------------------
// NVIDIA — free developer API (build.nvidia.com)
// ---------------------------------------------------------------------------

/// NVIDIA's free OpenAI-compatible API. Key from <https://build.nvidia.com>
/// (env: `NVIDIA_API_KEY`, falls back to `NVAPI_KEY`).
///
/// Default model is a capable free instruct model; override via `model`.
pub struct NvidiaBackend {
    inner: OpenAICompat,
}

impl NvidiaBackend {
    pub const DEFAULT_BASE_URL: &'static str = "https://integrate.api.nvidia.com/v1";
    /// Default model. Override via `NV_MODEL` with any slug from [`NvidiaBackend::MODELS`].
    /// `qwen/qwen3-next-80b-a3b-instruct` and `z-ai/glm-5.2` currently time out on the
    /// free NVIDIA tier, so the default is the responsive `nemotron` model.
    pub const DEFAULT_MODEL: &'static str = "nvidia/nemotron-3-super-120b-a12b";

    /// Curated NVIDIA-hosted models (provider/model slugs) available via the
    /// free NVIDIA API. Select one via `NV_MODEL`, or pass it to `new()`.
    pub const MODELS: &'static [&'static str] = &[
        "qwen/qwen3-next-80b-a3b-instruct",
        "nvidia/nemotron-3-super-120b-a12b",
        "z-ai/glm-5.2",
        "minimaxai/minimax-m3",
    ];

    /// Build from `NVIDIA_API_KEY` (or `NVAPI_KEY`), plus optional `NV_MODEL`.
    pub fn from_env() -> Result<Self, EngineError> {
        let key = env::var("NVIDIA_API_KEY")
            .or_else(|_| env::var("NVAPI_KEY"))
            .map_err(|_| {
                EngineError::Backend("NVIDIA_API_KEY (or NVAPI_KEY) not set".into())
            })?;
        let model = env::var("NV_MODEL").unwrap_or_else(|_| Self::DEFAULT_MODEL.into());
        Ok(Self::new(key, Some(model)))
    }

    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let inner = OpenAICompat::new(
            Self::DEFAULT_BASE_URL,
            api_key,
            model.unwrap_or_else(|| Self::DEFAULT_MODEL.into()),
        )
        // Nemotron supports JSON mode — helpful for schema-shaped output.
        .with_json_mode(true);
        Self { inner }
    }
}

impl ModelBackend for NvidiaBackend {
    fn name(&self) -> &str {
        "nvidia"
    }
    fn supports_constrained_decoding(&self) -> bool {
        self.inner.json_mode
    }
    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            self.inner.complete(&req).await
        })
    }
}

// ---------------------------------------------------------------------------
// Kilo-AI — OpenAI-compatible endpoint
// ---------------------------------------------------------------------------

/// Kilo-AI OpenAI-compatible backend.
///
/// Per <https://kilo.ai/docs/gateway>, the gateway is an OpenAI-compatible
/// `/chat/completions` endpoint at `https://api.kilo.ai/api/gateway`, keyed by
/// `KILO_API_KEY`. Model names are provider-prefixed slugs (e.g.
/// `anthropic/claude-sonnet-4.5`, `tencent/hy3:free`); set `KILO_MODEL` to the
/// model you want. The default is `tencent/hy3:free` with `medium` reasoning
/// effort (forwarded via the OpenAI-compatible `reasoning_effort` field).
pub struct KiloBackend {
    inner: OpenAICompat,
}

impl KiloBackend {
    /// Confirmed gateway base URL (kilo.ai/docs/gateway).
    pub const DEFAULT_BASE_URL: &'static str = "https://api.kilo.ai/api/gateway";
    /// Documented example model slug; override via `KILO_MODEL`.
    pub const DEFAULT_MODEL: &'static str = "tencent/hy3:free";

    /// Build from `KILO_API_KEY`, plus optional `KILO_BASE_URL` / `KILO_MODEL`.
    /// Defaults to `tencent/hy3:free` with `medium` reasoning effort.
    pub fn from_env() -> Result<Self, EngineError> {
        let key = env::var("KILO_API_KEY")
            .map_err(|_| EngineError::Backend("KILO_API_KEY not set".into()))?;
        let base = env::var("KILO_BASE_URL").unwrap_or_else(|_| Self::DEFAULT_BASE_URL.into());
        let model = env::var("KILO_MODEL").unwrap_or_else(|_| Self::DEFAULT_MODEL.into());
        Ok(Self::new(base, key, model))
    }

    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            inner: OpenAICompat::new(base_url, api_key, model)
                .with_reasoning_effort("medium"),
        }
    }
}

impl ModelBackend for KiloBackend {
    fn name(&self) -> &str {
        "kilo-ai"
    }
    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            self.inner.complete(&req).await
        })
    }
}

/// A provider-agnostic backend that dispatches to the configured concrete
/// backend. Lets the `Orchestrator` be built from a `Config` selection without
/// needing `dyn ModelBackend` (native async traits are not dyn-compatible).
pub enum AnyBackend {
    Mock(MockBackend),
    Nvidia(NvidiaBackend),
    Kilo(KiloBackend),
}

impl ModelBackend for AnyBackend {
    fn name(&self) -> &str {
        match self {
            AnyBackend::Mock(b) => b.name(),
            AnyBackend::Nvidia(b) => b.name(),
            AnyBackend::Kilo(b) => b.name(),
        }
    }
    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            match self {
                AnyBackend::Mock(b) => b.complete(req).await,
                AnyBackend::Nvidia(b) => b.complete(req).await,
                AnyBackend::Kilo(b) => b.complete(req).await,
            }
        })
    }
}

#[cfg(test)]
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
        assert!(NvidiaBackend::MODELS.contains(&"qwen/qwen3-next-80b-a3b-instruct"));
        assert!(NvidiaBackend::MODELS.contains(&"nvidia/nemotron-3-super-120b-a12b"));
        assert!(NvidiaBackend::MODELS.contains(&"z-ai/glm-5.2"));
        assert!(NvidiaBackend::MODELS.contains(&"minimaxai/minimax-m3"));
        assert!(NvidiaBackend::MODELS.contains(&NvidiaBackend::DEFAULT_MODEL));
    }

    #[test]
    fn kilo_backend_constructs_and_names() {
        let b = KiloBackend::new("https://api.kilo.ai/api/gateway", "dummy-key", "kilo");
        assert_eq!(b.name(), "kilo-ai");
    }

    #[test]
    fn kilo_request_body_has_model_and_reasoning_effort() {
        // Verify the exact wire format without hitting the network.
        let b = KiloBackend::new("https://api.kilo.ai/api/gateway", "k", "tencent/hy3:free");
        let req = CompletionRequest {
            system: "sys".into(),
            prompt: "hi".into(),
            output_schema: None,
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
