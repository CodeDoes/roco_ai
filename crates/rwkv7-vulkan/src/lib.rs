//! RWKV-7 Vulkan Support
//!
//! This crate provides a convenient interface for RWKV-7 inference
//! using Vulkan (via web-rwkv and wgpu).
//!
//! # Architecture
//!
//! ```text
//! This crate
//!     ↓
//! web-rwkv (WGSL shaders)
//!     ↓
//!   wgpu
//!     ↓
//! ┌────┴────┐
//! │ Vulkan  │  ← Linux, Android, Windows
//! │ Metal   │  ← macOS, iOS
//! │ DX12    │  ← Windows
//! └─────────┘
//! ```

// Re-export web-rwkv's Context which handles GPU backend selection
pub use web_rwkv::context::{Context, ContextError};
