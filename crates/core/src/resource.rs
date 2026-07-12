//! Resource-aware scheduler for multi-device model loading.
//!
//! Manages RAM and VRAM across all available devices (GPU0, GPU1, CPU).
//! Clients submit resource claims (e.g. "2GB GPU0 + 1GB GPU1 + 4GB RAM")
//! and the scheduler atomically checks if ALL constraints can be satisfied.
//! If not, the client blocks until space frees up or eviction makes room.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                  ResourceManager                 │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────┐  │
//! │  │  Node(GPU0) │  │  Node(GPU1) │  │ CPU RAM │  │
//! │  │  total:4GB  │  │  total:4GB  │  │ 32GB    │  │
//! │  │  used:2.7GB │  │  used:0GB   │  │ 8GB     │  │
//! │  └──────┬──────┘  └──────┬──────┘  └────┬────┘  │
//! │         │                │               │       │
//! │  ┌──────┴────────────────┴───────────────┴──┐    │
//! │  │           Reservation Queue              │    │
//! │  │  [ClaimA: GPU0=2GB, GPU1=1GB, RAM=4GB]  │    │
//! │  │  [ClaimB: GPU0=1GB, RAM=2GB]            │    │
//! │  └──────────────────────────────────────────┘    │
//! └──────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn default_instant() -> Instant { Instant::now() }
use parking_lot::{Mutex, Condvar};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};

// ---------------------------------------------------------------------------
// Resource identifiers
// ---------------------------------------------------------------------------

