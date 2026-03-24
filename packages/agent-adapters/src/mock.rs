//! Mock adapter for demos and testing.
//!
//! Returns realistic-looking outputs without calling any external service.
//! Activated when `SIEGE_DEMO_MODE=1` is set.

use std::time::Instant;

use crate::adapter::{
    AdapterProvenance, AdapterRequest, AdapterResponse, AdapterStatus, AgentAdapter, AgentKind,
    TokenUsage,
};

/// A mock adapter that returns canned but realistic outputs instantly.
pub struct MockAdapter;

impl MockAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl AgentAdapter for MockAdapter {
    fn name(&self) -> &str {
        "mock"
    }

    fn agent_kind(&self) -> AgentKind {
        AgentKind::Local
    }

    async fn invoke(&self, request: AdapterRequest) -> AdapterResponse {
        let started_at = chrono::Utc::now();
        let start = Instant::now();

        // Simulate 2-3 seconds of "thinking"
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let output = generate_mock_output(&request.prompt);
        let finished_at = chrono::Utc::now();

        AdapterResponse {
            task_id: request.task_id,
            status: AdapterStatus::Succeeded,
            output,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: start.elapsed().as_millis() as u64,
            token_usage: Some(TokenUsage {
                input_tokens: 1200,
                output_tokens: 800,
                cache_tokens: None,
            }),
            artifacts: Vec::new(),
            provenance: AdapterProvenance {
                adapter_name: "mock".to_string(),
                model_used: "mock-demo-v1".to_string(),
                provider: "mock".to_string(),
                invocation_id: uuid::Uuid::now_v7().to_string(),
                started_at: started_at.to_rfc3339(),
                finished_at: finished_at.to_rfc3339(),
            },
        }
    }
}

fn generate_mock_output(prompt: &str) -> String {
    let prompt_lower = prompt.to_lowercase();

    if prompt_lower.contains("auth") || prompt_lower.contains("login") {
        r#"## Implementation Complete

Created authentication module with:
- JWT token generation and validation
- Password hashing with bcrypt
- Login/logout endpoints
- Middleware for protected routes

Files modified:
- src/auth/mod.rs (new)
- src/auth/jwt.rs (new)
- src/auth/middleware.rs (new)
- src/routes/auth.rs (new)
- src/main.rs (updated routes)

All endpoints tested and working."#
            .to_string()
    } else if prompt_lower.contains("database")
        || prompt_lower.contains("schema")
        || prompt_lower.contains("model")
    {
        r#"## Implementation Complete

Created database models and migrations:
- User model with email, hashed_password, created_at
- Session model with token, user_id, expires_at
- Migration: 001_create_users.sql
- Migration: 002_create_sessions.sql
- Connection pool setup with sqlx

Files modified:
- src/models/user.rs (new)
- src/models/session.rs (new)
- db/migrations/001_create_users.sql (new)
- db/migrations/002_create_sessions.sql (new)
- src/db.rs (new)"#
            .to_string()
    } else if prompt_lower.contains("api")
        || prompt_lower.contains("endpoint")
        || prompt_lower.contains("route")
    {
        r#"## Implementation Complete

Created REST API endpoints:
- GET /api/users - list users (paginated)
- GET /api/users/:id - get user by id
- POST /api/users - create user
- PUT /api/users/:id - update user
- DELETE /api/users/:id - delete user

All endpoints return JSON, include error handling,
and require authentication via Bearer token.

Files modified:
- src/routes/users.rs (new)
- src/routes/mod.rs (updated)
- src/main.rs (updated routes)"#
            .to_string()
    } else if prompt_lower.contains("test") {
        r#"## Implementation Complete

Added test suite:
- Unit tests for auth module (8 tests)
- Integration tests for API endpoints (12 tests)
- Test fixtures and helpers

All 20 tests passing.

Files modified:
- tests/auth_test.rs (new)
- tests/api_test.rs (new)
- tests/helpers.rs (new)"#
            .to_string()
    } else {
        "## Implementation Complete\n\n\
         Task completed successfully. Applied changes based on the objective.\n\n\
         Summary:\n\
         - Analyzed requirements from the task description\n\
         - Implemented the requested functionality\n\
         - Followed existing code conventions\n\
         - Verified output correctness\n\n\
         Duration: 2.1s\n\
         Status: succeeded"
            .to_string()
    }
}
