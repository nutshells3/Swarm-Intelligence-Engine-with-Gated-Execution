/**
 * Auto-generated TypeScript types from the orchestration-api OpenAPI spec.
 *
 * Source of truth: services/orchestration-api/src/routes/*.rs (utoipa annotations)
 * OpenAPI spec:    /api-docs/openapi.json
 * Regenerate:      make sync-types
 *
 * DO NOT EDIT MANUALLY -- changes will be overwritten on next sync.
 *
 * OpenAPI schema -> TypeScript interface mapping:
 *
 *   #/components/schemas/HealthResponse           -> HealthResponse
 *   #/components/schemas/MetaResponse             -> MetaResponse
 *   #/components/schemas/ObjectiveResponse         -> ObjectiveResponse
 *   #/components/schemas/CreateObjectiveRequest    -> CreateObjectiveRequest
 *   #/components/schemas/PlanGateResponse          -> PlanGateResponse
 *   #/components/schemas/GateConditionEntry        -> GateConditionEntry
 *   #/components/schemas/MilestoneNodeResponse     -> MilestoneNodeResponse
 *   #/components/schemas/TaskResponse              -> TaskResponse
 *   #/components/schemas/CreateTaskRequest         -> CreateTaskRequest
 *   #/components/schemas/TaskAttemptResponse       -> TaskAttemptResponse
 *   #/components/schemas/CreateTaskAttemptRequest  -> CreateTaskAttemptRequest
 *   #/components/schemas/TaskLifecycleResponse     -> TaskLifecycleResponse
 *   #/components/schemas/CompleteTaskRequest       -> CompleteTaskRequest
 *   #/components/schemas/FailTaskRequest           -> FailTaskRequest
 *   #/components/schemas/PatchTaskRequest          -> PatchTaskRequest
 *   #/components/schemas/ArtifactEntry             -> ArtifactEntry
 *   #/components/schemas/AttemptLifecycleResponse  -> AttemptLifecycleResponse
 *   #/components/schemas/CompleteAttemptRequest    -> CompleteAttemptRequest
 *   #/components/schemas/EventResponse             -> EventResponse
 *   #/components/schemas/NodeResponse              -> NodeResponse
 *   #/components/schemas/CreateNodeRequest         -> CreateNodeRequest
 *   #/components/schemas/NodeEdgeResponse          -> NodeEdgeResponse
 *   #/components/schemas/CreateNodeEdgeRequest     -> CreateNodeEdgeRequest
 *   #/components/schemas/LoopResponse              -> LoopResponse
 *   #/components/schemas/CreateLoopRequest         -> CreateLoopRequest
 *   #/components/schemas/CycleResponse             -> CycleResponse
 *   #/components/schemas/CreateCycleRequest        -> CreateCycleRequest
 *   #/components/schemas/SessionResponse           -> SessionResponse
 *   #/components/schemas/SessionDetailResponse     -> SessionDetailResponse
 *   #/components/schemas/CreateSessionRequest      -> CreateSessionRequest
 *   #/components/schemas/AddMessageRequest         -> AddMessageRequest
 *   #/components/schemas/MessageResponse           -> MessageResponse
 *   #/components/schemas/ExtractResponse           -> ExtractResponse
 *   #/components/schemas/ChatToTasksResponse       -> ChatToTasksResponse
 *   #/components/schemas/RoadmapNodeResponse       -> RoadmapNodeResponse
 *   #/components/schemas/CreateRoadmapNodeRequest  -> CreateRoadmapNodeRequest
 *   #/components/schemas/AbsorptionResponse        -> AbsorptionResponse
 *   #/components/schemas/CreateAbsorptionRequest   -> CreateAbsorptionRequest
 *   #/components/schemas/AbsorbRoadmapRequest      -> AbsorbRoadmapRequest
 *   #/components/schemas/AbsorbRoadmapResponse     -> AbsorbRoadmapResponse
 *   #/components/schemas/ReorderRoadmapRequest     -> ReorderRoadmapRequest
 *   #/components/schemas/ReorderRoadmapResponse    -> ReorderRoadmapResponse
 *   #/components/schemas/ChangeTrackRequest        -> ChangeTrackRequest
 *   #/components/schemas/ChangeTrackResponse       -> ChangeTrackResponse
 *   #/components/schemas/RoadmapProjectionNode     -> RoadmapProjectionNode
 *   #/components/schemas/RoadmapProjectionResponse -> RoadmapProjectionResponse
 *   #/components/schemas/ReviewResponse            -> ReviewResponse
 *   #/components/schemas/CreateReviewRequest       -> CreateReviewRequest
 *   #/components/schemas/UpdateReviewRequest       -> UpdateReviewRequest
 *   #/components/schemas/ApproveReviewRequest      -> ApproveReviewRequest
 *   #/components/schemas/CertificationConfigResponse       -> CertificationConfigResponse
 *   #/components/schemas/UpdateCertificationConfigRequest  -> UpdateCertificationConfigRequest
 *   #/components/schemas/SubmitCertificationRequest        -> SubmitCertificationRequest
 *   #/components/schemas/CertificationSubmissionResponse   -> CertificationSubmissionResponse
 *   #/components/schemas/CertificationQueueEntryResponse   -> CertificationQueueEntryResponse
 *   #/components/schemas/CertificationResultResponse       -> CertificationResultResponse
 *   #/components/schemas/TaskMetrics               -> TaskMetrics
 *   #/components/schemas/SaturationMetrics         -> SaturationMetrics
 *   #/components/schemas/CycleMetric               -> CycleMetric
 *   #/components/schemas/CostMetric                -> CostMetric
 *   #/components/schemas/TokenMetrics              -> TokenMetrics
 *   #/components/schemas/WorkerMetric              -> WorkerMetric
 *   #/components/schemas/SkillPackResponse         -> SkillPackResponse
 *   #/components/schemas/CreateSkillPackRequest    -> CreateSkillPackRequest
 *   #/components/schemas/WorkerTemplateResponse    -> WorkerTemplateResponse
 *   #/components/schemas/CreateWorkerTemplateRequest -> CreateWorkerTemplateRequest
 *   #/components/schemas/PeerMessageResponse       -> PeerMessageResponse
 *   #/components/schemas/SendPeerMessageRequest    -> SendPeerMessageRequest
 *   #/components/schemas/AckResponse               -> AckResponse
 *   #/components/schemas/SubscriptionResponse      -> SubscriptionResponse
 *   #/components/schemas/TopicSummary              -> TopicSummary
 *   #/components/schemas/TaskBoardProjection       -> TaskBoardProjection
 *   #/components/schemas/TaskBoardItem             -> TaskBoardItem
 *   #/components/schemas/TaskBoardSummary          -> TaskBoardSummary
 *   #/components/schemas/NodeGraphProjection       -> NodeGraphProjection
 *   #/components/schemas/GraphNode                 -> GraphNode
 *   #/components/schemas/GraphEdge                 -> GraphEdge
 *   #/components/schemas/BranchMainlineProjection  -> BranchMainlineProjection
 *   #/components/schemas/BranchMainlineItem        -> BranchMainlineItem
 *   #/components/schemas/ReviewQueueProjection     -> ReviewQueueProjection
 *   #/components/schemas/ReviewQueueItem           -> ReviewQueueItem
 *   #/components/schemas/CertificationQueueProjection -> CertificationQueueProjection
 *   #/components/schemas/CertificationQueueItem    -> CertificationQueueItem
 *   #/components/schemas/ObjectiveProgressProjection -> ObjectiveProgressProjection
 *   #/components/schemas/ObjectiveProgressItem     -> ObjectiveProgressItem
 *   #/components/schemas/DriftProjection           -> DriftProjection
 *   #/components/schemas/DriftItem                 -> DriftItem
 *   #/components/schemas/LoopHistoryProjection     -> LoopHistoryProjection
 *   #/components/schemas/LoopHistoryCycleItem      -> LoopHistoryCycleItem
 *   #/components/schemas/ArtifactTimelineProjection -> ArtifactTimelineProjection
 *   #/components/schemas/ArtifactTimelineItem      -> ArtifactTimelineItem
 */

