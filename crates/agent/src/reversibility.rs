//! Reversibility — re-exported from `roco_workspace`.
//!
//! Lives in the workspace crate so `roco-app` can depend on timeline/VC
//! without pulling the full agent graph (faster incremental rebuilds when
//! editing agent code).

pub use roco_workspace::{ReversibleAction, Snapshot, SnapshotSummary, VersionControl};
