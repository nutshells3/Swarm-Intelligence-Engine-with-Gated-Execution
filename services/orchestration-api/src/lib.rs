pub mod error;
pub mod routes;
pub mod state;

use std::sync::Arc;

use axum::{Router, response::Json, routing::get};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
struct HealthResponse<'a> {
    status: &'a str,
}

#[derive(Serialize, utoipa::ToSchema)]
struct MetaResponse<'a> {
    service: &'a str,
    database_backend: &'a str,
    database_url_present: bool,
    write_path: &'a str,
    migrations_loaded: bool,
    active_agents: i64,
    queue_length: i64,
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Health check", body = HealthResponse)
    )
)]
async fn health() -> Json<HealthResponse<'static>> {
    Json(HealthResponse { status: "ok" })
}

#[utoipa::path(
    get,
    path = "/api/meta",
    responses(
        (status = 200, description = "Service metadata", body = MetaResponse)
    )
)]
async fn meta(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<MetaResponse<'static>> {
    let active_agents: i64 = sqlx::query_scalar(
        "select count(*) from task_attempts where status = 'running'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let queue_length: i64 = sqlx::query_scalar(
        "select count(*) from tasks where status = 'queued'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    Json(MetaResponse {
        service: "orchestration-api",
        database_backend: "postgresql",
        database_url_present: !state.database_url.is_empty(),
        write_path: "command -> transaction -> event_journal -> projection update",
        migrations_loaded: true,
        active_agents,
        queue_length,
    })
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        meta,
        // Objectives
        routes::objectives::list_objectives,
        routes::objectives::create_objective,
        routes::objectives::get_objective,
        routes::objectives::get_objective_gate,
        routes::objectives::get_objective_milestones,
        // Tasks
        routes::tasks::list_tasks,
        routes::tasks::get_task,
        routes::tasks::create_task,
        routes::tasks::create_task_attempt,
        routes::tasks::list_task_attempts,
        // Events
        routes::events::list_events,
        // Nodes
        routes::nodes::list_nodes,
        routes::nodes::create_node,
        routes::nodes::get_node,
        routes::nodes::create_node_edge,
        routes::nodes::list_node_edges,
        // Cycles
        routes::cycles::list_cycles,
        routes::cycles::get_cycle,
        // Loops
        routes::loops::list_loops,
        routes::loops::get_loop,
        // Chat
        routes::chat::create_chat_session,
        routes::chat::list_chat_sessions,
        routes::chat::get_chat_session,
        routes::chat::add_message,
        routes::chat::list_messages,
        routes::chat::extract_conversation,
        routes::chat::chat_to_tasks,
        // Roadmap
        routes::roadmap::create_roadmap_node,
        routes::roadmap::get_roadmap_node,
        routes::roadmap::list_roadmap_nodes,
        routes::roadmap::create_absorption,
        routes::roadmap::list_absorptions,
        routes::roadmap::absorb_roadmap,
        routes::roadmap::reorder_roadmap,
        routes::roadmap::change_track,
        routes::roadmap::roadmap_projection,
        // Reviews
        routes::reviews::create_review,
        routes::reviews::list_reviews,
        routes::reviews::get_review,
        routes::reviews::update_review,
        routes::reviews::approve_review,
        routes::reviews::review_digest,
        // Certification
        routes::certification::get_certification_config,
        routes::certification::update_certification_config,
        routes::certification::submit_certification,
        routes::certification::list_certification_queue,
        routes::certification::get_certification_result,
        // Metrics
        routes::metrics::cycle_metrics,
        routes::metrics::task_metrics,
        routes::metrics::cost_metrics,
        routes::metrics::token_metrics,
        routes::metrics::worker_metrics,
        routes::metrics::saturation_metrics,
        // Task lifecycle
        routes::task_lifecycle::complete_task,
        routes::task_lifecycle::fail_task,
        routes::task_lifecycle::patch_task,
        routes::task_lifecycle::complete_attempt,
        // Policies
        routes::policies::list_policies,
        routes::policies::create_policy_snapshot,
        routes::policies::get_policy_snapshot,
        routes::policies::update_certification,
        // Skills
        routes::skills::create_skill_pack,
        routes::skills::get_skill_pack,
        routes::skills::list_skill_packs,
        routes::skills::create_worker_template,
        routes::skills::get_worker_template,
        routes::skills::list_worker_templates,
        // Peer messaging
        routes::peer::send_peer_message,
        routes::peer::list_peer_messages,
        routes::peer::ack_peer_message,
        routes::peer::subscribe,
        routes::peer::unsubscribe,
        routes::peer::list_topics,
        // SQL projections
        routes::projections::task_board,
        routes::projections::node_graph,
        routes::projections::branch_mainline,
        routes::projections::review_queue,
        routes::projections::certification_queue,
        routes::projections::objective_progress,
        routes::projections::drift,
        routes::projections::loop_history,
        routes::projections::artifact_timeline,
        // Event projections
        routes::event_projections::rebuild_projections,
        // Deployment
        routes::deployment::get_deployment_config,
        routes::deployment::update_deployment_config,
        // Conflicts
        routes::conflicts::list_conflicts,
        routes::conflicts::get_conflict,
    ),
    components(schemas(
        HealthResponse,
        MetaResponse,
        routes::objectives::ObjectiveResponse,
        routes::objectives::CreateObjectiveRequest,
        routes::objectives::PlanGateResponse,
        routes::objectives::GateConditionEntry,
        routes::objectives::MilestoneNodeResponse,
        routes::tasks::TaskResponse,
        routes::tasks::CreateTaskRequest,
        routes::tasks::TaskAttemptResponse,
        routes::tasks::CreateTaskAttemptRequest,
        routes::events::EventResponse,
        routes::nodes::NodeResponse,
        routes::nodes::CreateNodeRequest,
        routes::nodes::NodeEdgeResponse,
        routes::nodes::CreateNodeEdgeRequest,
        routes::cycles::CycleResponse,
        routes::cycles::CreateCycleRequest,
        routes::loops::LoopResponse,
        routes::loops::CreateLoopRequest,
        routes::metrics::TaskMetrics,
        routes::metrics::SaturationMetrics,
        routes::metrics::CycleMetric,
        routes::metrics::CostMetric,
        routes::metrics::TokenMetrics,
        routes::metrics::WorkerMetric,
        routes::certification::CertificationConfigResponse,
        routes::certification::CertificationQueueEntryResponse,
        routes::certification::CertificationResultResponse,
        routes::certification::CertificationSubmissionResponse,
        routes::certification::UpdateCertificationConfigRequest,
        routes::certification::SubmitCertificationRequest,
        routes::roadmap::RoadmapNodeResponse,
        routes::roadmap::CreateRoadmapNodeRequest,
        routes::roadmap::AbsorptionResponse,
        routes::roadmap::CreateAbsorptionRequest,
        routes::roadmap::AbsorbRoadmapRequest,
        routes::roadmap::AbsorbRoadmapResponse,
        routes::roadmap::ReorderRoadmapRequest,
        routes::roadmap::ReorderRoadmapResponse,
        routes::roadmap::ChangeTrackRequest,
        routes::roadmap::ChangeTrackResponse,
        routes::roadmap::RoadmapProjectionNode,
        routes::roadmap::RoadmapProjectionResponse,
        routes::task_lifecycle::TaskLifecycleResponse,
        routes::task_lifecycle::CompleteTaskRequest,
        routes::task_lifecycle::FailTaskRequest,
        routes::task_lifecycle::PatchTaskRequest,
        routes::task_lifecycle::ArtifactEntry,
        routes::task_lifecycle::AttemptLifecycleResponse,
        routes::task_lifecycle::CompleteAttemptRequest,
        routes::chat::SessionResponse,
        routes::chat::SessionDetailResponse,
        routes::chat::MessageResponse,
        routes::chat::CreateSessionRequest,
        routes::chat::AddMessageRequest,
        routes::chat::ExtractResponse,
        routes::chat::ChatToTasksResponse,
        routes::reviews::ReviewResponse,
        routes::reviews::ReviewDigestResponse,
        routes::reviews::CreateReviewRequest,
        routes::reviews::UpdateReviewRequest,
        routes::reviews::ApproveReviewRequest,
        routes::skills::SkillPackResponse,
        routes::skills::CreateSkillPackRequest,
        routes::skills::WorkerTemplateResponse,
        routes::skills::CreateWorkerTemplateRequest,
        routes::peer::PeerMessageResponse,
        routes::peer::SendPeerMessageRequest,
        routes::peer::AckResponse,
        routes::peer::SubscriptionResponse,
        routes::peer::TopicSummary,
        routes::projections::TaskBoardProjection,
        routes::projections::TaskBoardItem,
        routes::projections::TaskBoardSummary,
        routes::projections::NodeGraphProjection,
        routes::projections::GraphNode,
        routes::projections::GraphEdge,
        routes::projections::BranchMainlineProjection,
        routes::projections::BranchMainlineItem,
        routes::projections::ReviewQueueProjection,
        routes::projections::ReviewQueueItem,
        routes::projections::CertificationQueueProjection,
        routes::projections::CertificationQueueItem,
        routes::projections::ObjectiveProgressProjection,
        routes::projections::ObjectiveProgressItem,
        routes::projections::DriftProjection,
        routes::projections::DriftItem,
        routes::projections::LoopHistoryProjection,
        routes::projections::LoopHistoryCycleItem,
        routes::projections::ArtifactTimelineProjection,
        routes::projections::ArtifactTimelineItem,
        routes::deployment::DeploymentConfigResponse,
        routes::deployment::EndpointSummary,
        routes::deployment::UpdateDeploymentConfigRequest,
        routes::policies::PolicySnapshotRequest,
        routes::policies::PolicySnapshotResponse,
        routes::policies::CertificationSettingsRequest,
        routes::policies::CertificationSettingsResponse,
        routes::policies::CertificationSettingsPayload,
        routes::conflicts::ConflictResponse,
    ))
)]
struct ApiDoc;

