//! Configuration for provider selection and orchestration.
//!
//! Drives the system from a JSON config file (see `Config::from_file`)
//! instead of hardcoded values. The default provider is **NVIDIA** (see `Config::preset`).

use std::path::Path;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::agent::{ContextBudget, RetryPolicy};

/// Which backend provider to use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Nvidia,
    Kilo,
    Mock,
    /// Local RWKV/SSM inference via `web-rwkv` (Phase 4). Returns an error until wired in.
    LocalRwkv,
}

fn d_max_task() -> usize {
    3
}
fn d_max_step() -> usize {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "d_max_task")]
    pub max_per_task: usize,
    #[serde(default = "d_max_step")]
    pub max_per_step: usize,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_per_task: 3,
            max_per_step: 2,
        }
    }
}

fn d_total() -> usize {
    4096
}
fn d_system() -> usize {
    700
}
fn d_task() -> usize {
    1200
}
fn d_tools() -> usize {
    800
}
fn d_scratch() -> usize {
    700
}
fn d_gen() -> usize {
    1536
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    #[serde(default = "d_total")]
    pub total: usize,
    #[serde(default = "d_system")]
    pub system: usize,
    #[serde(default = "d_task")]
    pub task_context: usize,
    #[serde(default = "d_tools")]
    pub tools: usize,
    #[serde(default = "d_scratch")]
    pub scratch: usize,
    #[serde(default = "d_gen")]
    pub generation: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            total: 4096,
            system: 700,
            task_context: 1200,
            tools: 800,
            scratch: 700,
            generation: 1536,
        }
    }
}

/// Top-level configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub nvidia_model: Option<String>,
    #[serde(default)]
    pub kilo_model: Option<String>,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub context: ContextConfig,
}

impl Config {
    /// NVIDIA preset: NVIDIA provider, the standard capacity pool, the 3B-friendly
    /// 4K context budget, and the curated default NVIDIA model.
    pub fn preset() -> Self {
        Config {
            provider: Provider::Nvidia,
            nvidia_model: Some("nvidia/nemotron-3-super-120b-a12b".into()),
            kilo_model: Some("tencent/hy3:free".into()),
            retry: RetryConfig::default(),
            context: ContextConfig::default(),
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path.as_ref()).with_context(|| {
            format!("reading config {}", path.as_ref().display())
        })?;
        let cfg: Config = serde_json::from_str(&text)
            .with_context(|| format!("parsing config {}", path.as_ref().display()))?;
        Ok(cfg)
    }

    /// Load from `path`, falling back to the NVIDIA preset if missing/invalid.
    pub fn load_or_preset(path: impl AsRef<Path>) -> Self {
        Self::from_file(&path).unwrap_or_else(|e| {
            tracing::warn!("config load failed ({}), using NVIDIA preset", e);
            Config::preset()
        })
    }

