//! Model downloader — fetches models from Hugging Face using the HF CLI.
//!
//! Supports downloading from any HF repo, including litert-community models,
//! RWKV repos, whisper.cpp models, etc.
//!
//! ```bash
//! # Download a single model
//! roco download litert-community/whisper-tiny
//!
//! # Download and categorize
//! roco download litert-community/whisper-tiny --category s2t
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::performance::ModelArch;

/// A model registered for download or already on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    /// HuggingFace model ID (e.g. "litert-community/whisper-tiny")
    pub hf_id: String,
    /// Local path where the model is (or will be) stored
    pub local_path: PathBuf,
    /// Category tag
    pub category: ModelCategory,
    /// Architecture family
    pub arch: ModelArch,
    /// Parameter count in billions
    pub params_b: f64,
    /// File size on disk (MB)
    pub size_mb: u64,
    /// Whether the model is downloaded
    pub downloaded: bool,
    /// Last used timestamp
    pub last_used: Option<u64>,
    /// Notes / description
    pub notes: String,
}

/// Broad category of what a model does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCategory {
    /// General text generation / chat
    Llm,
    /// Code generation
    Coding,
    /// Speech-to-text
    S2t,
    /// Text-to-speech
    T2s,
    /// Image generation
    Diffusion,
    /// Vision-language (image understanding)
    Vision,
    /// Text embeddings
    Embedding,
    /// Classification / tagging
    Classification,
    /// Function calling / computer use
    FunctionCalling,
    /// Reasoning
    Reasoning,
}

impl std::fmt::Display for ModelCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Llm => "llm",
            Self::Coding => "coding",
            Self::S2t => "s2t",
            Self::T2s => "t2s",
            Self::Diffusion => "diffusion",
            Self::Vision => "vision",
            Self::Embedding => "embedding",
            Self::Classification => "classification",
            Self::FunctionCalling => "function",
            Self::Reasoning => "reasoning",
        })
    }
}

impl ModelEntry {
    pub fn new(
        hf_id: impl Into<String>,
        local_dir: impl Into<PathBuf>,
        category: ModelCategory,
        arch: ModelArch,
        params_b: f64,
        notes: impl Into<String>,
    ) -> Self {
        let hf_id = hf_id.into();
        let dir: PathBuf = local_dir.into();
        // Determine local path: models/<category>/<model-name>
        let model_name = hf_id.split('/').last().unwrap_or(&hf_id);
        let local_path = dir.join(category.to_string()).join(model_name);

        // Estimate size: ~2GB per 1B params at FP16, scaled by quant
        let size_mb = (params_b * 2000.0).ceil() as u64;

        let downloaded = local_path.exists();

        Self {
            hf_id,
            local_path,
            category,
            arch,
            params_b,
            size_mb,
            downloaded,
            last_used: None,
            notes: notes.into(),
        }
    }

    /// Check if the model file exists on disk.
    pub fn check_downloaded(&mut self) {
        self.downloaded = self.local_path.exists();
    }
}

/// Download a model from Hugging Face using the `huggingface-cli` tool.
/// Falls back to `git lfs` if the CLI is not available.
pub async fn download_model(entry: &ModelEntry) -> Result<(), String> {
    let path = &entry.local_path;
    if path.exists() {
        info!(model = %entry.hf_id, path = %path.display(), "already downloaded");
        return Ok(());
    }

    // Create parent directory
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }

    // Try huggingface-cli first
    let hf_cli = which_hf_cli();
    let result = if let Some(cli) = &hf_cli {
        info!(model = %entry.hf_id, cli = %cli, "downloading via huggingface-cli");
        download_with_hf_cli(cli, &entry.hf_id, path).await
    } else {
        info!(model = %entry.hf_id, "huggingface-cli not found, trying git lfs");
        download_with_git_lfs(&entry.hf_id, path).await
    };

    match result {
        Ok(()) => {
            info!(model = %entry.hf_id, path = %path.display(), "download complete");
            Ok(())
        }
        Err(e) => {
            warn!(model = %entry.hf_id, error = %e, "download failed");
            Err(format!("failed to download {}: {e}", entry.hf_id))
        }
    }
}

/// Check if huggingface-cli is available.
fn which_hf_cli() -> Option<String> {
    // Check common names
    for name in &["huggingface-cli", "hf-cli", "huggingface_hub"] {
        let output = std::process::Command::new("which")
            .arg(name)
            .output()
            .ok()?;
        if output.status.success() {
            let path = std::str::from_utf8(&output.stdout).ok()?.trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }
    None
}

/// Download using the huggingface-cli tool.
async fn download_with_hf_cli(cli: &str, hf_id: &str, dest: &Path) -> Result<(), String> {
    let output = tokio::process::Command::new(cli)
        .arg("download")
        .arg(hf_id)
        .arg("--local-dir")
        .arg(dest)
        .arg("--local-dir-use-symlinks")
        .arg("False")
        .arg("--resume-download")
        .output()
        .await
        .map_err(|e| format!("failed to run {cli}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{cli} failed: {stderr}"));
    }
    Ok(())
}

