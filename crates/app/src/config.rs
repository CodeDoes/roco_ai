//! RoCo configuration — model path, server settings, and general options.
//!
//! Config file search order (first found wins):
//!   1. `$ROCO_CONFIG` — explicit config file path
//!   2. `.roco/config.toml` in current directory
//!   3. `~/.config/roco/config.toml`
//!   4. `~/.roco/config.toml`
//!
//! Environment variables always beat config file values:
//!   - `RWKV_MODEL` overrides `model.path`
//!   - `RWKV_VOCAB` overrides `model.vocab`
//!
//! If no config file is found and no env var is set, the model is auto-detected
//! via [`roco_inference::default_model_path`] (scans `models/` directory).

use std::path::PathBuf;

use serde::Deserialize;

// ═════════════════════════════════════════════════════════════════════════════
// Types
// ═════════════════════════════════════════════════════════════════════════════

/// Top-level configuration.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
#[derive(Default)]
pub struct RoCoConfig {
    pub hotreload: bool,
    pub model: ModelConfig,
    pub server: ServerConfig,
    pub gateway: GatewayConfig,
    pub template: TemplateConfig,
}

/// Message-exchange template settings.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TemplateConfig {
    pub think: bool,
    pub r#type: String,
    pub state_tune: bool,
    pub system_prompt: bool,
    pub context_state: bool,
    pub max_tokens: String,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            think: false,
            r#type: "prose".into(),
            state_tune: false,
            system_prompt: true,
            context_state: false,
            max_tokens: "inf".into(),
        }
    }
}

/// Model / inference settings.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
#[derive(Default)]
pub struct ModelConfig {
    /// Path to the RWKV-7 `.st` model file.
    pub path: Option<String>,
    /// Path to the tokenizer vocab JSON file.
    pub vocab: Option<String>,
}

/// Inference server settings (used by `roco server`).
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub hotreload: bool,
}

/// API gateway settings (used by `roco gateway`).
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub rate_limit: usize,
    pub hotreload: bool,
}

// ═════════════════════════════════════════════════════════════════════════════
// Defaults
// ═════════════════════════════════════════════════════════════════════════════

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 18080,
            hotreload: false,
        }
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 18000,
            rate_limit: 60,
            hotreload: false,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Loading
// ═════════════════════════════════════════════════════════════════════════════

impl RoCoConfig {
    /// Check whether hot-reload mode is enabled.
    /// Priority: `ROCO_HOTRELOAD` env var > config file settings (`hotreload = true`).
    pub fn is_hotreload_enabled(&self) -> bool {
        if let Ok(val) = std::env::var("ROCO_HOTRELOAD") {
            let val = val.trim().to_lowercase();
            if val == "1" || val == "true" || val == "yes" || val == "on" {
                return true;
            }
            if val == "0" || val == "false" || val == "no" || val == "off" {
                return false;
            }
        }
        self.hotreload || self.server.hotreload || self.gateway.hotreload
    }

