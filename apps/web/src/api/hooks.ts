import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiGet, apiPost, apiPatch } from './client';
import type {
  ObjectiveResponse,
  CreateObjectiveRequest,
  TaskResponse,
  LoopResponse,
  CycleResponse,
  NodeResponse,
  EventResponse,
  MetaResponse,
  PlanGateResponse,
  MilestoneNodeResponse,
  TaskMetrics,
  SaturationMetrics,
  CertificationQueueEntryResponse,
  PolicySnapshotResponse,
  TaskBoardProjection,
  BranchMainlineProjection,
  CertificationQueueProjection,
  ConflictResponse,
  ReviewResponse,
  ReviewDigestResponse,
  SkillPackResponse,
  WorkerTemplateResponse,
  CertificationSettingsRequest,
  CertificationSettingsResponse,
} from '../types/generated';

// ---- Objectives ----

export function useObjectives() {
  return useQuery<ObjectiveResponse[]>({
    queryKey: ['objectives'],
    queryFn: () => apiGet<ObjectiveResponse[]>('/objectives'),
    refetchInterval: 5000,
  });
}

export function useObjective(id: string) {
  return useQuery<ObjectiveResponse>({
    queryKey: ['objectives', id],
    queryFn: () => apiGet<ObjectiveResponse>(`/objectives/${id}`),
    enabled: !!id,
  });
}

export function useCreateObjective() {
  const qc = useQueryClient();
  return useMutation<ObjectiveResponse, Error, CreateObjectiveRequest>({
    mutationFn: (body) => apiPost<ObjectiveResponse>('/objectives', body),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['objectives'] }),
  });
}

// ---- Tasks ----

export function useTasks() {
  return useQuery<TaskResponse[]>({
    queryKey: ['tasks'],
    queryFn: () => apiGet<TaskResponse[]>('/tasks'),
    refetchInterval: 3000,
  });
}

// ---- Loops ----

export function useLoops() {
  return useQuery<LoopResponse[]>({
    queryKey: ['loops'],
    queryFn: () => apiGet<LoopResponse[]>('/loops'),
    refetchInterval: 5000,
  });
}

// ---- Cycles ----

export function useCycles() {
  return useQuery<CycleResponse[]>({
    queryKey: ['cycles'],
    queryFn: () => apiGet<CycleResponse[]>('/cycles'),
    refetchInterval: 5000,
  });
}

// ---- Nodes ----

export function useNodes() {
  return useQuery<NodeResponse[]>({
    queryKey: ['nodes'],
    queryFn: () => apiGet<NodeResponse[]>('/nodes'),
    refetchInterval: 5000,
  });
}

// ---- Policies ----

export function usePolicies() {
  return useQuery<PolicySnapshotResponse[]>({
    queryKey: ['policies'],
    queryFn: () => apiGet<PolicySnapshotResponse[]>('/policies'),
  });
}

// ---- Events ----

export function useEvents() {
  return useQuery<EventResponse[]>({
    queryKey: ['events'],
    queryFn: () => apiGet<EventResponse[]>('/events'),
    refetchInterval: 2000,
  });
}

// ---- Meta ----

export function useMeta() {
  return useQuery<MetaResponse>({
    queryKey: ['meta'],
    queryFn: () => apiGet<MetaResponse>('/meta'),
    refetchInterval: 5000,
  });
}

// ---- Plan Gate ----

export function usePlanGate(objectiveId: string) {
  return useQuery<PlanGateResponse | null>({
    queryKey: ['plan-gate', objectiveId],
    queryFn: () => apiGet<PlanGateResponse | null>(`/objectives/${objectiveId}/gate`),
    enabled: !!objectiveId,
    refetchInterval: 5000,
  });
}

export function useMilestones(objectiveId: string) {
  return useQuery<MilestoneNodeResponse[]>({
    queryKey: ['milestones', objectiveId],
    queryFn: () => apiGet<MilestoneNodeResponse[]>(`/objectives/${objectiveId}/milestones`),
    enabled: !!objectiveId,
    refetchInterval: 5000,
  });
}

// ---- Task Metrics ----

