//! RoCo Inference — RWKV-7 inference via `web-rwkv` + WGPU.
//!
//! The only actively-supported inference path. Owns a dedicated actor thread
//! that hosts all non-`Send` WGPU resources. Provides [`RwkvBackend`]
//! implementing [`roco_engine::ModelBackend`] and proxy-guided quantization
//! analysis.

pub mod backend;
pub mod actor;
pub mod config;
pub mod sampling;
pub mod quant;

pub use backend::RwkvBackend;
pub use config::default_model_path;
pub use quant::*;
