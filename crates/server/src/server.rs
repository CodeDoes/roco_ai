use std::sync::Arc;
use roco_engine::ModelBackend;
use crate::config::ServerConfig;
use crate::routes::create_router;
use tracing::info;

pub struct Server {
    pub config: ServerConfig,
    pub backend: Arc<dyn ModelBackend>,
}

impl Server {
    pub fn new(config: ServerConfig, backend: Arc<dyn ModelBackend>) -> Self {
        Self { config, backend }
    }

    pub async fn run(&self) -> Result<(), String> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        info!("Starting HTTP server on {}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await
            .map_err(|e| format!("Failed to bind to {addr}: {e}"))?;
        let app = create_router(self.backend.clone());
        axum::serve(listener, app).await
            .map_err(|e| format!("Server run error: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;

    #[tokio::test]
    async fn test_server_routes() {
        let backend = Arc::new(MockBackend::new("mock-backend", 0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = create_router(backend);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{port}");

        // 1. Health check
        let resp = client.get(format!("{base_url}/health")).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let health: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(health["status"], "ok");

        // 2. Direct completion
        let comp_req = roco_engine::CompletionRequest {
            system: "sys".to_string(),
            prompt: "Say yes".to_string(),
            ..Default::default()
        };
        let resp = client.post(format!("{base_url}/complete"))
            .json(&comp_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let comp_resp: roco_engine::CompletionResponse = resp.json().await.unwrap();
        assert!(comp_resp.text.contains("Say yes"));

        // 3. OpenAI-style completion
        let openai_req = serde_json::json!({
            "prompt": "OpenAI style prompt",
            "max_tokens": 10
        });
        let resp = client.post(format!("{base_url}/v1/completions"))
            .json(&openai_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let openai_resp: serde_json::Value = resp.json().await.unwrap();
        assert!(openai_resp["choices"][0]["text"].as_str().unwrap().contains("OpenAI style prompt"));
    }
}
