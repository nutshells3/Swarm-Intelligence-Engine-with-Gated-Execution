use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoadmapActionKind {
    CreateNode,
    AbsorbIntoNode,
    ReprioritizeNode,
    DeferNode,
    RejectNode,
    NoChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoadmapNode {
    pub roadmap_node_id: String,
    pub title: String,
    pub track: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoadmapAbsorptionRecord {
    pub absorption_id: String,
    pub action_kind: RoadmapActionKind,
    pub source_ref: String,
    pub target_ref: Option<String>,
    pub rationale: String,
    pub created_at: DateTime<Utc>,
}
