/// HTTP server (stub — to be implemented with axum).
pub struct Server;

impl Server {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(&self) -> Result<(), String> {
        Ok(())
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
