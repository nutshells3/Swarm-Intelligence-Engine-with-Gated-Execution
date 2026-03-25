//! Git worktree manager for task isolation.
//!
//! Provides real git worktree operations: create, remove, list, status check,
//! and diff retrieval. Each task gets its own worktree so workers do not
//! stomp each other's files.

use std::path::PathBuf;
use tokio::process::Command;

/// Manages git worktrees for task isolation.
///
/// Each task gets a dedicated worktree under `<repo_root>/.worktrees/task-<id>`,
/// with its own branch. This prevents file contention between concurrent workers.
pub struct WorktreeManager {
    /// Absolute path to the repository root.
    pub repo_root: PathBuf,
    /// Directory where worktrees are created (e.g. `.worktrees/`).
    pub worktree_dir: PathBuf,
}

impl WorktreeManager {
    /// Create a new WorktreeManager for the given repository root.
    ///
    /// Worktrees will be placed under `<repo_root>/.worktrees/`.
    pub fn new(repo_root: PathBuf) -> Self {
        let worktree_dir = repo_root.join(".worktrees");
        Self {
            repo_root,
            worktree_dir,
        }
    }

    /// Create an isolated worktree for a task.
    ///
    /// Runs `git worktree add .worktrees/task-<id> -b task-<id>` from the
    /// repository root. Creates the `.worktrees` directory if it does not exist.
    ///
    /// Returns the absolute path to the new worktree.
    pub async fn create_worktree(&self, task_id: &str) -> Result<PathBuf, WorktreeError> {
        let branch_name = format!("task-{}", task_id);
        let worktree_path = self.worktree_dir.join(&branch_name);

        // Ensure .worktrees directory exists
        tokio::fs::create_dir_all(&self.worktree_dir)
            .await
            .map_err(|e| WorktreeError::IoError(e.to_string()))?;

        let worktree_str = worktree_path
            .to_str()
            .ok_or_else(|| WorktreeError::IoError("Non-UTF-8 worktree path".to_string()))?;

        // Clean up stale branch/worktree from previous attempts before creating.
        // This handles retry scenarios where the branch already exists.
        if worktree_path.exists() {
            let _ = Command::new("git")
                .args(["worktree", "remove", worktree_str, "--force"])
                .current_dir(&self.repo_root)
                .output()
                .await;
        }
        let _ = Command::new("git")
            .args(["branch", "-D", &branch_name])
            .current_dir(&self.repo_root)
            .output()
            .await;

        // git worktree add .worktrees/task-{id} -b task-{id}
        let output = Command::new("git")
            .args(["worktree", "add", worktree_str, "-b", &branch_name])
            .current_dir(&self.repo_root)
            .output()
            .await
            .map_err(|e| WorktreeError::GitError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        tracing::info!(
            task_id,
            path = %worktree_path.display(),
            "Worktree created"
        );
        Ok(worktree_path)
    }

    /// Remove a worktree after task completion.
    ///
    /// Runs `git worktree remove --force` and then deletes the task branch.
    /// Logs a warning (but does not error) if the removal fails.
    pub async fn remove_worktree(&self, task_id: &str) -> Result<(), WorktreeError> {
        let branch_name = format!("task-{}", task_id);
        let worktree_path = self.worktree_dir.join(&branch_name);

        let worktree_str = worktree_path
            .to_str()
            .ok_or_else(|| WorktreeError::IoError("Non-UTF-8 worktree path".to_string()))?;

        // git worktree remove .worktrees/task-{id} --force
        let output = Command::new("git")
            .args(["worktree", "remove", worktree_str, "--force"])
            .current_dir(&self.repo_root)
            .output()
            .await
            .map_err(|e| WorktreeError::GitError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(task_id, error = %stderr, "Worktree removal failed");
        }

        // Delete the branch (best-effort)
        let _ = Command::new("git")
            .args(["branch", "-D", &branch_name])
            .current_dir(&self.repo_root)
            .output()
            .await;

        tracing::info!(task_id, "Worktree removed");
        Ok(())
    }

    /// List active worktrees by parsing `git worktree list --porcelain`.
    ///
    /// Returns a `WorktreeInfo` for each worktree known to git.
    pub async fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>, WorktreeError> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
            .await
            .map_err(|e| WorktreeError::GitError(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();

        // Porcelain format: blocks separated by blank lines.
        // Each block has lines like:
        //   worktree /path/to/wt
        //   HEAD <sha>
        //   branch refs/heads/<name>
        let mut current_path: Option<PathBuf> = None;
        let mut current_head = String::new();
        let mut current_branch = String::new();

        for line in stdout.lines() {
            if line.is_empty() {
                // End of block -- push if we have a path
                if let Some(path) = current_path.take() {
                    worktrees.push(WorktreeInfo {
                        path,
                        branch: current_branch.clone(),
                        head: current_head.clone(),
                    });
                }
                current_head.clear();
                current_branch.clear();
            } else if let Some(rest) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(rest));
            } else if let Some(rest) = line.strip_prefix("HEAD ") {
                current_head = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("branch ") {
                // Strip refs/heads/ prefix
                current_branch = rest
                    .strip_prefix("refs/heads/")
                    .unwrap_or(rest)
                    .to_string();
            }
        }

        // Flush the last block if the output did not end with a blank line
        if let Some(path) = current_path.take() {
            worktrees.push(WorktreeInfo {
                path,
                branch: current_branch,
                head: current_head,
            });
        }

        Ok(worktrees)
    }

    /// Check if a worktree has uncommitted changes.
    ///
    /// Runs `git status --porcelain` inside the worktree directory. Returns
    /// `true` if there is any output (i.e. the worktree is dirty).
    pub async fn is_dirty(&self, task_id: &str) -> Result<bool, WorktreeError> {
        let worktree_path = self.worktree_dir.join(format!("task-{}", task_id));

        if !worktree_path.exists() {
            return Err(WorktreeError::NotFound(format!(
                "Worktree not found for task {}",
                task_id
            )));
        }

        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&worktree_path)
            .output()
            .await
            .map_err(|e| WorktreeError::GitError(e.to_string()))?;

        Ok(!output.stdout.is_empty())
    }

