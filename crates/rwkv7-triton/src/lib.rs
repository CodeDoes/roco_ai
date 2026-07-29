//! RWKV-7 Triton Backend
//!
//! High-performance WKV kernel using Triton.
//! Supports both NVIDIA (CUDA) and AMD (ROCm) GPUs.

/// WKV7 kernel in Triton
pub struct WKV7Triton {
    head_size: usize,
}

impl WKV7Triton {
    pub fn new(head_size: usize) -> Self {
        Self { head_size }
    }
    
    /// Get the Python module path for the Triton kernel
    pub fn kernel_module() -> &'static str {
        "rwkv7_triton.src.wkv7_triton"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_creation() {
        let kernel = WKV7Triton::new(64);
        assert_eq!(kernel.head_size, 64);
    }
}
