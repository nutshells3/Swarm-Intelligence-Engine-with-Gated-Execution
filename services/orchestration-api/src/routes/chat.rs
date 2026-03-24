use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use agent_adapters::adapter::{AdapterRequest, AdapterStatus};
use crate::error::{ApiResult, bad_request, internal_error, not_found};
use crate::state::AppState;

// ── Request / Response types ────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    pub objective_id: Option<String>,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct SessionResponse {
    pub session_id: String,
    pub objective_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SessionDetailResponse {
    pub session: SessionResponse,
    pub messages: Vec<MessageResponse>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddMessageRequest {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct MessageResponse {
    pub message_id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ExtractResponse {
    pub extract_id: String,
    pub session_id: String,
    pub summarized_intent: String,
    pub extracted_constraints: serde_json::Value,
    pub extracted_decisions: serde_json::Value,
    pub extracted_open_questions: serde_json::Value,
    pub created_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ChatToTasksResponse {
    pub task_ids: Vec<String>,
    pub items_found: usize,
}

// ── Handlers ────────────────────────────────────────────────────────────

/// POST /api/chat/sessions
#[utoipa::path(
    post,
    path = "/api/chat/sessions",
    request_body = CreateSessionRequest,
    responses(
        (status = 200, description = "Created chat session", body = SessionResponse)
    )
)]
pub async fn create_chat_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> ApiResult<SessionResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let session_id = Uuid::now_v7().to_string();

    let row = sqlx::query(
        r#"INSERT INTO chat_sessions (session_id, objective_id, created_at, updated_at)
           VALUES ($1, $2, now(), now())
           RETURNING session_id, objective_id, created_at, updated_at"#,
    )
    .bind(&session_id)
    .bind(&req.objective_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'chat_session', $2, 'chat_session_created', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&session_id)
    .bind(&format!("create_session_{}", session_id))
    .bind(serde_json::json!({
        "session_id": session_id,
        "objective_id": req.objective_id,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> =
        row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(SessionResponse {
        session_id: row.try_get("session_id").map_err(internal_error)?,
        objective_id: row.try_get("objective_id").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    }))
}

/// GET /api/chat/sessions
#[utoipa::path(
    get,
    path = "/api/chat/sessions",
    responses(
        (status = 200, description = "List of chat sessions", body = Vec<SessionResponse>)
    )
)]
pub async fn list_chat_sessions(
    State(state): State<AppState>,
) -> ApiResult<Vec<SessionResponse>> {
    let rows = sqlx::query(
        "SELECT session_id, objective_id, created_at, updated_at FROM chat_sessions ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> =
            row.try_get("updated_at").map_err(internal_error)?;
        results.push(SessionResponse {
            session_id: row.try_get("session_id").map_err(internal_error)?,
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
        });
    }

    Ok(Json(results))
}

/// GET /api/chat/sessions/{id}
#[utoipa::path(
    get,
    path = "/api/chat/sessions/{id}",
    params(("id" = String, Path, description = "Chat session ID")),
    responses(
        (status = 200, description = "Chat session with messages", body = SessionDetailResponse)
    )
)]
pub async fn get_chat_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<SessionDetailResponse> {
    let session_row = sqlx::query(
        "SELECT session_id, objective_id, created_at, updated_at FROM chat_sessions WHERE session_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(session_row) = session_row else {
        return Err(not_found("chat session not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> =
        session_row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> =
        session_row.try_get("updated_at").map_err(internal_error)?;

    let session = SessionResponse {
        session_id: session_row.try_get("session_id").map_err(internal_error)?,
        objective_id: session_row.try_get("objective_id").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    };

    let msg_rows = sqlx::query(
        "SELECT message_id, session_id, role, content, created_at FROM chat_messages WHERE session_id = $1 ORDER BY created_at ASC",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut messages = Vec::with_capacity(msg_rows.len());
    for row in msg_rows {
        let msg_created: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at").map_err(internal_error)?;
        messages.push(MessageResponse {
            message_id: row.try_get("message_id").map_err(internal_error)?,
            session_id: row.try_get("session_id").map_err(internal_error)?,
            role: row.try_get("role").map_err(internal_error)?,
            content: row.try_get("content").map_err(internal_error)?,
            created_at: msg_created.to_rfc3339(),
        });
    }

    Ok(Json(SessionDetailResponse { session, messages }))
}

/// POST /api/chat/sessions/{id}/messages
///
/// Gap 2: If the session has no linked objective AND this is the first
/// user message, auto-create an objective from the message content.
#[utoipa::path(
    post,
    path = "/api/chat/sessions/{id}/messages",
    params(("id" = String, Path, description = "Chat session ID")),
    request_body = AddMessageRequest,
    responses(
        (status = 200, description = "Added message", body = MessageResponse)
    )
)]
pub async fn add_message(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<AddMessageRequest>,
) -> ApiResult<MessageResponse> {
    let valid_roles = ["user", "assistant", "system"];
    if !valid_roles.contains(&req.role.as_str()) {
        return Err(bad_request("role must be user, assistant, or system"));
    }

    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Verify session exists
    let session_row = sqlx::query(
        "SELECT session_id, objective_id FROM chat_sessions WHERE session_id = $1",
    )
    .bind(&session_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(session_row) = session_row else {
        return Err(not_found("chat session not found"));
    };

    let current_objective_id: Option<String> =
        session_row.try_get("objective_id").map_err(internal_error)?;

    // Track the effective objective_id (may be updated below if auto-created)
    let mut objective_id_for_session: Option<String> = current_objective_id.clone();

    let message_id = Uuid::now_v7().to_string();
    let content = req.content.clone();

    let row = sqlx::query(
        r#"INSERT INTO chat_messages (message_id, session_id, role, content, created_at)
           VALUES ($1, $2, $3, $4, now())
           RETURNING message_id, session_id, role, content, created_at"#,
    )
    .bind(&message_id)
    .bind(&session_id)
    .bind(&req.role)
    .bind(&req.content)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Update session timestamp
    sqlx::query("UPDATE chat_sessions SET updated_at = now() WHERE session_id = $1")
        .bind(&session_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;

    // ── Gap 2: Chat-to-objective hook ────────────────────────────────
    // If session has no linked objective AND role is "user" AND this is
    // the first user message, auto-create an objective.
    if current_objective_id.is_none() && req.role == "user" {
        let user_msg_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chat_messages WHERE session_id = $1 AND role = 'user'",
        )
        .bind(&session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        if user_msg_count == 1 {
            // This is the first (and only) user message — auto-create objective
            let objective_id = Uuid::now_v7().to_string();
            let summary = if req.content.len() > 200 {
                format!("{}...", &req.content[..200])
            } else {
                req.content.clone()
            };

            sqlx::query(
                r#"INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
                   VALUES ($1, $2, 'draft', 'open', now(), now())"#,
            )
            .bind(&objective_id)
            .bind(&summary)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

            // Link session to objective
            sqlx::query(
                "UPDATE chat_sessions SET objective_id = $1, updated_at = now() WHERE session_id = $2",
            )
            .bind(&objective_id)
            .bind(&session_id)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

            // Emit objective_created event
            sqlx::query(
                r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                   VALUES ($1, 'objective', $2, 'objective_created', $3, $4::jsonb, now())
                   ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
            )
            .bind(Uuid::now_v7().to_string())
            .bind(&objective_id)
            .bind(&format!("chat_auto_objective_{}", session_id))
            .bind(serde_json::json!({
                "objective_id": objective_id,
                "summary": summary,
                "planning_status": "draft",
                "plan_gate": "open",
                "source": "chat_auto",
                "session_id": session_id,
            }))
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

            // Emit session linked event
            sqlx::query(
                r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                   VALUES ($1, 'chat_session', $2, 'chat_session_linked_to_objective', $3, $4::jsonb, now())
                   ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
            )
            .bind(Uuid::now_v7().to_string())
            .bind(&session_id)
            .bind(&format!("link_session_objective_{}", session_id))
            .bind(serde_json::json!({
                "session_id": session_id,
                "objective_id": objective_id,
            }))
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

            objective_id_for_session = Some(objective_id);
        }
    }

    // ── Mid-flight conversation absorption ──────────────────────────
    // If session has a linked objective AND this is a user message:
    // 1. Auto-extract constraints/decisions/questions from the message
    // 2. Store as conversation_extract
    // 3. Emit event so loop-runner can pick up changes on next tick
    if req.role == "user" {
        if let Some(ref obj_id) = objective_id_for_session {
            let constraints = extract_constraints(&content);
            let decisions = extract_decisions(&content);
            let questions = extract_questions(&content);

            if !constraints.is_empty() || !decisions.is_empty() || !questions.is_empty() {
                let extract_id = Uuid::now_v7().to_string();
                sqlx::query(
                    "INSERT INTO conversation_extracts (extract_id, session_id, summarized_intent, extracted_constraints, extracted_decisions, extracted_open_questions, created_at) \
                     VALUES ($1, $2, $3, $4::jsonb, $5::jsonb, $6::jsonb, now())",
                )
                .bind(&extract_id)
                .bind(&session_id)
                .bind(&content)
                .bind(serde_json::json!(constraints))
                .bind(serde_json::json!(decisions))
                .bind(serde_json::json!(questions))
                .execute(&mut *tx)
                .await
                .map_err(internal_error)?;

                // Emit event so loop-runner knows about the new extract
                sqlx::query(
                    "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                     VALUES ($1, 'conversation', $2, 'constraints_extracted', $3, $4::jsonb, now()) \
                     ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
                )
                .bind(Uuid::now_v7().to_string())
                .bind(obj_id)
                .bind(format!("extract-{}", extract_id))
                .bind(serde_json::json!({
                    "extract_id": extract_id,
                    "constraints": constraints,
                    "decisions": decisions,
                    "questions": questions
                }))
                .execute(&mut *tx)
                .await
                .map_err(internal_error)?;
            }
        }
    }

    // Emit message event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'chat_message', $2, 'chat_message_added', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&message_id)
    .bind(&format!("add_message_{}", message_id))
    .bind(serde_json::json!({
        "message_id": message_id,
        "session_id": session_id,
        "role": req.role,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let msg_created: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(MessageResponse {
        message_id: row.try_get("message_id").map_err(internal_error)?,
        session_id: row.try_get("session_id").map_err(internal_error)?,
        role: row.try_get("role").map_err(internal_error)?,
        content: row.try_get("content").map_err(internal_error)?,
        created_at: msg_created.to_rfc3339(),
    }))
}

/// GET /api/chat/sessions/{id}/messages
#[utoipa::path(
    get,
    path = "/api/chat/sessions/{id}/messages",
    params(("id" = String, Path, description = "Chat session ID")),
    responses(
        (status = 200, description = "Messages in session", body = Vec<MessageResponse>)
    )
)]
pub async fn list_messages(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<Vec<MessageResponse>> {
    let rows = sqlx::query(
        "SELECT message_id, session_id, role, content, created_at FROM chat_messages WHERE session_id = $1 ORDER BY created_at ASC",
    )
    .bind(&session_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at").map_err(internal_error)?;
        results.push(MessageResponse {
            message_id: row.try_get("message_id").map_err(internal_error)?,
            session_id: row.try_get("session_id").map_err(internal_error)?,
            role: row.try_get("role").map_err(internal_error)?,
            content: row.try_get("content").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
        });
    }

    Ok(Json(results))
}

// ── Agent-based extraction types (CONV-005 ~ CONV-010) ──────────────

/// JSON schema the agent is asked to return. Parsed with serde; on
/// failure we fall back to keyword heuristic extraction.
#[derive(Debug, Deserialize)]
struct AgentExtractPayload {
    intent_summary: Option<String>,
    #[serde(default)]
    constraints: Vec<AgentConstraint>,
    #[serde(default)]
    decisions: Vec<AgentDecision>,
    #[serde(default)]
    open_questions: Vec<AgentQuestion>,
    #[serde(default)]
    backlog_items: Vec<AgentBacklogItem>,
}

#[derive(Debug, Deserialize)]
struct AgentConstraint {
    statement: String,
    #[serde(default = "default_kind")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct AgentDecision {
    decision: String,
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    affected_components: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AgentQuestion {
    question: String,
    #[serde(default = "default_blocking")]
    blocking_status: String,
    resolution_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentBacklogItem {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_priority")]
    priority: i32,
}

fn default_kind() -> String { "requirement".to_string() }
fn default_blocking() -> String { "non_blocking".to_string() }
fn default_priority() -> i32 { 50 }

/// Build the extraction prompt from collected messages.
fn build_extraction_prompt(messages: &[(String, String)]) -> String {
    let schema = r#"{
  "intent_summary": "<1-2 sentence summary of what the conversation is trying to achieve>",
  "constraints": [{"statement": "...", "kind": "requirement|resource_bound|compatibility|regulatory|preference"}],
  "decisions": [{"decision": "...", "rationale": "...", "affected_components": []}],
  "open_questions": [{"question": "...", "blocking_status": "non_blocking|blocking", "resolution_path": "..."}],
  "backlog_items": [{"title": "...", "description": "...", "priority": 50}]
}"#;

    let mut prompt = format!(
        "You are a structured-extraction agent. Analyze the following conversation \
         and return a JSON object (no markdown fences, raw JSON only) with these fields:\n\n\
         {schema}\n\n\
         Rules:\n\
         - constraints: lines with must/should/require/need to/has to/shall\n\
         - decisions: explicit choices (we'll use X, decided on Y, going with Z)\n\
         - open_questions: questions or unresolved items\n\
         - backlog_items: actionable tasks extracted from the conversation\n\
         - Return ONLY the JSON object, no extra text.\n\n\
         === CONVERSATION ===\n"
    );
    for (role, content) in messages {
        prompt.push_str(&format!("[{}]: {}\n", role, content));
    }
    prompt.push_str("=== END ===\n");
    prompt
}

/// Try to parse the agent response as AgentExtractPayload.
/// Handles responses that may have markdown fences or leading text.
fn parse_agent_response(raw: &str) -> Option<AgentExtractPayload> {
    // Try raw first.
    if let Ok(parsed) = serde_json::from_str::<AgentExtractPayload>(raw) {
        return Some(parsed);
    }
    // Strip markdown code fences.
    let trimmed = raw.trim();
    let body = if trimmed.starts_with("```") {
        let start = trimmed.find('\n').map(|i| i + 1).unwrap_or(3);
        let end = trimmed.rfind("```").unwrap_or(trimmed.len());
        &trimmed[start..end]
    } else {
        trimmed
    };
    // Try the stripped body.
    if let Ok(parsed) = serde_json::from_str::<AgentExtractPayload>(body) {
        return Some(parsed);
    }
    // Try extracting first { ... } block.
    if let Some(start) = body.find('{') {
        if let Some(end) = body.rfind('}') {
            if end > start {
                if let Ok(parsed) =
                    serde_json::from_str::<AgentExtractPayload>(&body[start..=end])
                {
                    return Some(parsed);
                }
            }
        }
    }
    None
}

/// POST /api/chat/sessions/{id}/extract
///
/// CONV-005: Agent-based extraction with keyword-heuristic fallback.
/// Calls an agent adapter to summarize the conversation into structured
/// JSON (constraints, decisions, open questions, backlog items).
/// On adapter unavailability or parse failure, falls back to the
/// original keyword heuristic.
///
/// CONV-006~008: Parsed sections are inserted into conversation_extracts
/// (extracted_constraints, extracted_decisions, extracted_open_questions).
///
/// CONV-009: Backlog items from the agent response are stored in the
/// extract payload.
///
/// CONV-010: If an active plan exists for the session's objective,
/// constraints are inserted into plan_invariants and open questions
/// into unresolved_questions.
#[utoipa::path(
    post,
    path = "/api/chat/sessions/{id}/extract",
    params(("id" = String, Path, description = "Chat session ID")),
    responses(
        (status = 200, description = "Extraction result", body = ExtractResponse)
    )
)]
pub async fn extract_conversation(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<ExtractResponse> {
    // Load all messages for the session.
    let msg_rows = sqlx::query(
        "SELECT message_id, role, content FROM chat_messages WHERE session_id = $1 ORDER BY created_at ASC",
    )
    .bind(&session_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    if msg_rows.is_empty() {
        return Err(not_found("no messages in session"));
    }

    // Collect messages as (role, content) pairs for the prompt, and
    // keep message_ids for provenance linking.
    let mut messages_for_prompt: Vec<(String, String)> = Vec::new();
    let mut all_message_ids: Vec<String> = Vec::new();
    for row in &msg_rows {
        let message_id: String = row.try_get("message_id").map_err(internal_error)?;
        let role: String = row.try_get("role").map_err(internal_error)?;
        let content: String = row.try_get("content").map_err(internal_error)?;
        all_message_ids.push(message_id);
        messages_for_prompt.push((role, content));
    }

    // ── CONV-005: Attempt agent-based extraction ─────────────────────
    let agent_result: Option<AgentExtractPayload> = if let Some(adapter) =
        state.adapter_registry.select(None)
    {
        let prompt = build_extraction_prompt(&messages_for_prompt);
        let request = AdapterRequest {
            task_id: format!("extract-{}", session_id),
            prompt,
            context_files: Vec::new(),
            working_directory: String::new(),
            model: None,
            provider_mode: "api".to_string(),
            timeout_seconds: 60,
            max_tokens: Some(4096),
            temperature: Some(0.2),
        };
        let response = adapter.invoke_boxed(request).await;
        if response.status == AdapterStatus::Succeeded && !response.output.is_empty() {
            match parse_agent_response(&response.output) {
                Some(parsed) => {
                    tracing::info!(
                        session_id = %session_id,
                        adapter = %response.provenance.adapter_name,
                        "Agent extraction succeeded"
                    );
                    Some(parsed)
                }
                None => {
                    tracing::warn!(
                        session_id = %session_id,
                        "Agent returned unparseable output, falling back to keyword heuristic"
                    );
                    None
                }
            }
        } else {
            tracing::warn!(
                session_id = %session_id,
                status = ?response.status,
                "Agent extraction failed, falling back to keyword heuristic"
            );
            None
        }
    } else {
        tracing::info!(
            session_id = %session_id,
            "No adapter available, using keyword heuristic extraction"
        );
        None
    };

    // ── Build the extract fields from agent result or keyword fallback ─

    let (summarized_intent, constraints_json, decisions_json, questions_json, backlog_json) =
        if let Some(agent_payload) = agent_result {
            // CONV-005: Use agent-derived intent summary.
            let intent = agent_payload
                .intent_summary
                .unwrap_or_else(|| "Agent extraction (no explicit summary)".to_string());

            // CONV-006: Build constraint objects.
            let constraints: Vec<serde_json::Value> = agent_payload
                .constraints
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "constraint_id": Uuid::now_v7().to_string(),
                        "statement": c.statement,
                        "kind": c.kind,
                        "source_message_ids": &all_message_ids,
                        "enforcement_status": "pending",
                    })
                })
                .collect();

            // CONV-007: Build decision objects.
            let decisions: Vec<serde_json::Value> = agent_payload
                .decisions
                .into_iter()
                .map(|d| {
                    serde_json::json!({
                        "decision_id": Uuid::now_v7().to_string(),
                        "decision": d.decision,
                        "rationale": d.rationale,
                        "affected_components": d.affected_components,
                        "source_message_ids": &all_message_ids,
                    })
                })
                .collect();

            // CONV-008: Build open-question objects.
            let questions: Vec<serde_json::Value> = agent_payload
                .open_questions
                .into_iter()
                .map(|q| {
                    serde_json::json!({
                        "question_id": Uuid::now_v7().to_string(),
                        "question": q.question,
                        "blocking_status": q.blocking_status,
                        "resolution_path": q.resolution_path,
                        "source_message_ids": &all_message_ids,
                    })
                })
                .collect();

            // CONV-009: Build backlog draft items.
            let backlog: Vec<serde_json::Value> = agent_payload
                .backlog_items
                .into_iter()
                .map(|b| {
                    serde_json::json!({
                        "item_id": Uuid::now_v7().to_string(),
                        "title": b.title,
                        "description": b.description,
                        "priority": b.priority,
                        "source": "agent_extraction",
                    })
                })
                .collect();

            (
                intent,
                serde_json::Value::Array(constraints),
                serde_json::Value::Array(decisions),
                serde_json::Value::Array(questions),
                serde_json::Value::Array(backlog),
            )
        } else {
            // ── Keyword heuristic fallback (original MVP logic) ──────
            let mut constraints = Vec::new();
            let mut decisions = Vec::new();
            let mut open_questions = Vec::new();
            let mut intent_parts = Vec::new();

            let constraint_kw = ["must", "should", "require"];
            let decision_kw = [
                "we'll use",
                "decided",
                "choosing",
                "we will use",
                "let's go with",
            ];
            let question_kw = ["?", "unclear", "need to decide", "not sure", "tbd"];

            for (idx, row) in msg_rows.iter().enumerate() {
                let message_id: String = row.try_get("message_id").map_err(internal_error)?;
                let content: String = row.try_get("content").map_err(internal_error)?;

                if idx < 3 {
                    let snippet = if content.len() > 100 {
                        format!("{}...", &content[..100])
                    } else {
                        content.clone()
                    };
                    intent_parts.push(snippet);
                }

                for line in content.lines() {
                    let lower_line = line.to_lowercase();

                    for kw in &constraint_kw {
                        if lower_line.contains(kw) {
                            constraints.push(serde_json::json!({
                                "constraint_id": Uuid::now_v7().to_string(),
                                "statement": line.trim(),
                                "kind": "requirement",
                                "source_message_ids": [&message_id],
                                "enforcement_status": "pending",
                            }));
                            break;
                        }
                    }

                    for kw in &decision_kw {
                        if lower_line.contains(kw) {
                            decisions.push(serde_json::json!({
                                "decision_id": Uuid::now_v7().to_string(),
                                "decision": line.trim(),
                                "rationale": "",
                                "affected_components": [],
                                "source_message_ids": [&message_id],
                            }));
                            break;
                        }
                    }

                    for kw in &question_kw {
                        if lower_line.contains(kw) {
                            open_questions.push(serde_json::json!({
                                "question_id": Uuid::now_v7().to_string(),
                                "question": line.trim(),
                                "blocking_status": "non_blocking",
                                "resolution_path": null,
                                "source_message_ids": [&message_id],
                            }));
                            break;
                        }
                    }
                }
            }

            let intent = if intent_parts.is_empty() {
                "No messages found".to_string()
            } else {
                intent_parts.join(" | ")
            };

            (
                intent,
                serde_json::Value::Array(constraints),
                serde_json::Value::Array(decisions),
                serde_json::Value::Array(open_questions),
                serde_json::Value::Array(Vec::new()), // no backlog from keyword fallback
            )
        };

    // ── Persist the extract ──────────────────────────────────────────

    let extract_id = Uuid::now_v7().to_string();
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    let row = sqlx::query(
        r#"INSERT INTO conversation_extracts (extract_id, session_id, summarized_intent, extracted_constraints, extracted_decisions, extracted_open_questions, created_at)
           VALUES ($1, $2, $3, $4::jsonb, $5::jsonb, $6::jsonb, now())
           RETURNING extract_id, session_id, summarized_intent, extracted_constraints, extracted_decisions, extracted_open_questions, created_at"#,
    )
    .bind(&extract_id)
    .bind(&session_id)
    .bind(&summarized_intent)
    .bind(&constraints_json)
    .bind(&decisions_json)
    .bind(&questions_json)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit extraction event.
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'conversation_extract', $2, 'conversation_extracted', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&extract_id)
    .bind(&format!("extract_{}", extract_id))
    .bind(serde_json::json!({
        "extract_id": extract_id,
        "session_id": session_id,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // ── CONV-009: Persist backlog draft if items exist ────────────────
    if let Some(items) = backlog_json.as_array() {
        if !items.is_empty() {
            let backlog_id = Uuid::now_v7().to_string();
            sqlx::query(
                r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                   VALUES ($1, 'conversation_extract', $2, 'backlog_draft_created', $3, $4::jsonb, now())
                   ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
            )
            .bind(Uuid::now_v7().to_string())
            .bind(&extract_id)
            .bind(&format!("backlog_draft_{}", extract_id))
            .bind(serde_json::json!({
                "backlog_id": backlog_id,
                "extract_id": extract_id,
                "session_id": session_id,
                "items": items,
            }))
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
        }
    }

    // ── CONV-010: Plan update — propagate to plan_invariants / unresolved_questions ─
    // Look up whether this session has a linked objective with an active plan.
    let session_row = sqlx::query(
        "SELECT objective_id FROM chat_sessions WHERE session_id = $1",
    )
    .bind(&session_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(ref srow) = session_row {
        let objective_id: Option<String> = srow.try_get("objective_id").map_err(internal_error)?;
        if let Some(ref obj_id) = objective_id {
            // Check for an active plan for this objective.
            let plan_row = sqlx::query(
                "SELECT plan_id FROM plans WHERE objective_id = $1 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(obj_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal_error)?;

            if let Some(ref prow) = plan_row {
                let plan_id: String = prow.try_get("plan_id").map_err(internal_error)?;

                // Insert constraints into plan_invariants.
                if let Some(constraint_arr) = constraints_json.as_array() {
                    for c in constraint_arr {
                        let invariant_id = Uuid::now_v7().to_string();
                        let description = c
                            .get("statement")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if description.is_empty() {
                            continue;
                        }
                        sqlx::query(
                            r#"INSERT INTO plan_invariants (invariant_id, objective_id, description, predicate, scope, enforcement, status, target_id, created_at)
                               VALUES ($1, $2, $3, $4, 'global', 'plan_validation', 'unchecked', $5, now())"#,
                        )
                        .bind(&invariant_id)
                        .bind(obj_id)
                        .bind(description)
                        .bind(description) // predicate mirrors description for now
                        .bind(&plan_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(internal_error)?;
                    }
                }

                // Insert open questions into unresolved_questions.
                if let Some(question_arr) = questions_json.as_array() {
                    for q in question_arr {
                        let question_id = Uuid::now_v7().to_string();
                        let question_text = q
                            .get("question")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if question_text.is_empty() {
                            continue;
                        }
                        let severity = match q
                            .get("blocking_status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("non_blocking")
                        {
                            "blocking" => "blocking",
                            _ => "important",
                        };
                        sqlx::query(
                            r#"INSERT INTO unresolved_questions (question_id, objective_id, question, context, severity, resolution_status, source_ref, created_at, updated_at)
                               VALUES ($1, $2, $3, $4, $5, 'open', $6, now(), now())"#,
                        )
                        .bind(&question_id)
                        .bind(obj_id)
                        .bind(question_text)
                        .bind(&format!("Extracted from session {}", session_id))
                        .bind(severity)
                        .bind(&extract_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(internal_error)?;
                    }

                    // Update the plan's unresolved_questions counter.
                    let uq_count: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM unresolved_questions WHERE objective_id = $1 AND resolution_status = 'open'",
                    )
                    .bind(obj_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(internal_error)?;

                    sqlx::query(
                        "UPDATE plans SET unresolved_questions = $1, updated_at = now() WHERE plan_id = $2",
                    )
                    .bind(uq_count as i32)
                    .bind(&plan_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(internal_error)?;
                }

                // Emit plan_updated event.
                sqlx::query(
                    r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                       VALUES ($1, 'plan', $2, 'plan_updated_from_extract', $3, $4::jsonb, now())
                       ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
                )
                .bind(Uuid::now_v7().to_string())
                .bind(&plan_id)
                .bind(&format!("plan_update_extract_{}", extract_id))
                .bind(serde_json::json!({
                    "plan_id": plan_id,
                    "extract_id": extract_id,
                    "objective_id": obj_id,
                    "session_id": session_id,
                }))
                .execute(&mut *tx)
                .await
                .map_err(internal_error)?;
            }
        }
    }

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(ExtractResponse {
        extract_id: row.try_get("extract_id").map_err(internal_error)?,
        session_id: row.try_get("session_id").map_err(internal_error)?,
        summarized_intent: row.try_get("summarized_intent").map_err(internal_error)?,
        extracted_constraints: row.try_get("extracted_constraints").map_err(internal_error)?,
        extracted_decisions: row.try_get("extracted_decisions").map_err(internal_error)?,
        extracted_open_questions: row
            .try_get("extracted_open_questions")
            .map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
    }))
}

/// POST /api/chat/sessions/{id}/to-tasks
///
/// Gap 3: Parse conversation for actionable items and create tasks
/// for the linked objective's nodes.
#[utoipa::path(
    post,
    path = "/api/chat/sessions/{id}/to-tasks",
    params(("id" = String, Path, description = "Chat session ID")),
    responses(
        (status = 200, description = "Tasks created from chat", body = ChatToTasksResponse)
    )
)]
pub async fn chat_to_tasks(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<ChatToTasksResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Get session and its linked objective
    let session_row = sqlx::query(
        "SELECT session_id, objective_id FROM chat_sessions WHERE session_id = $1",
    )
    .bind(&session_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(session_row) = session_row else {
        return Err(not_found("chat session not found"));
    };

    let objective_id: Option<String> =
        session_row.try_get("objective_id").map_err(internal_error)?;
    let Some(objective_id) = objective_id else {
        return Err(bad_request(
            "session has no linked objective; send a user message first",
        ));
    };

    // Find a node for this objective (use first available, or create one)
    let node_row = sqlx::query(
        "SELECT node_id FROM nodes WHERE objective_id = $1 LIMIT 1",
    )
    .bind(&objective_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let node_id = if let Some(nr) = node_row {
        nr.try_get::<String, _>("node_id").map_err(internal_error)?
    } else {
        // Auto-create a node for this objective
        let nid = Uuid::now_v7().to_string();
        sqlx::query(
            r#"INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
               VALUES ($1, $2, 'Chat-derived tasks', 'Tasks extracted from chat conversation', 'default', 'active', now(), now())"#,
        )
        .bind(&nid)
        .bind(&objective_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
        nid
    };

    // Load messages and extract actionable items
    let msg_rows = sqlx::query(
        "SELECT message_id, role, content FROM chat_messages WHERE session_id = $1 ORDER BY created_at ASC",
    )
    .bind(&session_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal_error)?;

    let action_keywords = [
        "todo", "task:", "action:", "implement", "create", "build", "fix",
        "add", "set up", "configure", "deploy", "write", "test",
    ];

    let mut task_ids = Vec::new();

    for row in &msg_rows {
        let content: String = row.try_get("content").map_err(internal_error)?;

        for line in content.lines() {
            let lower_line = line.to_lowercase().trim().to_string();
            let is_actionable = action_keywords
                .iter()
                .any(|kw| lower_line.starts_with(kw) || lower_line.contains(&format!(" {}", kw)));

            if is_actionable && line.trim().len() > 5 {
                let task_id = Uuid::now_v7().to_string();
                let description = line.trim();

                sqlx::query(
                    r#"INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
                       VALUES ($1, $2, 'general', 'default', 'queued', now(), now())"#,
                )
                .bind(&task_id)
                .bind(&node_id)
                .execute(&mut *tx)
                .await
                .map_err(internal_error)?;

                // Emit event
                sqlx::query(
                    r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                       VALUES ($1, 'task', $2, 'task_created', $3, $4::jsonb, now())
                       ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
                )
                .bind(Uuid::now_v7().to_string())
                .bind(&task_id)
                .bind(&format!("chat_task_{}", task_id))
                .bind(serde_json::json!({
                    "task_id": task_id,
                    "node_id": node_id,
                    "description": description,
                    "source": "chat_extraction",
                    "session_id": session_id,
                }))
                .execute(&mut *tx)
                .await
                .map_err(internal_error)?;

                task_ids.push(task_id);
            }
        }
    }

    tx.commit().await.map_err(internal_error)?;

    let items_found = task_ids.len();
    Ok(Json(ChatToTasksResponse {
        task_ids,
        items_found,
    }))
}

// ── Extraction helpers for mid-flight conversation absorption ────────

const CONSTRAINT_KEYWORDS: &[&str] = &["must", "should", "require", "need to", "has to", "shall"];
const DECISION_KEYWORDS: &[&str] = &[
    "we'll use",
    "decided",
    "choosing",
    "we will use",
    "let's go with",
    "going with",
    "selected",
    "picked",
];
const QUESTION_KEYWORDS: &[&str] = &["?", "unclear", "need to decide", "not sure", "tbd", "open question"];

/// Extract constraint statements from message content using keyword heuristics.
fn extract_constraints(content: &str) -> Vec<serde_json::Value> {
    let mut constraints = Vec::new();
    for line in content.lines() {
        let lower = line.to_lowercase();
        for kw in CONSTRAINT_KEYWORDS {
            if lower.contains(kw) && line.trim().len() > 5 {
                constraints.push(serde_json::json!({
                    "statement": line.trim(),
                    "kind": "requirement",
                    "enforcement_status": "pending"
                }));
                break;
            }
        }
    }
    constraints
}

/// Extract decision statements from message content using keyword heuristics.
fn extract_decisions(content: &str) -> Vec<serde_json::Value> {
    let mut decisions = Vec::new();
    for line in content.lines() {
        let lower = line.to_lowercase();
        for kw in DECISION_KEYWORDS {
            if lower.contains(kw) && line.trim().len() > 5 {
                decisions.push(serde_json::json!({
                    "decision": line.trim(),
                    "rationale": "",
                    "affected_components": []
                }));
                break;
            }
        }
    }
    decisions
}

/// Extract open questions from message content using keyword heuristics.
fn extract_questions(content: &str) -> Vec<serde_json::Value> {
    let mut questions = Vec::new();
    for line in content.lines() {
        let lower = line.to_lowercase();
        for kw in QUESTION_KEYWORDS {
            if lower.contains(kw) && line.trim().len() > 5 {
                questions.push(serde_json::json!({
                    "question": line.trim(),
                    "blocking_status": "non_blocking",
                    "resolution_path": null
                }));
                break;
            }
        }
    }
    questions
}
