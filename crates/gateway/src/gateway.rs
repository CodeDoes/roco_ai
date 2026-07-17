use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use axum::{
    extract::{State, Request},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router as AxumRouter,
};
use reqwest::Client;
use tracing::{info, warn};

#[derive(Clone)]
pub struct GatewayState {
    pub target_url: String,
    pub rate_limit_per_minute: usize,
    pub req_client: Client,
    pub rate_limiter: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

pub struct Gateway {
    pub host: String,
    pub port: u16,
    pub target_url: String,
    pub rate_limit_per_minute: usize,
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new("127.0.0.1".to_string(), 8000, "http://127.0.0.1:8080".to_string(), 60)
    }
}

impl Gateway {
    pub fn new(host: String, port: u16, target_url: String, rate_limit_per_minute: usize) -> Self {
        Self {
            host,
            port,
            target_url,
            rate_limit_per_minute,
        }
    }

    pub async fn run(&self) -> Result<(), String> {
        let state = GatewayState {
            target_url: self.target_url.clone(),
            rate_limit_per_minute: self.rate_limit_per_minute,
            req_client: Client::new(),
            rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        };

        let app = AxumRouter::new()
            .route("/health", get(handle_health))
            .route("/complete", post(handle_proxy))
            .route("/v1/completions", post(handle_proxy))
            .with_state(state);

        let addr = format!("{}:{}", self.host, self.port);
        info!("Starting API Gateway on {}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await
            .map_err(|e| format!("Failed to bind gateway to {addr}: {e}"))?;
        axum::serve(listener, app).await
            .map_err(|e| format!("Gateway run error: {e}"))?;
        Ok(())
    }
}

async fn handle_proxy(
    State(state): State<GatewayState>,
    req: Request,
) -> Response {
    let client_ip = "global".to_string();

    // Check rate limit
    {
        let mut limiter = state.rate_limiter.lock();
        let now = Instant::now();
        let timestamps = limiter.entry(client_ip.clone()).or_insert_with(Vec::new);

        // Retain only timestamps from the last minute
        timestamps.retain(|&t| now.duration_since(t) < Duration::from_secs(60));

        if timestamps.len() >= state.rate_limit_per_minute {
            warn!("Rate limit exceeded for client {}", client_ip);
            return (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "Too Many Requests - Rate limit exceeded"
                }))
            ).into_response();
        }

        timestamps.push(now);
    }

    let path = req.uri().path().to_string();
    let forward_url = format!("{}{}", state.target_url.trim_end_matches('/'), path);
    info!("Proxying request to {}", forward_url);

    // Get body bytes from request
    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Failed to read request body: {e}") }))
            ).into_response();
        }
    };

    // Forward using reqwest
    let upstream_res = match state.req_client.post(&forward_url)
        .header("Content-Type", "application/json")
        .body(body_bytes)
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("Failed to forward request to backend: {e}") }))
            ).into_response();
        }
    };

    let status = upstream_res.status();
    let res_bytes = match upstream_res.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to read backend response: {e}") }))
            ).into_response();
        }
    };

    Response::builder()
        .status(status.as_u16())
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(res_bytes))
        .unwrap_or_else(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR).into_response())
}

async fn handle_health(State(state): State<GatewayState>) -> impl IntoResponse {
    let forward_url = format!("{}/health", state.target_url.trim_end_matches('/'));
    let backend_status = match state.req_client.get(&forward_url).send().await {
        Ok(res) => {
            if res.status().is_success() {
                "online".to_string()
            } else {
                format!("offline (HTTP {})", res.status())
            }
        }
        Err(_) => "offline".to_string(),
    };

    Json(serde_json::json!({
        "gateway": "online",
        "backend_url": state.target_url,
        "backend_status": backend_status
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_proxy_and_rate_limiting() {
        // 1. Start a mock upstream server
        let mock_app = AxumRouter::new()
            .route("/health", get(|| async { Json(serde_json::json!({ "status": "ok" })) }))
            .route("/complete", post(|| async { Json(serde_json::json!({ "text": "mocked response" })) }));

        let mock_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_port = mock_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(mock_listener, mock_app).await.unwrap();
        });

        // 2. Start the Gateway pointing to mock upstream, limit = 2 per min
        let gw_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_listener.local_addr().unwrap().port();
        let gateway = Gateway::new(
            "127.0.0.1".to_string(),
            gw_port,
            format!("http://127.0.0.1:{mock_port}"),
            2,
        );

        let app_state = GatewayState {
            target_url: gateway.target_url.clone(),
            rate_limit_per_minute: gateway.rate_limit_per_minute,
            req_client: Client::new(),
            rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        };

        let gw_app = AxumRouter::new()
            .route("/health", get(handle_health))
            .route("/complete", post(handle_proxy))
            .with_state(app_state);

        tokio::spawn(async move {
            axum::serve(gw_listener, gw_app).await.unwrap();
        });

        let client = Client::new();
        let gw_url = format!("http://127.0.0.1:{gw_port}");

        // 3. Health Check proxying
        let res = client.get(format!("{gw_url}/health")).send().await.unwrap();
        assert_eq!(res.status(), 200);
        let health_data: serde_json::Value = res.json().await.unwrap();
        assert_eq!(health_data["gateway"], "online");
        assert_eq!(health_data["backend_status"], "online");

        // 4. Request 1: should pass
        let res1 = client.post(format!("{gw_url}/complete"))
            .json(&serde_json::json!({ "prompt": "hello" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res1.status(), 200);
        let data1: serde_json::Value = res1.json().await.unwrap();
        assert_eq!(data1["text"], "mocked response");

        // 5. Request 2: should pass
        let res2 = client.post(format!("{gw_url}/complete"))
            .json(&serde_json::json!({ "prompt": "world" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res2.status(), 200);

        // 6. Request 3: should be rate-limited (HTTP 429)
        let res3 = client.post(format!("{gw_url}/complete"))
            .json(&serde_json::json!({ "prompt": "too many requests" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res3.status(), 429);
        let err_data: serde_json::Value = res3.json().await.unwrap();
        assert!(err_data["error"].as_str().unwrap().contains("Rate limit exceeded"));
    }
}
