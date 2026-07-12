//! Hardware capability detection and modeling.
//!
//! Auto-detects GPU, VRAM, RAM, and compute capabilities. Provides a unified
//! view of what the system can run and where.

use serde::{Deserialize, Serialize};
use tracing::info;

/// What the system reports back about itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareCapabilities {
    /// CPU info
    pub cpu: CpuInfo,
    /// GPU info (primary adapter)
    pub gpu: Option<GpuInfo>,
    /// System RAM in MB
    pub total_ram_mb: u64,
    /// Available RAM in MB (at time of check)
    pub available_ram_mb: u64,
    /// PCIe bandwidth between CPU and GPU (MB/s)
    pub pcie_bandwidth_mb_s: u64,
    /// SSD sequential read speed (MB/s)
    pub ssd_read_mb_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub name: String,
    pub cores: u32,
    pub threads: u32,
    pub has_avx2: bool,
    pub has_avx512: bool,
    pub has_amx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub device_type: String,
    pub vram_total_mb: u64,
    pub vram_available_mb: u64,
    pub has_cooperative_matrix: bool,
    pub backend: String,
    /// Max buffer size in MB (determines max model size per allocation)
    pub max_buffer_mb: u64,
}

impl HardwareCapabilities {
    /// Auto-detect hardware capabilities (CPU/GPU/RAM/SSD).
    /// Currently uses best-effort probes; unknown values get reasonable defaults.
    pub async fn detect() -> Self {
        let cpu = Self::detect_cpu();
        let gpu = Self::detect_gpu().await;
        let ram = Self::detect_ram();
        let ssd = Self::estimate_ssd_speed();

        info!(
            cpu = %cpu.name,
            cores = cpu.cores,
            gpu = %gpu.as_ref().map(|g| g.name.as_str()).unwrap_or("none"),
            vram_mb = gpu.as_ref().map(|g| g.vram_total_mb).unwrap_or(0),
            ram_mb = ram.0,
            "hardware capabilities detected"
        );

        Self {
            cpu,
            gpu,
            total_ram_mb: ram.0,
            available_ram_mb: ram.1,
            pcie_bandwidth_mb_s: 6000, // PCIe 3.0 x8 ≈ 8 GB/s, practical ~6 GB/s
            ssd_read_mb_s: ssd,
        }
    }

    fn detect_cpu() -> CpuInfo {
        let name = std::env::consts::ARCH.to_string();
        let cores = num_cpus::get_physical() as u32;
        let threads = num_cpus::get() as u32;

        // Check CPU features via /proc/cpuinfo
        let flags = std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find_map(|l| l.strip_prefix("flags\t: "))
            .unwrap_or("")
            .to_string();

        CpuInfo {
            name: std::fs::read_to_string("/proc/cpuinfo")
                .unwrap_or_default()
                .lines()
                .find_map(|l| l.strip_prefix("model name\t: "))
                .unwrap_or(&name)
                .to_string(),
            cores,
            threads,
            has_avx2: flags.contains("avx2"),
            has_avx512: flags.contains("avx512"),
            has_amx: flags.contains("amx"),
        }
    }

    #[cfg(feature = "local-rwkv")]
    async fn detect_gpu() -> Option<GpuInfo> {
        let instance = wgpu::Instance::default();
        let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
        let best = adapters.into_iter().max_by_key(|a| {
            let i = a.get_info();
            let score = match i.device_type {
                wgpu::DeviceType::DiscreteGpu => 30,
                wgpu::DeviceType::IntegratedGpu => 20,
                wgpu::DeviceType::VirtualGpu => 15,
                _ => 5,
            };
            score
        })?;

        let info = best.get_info();
        let features = best.features();
        let limits = best.limits();

        Some(GpuInfo {
            name: info.name,
            device_type: format!("{:?}", info.device_type),
            vram_total_mb: limits.max_buffer_size / (1024 * 1024),
            vram_available_mb: limits.max_buffer_size / (1024 * 1024), // best guess
            has_cooperative_matrix: features
                .contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX),
            backend: format!("{:?}", info.backend),
            max_buffer_mb: limits.max_buffer_size / (1024 * 1024),
        })
    }

    #[cfg(not(feature = "local-rwkv"))]
    async fn detect_gpu() -> Option<GpuInfo> {
        // Without wgpu, try nvidia-smi or vulkaninfo
        let nvidia = Self::nvidia_smi();
        if nvidia.is_some() {
            return nvidia;
        }
        // Fallback: assume 4GB VRAM (RTX 2050 class)
        Some(GpuInfo {
            name: "Unknown GPU (assumed)".into(),
            device_type: "DiscreteGpu".into(),
            vram_total_mb: 4096,
            vram_available_mb: 3072,
            has_cooperative_matrix: false,
            backend: "unknown".into(),
            max_buffer_mb: 2048,
        })
    }

    fn nvidia_smi() -> Option<GpuInfo> {
        let output = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=name,memory.total,memory.free")
            .arg("--format=csv,noheader,nounits")
            .output()
            .ok()?;
        let line = std::str::from_utf8(&output.stdout).ok()?.lines().next()?.to_string();
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() < 3 {
            return None;
        }
        let name = parts[0].to_string();
        let total = parts[1].parse::<u64>().ok().unwrap_or(4096);
        let free = parts[2].parse::<u64>().ok().unwrap_or(total / 2);
        Some(GpuInfo {
            name,
            device_type: "DiscreteGpu".into(),
            vram_total_mb: total,
            vram_available_mb: free,
            has_cooperative_matrix: false,
            backend: "cuda".into(),
            max_buffer_mb: total / 2,
        })
    }

    fn detect_ram() -> (u64, u64) {
        // Try /proc/meminfo
        let info = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let total_kb = info
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))
            .and_then(|l| l.trim().strip_suffix(" kB"))
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(16_000_000); // default 16GB
        let avail_kb = info
            .lines()
            .find_map(|l| l.strip_prefix("MemAvailable:"))
            .and_then(|l| l.trim().strip_suffix(" kB"))
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(total_kb / 2);
        (total_kb / 1024, avail_kb / 1024)
    }

    fn estimate_ssd_speed() -> u64 {
        // Best-effort: check if NVMe via /sys
        let is_nvme = std::fs::read_dir("/sys/class/nvme").is_ok();
        if is_nvme {
            3500 // ~3.5 GB/s for gen3 NVMe
        } else {
            500 // ~500 MB/s for SATA SSD
        }
    }

    /// Estimate how much VRAM a model of given parameter count + quant will use.
    pub fn estimate_vram(&self, params_b: f64, quant_bits: u32) -> u64 {
        // formula: params * bits_per_param / 8 / 1024 / 1024
        let bytes_per_param = quant_bits as f64 / 8.0;
        let total_bytes = params_b * 1_000_000_000.0 * bytes_per_param;
        // Add ~10% overhead for KV cache, activations, etc.
        (total_bytes * 1.1 / (1024.0 * 1024.0)).ceil() as u64
    }

    /// Check if a model fits in VRAM with room to spare.
    pub fn fits_in_vram(&self, vram_mb: u64) -> bool {
        match &self.gpu {
            Some(gpu) => vram_mb + 512 <= gpu.vram_available_mb, // 512MB buffer
            None => false,
        }
    }

    /// Best hardware for a given model size.
    pub fn recommended_hardware(&self, model_vram_mb: u64) -> &str {
        if self.fits_in_vram(model_vram_mb) {
            "gpu"
        } else if model_vram_mb < self.available_ram_mb {
            "cpu"
        } else {
            "unable"
        }
    }
}