// ---- Health / Meta ----

export interface HealthResponse {
  status: string;
}

export interface MetaResponse {
  service: string;
  database_backend: string;
  database_url_present: boolean;
  write_path: string;
  migrations_loaded: boolean;
  active_agents: number;
  queue_length: number;
}

// ---- Objectives ----

export interface ObjectiveResponse {
  objective_id: string;
  summary: string;
  planning_status: string;
  plan_gate: string;
  created_at: string;
  updated_at: string;
  duplicated: boolean;
}

export interface CreateObjectiveRequest {
  summary: string;
  planning_status?: string;
  plan_gate?: string;
  idempotency_key: string;
}

export interface GateConditionEntry {
  label: string;
  passed: boolean;
  detail: string;
}

export interface PlanGateResponse {
  status: string;
  conditions: GateConditionEntry[];
  unresolved_questions: number;
  block_reason: string | null;
}

export interface MilestoneNodeResponse {
  milestone_id: string;
  title: string;
  description: string;
  status: string;
  parent_id: string | null;
  ordering: number;
}

// ---- Tasks ----

export interface TaskResponse {
  task_id: string;
  node_id: string;
  worker_role: string;
  skill_pack_id: string;
  status: string;
  created_at: string;
  updated_at: string;
  duplicated: boolean;
}