    pub fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy {
            max_per_task: self.retry.max_per_task,
            max_per_step: self.retry.max_per_step,
        }
    }

    pub fn context_budget(&self) -> ContextBudget {
        ContextBudget {
            total: self.context.total,
            system: self.context.system,
            task_context: self.context.task_context,
            tools: self.context.tools,
            scratch: self.context.scratch,
            generation: self.context.generation,
        }
    }

    pub fn resolved_nvidia_model(&self) -> String {
        self.nvidia_model
            .clone()
            .unwrap_or_else(|| "minimaxai/minimax-m3".into())
    }

    pub fn resolved_kilo_model(&self) -> String {
        self.kilo_model
            .clone()
            .unwrap_or_else(|| "tencent/hy3:free".into())
    }

    /// Build the configured backend. Requires the `http-backends` feature
    /// (except for `Mock` and `LocalRwkv` which return errors / placeholders).
    #[cfg(all(feature = "http-backends", not(feature = "local-rwkv")))]
    pub fn build_backend(&self) -> Result<crate::backends::AnyBackend> {
        use crate::backends::{AnyBackend, KiloBackend, LocalRwkvBackend, NvidiaBackend};
        use crate::engine::MockBackend;
        match self.provider {
            Provider::Mock => Ok(AnyBackend::Mock(MockBackend::default())),
            Provider::Nvidia => Ok(AnyBackend::Nvidia(NvidiaBackend::from_env()?)),
            Provider::Kilo => Ok(AnyBackend::Kilo(KiloBackend::from_env()?)),
            Provider::LocalRwkv => Ok(AnyBackend::LocalRwkv(LocalRwkvBackend::new(
                self.resolved_rwkv_size(),
                self.resolved_rwkv_mode(),
            ))),
        }
    }

    /// Build the configured backend with `http-backends` + `local-rwkv`.
    #[cfg(all(feature = "http-backends", feature = "local-rwkv"))]
    pub fn build_backend(&self) -> Result<crate::backends::AnyBackend> {
        use crate::backends::{AnyBackend, KiloBackend, LocalRwkvBackend, NvidiaBackend};
        use crate::engine::MockBackend;
        match self.provider {
            Provider::Mock => Ok(AnyBackend::Mock(MockBackend::default())),
            Provider::Nvidia => Ok(AnyBackend::Nvidia(NvidiaBackend::from_env()?)),
            Provider::Kilo => Ok(AnyBackend::Kilo(KiloBackend::from_env()?)),
            Provider::LocalRwkv => {
                // Try to find a converted .st model (prefer -converted.st, fall back to .st)
                let model = std::env::var("RWKV_MODEL").ok().or_else(|| {
                    let dir = std::env::current_dir().ok()?;
                    let c = dir.join("models/rwkv7-g1g-2.9b-20260526-ctx8192-converted.st");
                    if c.exists() { Some(c.to_string_lossy().to_string()) } else { None }
                });
                let vocab = std::env::var("RWKV_VOCAB").ok().or_else(|| {
                    let dir = std::env::current_dir().ok()?;
                    let v = dir.join("assets/vocab/rwkv_vocab_v20230424.json");
                    if v.exists() { Some(v.to_string_lossy().to_string()) } else { None }
                });
                if model.is_some() && vocab.is_some() {
                    let backend = crate::rwkv_backend::RwkvBackend::from_env()
                        .map_err(|e| anyhow::anyhow!("RWKV backend init failed: {e}"))?;
                    Ok(AnyBackend::RwkvBackend(backend))
                } else {
                    Ok(AnyBackend::LocalRwkv(LocalRwkvBackend::new(
                        self.resolved_rwkv_size(),
                        self.resolved_rwkv_mode(),
                    )))
                }
            }
        }
    }

    /// Build the configured backend with `local-rwkv` only (no HTTP backends).
    #[cfg(all(not(feature = "http-backends"), feature = "local-rwkv"))]
    pub fn build_backend(&self) -> Result<crate::backends::AnyBackend> {
        use crate::backends::{AnyBackend, LocalRwkvBackend};
        use crate::engine::MockBackend;
        match self.provider {
            Provider::Mock => Ok(AnyBackend::Mock(MockBackend::default())),
            Provider::LocalRwkv => {
                let model = std::env::var("RWKV_MODEL").ok();
                let vocab = std::env::var("RWKV_VOCAB").ok();
                if model.is_some() && vocab.is_some() {
                    let backend = crate::rwkv_backend::RwkvBackend::from_env()
                        .map_err(|e| anyhow::anyhow!("RWKV backend init failed: {e}"))?;
                    Ok(AnyBackend::RwkvBackend(backend))
                } else {
                    Ok(AnyBackend::LocalRwkv(LocalRwkvBackend::new(
                        self.resolved_rwkv_size(),
                        self.resolved_rwkv_mode(),
                    )))
                }
            }
            Provider::Nvidia | Provider::Kilo => Err(anyhow::anyhow!(
                "Provider {:?} requires the `http-backends` feature",
                self.provider
            )),
        }
    }

    /// Resolve the RWKV model size from config (defaults to 2.9B — fits the
    /// 4GB GPU cache pool).
    pub fn resolved_rwkv_size(&self) -> String {
        // Pull from the orchestrator_config if present; otherwise default.
        std::env::var("RWKV_SIZE").unwrap_or_else(|_| "rwkv7_g1g_2_9b".into())
    }

    /// Resolve the RWKV execution mode (defaults to GPU-direct-quantized).
    pub fn resolved_rwkv_mode(&self) -> String {
        std::env::var("RWKV_EXEC_MODE").unwrap_or_else(|_| "gpu_direct_quantized".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_uses_nvidia_with_full_capacity() {
        let c = Config::preset();
        assert_eq!(c.provider, Provider::Nvidia);
        assert_eq!(c.retry_policy().max_per_task, 3);
        assert_eq!(c.context_budget().task_context, 1200);
    }

    #[test]
    fn parses_json_config() {
        let json = r#"{"provider":"nvidia","capacity":{"nvidia_gpu":1},"retry":{"max_per_task":5}}"#;
        let c: Config = serde_json::from_str(json).unwrap();
        assert_eq!(c.provider, Provider::Nvidia);
        assert_eq!(c.retry_policy().max_per_task, 5);
    }
}
