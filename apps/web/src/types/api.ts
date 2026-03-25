/**
 * Public API type surface for the web app.
 *
 * All types re-exported from generated.ts (the source of truth).
 * This file exists for backward compatibility — new code should import
 * from './generated' directly.
 *
 * Regenerate generated.ts: make sync-types
 */

// Re-export everything from generated types
export type {
  ObjectiveResponse,
  CreateObjectiveRequest,
  GateConditionEntry,
  PlanGateResponse,
  MilestoneNodeResponse,
  TaskResponse,
  CreateTaskRequest,
  TaskAttemptResponse,
  LoopResponse,
  CycleResponse,
  NodeResponse,
  CreateNodeRequest,
  NodeEdgeResponse,
  EventResponse,
  MetaResponse,
  TaskMetrics,
  SaturationMetrics,
  CertificationQueueEntryResponse,
  PolicySnapshotResponse,
  // Lifecycle
  TaskLifecycleResponse,
  CompleteTaskRequest,
  FailTaskRequest,
  PatchTaskRequest,
  ArtifactEntry,
  AttemptLifecycleResponse,
  // Chat
  SessionResponse,
  SessionDetailResponse,
  MessageResponse,
  CreateSessionRequest,
  AddMessageRequest,
  ExtractResponse,
  ChatToTasksResponse,
  // Reviews
  ReviewResponse,
  ReviewDigestResponse,
  CreateReviewRequest,
  UpdateReviewRequest,
  ApproveReviewRequest,
  // Skills
  SkillPackResponse,
  CreateSkillPackRequest,
  WorkerTemplateResponse,
  CreateWorkerTemplateRequest,
  // Peer
  PeerMessageResponse,
  SendPeerMessageRequest,
  AckResponse,
  SubscriptionResponse,
  TopicSummary,
  // Projections
  TaskBoardProjection,
  TaskBoardItem,
  TaskBoardSummary,
  NodeGraphProjection,
  GraphNode,
  GraphEdge,
  BranchMainlineProjection,
  BranchMainlineItem,
  ReviewQueueProjection,
  ReviewQueueItem,
  PendingReviewItem,
  CertificationQueueProjection,
  CertificationQueueItem,
  ObjectiveProgressProjection,
  ObjectiveProgressItem,
  // Roadmap
  RoadmapNodeResponse,
  CreateRoadmapNodeRequest,
  AbsorptionResponse,
  CreateAbsorptionRequest,
  AbsorbRoadmapRequest,
  AbsorbRoadmapResponse,
  ReorderRoadmapRequest,
  ReorderRoadmapResponse,
  ChangeTrackRequest,
  ChangeTrackResponse,
  // Certification
  CertificationConfigResponse,
  UpdateCertificationConfigRequest,
  SubmitCertificationRequest,
  CertificationSubmissionResponse,
  CertificationResultResponse,
  // Conflicts
  ConflictResponse,
  CompetingArtifactLink,
} from './generated';

// ---- Backward-compat aliases ----
// The old hand-written api.ts used slightly different names for some types.
// These aliases keep existing component imports working.

import type {
  PlanGateResponse as _PlanGateResponse,
  MilestoneNodeResponse as _MilestoneNodeResponse,
  TaskMetrics as _TaskMetrics,
  SaturationMetrics as _SaturationMetrics,
  CertificationQueueEntryResponse as _CertificationQueueEntryResponse,
  PolicySnapshotResponse as _PolicySnapshotResponse,
  GateConditionEntry as _GateConditionEntry,
} from './generated';

/** @deprecated Use PlanGateResponse['status'] or a union type */
export type GateStatus = 'satisfied' | 'open' | 'overridden';

/** @deprecated Use GateConditionEntry */
export type GateCondition = _GateConditionEntry;

/** @deprecated Use MilestoneNodeResponse */
export interface MilestoneResponse {
  id: string;
  title: string;
  completed: boolean;
  children: MilestoneResponse[];
}

/** @deprecated Use TaskMetrics */
export type TaskMetricsResponse = _TaskMetrics;

/** @deprecated Use SaturationMetrics */
export type SaturationMetricsResponse = _SaturationMetrics;

/** @deprecated Use CertificationQueueEntryResponse */
export type CertificationQueueEntry = _CertificationQueueEntryResponse;

/** @deprecated Use TaskResponse['status'] */
export type TaskStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'timed_out' | 'review_needed' | 'archived';

/** @deprecated Use PolicySnapshotResponse */
export type PolicyResponse = _PolicySnapshotResponse;
