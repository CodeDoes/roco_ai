//! Master framework: every domain uses HarnessConfig + DomainHarness.
pub trait DomainHarness {
    fn name(&self) -> &str;
    fn init(&mut self, cfg: HarnessConfig);
    fn run(&self, input: &str) -> String;
    fn verify(&self, output: &str) -> bool;
    fn rollback(&self);
}

pub struct HarnessConfig {
    pub model_path: String,
    pub workspace_dir: String,
    pub use_local_inferd: bool,
    pub max_retries: u32,
    pub strict_grammar: bool,
}
