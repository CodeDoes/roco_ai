use anyhow::{Context, Result};
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// A simple logger that writes agent interactions to the workspace.
pub struct Logger {
    root: PathBuf,
}

impl Logger {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Write a piece of data to a JSONL file.
    pub fn jsonl(&self, filename: &str, _key: &str, data: &Value) -> Result<()> {
        let path = self.root.join(filename);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("failed to open jsonl file")?;

        let line = serde_json::to_string(data)?;
        writeln!(file, "{}", line).context("failed to write jsonl line")?;
        Ok(())
    }

    /// Write a string to a text file.
    pub fn stream(&self, filename: &str, _key: &str, data: &str) -> Result<()> {
        let path = self.root.join(filename);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("failed to open stream file")?;

        write!(file, "{}", data).context("failed to write stream data")?;
        Ok(())
    }

    /// Write a generic log message to a file.
    pub fn log(&self, filename: &str, _key: &str, data: &str) -> Result<()> {
        let path = self.root.join(filename);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("failed to open log file")?;

        writeln!(file, "{}", data).context("failed to write log line")?;
        Ok(())
    }
}