    /// Load config from the first available location.
    ///
    /// Returns `Default::default()` (all `None`s / safe defaults) when no
    /// config file exists — so the model auto-detection in
    /// [`roco_inference::default_model_path`] still works as a fallback.
    pub fn load() -> Self {
        let search_paths = Self::search_paths();
        for path in &search_paths {
            if path.exists() {
                let content = match std::fs::read_to_string(path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skipping unreadable config");
                        continue;
                    }
                };
                match toml::from_str(&content) {
                    Ok(cfg) => {
                        tracing::info!(path = %path.display(), "loaded config");
                        return cfg;
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "malformed config, falling back to defaults");
                        return Self::default();
                    }
                }
            }
        }
        tracing::debug!("no config file found, using defaults + env vars");
        Self::default()
    }

    /// Search paths for config files, in priority order.
    fn search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // 1. $ROCO_CONFIG — explicit path
        if let Ok(p) = std::env::var("ROCO_CONFIG") {
            paths.push(PathBuf::from(p));
        }

        // 2. .roco/config.toml in current directory
        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.join(".roco").join("config.toml"));
        }

        // 3. ~/.config/roco/config.toml (XDG)
        if let Ok(home) = std::env::var("HOME") {
            paths.push(
                PathBuf::from(home)
                    .join(".config")
                    .join("roco")
                    .join("config.toml"),
            );
        }

        // 4. ~/.roco/config.toml (legacy-style dotfile)
        if let Ok(home) = std::env::var("HOME") {
            paths.push(PathBuf::from(home).join(".roco").join("config.toml"));
        }

        paths
    }

    /// Apply model config to the environment so that `RwkvBackend::from_env()`
    /// (and any other code reading `RWKV_MODEL` / `RWKV_VOCAB`) picks it up.
    ///
    /// Environment variables already set take priority (user env > config file).
    pub fn apply_to_environment(&self) {
        if let Some(ref path) = self.model.path {
            if std::env::var("RWKV_MODEL").is_err() {
                tracing::debug!(model_path = %path, "setting RWKV_MODEL from config");
                std::env::set_var("RWKV_MODEL", path);
            }
        }
        if let Some(ref vocab) = self.model.vocab {
            if std::env::var("RWKV_VOCAB").is_err() {
                tracing::debug!(vocab_path = %vocab, "setting RWKV_VOCAB from config");
                std::env::set_var("RWKV_VOCAB", vocab);
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_config_defaults() {
        let cfg = RoCoConfig::default();
        assert!(cfg.model.path.is_none());
        assert!(cfg.model.vocab.is_none());
        assert_eq!(cfg.server.host, "127.0.0.1");
        assert_eq!(cfg.server.port, 18080);
        assert_eq!(cfg.gateway.host, "127.0.0.1");
        assert_eq!(cfg.gateway.port, 18000);
        assert_eq!(cfg.gateway.rate_limit, 60);
    }

    #[test]
    fn test_config_load_from_toml() {
        let toml_str = r#"
            [model]
            path = "/tmp/test_model.st"
            vocab = "/tmp/test_vocab.json"

            [server]
            host = "0.0.0.0"
            port = 9090

            [gateway]
            host = "0.0.0.0"
            port = 9091
            rate_limit = 100
        "#;

        let cfg: RoCoConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.model.path.unwrap(), "/tmp/test_model.st");
        assert_eq!(cfg.model.vocab.unwrap(), "/tmp/test_vocab.json");
        assert_eq!(cfg.server.host, "0.0.0.0");
        assert_eq!(cfg.server.port, 9090);
        assert_eq!(cfg.gateway.port, 9091);
        assert_eq!(cfg.gateway.rate_limit, 100);
    }

    #[test]
    fn test_config_partial_toml() {
        let toml_str = r#"
            [model]
            path = "/tmp/test.st"
        "#;

        let cfg: RoCoConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.model.path.unwrap(), "/tmp/test.st");
        // Falls back to defaults
        assert_eq!(cfg.server.port, 18080);
        assert_eq!(cfg.gateway.rate_limit, 60);
    }

    #[test]
    fn test_apply_to_environment() {
        // Save original env
        let orig_model = std::env::var("RWKV_MODEL").ok();
        let orig_vocab = std::env::var("RWKV_VOCAB").ok();

        std::env::remove_var("RWKV_MODEL");
        std::env::remove_var("RWKV_VOCAB");

        let mut cfg = RoCoConfig::default();
        cfg.model.path = Some("/env/test.st".into());
        cfg.model.vocab = Some("/env/vocab.json".into());
        cfg.apply_to_environment();

        assert_eq!(std::env::var("RWKV_MODEL").unwrap(), "/env/test.st");
        assert_eq!(std::env::var("RWKV_VOCAB").unwrap(), "/env/vocab.json");

        // Restore
        if let Some(v) = orig_model {
            std::env::set_var("RWKV_MODEL", v);
        }
        if let Some(v) = orig_vocab {
            std::env::set_var("RWKV_VOCAB", v);
        }
    }

    #[test]
    fn test_search_paths_order() {
        let paths = RoCoConfig::search_paths();
        // First should be $ROCO_CONFIG if set, but we can check structure
        assert!(paths.len() >= 3); // .roco/config.toml + ~/.config/roco + ~/.roco
        assert!(paths
            .iter()
            .any(|p| p.to_string_lossy().contains(".roco/config.toml")));
    }

    #[test]
    fn test_config_file_roundtrip() {
        let dir = std::env::temp_dir().join("roco_config_test");
        let _ = fs::create_dir_all(&dir);
        let config_path = dir.join("config.toml");

        let content = r#"
            [model]
            path = "/roundtrip/model.st"

            [server]
            port = 7070
        "#;
        fs::write(&config_path, content).unwrap();

        // Temporarily set ROCO_CONFIG to this path
        std::env::set_var("ROCO_CONFIG", config_path.to_string_lossy().as_ref());
        let cfg = RoCoConfig::load();
        assert_eq!(cfg.model.path.unwrap(), "/roundtrip/model.st");
        assert_eq!(cfg.server.port, 7070);

        fs::remove_dir_all(&dir).ok();
        std::env::remove_var("ROCO_CONFIG");
    }

    #[test]
    fn test_template_config_defaults() {
        let cfg = RoCoConfig::default();
        assert!(!cfg.template.think);
        assert_eq!(cfg.template.r#type, "prose");
        assert!(!cfg.template.state_tune);
        assert!(cfg.template.system_prompt);
        assert!(!cfg.template.context_state);
        assert_eq!(cfg.template.max_tokens, "inf");
    }

    #[test]
    fn test_template_config_from_toml() {
        let toml_str = r#"
            [template]
            think = true
            type = "xml"
            state_tune = true
            system_prompt = false
            context_state = true
            max_tokens = "inf"
        "#;

        let cfg: RoCoConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.template.think);
        assert_eq!(cfg.template.r#type, "xml");
        assert!(cfg.template.state_tune);
        assert!(!cfg.template.system_prompt);
        assert!(cfg.template.context_state);
        assert_eq!(cfg.template.max_tokens, "inf");
    }

    #[test]
    fn test_hotreload_config() {
        let mut cfg = RoCoConfig::default();
        assert!(!cfg.is_hotreload_enabled());

        cfg.hotreload = true;
        assert!(cfg.is_hotreload_enabled());

        let mut cfg2 = RoCoConfig::default();
        cfg2.server.hotreload = true;
        assert!(cfg2.is_hotreload_enabled());

        let content = r#"
            hotreload = true
            [server]
            port = 18080
        "#;
        let parsed: RoCoConfig = toml::from_str(content).unwrap();
        assert!(parsed.is_hotreload_enabled());
    }
}
