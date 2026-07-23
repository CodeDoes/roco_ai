//! Real unified scaffold — all 11 domains + framework trait + mock backend.
pub use framework::*;
pub use sandbox::Sandbox;
pub use verifier::Verifier;
pub use r#loop::ExecutionLoop;

pub mod framework;
pub mod writing; pub mod coding; pub mod html; pub mod chat;
pub mod organization; pub mod pet; pub mod debug; pub mod email;
pub mod research; pub mod aggregate; pub mod browser;
pub mod full_stack;
pub mod sandbox;
pub mod verifier;
pub mod r#loop;
pub mod use_cases;
pub mod test_clones;
