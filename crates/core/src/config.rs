//! Configuration for provider selection, capacity, and orchestration.
//!
//! Drives the system from a config file (e.g. `model/default_config`) instead
//! of hardcoded values. The default provider is **NVIDIA** (see `Config::preset`).

use std::path::Path;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::agent::{ContextBudget, RetryPolicy};
use crate::capacity::Capacity;

/// Which backend provider to use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Nvidia,
    Kilo,
    Mock,
}

/// Capacity pool, in the same units as [`crate::capacity::Capacity`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapacityConfig {
    #[serde(default)]
    pub local_gpu_gb: u32,
    #[serde(default)]
    pub local_cache_gb: u32,
    #[serde(default)]
    pub cpu_model: u32,
    #[serde(default)]
    pub cpu_ram_gb: u32,
    #[serde(default)]
    pub kilo_tencent: u32,
    #[serde(default)]
    pub nvidia_gpu: u32,
}

impl CapacityConfig {
    pub fn to_capacity(&self) -> Capacity {
        Capacity {
            local_gpu_gb: self.local_gpu_gb,
            local_cache_gb: self.local_cache_gb,
            cpu_model: self.cpu_model,
            cpu_ram_gb: self.cpu_ram_gb,
            kilo_tencent: self.kilo_tencent,
            nvidia_gpu: self.nvidia_gpu,
        }
    }
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
    pub capacity: CapacityConfig,
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
            capacity: CapacityConfig {
                local_gpu_gb: 4,
                local_cache_gb: 8,
                cpu_model: 1,
                cpu_ram_gb: 32,
                kilo_tencent: 1,
                nvidia_gpu: 1,
            },
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

    pub fn capacity(&self) -> Capacity {
        self.capacity.to_capacity()
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
            .unwrap_or_else(|| "qwen/qwen3-next-80b-a3b-instruct".into())
    }

    pub fn resolved_kilo_model(&self) -> String {
        self.kilo_model
            .clone()
            .unwrap_or_else(|| "tencent/hy3:free".into())
    }

    /// Build the configured backend. Requires the `http-backends` feature
    /// (except for `Mock`).
    #[cfg(feature = "http-backends")]
    pub fn build_backend(&self) -> Result<crate::backends::AnyBackend> {
        use crate::backends::{AnyBackend, KiloBackend, NvidiaBackend};
        use crate::engine::MockBackend;
        match self.provider {
            Provider::Mock => Ok(AnyBackend::Mock(MockBackend::default())),
            Provider::Nvidia => Ok(AnyBackend::Nvidia(NvidiaBackend::from_env()?)),
            Provider::Kilo => Ok(AnyBackend::Kilo(KiloBackend::from_env()?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_uses_nvidia_with_full_capacity() {
        let c = Config::preset();
        assert_eq!(c.provider, Provider::Nvidia);
        let cap = c.capacity();
        assert_eq!(cap.local_gpu_gb, 4);
        assert_eq!(cap.nvidia_gpu, 1);
        assert_eq!(c.retry_policy().max_per_task, 3);
        assert_eq!(c.context_budget().task_context, 1200);
    }

    #[test]
    fn parses_json_config() {
        let json = r#"{"provider":"nvidia","capacity":{"nvidia_gpu":1},"retry":{"max_per_task":5}}"#;
        let c: Config = serde_json::from_str(json).unwrap();
        assert_eq!(c.provider, Provider::Nvidia);
        assert_eq!(c.capacity().nvidia_gpu, 1);
        assert_eq!(c.retry_policy().max_per_task, 5);
    }
}
