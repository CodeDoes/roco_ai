/// How to format output for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Plain,
    Rich,
    Json,
}

/// Display settings for the frontend.
#[derive(Debug, Clone)]
pub struct DisplaySettings {
    pub format: OutputFormat,
    pub show_tokens: bool,
    pub show_latency: bool,
    pub max_width: usize,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            format: OutputFormat::Plain,
            show_tokens: false,
            show_latency: false,
            max_width: 80,
        }
    }
}