export interface CreateTaskRequest {
  node_id: string;
  worker_role: string;
  skill_pack_id: string;
  idempotency_key: string;
}

// ---- Task Attempts ----

export interface TaskAttemptResponse {
  task_attempt_id: string;
  task_id: string;
  attempt_index: number;
  lease_owner: string | null;
  status: string;
  started_at: string | null;
  finished_at: string | null;
  duplicated: boolean;
}

export interface CreateTaskAttemptRequest {
  task_id: string;
  lease_owner?: string | null;
  idempotency_key: string;
}

// ---- Task Lifecycle ----

export interface CompleteTaskRequest {
  output?: string | null;
  artifacts?: ArtifactEntry[];
}

export interface ArtifactEntry {
  artifact_kind: string;
  artifact_uri: string;
  metadata?: unknown;
}

export interface TaskLifecycleResponse {
  task_id: string;
  status: string;
  previous_status: string;
  node_id: string;
  node_lifecycle: string;
  artifacts_stored: number;
}

export interface FailTaskRequest {
  error_message?: string | null;
  error_code?: string | null;
}

export interface PatchTaskRequest {
  status: string;
  output?: string | null;
  artifacts?: ArtifactEntry[];
}

export interface CompleteAttemptRequest {
  status: string;
  output?: string | null;
  artifacts?: ArtifactEntry[];
}

export interface AttemptLifecycleResponse {
  task_attempt_id: string;
  task_id: string;
  attempt_status: string;
  previous_attempt_status: string;
  task_status: string;
  node_lifecycle: string;
  artifacts_stored: number;
}

// ---- Loops ----

export interface LoopResponse {
  loop_id: string;
  objective_id: string;
  cycle_index: number;
  active_track: string;
  created_at: string;
  updated_at: string;
  duplicated: boolean;
}

export interface CreateLoopRequest {
  objective_id: string;
  active_track: string;
  idempotency_key: string;
}

// ---- Cycles ----

export interface CycleResponse {
  cycle_id: string;
  loop_id: string;
  phase: string;
  policy_snapshot: unknown;
  created_at: string;
  updated_at: string;
  duplicated: boolean;
}

export interface CreateCycleRequest {
  loop_id: string;
  idempotency_key: string;
}