    /// Get the diff of changes in a worktree relative to HEAD.
    ///
    /// Runs `git diff HEAD` inside the worktree directory and returns
    /// the unified diff as a string.
    pub async fn get_diff(&self, task_id: &str) -> Result<String, WorktreeError> {
        let worktree_path = self.worktree_dir.join(format!("task-{}", task_id));

        if !worktree_path.exists() {
            return Err(WorktreeError::NotFound(format!(
                "Worktree not found for task {}",
                task_id
            )));
        }

        let output = Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(&worktree_path)
            .output()
            .await
            .map_err(|e| WorktreeError::GitError(e.to_string()))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check whether a worktree exists on disk for a given task.
    pub fn worktree_exists(&self, task_id: &str) -> bool {
        let worktree_path = self.worktree_dir.join(format!("task-{}", task_id));
        worktree_path.exists()
    }
}

/// Information about a single git worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch the worktree is on (without `refs/heads/` prefix).
    pub branch: String,
    /// HEAD commit SHA.
    pub head: String,
}

/// Errors that can occur during worktree operations.
#[derive(Debug)]
pub enum WorktreeError {
    /// A git command failed.
    GitError(String),
    /// A filesystem I/O operation failed.
    IoError(String),
    /// The requested worktree was not found.
    NotFound(String),
}

impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorktreeError::GitError(msg) => write!(f, "git error: {}", msg),
            WorktreeError::IoError(msg) => write!(f, "I/O error: {}", msg),
            WorktreeError::NotFound(msg) => write!(f, "not found: {}", msg),
        }
    }
}

impl std::error::Error for WorktreeError {}
