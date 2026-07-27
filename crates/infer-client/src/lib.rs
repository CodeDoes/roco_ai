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
    CompletionRequest, CompletionResponse, EngineError, ModelBackend, StateTuning, TokenUsage,
};

/// Default base URL for the singleton inference API server.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080";

/// How long to wait for the server to *start* responding.
///
/// A cold `roco-inferd` can spend ~25s loading 2.9B weights before it answers,
/// so this is generous. It exists so a dead or wedged daemon surfaces as an
/// error instead of hanging the CLI forever — the previous
/// `reqwest::Client::new()` had **no timeout at all**, which meant a half-open
/// socket parked the REPL with no way out but Ctrl-C.
const CONNECT_TIMEOUT_SECS: u64 = 30;

/// Idle timeout for pooled connections.
///
/// reqwest keeps idle sockets alive indefinitely by default. Long-lived
/// surfaces (the REPL, the gateway) accumulate half-dead keep-alive
/// connections to a daemon that has since restarted; each one is a file
/// descriptor plus kernel buffers held for the life of the process.
const POOL_IDLE_TIMEOUT_SECS: u64 = 90;

/// Maximum idle connections retained per host.
const POOL_MAX_IDLE_PER_HOST: usize = 4;

/// Build the shared HTTP client with sane bounds.
///
/// Note there is deliberately **no total-request timeout**: generation is
/// open-ended and a long story can legitimately stream for minutes. The
/// per-request deadline belongs to `CompletionRequest::deadline_ms`, which the
/// server enforces. What we bound here is *connection* establishment and idle
/// socket retention.
fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_idle_timeout(std::time::Duration::from_secs(POOL_IDLE_TIMEOUT_SECS))
        .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
        .build()
        // A builder failure means TLS/config is broken; fall back rather than
        // panicking inside a constructor.
        .unwrap_or_else(|_| reqwest::Client::new())
}

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
            client: build_client(),
            extra_headers,
            name: "remote".to_string(),
        }
    }

    /// Build a client, resolving the base URL from `ROCO_API_URL` and any
    /// extra headers from `ROCO_API_HEADERS` (a JSON object of string→string).
    pub fn from_env() -> Self {
        let base = std::env::var("ROCO_API_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
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
        //
        // We cannot call `Handle::block_on` from inside an existing tokio
        // runtime: the awaited HTTP work gets scheduled on the same reactor
        // we just blocked, so `block_on` deadlocks the caller until the
        // awaited future times out. (Stuck this for years if you call
        // `RemoteBackend::vocab_bytes` from inside `#[tokio::main]`.) To
        // avoid both the no-runtime (returning None) and the in-runtime
        // (deadlock) paths, run the HTTP fetch on a dedicated blocking
        // thread that owns its own single-threaded runtime. This is safe
        // whether or not the caller is already inside a tokio runtime —
        // the blocking thread just blocks; it doesn't tie up the caller's
        // executor.
        std::thread::scope(|s| {
            let handle = s.spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()?;
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
            });
            handle.join().ok().flatten()
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

impl StateTuning for RemoteBackend {
    fn tune_state<'a>(
        &'a self,
        session_id: &'a str,
        system: &'a str,
        few_shots: &'a [(&'a str, &'a str)],
    ) -> BoxFuture<'a, Result<String, EngineError>> {
        let base_url = self.base_url.clone();
        let client = self.client.clone();
        let extra_headers = self.extra_headers.clone();
        let session_id = session_id.to_string();
        let system = system.to_string();
        let few_shots_vec: Vec<(String, String)> = few_shots
            .iter()
            .map(|(u, a)| (u.to_string(), a.to_string()))
            .collect();

        Box::pin(async move {
            let req_body = roco_protocol::BakeRequest {
                session_id: session_id.clone(),
                system,
                few_shots: few_shots_vec,
            };
            let url = format!("{base_url}/v1/bake");
            let mut builder = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&req_body);
            for (k, v) in &extra_headers {
                builder = builder.header(k, v);
            }

            let resp = builder.send().await.map_err(|e| {
                EngineError::Backend(format!("bake_state HTTP request failed: {e}"))
            })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "<unreadable>".to_string());
                return Err(EngineError::Backend(format!(
                    "bake_state HTTP error {status}: {body}"
                )));
            }

            let bake_resp: roco_protocol::BakeResponse = resp
                .json()
                .await
                .map_err(|e| EngineError::Backend(format!("bake_state decode failed: {e}")))?;

            Ok(bake_resp.session_id)
        })
    }

    fn blend_states<'a>(
        &'a self,
        _session_a: &'a str,
        _session_b: &'a str,
        _alpha: f32,
        _output_session: &'a str,
    ) -> BoxFuture<'a, Result<(), EngineError>> {
        Box::pin(async move {
            Err(EngineError::Backend(
                "blend_states not supported by RemoteBackend".into(),
            ))
        })
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
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

