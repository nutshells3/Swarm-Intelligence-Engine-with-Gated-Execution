use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use tokio::sync::Mutex;

use git_control::WorktreeManager;

use crate::worker_isolation::{IsolationError, WorkerIsolation};

pub struct PooledWorktreeIsolation {
    manager: WorktreeManager,
    pool_size: usize,
    available: Mutex<VecDeque<String>>,
    assignments: Mutex<HashMap<String, String>>, // task_id -> slot_id
}

impl PooledWorktreeIsolation {
    pub async fn new(repo_root: PathBuf, pool_size: usize) -> Result<Self, IsolationError> {
        let manager = WorktreeManager::new(repo_root);
        let mut available = VecDeque::with_capacity(pool_size);

        // Pre-create worktrees
        for i in 0..pool_size {
            let slot_id = format!("pool-slot-{i:04}");
            match manager.create_worktree(&slot_id).await {
                Ok(_) => available.push_back(slot_id),
                Err(e) => {
                    // Worktree might already exist from previous run
                    tracing::debug!(slot_id, error = %e, "Worktree slot already exists, reusing");
                    available.push_back(slot_id);
                }
            }
        }

        tracing::info!(pool_size, "Worktree pool initialized");

        Ok(Self {
            manager,
            pool_size,
            available: Mutex::new(available),
            assignments: Mutex::new(HashMap::new()),
        })
    }
}

#[async_trait::async_trait]
impl WorkerIsolation for PooledWorktreeIsolation {
    async fn acquire(&self, task_id: &str) -> Result<PathBuf, IsolationError> {
        let mut available = self.available.lock().await;
        let slot_id = available
            .pop_front()
            .ok_or(IsolationError::PoolExhausted(self.pool_size))?;

        let path = self.manager.worktree_dir.join(format!("task-{slot_id}"));

        // Reset the worktree to clean state
        let _ = tokio::process::Command::new("git")
            .args(["checkout", ".", "--"])
            .current_dir(&path)
            .output()
            .await;
        let _ = tokio::process::Command::new("git")
            .args(["clean", "-fd"])
            .current_dir(&path)
            .output()
            .await;

        self.assignments
            .lock()
            .await
            .insert(task_id.to_string(), slot_id);
        Ok(path)
    }

    async fn release(&self, task_id: &str) -> Result<(), IsolationError> {
        let mut assignments = self.assignments.lock().await;
        if let Some(slot_id) = assignments.remove(task_id) {
            self.available.lock().await.push_back(slot_id);
        }
        Ok(())
    }

    async fn is_dirty(&self, task_id: &str) -> Result<bool, IsolationError> {
        let assignments = self.assignments.lock().await;
        let slot_id = assignments
            .get(task_id)
            .ok_or_else(|| IsolationError::Git("task not assigned to pool slot".into()))?;
        self.manager
            .is_dirty(slot_id)
            .await
            .map_err(|e| IsolationError::Git(e.to_string()))
    }

    async fn get_diff(&self, task_id: &str) -> Result<String, IsolationError> {
        let assignments = self.assignments.lock().await;
        let slot_id = assignments
            .get(task_id)
            .ok_or_else(|| IsolationError::Git("task not assigned to pool slot".into()))?;
        self.manager
            .get_diff(slot_id)
            .await
            .map_err(|e| IsolationError::Git(e.to_string()))
    }
}
