//! Capacity model and backend routing (decision policy).
//!
//! Encodes the project's resource budget and per-backend cost so the
//! orchestrator can decide *where* to run each subtask. Spec:
//!
//! | Resource pool (total) | Local RWKV7 GPU (1.5B/2.9B) | Local RWKV7 CPU (7.2B) | Local RWKV7 CPU (13B) | Kilo hy3        | NVIDIA         |
//! |-----------------------|------------------------|----------------------|----------------------|----------------|----------------|
//! | `local_gpu:4gb`       | `local_gpu:4gb`        | —                    | —                    | —              | —              |
//! | `local_cache:8gb`     | `local_cache:4gb`      | —                    | —                    | —              | —              |
//! | `cpu_model:1`         | —                      | `cpu_model:1`        | `cpu_model:1`        | —              | —              |
//! | `cpu_ram_gb:32`       | —                      | `cpu_ram_gb:15`      | `cpu_ram_gb:27`      | —              | —              |
//! | `kilo:1tencent`      | —                      | —                    | —                    | `kilo:1tencent`| —              |
//! | `nvidia:1gpu`         | —                      | —                    | —                    | —              | `nvidia:1gpu`  |
//!
//! Consequences:
//! - The 8gb cache holds up to **two** 4gb GPU models in memory at once.
//! - **1 GPU model + 1 CPU model** can run concurrently (disjoint resources).
//! - CPU model footprints are quantized approximations, rounded **up** to
//!   integer GB (14.4 → 15, 26.5 → 27).
//!
//! The CPU execution paths are *future* (no backend implemented yet); this
//! module only models the capacity and routing decision.
//!
//! Local RWKV execution modes (future — see `provider/local_rwkv_adapter.json` and
//! <https://github.com/cryscan/web-rwkv>):
//! - **gpu_direct_quantized**: quantized weights loaded straight onto the GPU
//!   from disk (costs `local_gpu` + `local_cache`).
//! - **gpu_partial_offload**: hybrid — some layers on GPU, the rest offloaded
//!   to CPU (costs part `local_gpu` + part `cpu_ram` + `local_cache`).
//! - **cpu_only**: runs entirely on CPU (costs `cpu_model` + `cpu_ram`).
//! In all modes weights are retained in `local_cache` so re-use avoids a disk
//! re-load. No local backend is implemented yet; this module only models the
//! capacity and routing decision.

/// A multiset of heterogeneous capacity units. Each field is a distinct
/// resource measured in its own unit (GB, model slots, tenant slots, GPU slots).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Capacity {
    pub local_gpu_gb: u32,
    pub local_cache_gb: u32,
    pub cpu_model: u32,
    pub cpu_ram_gb: u32,
    pub kilo_tencent: u32,
    pub nvidia_gpu: u32,
}

impl Capacity {
    /// The full capacity pool declared for this machine.
    pub fn total_capacity() -> Self {
        Capacity {
            local_gpu_gb: 4,
            local_cache_gb: 8,
            cpu_model: 1,
            cpu_ram_gb: 32,
            kilo_tencent: 1,
            nvidia_gpu: 1,
        }
    }

    /// Cost of a local RWKV7 GPU model (1.5B or 2.9B) — same footprint for
    /// both sizes.
    pub fn local_rwkv7() -> Self {
        Capacity {
            local_gpu_gb: 4,
            local_cache_gb: 4,
            ..Default::default()
        }
    }

    /// Cost of a local RWKV7 CPU model, 7.2B (quantized, ~14.4gb → 15gb).
    pub fn local_rwkv7_cpu_7b() -> Self {
        Capacity {
            cpu_model: 1,
            cpu_ram_gb: 15,
            ..Default::default()
        }
    }

    /// Cost of a local RWKV7 CPU model, 13B (quantized, ~26.5gb → 27gb).
    pub fn local_rwkv7_cpu_13b() -> Self {
        Capacity {
            cpu_model: 1,
            cpu_ram_gb: 27,
            ..Default::default()
        }
    }

    /// Cost of a Kilo hy3 (`tencent/hy3:free`) request.
    pub fn kilo_hy3() -> Self {
        Capacity {
            kilo_tencent: 1,
            ..Default::default()
        }
    }

    /// Cost of an NVIDIA request.
    pub fn nvidia() -> Self {
        Capacity {
            nvidia_gpu: 1,
            ..Default::default()
        }
    }