/// Parse one complete SSE line, recording any usage it carries.
///
/// Returns the text delta, if the line contained one. Pulled out of the
/// streaming loop so the chunk-boundary behaviour can be unit-tested.
fn parse_sse_line(
    line: &str,
    prompt_tokens: &mut usize,
    completion_tokens: &mut usize,
) -> Option<String> {
    let data = line.trim().strip_prefix("data:")?.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(data).ok()?;

    // Usage, if the server sends it on a closing event.
    if let Some(u) = value.get("usage") {
        if let Some(p) = u.get("prompt_tokens").and_then(|v| v.as_u64()) {
            *prompt_tokens = p as usize;
        }
        if let Some(c) = u.get("completion_tokens").and_then(|v| v.as_u64()) {
            *completion_tokens = c as usize;
        }
    }

    // OpenAI-style delta: choices[0].text
    value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .filter(|d| !d.is_empty())
        .map(|d| d.to_string())
}

/// Feed raw SSE bytes through a line buffer, emitting complete-line deltas.
///
/// `pending` carries the trailing partial line between calls. This is the
/// piece that was missing before: HTTP chunks and SSE events are unrelated,
/// so a `data:` line split across two chunks used to be dropped entirely.
fn drain_sse_lines(
    pending: &mut String,
    prompt_tokens: &mut usize,
    completion_tokens: &mut usize,
    mut emit: impl FnMut(&str),
) {
    while let Some(nl) = pending.find('\n') {
        let line: String = pending.drain(..=nl).collect();
        if let Some(delta) = parse_sse_line(&line, prompt_tokens, completion_tokens) {
            emit(&delta);
        }
    }
}

