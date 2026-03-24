use async_trait::async_trait;
use std::path::PathBuf;

#[async_trait]
pub trait WorkerIsolation: Send + Sync {
    /// Acquire an isolated workspace for a task. Returns the working directory path.
    async fn acquire(&self, task_id: &str) -> Result<PathBuf, IsolationError>;

    /// Release the workspace after task completion.
    async fn release(&self, task_id: &str) -> Result<(), IsolationError>;

    /// Check if the workspace has uncommitted changes.
    async fn is_dirty(&self, task_id: &str) -> Result<bool, IsolationError>;

    /// Get the diff of changes in the workspace.
    async fn get_diff(&self, task_id: &str) -> Result<String, IsolationError>;
}

#[derive(Debug, thiserror::Error)]
pub enum IsolationError {
    #[error("git error: {0}")]
    Git(String),
    #[error("pool exhausted: all {0} slots in use")]
    PoolExhausted(usize),
    #[error("container error: {0}")]
    Container(String),
}