    /// True if `self` can satisfy `cost` on every resource.
    pub fn can_fit(&self, cost: Capacity) -> bool {
        self.local_gpu_gb >= cost.local_gpu_gb
            && self.local_cache_gb >= cost.local_cache_gb
            && self.cpu_model >= cost.cpu_model
            && self.cpu_ram_gb >= cost.cpu_ram_gb
            && self.kilo_tencent >= cost.kilo_tencent
            && self.nvidia_gpu >= cost.nvidia_gpu
    }

    pub fn saturating_sub(&self, other: Capacity) -> Capacity {
        Capacity {
            local_gpu_gb: self.local_gpu_gb.saturating_sub(other.local_gpu_gb),
            local_cache_gb: self.local_cache_gb.saturating_sub(other.local_cache_gb),
            cpu_model: self.cpu_model.saturating_sub(other.cpu_model),
            cpu_ram_gb: self.cpu_ram_gb.saturating_sub(other.cpu_ram_gb),
            kilo_tencent: self.kilo_tencent.saturating_sub(other.kilo_tencent),
            nvidia_gpu: self.nvidia_gpu.saturating_sub(other.nvidia_gpu),
        }
    }

    pub fn add(&self, other: Capacity) -> Capacity {
        Capacity {
            local_gpu_gb: self.local_gpu_gb + other.local_gpu_gb,
            local_cache_gb: self.local_cache_gb + other.local_cache_gb,
            cpu_model: self.cpu_model + other.cpu_model,
            cpu_ram_gb: self.cpu_ram_gb + other.cpu_ram_gb,
            kilo_tencent: self.kilo_tencent + other.kilo_tencent,
            nvidia_gpu: self.nvidia_gpu + other.nvidia_gpu,
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Capacity::default()
    }
}

/// A selectable backend, with its associated capacity cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Local RWKV7 model on GPU (1.5B or 2.9B).
    LocalRwkv7,
    /// Local RWKV7 CPU model, 7.2B (quantized).
    LocalRwkv7Cpu7B,
    /// Local RWKV7 CPU model, 13B (quantized).
    LocalRwkv7Cpu13B,
    /// Kilo hy3 (`tencent/hy3:free`).
    KiloHy3,
    /// NVIDIA-hosted model.
    Nvidia,
}

impl BackendKind {
    /// Capacity consumed when dispatching to this backend.
    pub fn cost(&self) -> Capacity {
        match self {
            BackendKind::LocalRwkv7 => Capacity::local_rwkv7(),
            BackendKind::LocalRwkv7Cpu7B => Capacity::local_rwkv7_cpu_7b(),
            BackendKind::LocalRwkv7Cpu13B => Capacity::local_rwkv7_cpu_13b(),
            BackendKind::KiloHy3 => Capacity::kilo_hy3(),
            BackendKind::Nvidia => Capacity::nvidia(),
        }
    }

    /// Human-readable capacity label, as used in config/logs.
    pub fn label(&self) -> &'static str {
        match self {
            BackendKind::LocalRwkv7 => "local_gpu:4gb+local_cache:4gb",
            BackendKind::LocalRwkv7Cpu7B => "cpu_model:1+cpu_ram_gb:15",
            BackendKind::LocalRwkv7Cpu13B => "cpu_model:1+cpu_ram_gb:27",
            BackendKind::KiloHy3 => "kilo:1tencent",
            BackendKind::Nvidia => "nvidia:1gpu",
        }
    }
}

/// Whether `order` (highest preference first) can be satisfied by `free`;
/// returns the first kind whose cost fits.
pub fn select(free: &Capacity, order: &[BackendKind]) -> Option<BackendKind> {
    order.iter().copied().find(|k| free.can_fit(k.cost()))
}

/// Tracks allocated vs. free capacity for a machine, allowing acquire/release
/// as subtasks start and finish.
#[derive(Debug, Clone, Copy)]
pub struct CapacityPool {
    total: Capacity,
    free: Capacity,
}

impl CapacityPool {
    pub fn new(total: Capacity) -> Self {
        Self { total, free: total }
    }

    pub fn total(&self) -> Capacity {
        self.total
    }

    pub fn free(&self) -> Capacity {
        self.free
    }

    pub fn can_acquire(&self, cost: Capacity) -> bool {
        self.free.can_fit(cost)
    }

    /// Try to reserve `cost`. Returns false (and reserves nothing) if it does
    /// not fit.
    pub fn acquire(&mut self, cost: Capacity) -> bool {
        if self.can_acquire(cost) {
            self.free = self.free.saturating_sub(cost);
            true
        } else {
            false
        }
    }