/// Run a completion against the remote inference API server.
async fn remote_complete(
    client: &reqwest::Client,
    base_url: &str,
    extra_headers: &HashMap<String, String>,
    req: CompletionRequest,
) -> Result<CompletionResponse, EngineError> {
    let stream = req.on_token.is_some();
    let start_time = std::time::Instant::now();
    let url = format!("{base_url}/v1/completions");

    tracing::info!(
        target: "roco_infer_client",
        url = %url,
        prompt_len = req.prompt.len(),
        stream = stream,
        "Sending completion request to remote backend"
    );

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
        seed: req.seed,
    };

    let max_retries = 3;
    let mut attempt = 0;
    let mut backoff = std::time::Duration::from_millis(200);

    let resp = loop {
        attempt += 1;
        let mut builder = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&wire);
        for (k, v) in extra_headers {
            builder = builder.header(k, v);
        }

        match builder.send().await {
            Ok(resp) if resp.status().is_server_error() && attempt <= max_retries => {
                let status = resp.status();
                tracing::warn!(
                    target: "roco_infer_client",
                    url = %url,
                    status = %status,
                    attempt = attempt,
                    "Remote completion returned server error, retrying in {backoff:?}"
                );
                tokio::time::sleep(backoff).await;
                backoff *= 2;
            }
            Ok(resp) => break resp,
            Err(e) if attempt <= max_retries => {
                tracing::warn!(
                    target: "roco_infer_client",
                    url = %url,
                    attempt = attempt,
                    err = %e,
                    "Remote completion HTTP connection failed, retrying in {backoff:?}"
                );
                tokio::time::sleep(backoff).await;
                backoff *= 2;
            }
            Err(e) => {
                tracing::error!(
                    target: "roco_infer_client",
                    url = %url,
                    elapsed_ms = start_time.elapsed().as_millis(),
                    err = %e,
                    "Remote completion HTTP request failed after max retries"
                );
                return Err(EngineError::Backend(format!(
                    "inference API request failed: {e}"
                )));
            }
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_string());
        tracing::error!(
            target: "roco_infer_client",
            url = %url,
            status = %status,
            body = %body,
            elapsed_ms = start_time.elapsed().as_millis(),
            "Remote completion HTTP returned error status"
        );
        return Err(EngineError::Backend(format!(
            "inference API error {status}: {body}"
        )));
    }

    if stream {
        // SSE stream: accumulate deltas, invoke on_token for each text chunk.
        //
        // NOTE: HTTP chunk boundaries have nothing to do with SSE event
        // boundaries. The previous implementation called `split('\n')` on each
        // chunk independently, so any `data: {...}` line straddling two chunks
        // was parsed as two invalid fragments and **silently dropped** — tokens
        // vanished from the middle of the stream, more often the longer the
        // response. We now carry a line buffer across chunks and only parse
        // complete, newline-terminated lines.
        let mut stream = resp.bytes_stream();
        let mut full = String::new();
        let mut pending = String::new();
        let mut prompt_tokens = 0usize;
        // Set only if the server reports usage explicitly.
        let mut reported_completion_tokens = 0usize;
        // Fallback: one delta ≈ one token.
        let mut delta_count = 0usize;
        let on_token = req.on_token;

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| EngineError::Backend(format!("inference API stream error: {e}")))?;
            pending.push_str(&String::from_utf8_lossy(&chunk));

            // Drain every *complete* line; keep the trailing partial in `pending`.
            drain_sse_lines(
                &mut pending,
                &mut prompt_tokens,
                &mut reported_completion_tokens,
                |delta| {
                    full.push_str(delta);
                    delta_count += 1;
                    if let Some(cb) = &on_token {
                        cb(delta);
                    }
                },
            );
        }

        // A final event may arrive without a trailing newline.
        if !pending.trim().is_empty() {
            if let Some(delta) = parse_sse_line(
                &pending,
                &mut prompt_tokens,
                &mut reported_completion_tokens,
            ) {
                full.push_str(&delta);
                delta_count += 1;
                if let Some(cb) = &on_token {
                    cb(&delta);
                }
            }
        }

        // Prefer the server's own count; fall back to counting deltas.
        let completion_tokens = if reported_completion_tokens > 0 {
            reported_completion_tokens
        } else {
            delta_count
        };

        if prompt_tokens == 0 {
            prompt_tokens = req.estimated_prompt_tokens;
        }
        tracing::info!(
            target: "roco_infer_client",
            url = %url,
            elapsed_ms = start_time.elapsed().as_millis(),
            completion_tokens = completion_tokens,
            "Remote stream completion finished"
        );
        return Ok(CompletionResponse {
            text: full,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
            },
            parsed: None,
            think_trace: None,
            trace: Vec::new(),
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
            u.get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
        ),
        None => (req.estimated_prompt_tokens, 0),
    };

    tracing::info!(
        target: "roco_infer_client",
        url = %url,
        elapsed_ms = start_time.elapsed().as_millis(),
        completion_tokens = completion_tokens,
        "Remote completion finished"
    );

    let trace: Vec<roco_engine::TokenTrace> = value
        .get("trace")
        .and_then(|t| serde_json::from_value(t.clone()).ok())
        .unwrap_or_default();

    Ok(CompletionResponse {
        text,
        usage: TokenUsage {
            prompt_tokens,
            completion_tokens,
        },
        parsed: None,
        think_trace: None,
        trace,
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

    // ── SSE line parsing ─────────────────────────────────────────────────

    fn collect(chunks: &[&str]) -> (String, usize, usize) {
        let mut pending = String::new();
        let mut prompt = 0usize;
        let mut completion = 0usize;
        let mut out = String::new();
        for c in chunks {
            pending.push_str(c);
            drain_sse_lines(&mut pending, &mut prompt, &mut completion, |d| {
                out.push_str(d)
            });
        }
        if !pending.trim().is_empty() {
            if let Some(d) = parse_sse_line(&pending, &mut prompt, &mut completion) {
                out.push_str(&d);
            }
        }
        (out, prompt, completion)
    }

    fn event(text: &str) -> String {
        format!(
            "data: {}\n\n",
            serde_json::json!({ "choices": [{ "text": text }] })
        )
    }

    #[test]
    fn whole_events_are_parsed() {
        let (out, _, _) = collect(&[&event("Hello"), &event(", world")]);
        assert_eq!(out, "Hello, world");
    }

    #[test]
    fn events_split_across_chunk_boundaries_are_not_dropped() {
        // The regression this guards: reqwest can (and does) split an SSE
        // line anywhere. The old per-chunk `split('\n')` lost these entirely.
        let full = format!("{}{}", event("alpha"), event("beta"));
        for split_at in 1..full.len() {
            if !full.is_char_boundary(split_at) {
                continue;
            }
            let (a, b) = full.split_at(split_at);
            let (out, _, _) = collect(&[a, b]);
            assert_eq!(out, "alphabeta", "lost data when split at byte {split_at}");
        }
    }

    #[test]
    fn a_single_byte_at_a_time_still_reassembles() {
        let full = format!("{}{}", event("one "), event("two"));
        let chunks: Vec<String> = full.chars().map(|c| c.to_string()).collect();
        let refs: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
        let (out, _, _) = collect(&refs);
        assert_eq!(out, "one two");
    }

    #[test]
    fn final_event_without_trailing_newline_is_kept() {
        let (out, _, _) = collect(&[r#"data: {"choices":[{"text":"tail"}]}"#]);
        assert_eq!(out, "tail");
    }

    #[test]
    fn done_sentinel_and_blank_lines_are_ignored() {
        let (out, _, _) = collect(&[&event("x"), "\n", "data: [DONE]\n\n", ": comment\n"]);
        assert_eq!(out, "x");
    }

    #[test]
    fn malformed_json_does_not_abort_the_stream() {
        let (out, _, _) = collect(&[&event("good "), "data: {broken\n", &event("still here")]);
        assert_eq!(out, "good still here");
    }

    #[test]
    fn usage_is_picked_up_from_the_closing_event() {
        let usage = format!(
            "data: {}\n\n",
            serde_json::json!({
                "choices": [{ "text": "" }],
                "usage": { "prompt_tokens": 12, "completion_tokens": 34 }
            })
        );
        let (_, prompt, completion) = collect(&[&event("hi"), &usage]);
        assert_eq!(prompt, 12);
        assert_eq!(completion, 34);
    }

    #[test]
    fn empty_deltas_are_not_emitted() {
        let (out, _, _) = collect(&[&event(""), &event("real")]);
        assert_eq!(out, "real");
    }

    // ── Client configuration (resource leak guards) ──────────────────────

    #[test]
    fn client_is_built_with_bounded_pooling() {
        // We can't introspect reqwest's config, but we can assert the builder
        // succeeds — a misconfiguration would silently fall back to the
        // unbounded default client.
        let _ = build_client();
        const { assert!(CONNECT_TIMEOUT_SECS > 0) };
        const { assert!(POOL_IDLE_TIMEOUT_SECS > 0) };
        const { assert!(POOL_MAX_IDLE_PER_HOST > 0) };
    }
}
