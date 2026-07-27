#![allow(
    clippy::items_after_test_module,
    clippy::single_match,
    clippy::new_without_default
)]
//! Real unified scaffold — all 11 domains + framework trait + mock backend.
pub use framework::*;
pub use r#loop::ExecutionLoop;
pub use sandbox::Sandbox;
pub use verifier::Verifier;

pub mod aggregate;
pub mod browser;
pub mod chat;
pub mod coding;
pub mod debug;
pub mod email;
pub mod framework;
pub mod full_stack;
pub mod html;
pub mod r#loop;
pub mod organization;
pub mod pet;
pub mod research;
pub mod sandbox;
pub mod use_cases;
pub mod verifier;
pub mod writing;