/// Identifier for a resource pool.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceId {
    /// System RAM
    SystemRam,
    /// A specific GPU by index (0, 1, ...)
    GpuVram(u32),
    /// Named resource (for future expansion: NPU, TPU, etc.)
    Named(String),
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceId::SystemRam => write!(f, "ram"),
            ResourceId::GpuVram(idx) => write!(f, "gpu{idx}"),
            ResourceId::Named(name) => write!(f, "{name}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Resource node — tracks capacity for one device
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceNode {
    /// Human-readable label (e.g. "NVIDIA RTX 2050")
    pub label: String,
    /// Total capacity in MB
    pub total_mb: u64,
    /// Currently reserved/used in MB
    pub used_mb: u64,
}

impl ResourceNode {
    pub fn new(label: impl Into<String>, total_mb: u64) -> Self {
        Self {
            label: label.into(),
            total_mb,
            used_mb: 0,
        }
    }

    pub fn available_mb(&self) -> u64 {
        self.total_mb.saturating_sub(self.used_mb)
    }

    /// Reserve `amount` MB. Returns false if insufficient capacity.
    fn reserve(&mut self, amount: u64) -> bool {
        if self.available_mb() >= amount {
            self.used_mb += amount;
            true
        } else {
            false
        }
    }

    fn release(&mut self, amount: u64) {
        self.used_mb = self.used_mb.saturating_sub(amount);
    }
}

// ---------------------------------------------------------------------------
// Resource claim — what a client needs
// ---------------------------------------------------------------------------

/// A set of resource requirements that must ALL be satisfied atomically.
#[derive(Clone, Serialize, Deserialize)]
pub struct ResourceClaim {
    /// Unique identifier for this claim (set by manager on admission)
    pub id: u64,
    /// Human-readable label (e.g. model name)
    pub label: String,
    /// Per-resource requirements in MB
    pub requirements: HashMap<ResourceId, u64>,
    /// When this claim was created
    #[serde(skip, default = "default_instant")]
    pub created_at: Instant,
    /// Priority (higher = more urgent)
    pub priority: i32,
    /// Client callback for forced eviction notification
    #[serde(skip)]
    pub evict_notify: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl std::fmt::Debug for ResourceClaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceClaim")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("requirements", &self.requirements)
            .field("priority", &self.priority)
            .field("evict_notify", &self.evict_notify.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl ResourceClaim {
    pub fn new(label: impl Into<String>, requirements: HashMap<ResourceId, u64>) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            label: label.into(),
            requirements,
            created_at: Instant::now(),
            priority: 0,
            evict_notify: None,
        }
    }

    /// Set priority (higher = more important, less likely to be evicted).
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set eviction notification callback.
    pub fn with_evict_notify(mut self, f: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.evict_notify = Some(f);
        self
    }
}

// ---------------------------------------------------------------------------
// Resource Manager
// ---------------------------------------------------------------------------

/// Manages resource allocation across all devices.
/// Thread-safe. Supports blocking reservations and LRU eviction.
pub struct ResourceManager {
    /// All resource pools
    nodes: Mutex<HashMap<ResourceId, ResourceNode>>,
    /// Active claims
    claims: Mutex<Vec<ResourceClaim>>,
    /// For blocking reservations
    condvar: Condvar,
    /// Auto-incrementing claim ID
    next_claim_id: std::sync::atomic::AtomicU64,
    /// Tracking stats
    stats: Mutex<ResourceStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceStats {
    pub total_admissions: u64,
    pub total_denials: u64,
    pub total_evictions: u64,
    pub peak_ram_mb: u64,
    pub peak_vram_mb: u64,
}

impl ResourceManager {
    /// Create a new manager with the given resource pools.
    pub fn new(nodes: HashMap<ResourceId, ResourceNode>) -> Self {
        Self {
            nodes: Mutex::new(nodes),
            claims: Mutex::new(Vec::new()),
            condvar: Condvar::new(),
            next_claim_id: std::sync::atomic::AtomicU64::new(1),
            stats: Mutex::new(ResourceStats::default()),
        }
    }

    /// Auto-detect and create resource nodes from hardware capabilities.
    pub async fn from_hardware() -> Self {
        let hw = crate::inference::hardware::HardwareCapabilities::detect().await;
        let mut nodes = HashMap::new();

        // System RAM — use available, not total (OS + other processes need room)
        let ram_reserve = hw.available_ram_mb.min(hw.total_ram_mb.saturating_sub(2048));
        nodes.insert(
            ResourceId::SystemRam,
            ResourceNode::new("System RAM", ram_reserve),
        );

        // GPU VRAM — one node per adapter
        // For now, we detect a single GPU via HardwareCapabilities.
        // Multi-GPU would require enumerating all adapters.
        if let Some(gpu) = &hw.gpu {
            // Reserve 512MB for OS/WGPU overhead
            let usable = gpu.vram_total_mb.saturating_sub(512);
            nodes.insert(
                ResourceId::GpuVram(0),
                ResourceNode::new(gpu.name.clone(), usable),
            );
        }

        info!(
            "resource manager initialized: {} pools",
            nodes.len()
        );
        for (id, node) in &nodes {
            info!("  {}: {} MB total ({} MB label)", id, node.total_mb, node.label);
        }

        Self::new(nodes)
    }

    /// Current snapshot of all resource pools.
    pub fn snapshot(&self) -> HashMap<ResourceId, ResourceNode> {
        self.nodes.lock().clone()
    }

    /// List all active claims.
    pub fn active_claims(&self) -> Vec<ResourceClaim> {
        self.claims.lock().clone()
    }

    /// Stats since startup.
    pub fn stats(&self) -> ResourceStats {
        self.stats.lock().clone()
    }

    /// Try to reserve resources for a claim. Returns immediately.
    /// Returns Ok(()) if all resources could be reserved, Err with
    /// a map of what's missing otherwise.
    pub fn try_reserve(&self, claim: &ResourceClaim) -> Result<(), HashMap<ResourceId, (u64, u64)>> {
        let mut nodes = self.nodes.lock();
        let mut missing = HashMap::new();

        // Check all requirements
        for (res_id, needed) in &claim.requirements {
            let node = match nodes.get(res_id) {
                Some(n) => n,
                None => {
                    missing.insert(res_id.clone(), (*needed, 0));
                    continue;
                }
            };
            if node.available_mb() < *needed {
                missing.insert(res_id.clone(), (*needed, node.available_mb()));
            }
        }

        if !missing.is_empty() {
            return Err(missing);
        }

        // All good — reserve
        for (res_id, needed) in &claim.requirements {
            if let Some(node) = nodes.get_mut(res_id) {
                node.reserve(*needed);
            }
        }

        // Track stats
        let mut stats = self.stats.lock();
        stats.total_admissions += 1;
        for (res_id, node) in nodes.iter() {
            match res_id {
                ResourceId::SystemRam => stats.peak_ram_mb = stats.peak_ram_mb.max(node.used_mb),
                ResourceId::GpuVram(_) => stats.peak_vram_mb = stats.peak_vram_mb.max(node.used_mb),
                _ => {}
            }
        }

        Ok(())
    }

    /// Release a claim's resources.
    pub fn release(&self, claim: &ResourceClaim) {
        let mut nodes = self.nodes.lock();
        let mut claims = self.claims.lock();

        // Release resources
        for (res_id, amount) in &claim.requirements {
            if let Some(node) = nodes.get_mut(res_id) {
                node.release(*amount);
            }
        }

        // Remove from active claims
        claims.retain(|c| c.id != claim.id);

        // Notify waiters
        self.condvar.notify_all();

        debug!(claim = %claim.label, "released resources");
    }

    /// Block until resources can be reserved for this claim.
    /// Returns the claim on success.
    pub fn reserve_blocking(
        &self,
        mut claim: ResourceClaim,
        timeout: Option<Duration>,
    ) -> Result<ResourceClaim, ResourceError> {
        let deadline = timeout.map(|t| Instant::now() + t);

        // Helper: check if claim fits and admit if so.
        // Returns true if admitted. Operates on already-locked data.
        fn try_admit(
            claim: &mut ResourceClaim,
            nodes: &mut HashMap<ResourceId, ResourceNode>,
            active: &mut Vec<ResourceClaim>,
            next_id: &std::sync::atomic::AtomicU64,
            stats: &mut ResourceStats,
        ) -> bool {
            for (res_id, needed) in &claim.requirements {
                let enough = match nodes.get(res_id) {
                    Some(n) => n.available_mb() >= *needed,
                    None => false,
                };
                if !enough {
                    return false;
                }
            }
            // Admit
            for (res_id, needed) in &claim.requirements {
                if let Some(node) = nodes.get_mut(res_id) {
                    node.reserve(*needed);
                }
            }
            claim.id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            active.push(claim.clone());
            stats.total_admissions += 1;
            true
        }

        // First, try immediate admit (no wait, no eviction)
        {
            let mut nodes = self.nodes.lock();
            let mut active = self.claims.lock();
            let mut stats = self.stats.lock();
            if try_admit(&mut claim, &mut nodes, &mut active, &self.next_claim_id, &mut stats) {
                return Ok(claim);
            }

            // Try eviction-based admit
            if self.try_evict_for(&mut nodes, &mut active, &claim) {
                if try_admit(&mut claim, &mut nodes, &mut active, &self.next_claim_id, &mut stats) {
                    stats.total_evictions += 1;
                    return Ok(claim);
                }
            }
            drop(stats);
            drop(active);
            drop(nodes);
        }

        // Need to wait — block until resources free up
        let mut active = self.claims.lock();
        loop {
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    let mut stats = self.stats.lock();
                    stats.total_denials += 1;
                    return Err(ResourceError::Timeout);
                }
            }

            let mut nodes = self.nodes.lock();
            let mut stats = self.stats.lock();
            if try_admit(&mut claim, &mut nodes, &mut active, &self.next_claim_id, &mut stats) {
                drop(stats);
                drop(nodes);
                drop(active);
                self.condvar.notify_all();
                return Ok(claim);
            }
            drop(stats);

            // Try eviction
            if self.try_evict_for(&mut nodes, &mut active, &claim) {
                drop(nodes);
                let mut stats = self.stats.lock();
                stats.total_evictions += 1;
                continue;
            }
            drop(nodes);

            // Wait for signal
            let wait_ok = if let Some(deadline) = deadline {
                let dur = deadline.saturating_duration_since(Instant::now());
                !self.condvar.wait_for(&mut active, dur).timed_out()
            } else {
                self.condvar.wait(&mut active);
                true
            };

            if !wait_ok {
                let mut stats = self.stats.lock();
                stats.total_denials += 1;
                return Err(ResourceError::Timeout);
            }
        }
    }

    /// Try to evict lower-priority claims to make room for `incoming`.
    /// Returns true if at least one claim was evicted.
    fn try_evict_for(
        &self,
        nodes: &mut HashMap<ResourceId, ResourceNode>,
        claims: &mut Vec<ResourceClaim>,
        incoming: &ResourceClaim,
    ) -> bool {
        // Figure out which resources are overcommitted
        let constrained: Vec<ResourceId> = incoming.requirements.keys().cloned().collect();

        // Sort claim indices by priority (lowest first), then age (oldest first)
        let mut candidates: Vec<usize> = (0..claims.len()).collect();
        candidates.sort_by_key(|&i| {
            let c = &claims[i];
            (c.priority, c.created_at)
        });

        let mut evicted_indices: Vec<usize> = Vec::new();
        for &idx in &candidates {
            if evicted_indices.len() > 10 {
                break;
            }
            let claim = &claims[idx];
            if claim.priority >= incoming.priority {
                continue;
            }

            // Only evict claims that compete for the same resources
            let competes = constrained.iter().any(|res_id| claim.requirements.contains_key(res_id));
            if !competes {
                continue;
            }

            evicted_indices.push(idx);
        }

        if evicted_indices.is_empty() {
            return false;
        }

        // Evict in reverse order to preserve indices
        evicted_indices.sort_by(|a, b| b.cmp(a));
        for idx in evicted_indices {
            let claim = claims.remove(idx);
            info!(
                evicting = %claim.label,
                priority = claim.priority,
                "evicted to make room"
            );
            for (res_id, amount) in &claim.requirements {
                if let Some(node) = nodes.get_mut(res_id) {
                    node.release(*amount);
                }
            }
            if let Some(ref notify) = claim.evict_notify {
                (notify)();
            }
        }

        true
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, thiserror::Error)]
pub enum ResourceError {
    #[error("timeout waiting for resources")]
    Timeout,
    #[error("insufficient resources: {0}")]
    Insufficient(String),
    #[error("unknown resource: {0}")]
    UnknownResource(String),
}

