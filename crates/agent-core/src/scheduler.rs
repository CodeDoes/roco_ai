//! Scheduled tasks — deferred and periodic (cron-like) agent work.
//!
//! A [`Scheduler`] holds one-off and periodic tasks, each with a `next_run`
//! time. The host calls [`Scheduler::due`] to see what's pending and
//! [`Scheduler::run_due`] to execute due tasks against the backend (one-off
//! tasks are removed; periodic tasks are rescheduled by their interval).
//! The model schedules work via the `schedule` tool. Time is injectable, so
//! the whole thing is testable with a fake clock (no real waiting).
//! Satisfies `goals/agent/scheduled_tasks.md`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use roco_engine::{CompletionRequest, ModelBackend};
use roco_tools::{Tool, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AgentError;

static SCHED_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_sched_id() -> String {
    let c = SCHED_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("task-{}-{:x}", crate::memory::now_secs(), c)
}

/// A single scheduled task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub description: String,
    /// Optional free-form payload carried with the task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// `one_off` or `periodic`.
    pub kind: String,
    /// Unix seconds at which this task is next due.
    pub next_run: u64,
    /// For `periodic` tasks, the repeat interval in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<u64>,
}

/// The outcome of running one due task.
#[derive(Debug, Clone)]
pub struct ScheduledOutcome {
    pub task_id: String,
    pub description: String,
    pub output: String,
}

/// A scheduler for deferred / periodic tasks.
pub struct Scheduler {
    tasks: RwLock<Vec<ScheduledTask>>,
    path: RwLock<Option<PathBuf>>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    /// Scheduler using the real wall clock.
    pub fn new() -> Self {
        Self::with_clock(crate::memory::now_secs)
    }

    /// Scheduler using an injectable clock (for testing with a fake clock).
    pub fn with_clock(now: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        Self {
            tasks: RwLock::new(Vec::new()),
            path: RwLock::new(None),
            clock: Arc::new(now),
        }
    }

    /// Open a persistent scheduler at `path`, loading existing tasks if present.
    pub fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let store = Self::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            if !text.trim().is_empty() {
                let loaded: Vec<ScheduledTask> = serde_json::from_str(&text)?;
                *store.tasks.write().expect("sched lock poisoned") = loaded;
            }
        }
        *store.path.write().expect("sched path lock poisoned") = Some(path);
        Ok(store)
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Schedule a one-off task due at unix time `at`.
    pub fn schedule_one_off(&self, description: &str, at: u64, payload: Option<Value>) -> String {
        let id = new_sched_id();
        self.tasks
            .write()
            .expect("sched lock poisoned")
            .push(ScheduledTask {
                id: id.clone(),
                description: description.to_string(),
                payload,
                kind: "one_off".into(),
                next_run: at,
                interval: None,
            });
        let _ = self.save();
        id
    }

    /// Schedule a periodic task that first runs at `start` (or now) and then
    /// every `interval` seconds.
    pub fn schedule_periodic(
        &self,
        description: &str,
        interval: u64,
        start: Option<u64>,
        payload: Option<Value>,
    ) -> String {
        let next_run = start.unwrap_or_else(|| self.now());
        let id = new_sched_id();
        self.tasks
            .write()
            .expect("sched lock poisoned")
            .push(ScheduledTask {
                id: id.clone(),
                description: description.to_string(),
                payload,
                kind: "periodic".into(),
                next_run,
                interval: Some(interval),
            });
        let _ = self.save();
        id
    }

    /// Tasks whose `next_run` is at or before the current time.
    pub fn due(&self) -> Vec<ScheduledTask> {
        let now = self.now();
        self.tasks
            .read()
            .expect("sched lock poisoned")
            .iter()
            .filter(|t| t.next_run <= now)
            .cloned()
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<ScheduledTask> {
        self.tasks
            .read()
            .expect("sched lock poisoned")
            .iter()
            .find(|t| t.id == id)
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.tasks.read().expect("sched lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn remove(&self, id: &str) {
        self.tasks
            .write()
            .expect("sched lock poisoned")
            .retain(|t| t.id != id);
    }

    fn set_next_run(&self, id: &str, next_run: u64) {
        let mut tasks = self.tasks.write().expect("sched lock poisoned");
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.next_run = next_run;
        }
    }

    /// Execute all due tasks against `backend`. One-off tasks are removed
    /// after running; periodic tasks are rescheduled by their interval (past
    /// due times are skipped forward so a long-paused scheduler doesn't
    /// replay a backlog).
    pub async fn run_due(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<Vec<ScheduledOutcome>, AgentError> {
        let due = self.due();
        let mut outcomes = Vec::with_capacity(due.len());
        for task in due {
            let req = CompletionRequest::new(
                "You are executing a scheduled task. Produce only the result.",
                task.description.clone(),
            );
            let resp = backend
                .complete(req)
                .await
                .map_err(|e| AgentError::BackendError(e.to_string()))?;
            outcomes.push(ScheduledOutcome {
                task_id: task.id.clone(),
                description: task.description.clone(),
                output: resp.text.clone(),
            });
            if task.kind == "periodic" {
                if let Some(iv) = task.interval {
                    let iv = iv.max(1);
                    let mut next = task.next_run + iv;
                    let now = self.now();
                    while next <= now {
                        next += iv;
                    }
                    self.set_next_run(&task.id, next);
                } else {
                    self.remove(&task.id);
                }
            } else {
                self.remove(&task.id);
            }
        }
        let _ = self.save();
        Ok(outcomes)
    }

    /// Persist tasks to `path`, if configured.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self.path.read().expect("sched path lock poisoned");
        if let Some(p) = path.as_ref() {
            let tasks = self.tasks.read().expect("sched lock poisoned");
            let text = serde_json::to_string_pretty(&*tasks)?;
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(p, text)?;
        }
        Ok(())
    }

    /// The `schedule` tool bound to this scheduler.
    pub fn scoped_tools(scheduler: Arc<Scheduler>) -> Vec<Arc<dyn Tool>> {
        vec![Arc::new(ScheduleTool { scheduler })]
    }
}