/// Build the full application router and connection pool.
///
/// This is the main entry point for both the standalone binary and
/// embedded use (e.g. from a Tauri desktop app).
pub async fn build_app(
    database_url: &str,
) -> Result<(Router, sqlx::PgPool), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(1))
        .connect(database_url)
        .await?;

    sqlx::migrate!("../../db/migrations").run(&pool).await?;

    let adapter_registry = agent_adapters::AdapterRegistry::auto_detect();
    tracing::info!(
        adapters = ?adapter_registry.list(),
        "Agent adapter registry initialized with {} adapter(s)",
        adapter_registry.len()
    );

    let state = AppState {
        database_url: database_url.to_owned(),
        pool: pool.clone(),
        adapter_registry: Arc::new(adapter_registry),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/meta", get(meta))
        // Objectives
        .route(
            "/api/objectives",
            axum::routing::post(routes::objectives::create_objective)
                .get(routes::objectives::list_objectives),
        )
        .route(
            "/api/objectives/{id}",
            get(routes::objectives::get_objective),
        )
        .route(
            "/api/objectives/{id}/gate",
            get(routes::objectives::get_objective_gate),
        )
        .route(
            "/api/objectives/{id}/milestones",
            get(routes::objectives::get_objective_milestones),
        )
        // Loops
        .route(
            "/api/loops",
            axum::routing::post(routes::loops::create_loop).get(routes::loops::list_loops),
        )
        .route("/api/loops/{id}", get(routes::loops::get_loop))
        // Cycles
        .route(
            "/api/cycles",
            axum::routing::post(routes::cycles::create_cycle).get(routes::cycles::list_cycles),
        )
        .route("/api/cycles/{id}", get(routes::cycles::get_cycle))
        // Nodes
        .route(
            "/api/nodes",
            axum::routing::post(routes::nodes::create_node).get(routes::nodes::list_nodes),
        )
        .route("/api/nodes/{id}", get(routes::nodes::get_node))
        // Node edges
        .route(
            "/api/node-edges",
            axum::routing::post(routes::nodes::create_node_edge)
                .get(routes::nodes::list_node_edges),
        )
        // Tasks
        .route(
            "/api/tasks",
            axum::routing::post(routes::tasks::create_task).get(routes::tasks::list_tasks),
        )
        .route(
            "/api/tasks/{id}",
            get(routes::tasks::get_task).patch(routes::task_lifecycle::patch_task),
        )
        // Task lifecycle
        .route(
            "/api/tasks/{id}/complete",
            axum::routing::post(routes::task_lifecycle::complete_task),
        )
        .route(
            "/api/tasks/{id}/fail",
            axum::routing::post(routes::task_lifecycle::fail_task),
        )
        // Task attempts
        .route(
            "/api/task-attempts",
            axum::routing::post(routes::tasks::create_task_attempt)
                .get(routes::tasks::list_task_attempts),
        )
        .route(
            "/api/task-attempts/{attempt_id}/complete",
            axum::routing::post(routes::task_lifecycle::complete_attempt),
        )
        // Policies
        .route(
            "/api/policies",
            axum::routing::post(routes::policies::create_policy_snapshot)
                .get(routes::policies::list_policies),
        )
        .route(
            "/api/policies/{id}",
            get(routes::policies::get_policy_snapshot),
        )
        // Roadmap
        .route(
            "/api/roadmap/nodes",
            axum::routing::post(routes::roadmap::create_roadmap_node)
                .get(routes::roadmap::list_roadmap_nodes),
        )
        .route(
            "/api/roadmap/nodes/{id}",
            get(routes::roadmap::get_roadmap_node),
        )
        .route(
            "/api/roadmap/absorptions",
            axum::routing::post(routes::roadmap::create_absorption)
                .get(routes::roadmap::list_absorptions),
        )
        .route(
            "/api/roadmap/absorb",
            axum::routing::post(routes::roadmap::absorb_roadmap),
        )
        .route(
            "/api/roadmap/reorder",
            axum::routing::post(routes::roadmap::reorder_roadmap),
        )
        .route(
            "/api/roadmap/nodes/{id}/track",
            axum::routing::patch(routes::roadmap::change_track),
        )
        .route(
            "/api/projections/roadmap",
            get(routes::roadmap::roadmap_projection),
        )
        // Chat sessions
        .route(
            "/api/chat/sessions",
            axum::routing::post(routes::chat::create_chat_session)
                .get(routes::chat::list_chat_sessions),
        )
        .route(
            "/api/chat/sessions/{id}",
            get(routes::chat::get_chat_session),
        )
        .route(
            "/api/chat/sessions/{id}/messages",
            axum::routing::post(routes::chat::add_message)
                .get(routes::chat::list_messages),
        )
        .route(
            "/api/chat/sessions/{id}/extract",
            axum::routing::post(routes::chat::extract_conversation),
        )
        .route(
            "/api/chat/sessions/{id}/to-tasks",
            axum::routing::post(routes::chat::chat_to_tasks),
        )
        // Reviews
        .route(
            "/api/reviews",
            axum::routing::post(routes::reviews::create_review)
                .get(routes::reviews::list_reviews),
        )
        // Human digest summary (must precede {id} wildcard)
        .route(
            "/api/reviews/digest",
            get(routes::reviews::review_digest),
        )
        .route(
            "/api/reviews/{id}",
            get(routes::reviews::get_review)
                .patch(routes::reviews::update_review),
        )
        .route(
            "/api/reviews/{id}/approve",
            axum::routing::post(routes::reviews::approve_review),
        )
        // Policy certification toggle
        .route(
            "/api/policies/{id}/certification",
            axum::routing::patch(routes::policies::update_certification),
        )
        // Skills
        .route(
            "/api/skills",
            axum::routing::post(routes::skills::create_skill_pack)
                .get(routes::skills::list_skill_packs),
        )
        .route("/api/skills/{id}", get(routes::skills::get_skill_pack))
        // Worker templates
        .route(
            "/api/templates",
            axum::routing::post(routes::skills::create_worker_template)
                .get(routes::skills::list_worker_templates),
        )
        .route(
            "/api/templates/{id}",
            get(routes::skills::get_worker_template),
        )
        // Certification
        .route(
            "/api/certification/config",
            get(routes::certification::get_certification_config)
                .patch(routes::certification::update_certification_config),
        )
        .route(
            "/api/certification/submit",
            axum::routing::post(routes::certification::submit_certification),
        )
        .route(
            "/api/certification/queue",
            get(routes::certification::list_certification_queue),
        )
        .route(
            "/api/certification/results/{submission_id}",
            get(routes::certification::get_certification_result),
        )
        .route(
            "/api/deployment/config",
            get(routes::deployment::get_deployment_config)
                .patch(routes::deployment::update_deployment_config),
        )
        // Conflicts
        .route(
            "/api/conflicts",
            get(routes::conflicts::list_conflicts),
        )
        .route(
            "/api/conflicts/{id}",
            get(routes::conflicts::get_conflict),
        )
        // Projections
        .route(
            "/api/projections/rebuild",
            axum::routing::post(routes::event_projections::rebuild_projections),
        )
        .route(
            "/api/projections/task-board",
            get(routes::event_projections::get_task_board_projection),
        )
        .route(
            "/api/projections/branch-mainline",
            get(routes::event_projections::get_branch_mainline_projection),
        )
        .route(
            "/api/projections/review-queue",
            get(routes::event_projections::get_review_queue_projection),
        )
        .route(
            "/api/projections/certification-queue",
            get(routes::event_projections::get_certification_queue_projection),
        )
        // Metrics
        .route(
            "/api/metrics/cycles",
            get(routes::metrics::cycle_metrics),
        )
        .route(
            "/api/metrics/tasks",
            get(routes::metrics::task_metrics),
        )
        .route(
            "/api/metrics/costs",
            get(routes::metrics::cost_metrics),
        )
        .route(
            "/api/metrics/tokens",
            get(routes::metrics::token_metrics),
        )
        .route(
            "/api/metrics/workers",
            get(routes::metrics::worker_metrics),
        )
        .route(
            "/api/metrics/saturation",
            get(routes::metrics::saturation_metrics),
        )
        // SQL-projection endpoints (node graph, objective progress)
        .route(
            "/api/projections/node-graph",
            get(routes::projections::node_graph),
        )
        .route(
            "/api/projections/objective-progress",
            get(routes::projections::objective_progress),
        )
        .route(
            "/api/projections/drift",
            get(routes::projections::drift),
        )
        .route(
            "/api/projections/loop-history",
            get(routes::projections::loop_history),
        )
        .route(
            "/api/projections/artifact-timeline",
            get(routes::projections::artifact_timeline),
        )
        // Peer messaging
        .route(
            "/api/peer/messages",
            axum::routing::post(routes::peer::send_peer_message)
                .get(routes::peer::list_peer_messages),
        )
        .route(
            "/api/peer/messages/{message_id}/ack",
            axum::routing::post(routes::peer::ack_peer_message),
        )
        .route(
            "/api/peer/subscribe",
            axum::routing::post(routes::peer::subscribe)
                .delete(routes::peer::unsubscribe),
        )
        .route("/api/peer/topics", get(routes::peer::list_topics))
        .route("/api/peer/stream", get(routes::peer::peer_stream))
        // Events
        .route("/api/events", get(routes::events::list_events))
        // SSE event stream
        .route("/api/events/stream", get(routes::stream::event_stream))
        // Middleware
        .layer(cors)
        // Static file fallback (serves React SPA in production)
        .fallback_service(
            ServeDir::new("static").fallback(ServeFile::new("static/index.html")),
        )
        .with_state(state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()));

    Ok((app, pool))
}