/// Download using git lfs (works for all HF repos).
async fn download_with_git_lfs(hf_id: &str, dest: &Path) -> Result<(), String> {
    let url = format!("https://huggingface.co/{hf_id}");
    let output = tokio::process::Command::new("git")
        .arg("clone")
        .arg(&url)
        .arg(dest)
        .output()
        .await
        .map_err(|e| format!("failed to run git clone: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git clone failed: {stderr}"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Suggested models from litert-community to download and test
// ---------------------------------------------------------------------------

/// Models worth downloading immediately for testing.
pub fn suggested_downloads(base_dir: impl Into<PathBuf>) -> Vec<ModelEntry> {
    let dir: PathBuf = base_dir.into();
    vec![
        // Speech
        ModelEntry::new(
            "litert-community/whisper-tiny",
            &dir, ModelCategory::S2t,
            ModelArch::Speech, 0.039,
            "Whisper Tiny S2T — 75MB, fastest speech recognition",
        ),
        ModelEntry::new(
            "litert-community/parakeet-tdt-0.6b-v3",
            &dir, ModelCategory::T2s,
            ModelArch::Speech, 0.6,
            "Parakeet TTS — text-to-speech, 0.6B params",
        ),
        // Small LLMs
        ModelEntry::new(
            "litert-community/SmolLM2-360M-Instruct",
            &dir, ModelCategory::Llm,
            ModelArch::FfnTransformer, 0.36,
            "SmolLM2 360M — tiny general LLM, good for CPU inference",
        ),
        ModelEntry::new(
            "litert-community/Qwen2.5-0.5B-Instruct",
            &dir, ModelCategory::Llm,
            ModelArch::FfnTransformer, 0.5,
            "Qwen2.5 0.5B — smallest Qwen, fast on GPU/CPU",
        ),
        ModelEntry::new(
            "litert-community/Qwen2.5-1.5B-Instruct",
            &dir, ModelCategory::Llm,
            ModelArch::FfnTransformer, 1.5,
            "Qwen2.5 1.5B — solid all-rounder, fits in VRAM alongside RWKV",
        ),
        // Vision
        ModelEntry::new(
            "litert-community/FastVLM-0.5B",
            &dir, ModelCategory::Vision,
            ModelArch::VisionEncoder, 0.5,
            "FastVLM 0.5B — vision-language model, image understanding",
        ),
        // Embeddings
        ModelEntry::new(
            "litert-community/embeddinggemma-300m",
            &dir, ModelCategory::Embedding,
            ModelArch::Embedding, 0.3,
            "EmbeddingGemma 300M — text embeddings for RAG/semantic search",
        ),
        // Function calling
        ModelEntry::new(
            "litert-community/functiongemma-270m-ft-mobile-actions",
            &dir, ModelCategory::FunctionCalling,
            ModelArch::FfnTransformer, 0.27,
            "FunctionGemma 270M — function calling + computer use actions",
        ),
        // Coding
        ModelEntry::new(
            "litert-community/Qwen2.5-1.5B-Instruct",
            &dir, ModelCategory::Coding,
            ModelArch::FfnTransformer, 1.5,
            "Qwen2.5 1.5B — also good for code (dual-use)",
        ),
        // Diffusion (image generation) — large, needs GPU
        ModelEntry::new(
            "litert-community/FLUX.2-klein-4B-LiteRT",
            &dir, ModelCategory::Diffusion,
            ModelArch::Diffusion, 4.0,
            "FLUX.2 klein 4B — text-to-image, needs 4GB+ VRAM",
        ),
    ]
}

/// Print a download plan with size estimates.
pub fn print_download_plan(entries: &[ModelEntry]) {
    let total_mb: u64 = entries.iter().map(|e| e.size_mb).sum();
    let to_download: Vec<&ModelEntry> = entries.iter().filter(|e| !e.downloaded).collect();
    let download_mb: u64 = to_download.iter().map(|e| e.size_mb).sum();

    println!("\n=== Model Download Plan ===");
    println!("  Total models:    {}", entries.len());
    println!("  Already have:    {}", entries.len() - to_download.len());
    println!("  To download:     {}", to_download.len());
    println!("  Total size:      {} MB", total_mb);
    println!("  Download size:   {} MB", download_mb);
    println!();

    for entry in entries {
        let status = if entry.downloaded { "✅" } else { "⬇️ " };
        println!("  {status} {:<50} {:>8} MB  [{:12}]",
            entry.hf_id, entry.size_mb, entry.category.to_string());
    }
    println!();
}