    /// Return `cost` to the pool (capped at `total`).
    pub fn release(&mut self, cost: Capacity) {
        self.free = self.free.add(cost).min(self.total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_capacity_matches_spec() {
        let t = Capacity::total_capacity();
        assert_eq!(t.local_gpu_gb, 4);
        assert_eq!(t.local_cache_gb, 8);
        assert_eq!(t.cpu_model, 1);
        assert_eq!(t.cpu_ram_gb, 32);
        assert_eq!(t.kilo_tencent, 1);
        assert_eq!(t.nvidia_gpu, 1);
    }

    #[test]
    fn each_backend_cost_matches_spec() {
        assert_eq!(
            Capacity::local_rwkv7(),
            Capacity { local_gpu_gb: 4, local_cache_gb: 4, ..Default::default() }
        );
        assert_eq!(
            Capacity::local_rwkv7_cpu_7b(),
            Capacity { cpu_model: 1, cpu_ram_gb: 15, ..Default::default() }
        );
        assert_eq!(
            Capacity::local_rwkv7_cpu_13b(),
            Capacity { cpu_model: 1, cpu_ram_gb: 27, ..Default::default() }
        );
        assert_eq!(Capacity::kilo_hy3(), Capacity { kilo_tencent: 1, ..Default::default() });
        assert_eq!(Capacity::nvidia(), Capacity { nvidia_gpu: 1, ..Default::default() });
    }

    #[test]
    fn cache_holds_two_gpu_models_but_only_one_runs() {
        // The 8gb cache can *hold* two 4gb GPU models in memory at once.
        let two_cache = Capacity::local_rwkv7().add(Capacity::local_rwkv7()).local_cache_gb;
        assert_eq!(two_cache, 8);
        assert!(Capacity::total_capacity().local_cache_gb >= two_cache);

        // But only one GPU model can *run* at once (the 4gb GPU slot is the
        // binding constraint), so two cannot be acquired simultaneously.
        let two_running = Capacity::local_rwkv7().add(Capacity::local_rwkv7());
        assert!(!Capacity::total_capacity().can_fit(two_running));
    }

    #[test]
    fn only_one_local_gpu_and_one_cpu_run_at_once() {
        // Only one GPU model (consumes the whole 4gb GPU).
        let two_gpu = Capacity::local_rwkv7().add(Capacity::local_rwkv7());
        assert!(!Capacity::total_capacity().can_fit(two_gpu));
        // Only one CPU model (cpu_model slot = 1), even though RAM could
        // technically hold two 7.2B footprints.
        let two_cpu = Capacity::local_rwkv7_cpu_7b().add(Capacity::local_rwkv7_cpu_7b());
        assert!(!Capacity::total_capacity().can_fit(two_cpu));
    }

    #[test]
    fn gpu_and_cpu_run_concurrently() {
        // 1 GPU model + 1 CPU model fit together (disjoint resources).
        let mix = Capacity::local_rwkv7().add(Capacity::local_rwkv7_cpu_13b());
        assert!(Capacity::total_capacity().can_fit(mix));
    }

    #[test]
    fn router_prefers_local_then_falls_back() {
        let order = [BackendKind::LocalRwkv7, BackendKind::KiloHy3, BackendKind::Nvidia];

        let free = Capacity::total_capacity();
        assert_eq!(select(&free, &order), Some(BackendKind::LocalRwkv7));

        let free = free.saturating_sub(Capacity::local_rwkv7());
        assert_eq!(select(&free, &order), Some(BackendKind::KiloHy3));

        let free = free.saturating_sub(Capacity::kilo_hy3());
        assert_eq!(select(&free, &order), Some(BackendKind::Nvidia));

        let free = free.saturating_sub(Capacity::nvidia());
        assert_eq!(select(&free, &order), None);
    }

    #[test]
    fn pool_acquire_and_release() {
        let mut pool = CapacityPool::new(Capacity::total_capacity());
        assert!(pool.acquire(Capacity::local_rwkv7()));
        assert!(!pool.acquire(Capacity::local_rwkv7())); // GPU busy
        assert!(pool.acquire(Capacity::nvidia()));
        assert!(pool.can_acquire(Capacity::kilo_hy3())); // kilo still free

        // A CPU model can also run alongside the GPU model.
        assert!(pool.acquire(Capacity::local_rwkv7_cpu_13b()));

        pool.release(Capacity::local_rwkv7());
        assert!(pool.acquire(Capacity::local_rwkv7())); // freed up
    }
}