export function useTaskMetrics() {
  return useQuery<TaskMetrics>({
    queryKey: ['metrics-tasks'],
    queryFn: () => apiGet<TaskMetrics>('/metrics/tasks'),
    refetchInterval: 2000,
  });
}

// ---- Saturation Metrics ----

export function useSaturationMetrics() {
  return useQuery<SaturationMetrics>({
    queryKey: ['metrics-saturation'],
    queryFn: () => apiGet<SaturationMetrics>('/metrics/saturation'),
    refetchInterval: 2000,
  });
}

// ---- Certification Queue ----

export function useCertificationQueue() {
  return useQuery<CertificationQueueEntryResponse[]>({
    queryKey: ['certification-queue'],
    queryFn: () => apiGet<CertificationQueueEntryResponse[]>('/certification/queue'),
    refetchInterval: 5000,
  });
}

// ---- Projections ----

export function useTaskBoardProjection() {
  return useQuery<TaskBoardProjection>({
    queryKey: ['projection-task-board'],
    queryFn: () => apiGet<TaskBoardProjection>('/projections/task-board'),
    refetchInterval: 3000,
  });
}

export function useBranchMainlineProjection() {
  return useQuery<BranchMainlineProjection>({
    queryKey: ['projection-branch-mainline'],
    queryFn: () => apiGet<BranchMainlineProjection>('/projections/branch-mainline'),
    refetchInterval: 5000,
  });
}

export function useCertificationQueueProjection() {
  return useQuery<CertificationQueueProjection>({
    queryKey: ['projection-certification-queue'],
    queryFn: () => apiGet<CertificationQueueProjection>('/projections/certification-queue'),
    refetchInterval: 5000,
  });
}

// ---- Conflicts ----

export function useConflicts() {
  return useQuery<ConflictResponse[]>({
    queryKey: ['conflicts'],
    queryFn: () => apiGet<ConflictResponse[]>('/conflicts'),
    refetchInterval: 5000,
  });
}

// ---- Reviews (IDE-011 / IDE-012 / IDE-013) ----

export function useReviews() {
  return useQuery<ReviewResponse[]>({
    queryKey: ['reviews'],
    queryFn: () => apiGet<ReviewResponse[]>('/reviews'),
    refetchInterval: 5000,
  });
}

export function useApproveReview() {
  const qc = useQueryClient();
  return useMutation<unknown, Error, { reviewId: string; verdict?: string; approval_effect?: string }>({
    mutationFn: ({ reviewId, verdict, approval_effect }) =>
      apiPost(`/reviews/${reviewId}/approve`, { verdict: verdict ?? null, approval_effect: approval_effect ?? null }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['reviews'] }),
  });
}

// REV-019: Human digest summary
export function useReviewDigest() {
  return useMutation<ReviewDigestResponse, Error, { objectiveId: string }>({
    mutationFn: ({ objectiveId }) =>
      apiGet<ReviewDigestResponse>(`/reviews/digest?objective_id=${encodeURIComponent(objectiveId)}`),
  });
}

// ---- Skills (IDE-009) ----

export function useSkillPacks() {
  return useQuery<SkillPackResponse[]>({
    queryKey: ['skills'],
    queryFn: () => apiGet<SkillPackResponse[]>('/skills'),
    refetchInterval: 10000,
  });
}

export function useWorkerTemplates() {
  return useQuery<WorkerTemplateResponse[]>({
    queryKey: ['templates'],
    queryFn: () => apiGet<WorkerTemplateResponse[]>('/templates'),
    refetchInterval: 10000,
  });
}

// ---- Settings (IDE-008) ----

export function usePolicy(id: string) {
  return useQuery<PolicySnapshotResponse>({
    queryKey: ['policies', id],
    queryFn: () => apiGet<PolicySnapshotResponse>(`/policies/${id}`),
    enabled: !!id,
  });
}

export function useUpdateCertificationConfig() {
  const qc = useQueryClient();
  return useMutation<CertificationSettingsResponse, Error, CertificationSettingsRequest>({
    mutationFn: (body) =>
      apiPatch<CertificationSettingsResponse>('/certification/config', body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['policies'] });
    },
  });
}