/// Tool that lets the model schedule deferred / periodic tasks.
pub struct ScheduleTool {
    pub(crate) scheduler: Arc<Scheduler>,
}

impl Tool for ScheduleTool {
    fn name(&self) -> &str {
        "schedule"
    }
    fn description(&self) -> &str {
        "Schedule work for later. Provide `description`; `delay_seconds` (deferred) or \
         `interval_seconds` (repeating). Optional `start_at` overrides the first run time \
         (unix seconds)."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {"type": "string", "description": "What to do when the task is due"},
                "delay_seconds": {"type": "integer", "description": "Run once, this many seconds from now"},
                "interval_seconds": {"type": "integer", "description": "Repeat every this many seconds"},
                "start_at": {"type": "integer", "description": "Unix seconds for the first run"}
            },
            "required": ["description"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'description' argument".into()))?;
        let now = self.scheduler.now();

        if let Some(iv) = args.get("interval_seconds").and_then(|v| v.as_u64()) {
            let start = args.get("start_at").and_then(|v| v.as_u64()).unwrap_or(now);
            let id = self
                .scheduler
                .schedule_periodic(description, iv, Some(start), None);
            return Ok(serde_json::json!({
                "ok": true, "id": id, "kind": "periodic", "next_run": start, "interval_seconds": iv
            }));
        }

        let at = args
            .get("start_at")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                args.get("delay_seconds")
                    .and_then(|v| v.as_u64())
                    .map(|d| now + d)
            })
            .unwrap_or(now);
        let id = self.scheduler.schedule_one_off(description, at, None);
        Ok(serde_json::json!({ "ok": true, "id": id, "kind": "one_off", "next_run": at }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;
    use std::sync::atomic::AtomicU64;

    /// A scheduler backed by a mutable fake clock.
    fn fake_scheduler(time: Arc<AtomicU64>) -> Scheduler {
        Scheduler::with_clock({
            let t = time.clone();
            move || t.load(Ordering::Relaxed)
        })
    }

    #[tokio::test]
    async fn one_off_runs_once_then_removed() {
        let time = Arc::new(AtomicU64::new(1000));
        let sched = fake_scheduler(time.clone());
        let id = sched.schedule_one_off("say hi", 1000, None);
        assert_eq!(sched.due().len(), 1);
        let outcomes = sched.run_due(&MockBackend::default()).await.unwrap();
        assert_eq!(outcomes.len(), 1);
        assert!(
            sched.get(&id).is_none(),
            "one-off task removed after running"
        );
    }

    #[tokio::test]
    async fn periodic_reschedules_after_run() {
        let time = Arc::new(AtomicU64::new(1000));
        let sched = fake_scheduler(time.clone());
        let id = sched.schedule_periodic("tick", 100, None, None);
        assert_eq!(sched.due().len(), 1);
        sched.run_due(&MockBackend::default()).await.unwrap();
        let t = sched.get(&id).unwrap();
        assert!(
            t.next_run > 1000,
            "periodic task rescheduled into the future"
        );
        assert_eq!(sched.due().len(), 0, "not due immediately after reschedule");
        time.store(2000, Ordering::Relaxed);
        assert_eq!(sched.due().len(), 1, "due again after interval elapses");
    }

    #[tokio::test]
    async fn due_only_when_time_reaches_next_run() {
        let time = Arc::new(AtomicU64::new(1000));
        let sched = fake_scheduler(time.clone());
        sched.schedule_one_off("later", 5000, None);
        assert!(sched.due().is_empty(), "not due before next_run");
        time.store(5000, Ordering::Relaxed);
        assert_eq!(sched.due().len(), 1, "due at next_run");
    }

    #[test]
    fn schedule_tool_adds_task() {
        let time = Arc::new(AtomicU64::new(1000));
        let sched = Arc::new(fake_scheduler(time.clone()));
        let tools = Scheduler::scoped_tools(sched.clone());
        let tool = tools.iter().find(|t| t.name() == "schedule").unwrap();
        let r = tool
            .call(serde_json::json!({ "description": "remind me", "delay_seconds": 60 }))
            .unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(sched.len(), 1);
        assert_eq!(
            sched.get(r["id"].as_str().unwrap()).unwrap().kind,
            "one_off"
        );
    }

    #[test]
    fn schedule_tool_periodic() {
        let time = Arc::new(AtomicU64::new(1000));
        let sched = Arc::new(fake_scheduler(time.clone()));
        let tools = Scheduler::scoped_tools(sched.clone());
        let tool = tools.iter().find(|t| t.name() == "schedule").unwrap();
        let r = tool
            .call(serde_json::json!({ "description": "heartbeat", "interval_seconds": 3600 }))
            .unwrap();
        assert_eq!(r["kind"], "periodic");
        assert_eq!(
            sched.get(r["id"].as_str().unwrap()).unwrap().interval,
            Some(3600)
        );
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "roco-sched-test-{}.json",
            crate::memory::now_secs()
        ));
        {
            let sched = Scheduler::open(&dir).unwrap();
            sched.schedule_one_off("persisted task", 9999, None);
            sched.save().unwrap();
        }
        {
            let sched = Scheduler::open(&dir).unwrap();
            assert_eq!(sched.len(), 1);
            assert_eq!(
                sched
                    .get(&sched.due().first().unwrap().id)
                    .unwrap()
                    .description,
                "persisted task"
            );
        }
        let _ = std::fs::remove_file(&dir);
    }
}
