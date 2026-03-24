//! UI models -- read-model projections and IDE panel data models (M4).
//!
//! This crate provides:
//!
//! - **Projections** (RDM-001 to RDM-010): Read-only structs that
//!   expose orchestration state as queryable views for UI panels
//!   and agent context windows.
//!
//! - **Panels** (IDE-001 to IDE-013): Data models backing each IDE
//!   surface panel. These are display-only models; all mutations go
//!   through control-plane commands.
//!
//! Key design rule:
//! - Projections are *derived* from authoritative state, never a
//!   hidden second state store.

pub mod panels;
pub mod projections;

// Re-export primary types for ergonomic imports.
pub use panels::{
    ArchitectureReviewPageData, BranchMainlinePanelData, CertificationCardData,
    CertificationQueuePanelData, ComponentDisplay, ConflictCardData, ConflictQueuePanelData,
    CycleComparisonData, DevelopmentDirectionReviewPageData, ExecutionSettingsPanelData,
    GateConditionDisplay, LaneColumnData, LaneNodeCardData, LaneSummary,
    LoopHistoryComparisonPanelData, MilestoneTreeNodeDisplay, MilestoneTreePanelData,
    ObjectiveIntakePanelData, PlanReviewPageData, PlanningPanelData, RiskDisplay,
    SkillPackSummary, SkillTemplatePanelData, TaskBoardCardData, TaskBoardColumn,
    TaskBoardPanelData, UnresolvedQuestionDisplay, WorkerTemplateSummary,
};
pub use projections::{
    ArtifactTimelineItem, ArtifactTimelineProjection, BranchMainlineItem,
    BranchMainlineProjection, CertificationQueueItem, CertificationQueueProjection,
    ConflictQueueItem, ConflictQueueProjection, DriftItem, DriftProjection,
    LoopHistoryCycleItem, LoopHistoryProjection, NodeGraphEdge, NodeGraphItem,
    NodeGraphProjection, ObjectiveProgressItem, ObjectiveProgressProjection, ReviewQueueItem,
    ReviewQueueProjection, TaskBoardItem, TaskBoardProjection,
};
