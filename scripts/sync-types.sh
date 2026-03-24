#!/bin/bash
# Generate TypeScript types from OpenAPI spec.
# Run after modifying Rust API handlers.
#
# Usage:
#   bash scripts/sync-types.sh          # Fetch from running server
#   bash scripts/sync-types.sh --offline # Skip server, just validate generated.ts exists
#
# Prerequisites:
#   - Running API server at $API_URL (default: http://127.0.0.1:8845)
#   - npx available (Node.js)
#   - openapi-typescript installed (will be npx-fetched if missing)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

API_URL="${API_URL:-http://127.0.0.1:8845}"
SPEC_FILE="$PROJECT_ROOT/apps/web/openapi.json"
TYPES_FILE="$PROJECT_ROOT/apps/web/src/types/generated.ts"

# ── Offline mode: just verify the generated file exists ──
if [[ "$1" == "--offline" ]]; then
    if [[ -f "$TYPES_FILE" ]]; then
        echo "[sync-types] generated.ts exists (offline mode, skipping fetch)."
        exit 0
    else
        echo "[sync-types] ERROR: $TYPES_FILE does not exist."
        echo "  Run without --offline while the API server is running, or"
        echo "  manually create generated.ts from Rust structs."
        exit 1
    fi
fi

# ── Online mode: fetch spec and generate types ──

echo "[sync-types] Checking API server at $API_URL ..."
if ! curl -sf --connect-timeout 3 "$API_URL/health" > /dev/null 2>&1; then
    echo ""
    echo "  API server not reachable at $API_URL."
    echo "  Start it with:  make api"
    echo "  Or run offline:  bash scripts/sync-types.sh --offline"
    echo ""
    exit 1
fi

echo "[sync-types] Fetching OpenAPI spec from $API_URL/api-docs/openapi.json ..."
curl -sf "$API_URL/api-docs/openapi.json" -o "$SPEC_FILE"
echo "[sync-types] Spec saved to $SPEC_FILE"

echo "[sync-types] Generating TypeScript types ..."
cd "$PROJECT_ROOT/apps/web"
npx openapi-typescript "$SPEC_FILE" -o "src/types/generated.ts"

# ── BND-007: Prepend header mapping each TS interface to its OpenAPI schema ──
echo "[sync-types] Adding OpenAPI schema provenance header ..."
HEADER=$(cat <<'HEADER_EOF'
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
HEADER_EOF
)

# Prepend header to generated file (replace existing header if present)
if [[ -f "$TYPES_FILE" ]]; then
    # Remove any existing header block (lines starting with /** up to */)
    BODY=$(sed -n '/^\/\*\*/,/^\s*\*\//!p' "$TYPES_FILE")
    printf "%s\n\n%s\n" "$HEADER" "$BODY" > "$TYPES_FILE"
fi

echo ""
echo "[sync-types] Done. Types written to $TYPES_FILE"
echo "  Update src/types/api.ts re-exports if new types were added."
