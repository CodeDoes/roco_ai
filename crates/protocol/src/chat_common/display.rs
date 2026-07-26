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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_settings_default() {
        let settings = DisplaySettings::default();
        assert_eq!(settings.format, OutputFormat::Plain);
        assert!(!settings.show_tokens);
        assert!(!settings.show_latency);
        assert_eq!(settings.max_width, 80);
    }

    #[test]
    fn test_display_settings_custom() {
        let settings = DisplaySettings {
            format: OutputFormat::Rich,
            show_tokens: true,
            show_latency: true,
            max_width: 120,
        };
        assert_eq!(settings.format, OutputFormat::Rich);
        assert!(settings.show_tokens);
        assert!(settings.show_latency);
        assert_eq!(settings.max_width, 120);
    }

    #[test]
    fn test_output_format_equality() {
        assert_eq!(OutputFormat::Plain, OutputFormat::Plain);
        assert_eq!(OutputFormat::Rich, OutputFormat::Rich);
        assert_eq!(OutputFormat::Json, OutputFormat::Json);
        assert_ne!(OutputFormat::Plain, OutputFormat::Rich);
        assert_ne!(OutputFormat::Json, OutputFormat::Plain);
    }

    #[test]
    fn test_output_format_debug() {
        assert_eq!(format!("{:?}", OutputFormat::Plain), "Plain");
        assert_eq!(format!("{:?}", OutputFormat::Rich), "Rich");
        assert_eq!(format!("{:?}", OutputFormat::Json), "Json");
    }

    #[test]
    fn test_display_settings_clone() {
        let a = DisplaySettings {
            format: OutputFormat::Json,
            show_tokens: true,
            show_latency: false,
            max_width: 100,
        };
        let b = a.clone();
        assert_eq!(a.format, b.format);
        assert_eq!(a.show_tokens, b.show_tokens);
        assert_eq!(a.show_latency, b.show_latency);
        assert_eq!(a.max_width, b.max_width);
    }

    #[test]
    fn test_display_settings_debug() {
        let settings = DisplaySettings::default();
        let debug_str = format!("{:?}", settings);
        assert!(debug_str.contains("Plain"));
        assert!(debug_str.contains("80"));
    }
}
