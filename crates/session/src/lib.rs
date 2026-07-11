use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::Result;

use roco_core::engine::ModelBackend;
use roco_core::agent::{Orchestrator, ContextBudget, RetryPolicy, Task, ChecklistVerifier};
use roco_workspace::Workspace;
use roco_core::sandbox::Sandbox;
use roco_core::policy::ComposedPolicy;
use roco_core::builtins::default_agent_toolkit;

pub mod store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

pub struct Engine {
    orchestrator: Arc<Orchestrator<dyn ModelBackend + Send + Sync, ChecklistVerifier>>,
    pub workspace: Workspace,
    messages: Arc<Mutex<Vec<Message>>>,
    stream: Arc<Mutex<String>>,
    events: Arc<Mutex<Vec<String>>>,
    finished_tx: watch::Sender<bool>,
}

impl Engine {
    pub fn new(backend: Arc<dyn ModelBackend + Send + Sync>, workspace: Workspace) -> Self {
        // Setup orchestrator with tooling bound to workspace
        let budget = ContextBudget::default();
        let toolkit = default_agent_toolkit(workspace.root.clone(), Sandbox::new());
        
        let mut orch = Orchestrator::new(
            backend,
            budget,
            ChecklistVerifier,
            RetryPolicy::default(),
        );
        
        orch = orch.with_tooling(
            Arc::new(toolkit),
            Arc::new(Sandbox::new()),
            Arc::new(ComposedPolicy::new()),
        );

        let (finished_tx, _) = watch::channel(false);

        Self {
            orchestrator: Arc::new(orch),
            workspace,
            messages: Arc::new(Mutex::new(Vec::new())),
            stream: Arc::new(Mutex::new(String::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            finished_tx,
        }
    }

    pub fn queue_message(&self, role: &str, content: &str) {
        let mut msgs = self.messages.lock().unwrap();
        msgs.push(Message {
            role: role.to_string(),
            content: content.to_string(),
        });
    }

    /// Restore a set of messages into the engine (used when resuming a session).
    pub fn restore_messages(&self, messages: Vec<Message>) {
        let mut msgs = self.messages.lock().unwrap();
        *msgs = messages;
    }

    /// Get a snapshot of all messages (owned copy).
    pub fn message_snapshot(&self) -> Vec<Message> {
        self.messages.lock().unwrap().clone()
    }

    pub async fn poll(&self) -> Result<()> {
        let msgs = self.messages.lock().unwrap().clone();
        let last_msg = msgs.last().ok_or_else(|| anyhow::anyhow!("no messages"))?;

        // Map the last user message to a Task
        let task = Task {
            id: format!("step-{}", msgs.len()),
            objective: last_msg.content.clone(),
            context: msgs.iter().map(|m| format!("{}: {}", m.role, m.content)).collect::<Vec<_>>().join("\n"),
            output_schema: r#"{"result": "string"}"#.into(),
            allow_abstain: true,
        };

        self.events.lock().unwrap().push(format!("Running task {}", task.id));
        
        let result = self.orchestrator.run(&task).await?;
        
        // Log the answer back to messages — try common keys, then fall back to raw text
        let answer = result.outputs.first()
            .and_then(|v| {
                v.get("result").or_else(|| v.get("answer"))
                    .or_else(|| v.get("text"))
                    .or_else(|| v.get("output"))
            })
            .and_then(|a| a.as_str())
            .or_else(|| {
                // Try the raw field or the parsed text
                result.outputs.first().and_then(|v| v.as_str())
            })
            .unwrap_or("The task ran but produced no structured output.");

        self.messages.lock().unwrap().push(Message {
            role: "assistant".to_string(),
            content: answer.to_string(),
        });

        self.stream.lock().unwrap().push_str(answer);
        self.events.lock().unwrap().push(format!("Task {} finished", task.id));
        
        // Mark as finished for this turn
        let _ = self.finished_tx.send(true);
        
        Ok(())
    }

    pub fn messages(&self) -> Value {
        let msgs = self.messages.lock().unwrap();
        serde_json::json!(msgs.iter().map(|m| {
            serde_json::json!({ "role": m.role, "content": m.content })
        }).collect::<Vec<_>>())
    }

    pub fn stream(&self) -> String {
        self.stream.lock().unwrap().clone()
    }

    pub fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    pub async fn wait_until_finished(&self) {
        let mut rx = self.finished_tx.subscribe();
        let _ = rx.changed().await;
    }
}