// ---------------------------------------------------------------------------
// Convenience: build claim from model estimates
// ---------------------------------------------------------------------------

/// Estimate resource requirements for a model from its performance profile.
pub fn claim_for_model(
    label: impl Into<String>,
    vram_mb: u64,
    ram_mb: u64,
    gpu_index: u32,
) -> ResourceClaim {
    let mut req = HashMap::new();
    if vram_mb > 0 {
        req.insert(ResourceId::GpuVram(gpu_index), vram_mb);
    }
    if ram_mb > 0 {
        req.insert(ResourceId::SystemRam, ram_mb);
    }
    ResourceClaim::new(label, req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_reservation() {
        let mut nodes = HashMap::new();
        nodes.insert(ResourceId::SystemRam, ResourceNode::new("RAM", 16384));
        nodes.insert(ResourceId::GpuVram(0), ResourceNode::new("GPU0", 4096));
        let mgr = ResourceManager::new(nodes);

        // Should fit
        let claim = claim_for_model("test-model", 2048, 4096, 0);
        assert!(mgr.try_reserve(&claim).is_ok());

        // Check usage
        let snap = mgr.snapshot();
        assert_eq!(snap.get(&ResourceId::GpuVram(0)).unwrap().used_mb, 2048);
        assert_eq!(snap.get(&ResourceId::SystemRam).unwrap().used_mb, 4096);

        // Release
        mgr.release(&claim);
        let snap = mgr.snapshot();
        assert_eq!(snap.get(&ResourceId::GpuVram(0)).unwrap().used_mb, 0);
    }

    #[test]
    fn test_insufficient() {
        let mut nodes = HashMap::new();
        nodes.insert(ResourceId::GpuVram(0), ResourceNode::new("GPU0", 1024));
        let mgr = ResourceManager::new(nodes);

        let claim = claim_for_model("too-big", 2048, 0, 0);
        let result = mgr.try_reserve(&claim);
        assert!(result.is_err());
    }

    #[test]
    fn test_multi_gpu_claim() {
        let mut nodes = HashMap::new();
        nodes.insert(ResourceId::GpuVram(0), ResourceNode::new("GPU0", 2048));
        nodes.insert(ResourceId::GpuVram(1), ResourceNode::new("GPU1", 4096));
        nodes.insert(ResourceId::SystemRam, ResourceNode::new("RAM", 8192));
        let mgr = ResourceManager::new(nodes);

        let mut req = HashMap::new();
        req.insert(ResourceId::GpuVram(0), 1024);
        req.insert(ResourceId::GpuVram(1), 2048);
        req.insert(ResourceId::SystemRam, 4096);
        let claim = ResourceClaim::new("multi-gpu-model", req);

        assert!(mgr.try_reserve(&claim).is_ok());

        let snap = mgr.snapshot();
        assert_eq!(snap.get(&ResourceId::GpuVram(0)).unwrap().used_mb, 1024);
        assert_eq!(snap.get(&ResourceId::GpuVram(1)).unwrap().used_mb, 2048);
        assert_eq!(snap.get(&ResourceId::SystemRam).unwrap().used_mb, 4096);

        mgr.release(&claim);
    }

    #[test]
    fn test_blocking_timeout() {
        let mut nodes = HashMap::new();
        nodes.insert(ResourceId::GpuVram(0), ResourceNode::new("GPU0", 1024));
        let mgr = ResourceManager::new(nodes);

        // Fill GPU
        let claim1 = claim_for_model("filler", 1024, 0, 0);
        assert!(mgr.try_reserve(&claim1).is_ok());

        // Try to claim more than available — should timeout
        let claim2 = claim_for_model("requester", 512, 0, 0);
        let result = mgr.reserve_blocking(claim2, Some(Duration::from_millis(100)));
        assert!(matches!(result, Err(ResourceError::Timeout)));
    }

    #[test]
    fn test_release_frees_space() {
        let mut nodes = HashMap::new();
        nodes.insert(ResourceId::GpuVram(0), ResourceNode::new("GPU0", 1024));
        let mgr = ResourceManager::new(nodes);

        let claim = claim_for_model("model", 1024, 0, 0);
        assert!(mgr.try_reserve(&claim).is_ok());

        mgr.release(&claim);

        // Should be able to reserve again
        let claim2 = claim_for_model("model2", 1024, 0, 0);
        assert!(mgr.try_reserve(&claim2).is_ok());
    }

    #[test]
    fn test_from_hardware() {
        // Just check it doesn't panic
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mgr = ResourceManager::from_hardware().await;
            let snap = mgr.snapshot();
            assert!(!snap.is_empty());
        });
    }
}
