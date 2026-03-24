use git_control::WorktreeManager;

use crate::worker_isolation::{IsolationError, WorkerIsolation};

pub struct WorktreeIsolation {
    manager: WorktreeManager,
}

impl WorktreeIsolation {
    pub fn new(repo_root: std::path::PathBuf) -> Self {
        Self {
            manager: WorktreeManager::new(repo_root),
        }
    }
}

#[async_trait::async_trait]
impl WorkerIsolation for WorktreeIsolation {
    async fn acquire(&self, task_id: &str) -> Result<std::path::PathBuf, IsolationError> {
        self.manager
            .create_worktree(task_id)
            .await
            .map_err(|e| IsolationError::Git(e.to_string()))
    }

    async fn release(&self, task_id: &str) -> Result<(), IsolationError> {
        self.manager
            .remove_worktree(task_id)
            .await
            .map_err(|e| IsolationError::Git(e.to_string()))
    }

    async fn is_dirty(&self, task_id: &str) -> Result<bool, IsolationError> {
        self.manager
            .is_dirty(task_id)
            .await
            .map_err(|e| IsolationError::Git(e.to_string()))
    }

    async fn get_diff(&self, task_id: &str) -> Result<String, IsolationError> {
        self.manager
            .get_diff(task_id)
            .await
            .map_err(|e| IsolationError::Git(e.to_string()))
    }
}
