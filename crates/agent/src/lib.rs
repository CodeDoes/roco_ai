//! RoCo Agent — facade crate re-exporting [`roco_agent_core`] and
//! [`roco_agent_story`].
//!
//! This crate exists for backward compatibility. New code should depend on
//! `roco-agent-core` (ReAct/plan loop) or `roco-agent-story` (story pipeline)
//! directly.
//!
//! # Re-exports
//!
//! Everything from `roco-agent-core` and `roco-agent-story` is available at
//! this crate's root — so `use roco_agent::StoryEngine` and `use
//! roco_agent::MechaAgent` both work.

pub use roco_agent_core::*;
pub use roco_agent_story::*;

/// Backward-compat re-export of mecha_agent as mechanistic.
#[doc(hidden)]
pub mod mechanistic {
    pub use roco_agent_core::mecha_agent::*;
    /// Backward-compat alias.
    pub type MechanicAgent = MechanisticAgent;
}