// ---- Nodes ----

export interface NodeResponse {
  node_id: string;
  objective_id: string;
  title: string;
  statement: string;
  lane: string;
  lifecycle: string;
  created_at: string;
  updated_at: string;
  duplicated: boolean;
}

export interface CreateNodeRequest {
  objective_id: string;
  title: string;
  statement: string;
  lane: string;
  idempotency_key: string;
}

export interface NodeEdgeResponse {
  edge_id: string;
  from_node_id: string;
  to_node_id: string;
  edge_kind: string;
  duplicated: boolean;
}

export interface CreateNodeEdgeRequest {
  from_node_id: string;
  to_node_id: string;
  edge_kind: string;
  idempotency_key: string;
}

// ---- Events ----

export interface EventResponse {
  event_id: string;
  aggregate_kind: string;
  aggregate_id: string;
  event_kind: string;
  idempotency_key: string;
  payload: Record<string, unknown>;
  created_at: string;
}

// ---- Metrics ----

export interface TaskMetrics {
  total: number;
  succeeded: number;
  failed: number;
  queued: number;
  running: number;
  timed_out: number;
  cancelled: number;
  success_rate: number;
}

export interface SaturationMetrics {
  queued_tasks: number;
  running_tasks: number;
  active_workers: number;
  queue_pressure: number;
  worker_success_rate: number | null;
  worker_total_attempts: number;
}

export interface CycleMetric {
  cycle_id: string;
  phase: string;
  created_at: string;
  updated_at: string;
  duration_ms: number | null;
  tasks_completed: number;
  tasks_failed: number;
  tasks_queued: number;
  tasks_running: number;
}

