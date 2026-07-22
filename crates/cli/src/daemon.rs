//! Re-export of the shared daemon lifecycle from `roco_app`.
//!
//! The actual implementation lives in `roco_app::daemon` so that every
//! human-facing surface (`cli`, `tui`, `gui`) shares one backend-resolution
//! and daemon-management path. This file exists only so existing
//! `crate::daemon::*` references in the CLI keep resolving.

pub use roco_app::daemon::*;
