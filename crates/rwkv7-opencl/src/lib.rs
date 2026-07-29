//! RWKV-7 High-Performance Non-CUDA Backend
//!
//! This crate provides high-performance RWKV-7 inference using:
//! - **Triton** (NVIDIA + AMD via ROCm)
//! - **OpenCL 3.0** (Any GPU)
//!
//! Targets performance parity with Albatross CUDA through:
//! 1. Subgroup/warp-level operations
//! 2. Vectorized memory access
//! 3. Kernel fusion
//! 4. Loop unrolling

use std::sync::Arc;

mod backend;
pub use backend::{Backend, BackendType, DeviceInfo};

/// WKV kernel configuration
#[derive(Debug, Clone)]
pub struct WKV7Config {
    pub head_size: u32,
    pub num_heads: u32,
    pub channels: u32,
}

impl WKV7Config {
    pub fn new(channels: u32, num_heads: u32) -> Self {
        assert_eq!(channels % num_heads, 0);
        Self {
            head_size: channels / num_heads,
            num_heads,
            channels,
        }
    }
}

/// WKV7 kernel interface
pub trait WKV7Kernel: Send + Sync {
    /// Forward pass of WKV recurrence
    fn forward(
        &self,
        r: &[f32],
        w: &[f32],
        k: &[f32],
        v: &[f32],
        a: &[f32],
        b: &[f32],
        state: &mut [f32],
        y: &mut [f32],
        batch: u32,
        time: u32,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// WKV7 engine with automatic backend selection
pub struct RWKV7Engine {
    config: WKV7Config,
    backend: Arc<dyn Backend>,
    kernel: Box<dyn WKV7Kernel>,
}

impl RWKV7Engine {
    /// Create new engine with best available backend
    pub fn new(config: WKV7Config) -> Result<Self, Box<dyn std::error::Error>> {
        let backend = Backend::best_available()?;
        log::info!("Using backend: {:?}", backend.info());
        
        let kernel = backend.create_wkv7_kernel(&config)?;
        
        Ok(Self {
            config,
            backend: Arc::new(backend),
            kernel,
        })
    }
    
    /// Create engine with specific backend
    pub fn with_backend(
        config: WKV7Config,
        backend_type: BackendType,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let backend = Backend::new(backend_type)?;
        log::info!("Using backend: {:?}", backend.info());
        
        let kernel = backend.create_wkv7_kernel(&config)?;
        
        Ok(Self {
            config,
            backend: Arc::new(backend),
            kernel,
        })
    }
    
    /// Forward pass
    pub fn forward(
        &self,
        r: &[f32],
        w: &[f32],
        k: &[f32],
        v: &[f32],
        a: &[f32],
        b: &[f32],
        state: &mut [f32],
        y: &mut [f32],
        batch: u32,
        time: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.kernel.forward(r, w, k, v, a, b, state, y, batch, time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config() {
        let config = WKV7Config::new(4096, 64);
        assert_eq!(config.head_size, 64);
    }
}
