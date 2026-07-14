/// Terminal UI application (stub — to be implemented with ratatui).
pub struct TuiApp;

impl TuiApp {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) -> Result<(), String> {
        Ok(())
    }
}

impl Default for TuiApp {
    fn default() -> Self {
        Self::new()
    }
}
