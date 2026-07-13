//! Self-regulation / safety policy layer for RoCo AI agents.
//!
//! A [`Policy`] inspects a planned [`Action`] (tool call, shell command,
//! delegation, response, abstain, escalate) and returns a [`PolicyVerdict`].
//! This is the decision gate the orchestrator consults *before* executing an
//! action — overlapping with the harness evals `policy_follow` and
//! `sandbox_guard_intercept_handling`. Policies compose: a [`ComposedPolicy`]
//! denies if any member denies, and escalates to human review if any member
//! asks for review but none denies.
//!
//! This module wires the [`crate::sandbox`] guard and the [`crate::tools`]
//! registry into one coherent safety layer.

use crate::sandbox::{GuardPolicy, GuardVerdict};

/// A planned agent action to be vetted by policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    CallTool { name: String, input: String },
    RunCommand { command: String },
    Delegate { subtask: String },
    Respond { text: String },
    Abstain,
    Escalate,
}

/// Verdict returned by a policy for a given action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyVerdict {
    Allow,
    Deny(String),
    /// Action is permitted only after human confirmation.
    Review(String),
}

/// A decision policy over [`Action`]s.
pub trait Policy: Send + Sync {
    fn evaluate(&self, action: &Action) -> PolicyVerdict;
}

/// Applies a sandbox [`GuardPolicy`] to `RunCommand` actions. Other action
/// kinds are unaffected (the sandbox only governs shell execution).
pub struct SandboxGuardPolicy {
    guard: GuardPolicy,
}

impl SandboxGuardPolicy {
    pub fn new(guard: GuardPolicy) -> Self {
        Self { guard }
    }
}

impl Policy for SandboxGuardPolicy {
    fn evaluate(&self, action: &Action) -> PolicyVerdict {
        if let Action::RunCommand { command } = action {
            match self.guard.check(command) {
                GuardVerdict::Allow => PolicyVerdict::Allow,
                GuardVerdict::Deny(reason) => PolicyVerdict::Deny(reason),
            }
        } else {
            PolicyVerdict::Allow
        }
    }
}

/// Only allows tool calls whose name is in the allowlist.
pub struct ToolAllowListPolicy {
    allowed: Vec<String>,
}

impl ToolAllowListPolicy {
    pub fn new(allowed: Vec<String>) -> Self {
        Self { allowed }
    }
}

impl Policy for ToolAllowListPolicy {
    fn evaluate(&self, action: &Action) -> PolicyVerdict {
        if let Action::CallTool { name, .. } = action {
            if self.allowed.iter().any(|a| a == name) {
                PolicyVerdict::Allow
            } else {
                PolicyVerdict::Deny(format!("tool '{name}' is not permitted by policy"))
            }
        } else {
            PolicyVerdict::Allow
        }
    }
}

/// Treats risky actions (shell commands, delegation) as needing human review
/// unless `auto_approve` is set.
pub struct HumanInTheLoopPolicy {
    auto_approve: bool,
}

impl HumanInTheLoopPolicy {
    pub fn new(auto_approve: bool) -> Self {
        Self { auto_approve }
    }
}

impl Policy for HumanInTheLoopPolicy {
    fn evaluate(&self, action: &Action) -> PolicyVerdict {
        let risky = matches!(action, Action::RunCommand { .. } | Action::Delegate { .. });
        if !risky {
            return PolicyVerdict::Allow;
        }
        if self.auto_approve {
            PolicyVerdict::Allow
        } else {
            PolicyVerdict::Review("risky action requires confirmation".to_string())
        }
    }
}

/// Combines policies. Precedence: `Deny` > `Review` > `Allow`.
pub struct ComposedPolicy {
    policies: Vec<Box<dyn Policy>>,
}

impl Default for ComposedPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposedPolicy {
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
        }
    }

    pub fn with(mut self, policy: Box<dyn Policy>) -> Self {
        self.policies.push(policy);
        self
    }
}

impl Policy for ComposedPolicy {
    fn evaluate(&self, action: &Action) -> PolicyVerdict {
        let mut review: Option<String> = None;
        for p in &self.policies {
            match p.evaluate(action) {
                PolicyVerdict::Deny(reason) => return PolicyVerdict::Deny(reason),
                PolicyVerdict::Review(reason) => {
                    if review.is_none() {
                        review = Some(reason);
                    }
                }
                PolicyVerdict::Allow => {}
            }
        }
        match review {
            Some(reason) => PolicyVerdict::Review(reason),
            None => PolicyVerdict::Allow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_guard_denies_blocked_command() {
        let p = SandboxGuardPolicy::new(GuardPolicy::DenyList(vec!["rm -rf /".to_string()]));
        let verdict = p.evaluate(&Action::RunCommand {
            command: "rm -rf /".to_string(),
        });
        assert!(matches!(verdict, PolicyVerdict::Deny(_)));
    }

    #[test]
    fn sandbox_guard_allows_safe_command() {
        let p = SandboxGuardPolicy::new(GuardPolicy::DenyList(vec!["rm -rf /".to_string()]));
        let verdict = p.evaluate(&Action::RunCommand {
            command: "ls -la".to_string(),
        });
        assert_eq!(verdict, PolicyVerdict::Allow);
    }

    #[test]
    fn tool_allowlist_blocks_unlisted_tool() {
        let p = ToolAllowListPolicy::new(vec!["read".to_string(), "write".to_string()]);
        let blocked = p.evaluate(&Action::CallTool {
            name: "bash".to_string(),
            input: "{}".to_string(),
        });
        assert!(matches!(blocked, PolicyVerdict::Deny(_)));
        let allowed = p.evaluate(&Action::CallTool {
            name: "read".to_string(),
            input: "{}".to_string(),
        });
        assert_eq!(allowed, PolicyVerdict::Allow);
    }

    #[test]
    fn human_in_the_loop_reviews_risky_unless_auto_approve() {
        let review = HumanInTheLoopPolicy::new(false).evaluate(&Action::RunCommand {
            command: "echo hi".to_string(),
        });
        assert!(matches!(review, PolicyVerdict::Review(_)));

        let allow = HumanInTheLoopPolicy::new(true).evaluate(&Action::RunCommand {
            command: "echo hi".to_string(),
        });
        assert_eq!(allow, PolicyVerdict::Allow);

        // Non-risky actions are allowed regardless.
        let safe = HumanInTheLoopPolicy::new(false).evaluate(&Action::Respond {
            text: "done".to_string(),
        });
        assert_eq!(safe, PolicyVerdict::Allow);
    }

    #[test]
    fn composed_policy_denies_over_reviews() {
        let policy = ComposedPolicy::new()
            .with(Box::new(HumanInTheLoopPolicy::new(false)))
            .with(Box::new(SandboxGuardPolicy::new(GuardPolicy::DenyList(
                vec!["mkfs".to_string()],
            ))));

        // mkfs is denied by the sandbox guard -> hard deny, even though the
        // HITL policy would only review.
        let denied = policy.evaluate(&Action::RunCommand {
            command: "mkfs.ext4 /dev/sda".to_string(),
        });
        assert!(matches!(denied, PolicyVerdict::Deny(_)));

        // echo is only reviewed by HITL -> Review (no deny present).
        let reviewed = policy.evaluate(&Action::RunCommand {
            command: "echo hi".to_string(),
        });
        assert!(matches!(reviewed, PolicyVerdict::Review(_)));

        // A safe response -> Allow.
        let allowed = policy.evaluate(&Action::Respond {
            text: "ok".to_string(),
        });
        assert_eq!(allowed, PolicyVerdict::Allow);
    }
}
