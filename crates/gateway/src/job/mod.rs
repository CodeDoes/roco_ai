//! Job queue for background generation tasks.
//!
//! Jobs survive client disconnections. The gateway polls inferd,
//! accumulates tokens, and writes results to the workspace.

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::info;

/// Job lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Job queued but not started.
    Queued,
    /// Currently running on inferd.
    Running,
    /// Completed successfully.
    Completed,
    /// Failed (inferd error, timeout, etc.).
    Failed,
    /// Cancelled by client.
    Cancelled,
}

/// A background generation job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub status: JobStatus,
    pub prompt: String,
    pub grammar: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub result_text: Option<String>,
    pub error_message: Option<String>,
    pub token_count: usize,
}

/// Job manager with background worker.
pub struct JobQueue {
    jobs: RwLock<HashMap<String, Job>>,
    /// Broadcast channel for job status updates.
    tx: broadcast::Sender<JobEvent>,
}

/// Events emitted when a job changes state.
#[derive(Debug, Clone)]
pub enum JobEvent {
    Started { job_id: String },
    Token { job_id: String, token: String },
    Completed { job_id: String },
    Failed { job_id: String, error: String },
    Cancelled { job_id: String },
}

impl JobQueue {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            jobs: RwLock::new(HashMap::new()),
            tx,
        }
    }

    /// Submit a new job. Returns the job ID immediately.
    pub fn submit(&self, session_id: impl Into<String>, workspace_id: impl Into<String>, prompt: impl Into<String>) -> String {
        let id = format!("job-{}", uuid::Uuid::new_v4());
        let job = Job {
            id: id.clone(),
            session_id: session_id.into(),
            workspace_id: workspace_id.into(),
            status: JobStatus::Queued,
            prompt: prompt.into(),
            grammar: None,
            temperature: 0.7,
            max_tokens: 512,
            created_at: now_secs(),
            started_at: None,
            completed_at: None,
            result_text: None,
            error_message: None,
            token_count: 0,
        };
        self.jobs.write().insert(id.clone(), job);
        info!("Submitted job {}", id);
        id
    }

    /// Get a job by ID.
    pub fn get(&self, id: &str) -> Option<Job> {
        self.jobs.read().get(id).cloned()
    }

    /// Update a job.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Job)) -> bool {
        let mut jobs = self.jobs.write();
        if let Some(job) = jobs.get_mut(id) {
            f(job);
            true
        } else {
            false
        }
    }

    /// Start a job (called by the worker).
    pub fn start(&self, id: &str) -> bool {
        let updated = self.update(id, |j| {
            j.status = JobStatus::Running;
            j.started_at = Some(now_secs());
        });
        if updated {
            let _ = self.tx.send(JobEvent::Started { job_id: id.into() });
        }
        updated
    }

    /// Append a token to a running job.
    pub fn append_token(&self, id: &str, token: impl Into<String>) -> bool {
        let token = token.into();
        let updated = self.update(id, |j| {
            if j.result_text.is_none() {
                j.result_text = Some(String::new());
            }
            j.result_text.as_mut().unwrap().push_str(&token);
            j.token_count += 1;
        });
        if updated {
            let _ = self.tx.send(JobEvent::Token {
                job_id: id.into(),
                token,
            });
        }
        updated
    }

    /// Mark a job as completed.
    pub fn complete(&self, id: &str) -> bool {
        let updated = self.update(id, |j| {
            j.status = JobStatus::Completed;
            j.completed_at = Some(now_secs());
        });
        if updated {
            let _ = self.tx.send(JobEvent::Completed { job_id: id.into() });
        }
        updated
    }

    /// Mark a job as failed.
    pub fn fail(&self, id: &str, error: impl Into<String>) -> bool {
        let error = error.into();
        let updated = self.update(id, |j| {
            j.status = JobStatus::Failed;
            j.error_message = Some(error.clone());
            j.completed_at = Some(now_secs());
        });
        if updated {
            let _ = self.tx.send(JobEvent::Failed {
                job_id: id.into(),
                error,
            });
        }
        updated
    }

    /// Cancel a job.
    pub fn cancel(&self, id: &str) -> bool {
        let updated = self.update(id, |j| {
            j.status = JobStatus::Cancelled;
            j.completed_at = Some(now_secs());
        });
        if updated {
            let _ = self.tx.send(JobEvent::Cancelled { job_id: id.into() });
        }
        updated
    }

    /// Subscribe to job events.
    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.tx.subscribe()
    }

    /// List all jobs for a session.
    pub fn list_for_session(&self, session_id: &str) -> Vec<Job> {
        let jobs = self.jobs.read();
        jobs
            .values()
            .filter(|j| j.session_id == session_id)
            .cloned()
            .collect()
    }

    /// Clean up old completed jobs.
    pub fn gc(&self, max_age_secs: u64) {
        let cutoff = now_secs().saturating_sub(max_age_secs);
        let mut jobs = self.jobs.write();
        let to_remove: Vec<String> = jobs
            .iter()
            .filter(|(_, j)| {
                matches!(j.status, JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled)
                    && j.completed_at.map(|t| t < cutoff).unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in to_remove {
            jobs.remove(&id);
            info!("GC'd job {}", id);
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}