export interface CostMetric {
  total_invocations: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

export interface TokenMetrics {
  total_input_tokens: number;
  total_output_tokens: number;
  total_tokens: number;
  average_input_per_attempt: number;
  average_output_per_attempt: number;
}

export interface WorkerMetric {
  worker_role: string;
  total_attempts: number;
  succeeded: number;
  failed: number;
  success_rate: number;
}

// ---- Certification ----

export interface CertificationConfigResponse {
  enabled: boolean;
  frequency: string;
  routing: string;
  policy_id: string;
  revision: number;
}

export interface UpdateCertificationConfigRequest {
  enabled?: boolean;
  frequency?: string;
}

export interface SubmitCertificationRequest {
  node_id: string;
  task_id: string;
  claim_summary: string;
  source_anchors?: string[] | null;
  eligibility_reason: string;
  idempotency_key: string;
}

export interface CertificationSubmissionResponse {
  candidate_id: string;
  submission_id: string;
  queue_status: string;
  duplicated: boolean;
}

export interface CertificationQueueEntryResponse {
  submission_id: string;
  candidate_id: string;
  node_id: string;
  task_id: string;
  claim_summary: string;
  queue_status: string;
  submitted_at: string;
  retry_count: number;
  elapsed_display: string;
  eligibility_reason: string;
  external_gate: string | null;
  local_gate_effect: string | null;
}

export interface CertificationResultResponse {
  submission_id: string;
  candidate_id: string;
  queue_status: string;
  external_gate: string | null;
  local_gate_effect: string | null;
  lane_transition: string | null;
  projected_grade: string | null;
  projected_at: string | null;
}

// ---- Roadmap ----

export interface RoadmapNodeResponse {
  roadmap_node_id: string;
  title: string;
  track: string;
  status: string;
  created_at: string;
  updated_at: string;
  duplicated: boolean;
}

export interface CreateRoadmapNodeRequest {
  objective_id: string;
  title: string;
  track: string;
  idempotency_key: string;
}

export interface AbsorptionResponse {
  absorption_id: string;
  roadmap_node_id: string;
  action_kind: string;
  source_ref: string;
  target_ref: string | null;
  rationale: string;
  created_at: string;
  duplicated: boolean;
}

export interface CreateAbsorptionRequest {
  roadmap_node_id: string;
  action_kind: string;
  source_ref: string;
  target_ref?: string | null;
  rationale: string;
  idempotency_key: string;
}

export interface AbsorbRoadmapRequest {
  source_ref: string;
  action_kind: string;
  target_node_id?: string | null;
  rationale: string;
  title?: string | null;
  objective_id?: string | null;
  track?: string | null;
}

export interface AbsorbRoadmapResponse {
  absorption_id: string;
  action_kind: string;
  affected_node_id: string | null;
  created_at: string;
}

export interface ReorderRoadmapRequest {
  objective_id: string;
  node_sequence: string[];
}

export interface ReorderRoadmapResponse {
  objective_id: string;
  node_sequence: string[];
  updated: boolean;
}

export interface ChangeTrackRequest {
  track: string;
}

export interface ChangeTrackResponse {
  roadmap_node_id: string;
  track: string;
  updated: boolean;
}

export interface RoadmapProjectionNode {
  roadmap_node_id: string;
  title: string;
  track: string;
  status: string;
  objective_id: string;
  ordering_position: number | null;
  created_at: string;
  updated_at: string;
}

export interface RoadmapProjectionResponse {
  nodes: RoadmapProjectionNode[];
  total_count: number;
  open_count: number;
  deferred_count: number;
  rejected_count: number;
}

// ---- Chat ----

export interface SessionResponse {
  session_id: string;
  objective_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface SessionDetailResponse {
  session: SessionResponse;
  messages: MessageResponse[];
}

export interface CreateSessionRequest {
  objective_id?: string | null;
}

export interface AddMessageRequest {
  role: string;
  content: string;
}

export interface MessageResponse {
  message_id: string;
  session_id: string;
  role: string;
  content: string;
  created_at: string;
}

export interface ExtractResponse {
  extract_id: string;
  session_id: string;
  summarized_intent: string;
  extracted_constraints: unknown;
  extracted_decisions: unknown;
  extracted_open_questions: unknown;
  created_at: string;
}

export interface ChatToTasksResponse {
  task_ids: string[];
  items_found: number;
}

// ---- Reviews ----

export interface ReviewResponse {
  review_id: string;
  review_kind: string;
  target_ref: string;
  reviewer_template_id: string | null;
  status: string;
  score_or_verdict: string | null;
  findings_summary: string;
  conditions: unknown;
  approval_effect: string | null;
  is_auto_approval: boolean;
  recorded_at: string;
  duplicated: boolean;
}

export interface ReviewDigestResponse {
  objective_id: string;
  review_count: number;
  digest: string;
}

export interface CreateReviewRequest {
  review_kind: string;
  target_ref: string;
  reviewer_template_id?: string | null;
  idempotency_key: string;
}

export interface UpdateReviewRequest {
  status?: string | null;
  score_or_verdict?: string | null;
}

export interface ApproveReviewRequest {
  verdict?: string | null;
  approval_effect?: string | null;
}

// ---- Skills ----

export interface SkillPackResponse {
  skill_pack_id: string;
  worker_role: string;
  description: string;
  accepted_task_kinds: unknown;
  references: unknown;
  scripts: unknown;
  created_at: string;
  duplicated: boolean;
  expected_output_contract?: string;
  version?: string;
  deprecated: boolean;
}

export interface CreateSkillPackRequest {
  worker_role: string;
  description: string;
  accepted_task_kinds: unknown;
  references: unknown;
  scripts: unknown;
  idempotency_key: string;
  expected_output_contract?: string;
  version?: string;
}

export interface WorkerTemplateResponse {
  template_id: string;
  role: string;
  skill_pack_id: string;
  provider_mode: string;
  model_binding: string;
  allowed_task_kinds: unknown;
  created_at: string;
  duplicated: boolean;
}

export interface CreateWorkerTemplateRequest {
  role: string;
  skill_pack_id: string;
  provider_mode: string;
  model_binding: string;
  allowed_task_kinds: unknown;
  idempotency_key: string;
}

// ---- Peer Messaging ----

export interface PeerMessageResponse {
  message_id: string;
  from_task_id: string;
  to_task_id: string | null;
  topic: string;
  kind: string;
  payload: unknown;
  created_at: string;
}

export interface SendPeerMessageRequest {
  from_task_id: string;
  to_task_id?: string | null;
  topic: string;
  kind: string;
  payload?: unknown;
}

export interface AckResponse {
  ack_id: string;
  message_id: string;
  acknowledged_by: string;
  response: unknown | null;
  created_at: string;
}

export interface SubscriptionResponse {
  subscription_id: string;
  subscriber_task_id: string;
  topic: string;
  created_at: string;
}

export interface TopicSummary {
  topic: string;
  message_count: number;
  latest_at: string | null;
}

// ---- Projections ----

export interface TaskBoardItem {
  task_id: string;
  node_id: string;
  worker_role: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface TaskBoardSummary {
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  total: number;
}

export interface TaskBoardProjection {
  queued: TaskBoardItem[];
  running: TaskBoardItem[];
  succeeded: TaskBoardItem[];
  failed: TaskBoardItem[];
  summary: TaskBoardSummary;
}

export interface GraphNode {
  id: string;
  label: string;
  lane: string;
  lifecycle: string;
  objective_id: string;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: string;
}

export interface NodeGraphProjection {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface BranchMainlineItem {
  node_id: string;
  title: string;
  lane: string;
  lifecycle: string;
}

export interface BranchMainlineProjection {
  branch: BranchMainlineItem[];
  mainline_candidate: BranchMainlineItem[];
  mainline: BranchMainlineItem[];
  blocked: BranchMainlineItem[];
}

export interface ReviewQueueItem {
  node_id: string;
  title: string;
  lifecycle: string;
  lane: string;
}

export interface PendingReviewItem {
  review_id: string;
  review_kind: string;
  target_ref: string;
  status: string;
  recorded_at: string;
}

export interface ReviewQueueProjection {
  items: ReviewQueueItem[];
  pending_reviews: PendingReviewItem[];
  pending_count: number;
  review_artifact_count: number;
}

export interface CertificationQueueItem {
  node_id: string;
  title: string;
  lifecycle: string;
  lane: string;
}

export interface CertificationQueueProjection {
  items: CertificationQueueItem[];
  pending_count: number;
}

export interface ObjectiveProgressItem {
  objective_id: string;
  summary: string;
  total_nodes: number;
  completed_nodes: number;
  blocked_nodes: number;
  total_tasks: number;
  completed_tasks: number;
  progress_percent: number;
}

export interface ObjectiveProgressProjection {
  objectives: ObjectiveProgressItem[];
}

// ---- Policies ----

export interface PolicySnapshotResponse {
  policy_id: string;
  revision: number;
  duplicated: boolean;
  policy_payload: unknown;
}

export interface PolicySnapshotRequest {
  policy_id: string;
  idempotency_key: string;
  policy_payload: unknown;
}

export interface CertificationSettingsRequest {
  enabled: boolean;
  frequency: string;
  routing: string;
}

export interface CertificationSettingsResponse {
  policy_id: string;
  revision: number;
  certification: CertificationSettingsPayload;
  updated: boolean;
}

export interface CertificationSettingsPayload {
  enabled: boolean;
  frequency: string;
  routing: string;
}

// ---- Conflicts ----

export interface CompetingArtifactLink {
  node_id: string;
  task_id: string;
  artifact_hash: string;
  artifact_summary: string;
  produced_at: string;
}

export interface ConflictResponse {
  conflict_id: string;
  conflict_fingerprint: string;
  conflict_class: string;
  trigger: string;
  status: string;
  competing_artifacts: CompetingArtifactLink[];
  description: string;
  blocks_promotion: boolean;
  semantic_conflict_id: string | null;
  created_at: string;
  updated_at: string;
}
