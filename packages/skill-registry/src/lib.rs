use serde::{Deserialize, Serialize};

// ── SKL-001/002: ProviderMode re-export from worker-protocol ─────────────
// The canonical enum lives in worker-protocol. We define a local mirror
// for skill-registry consumers that don't pull in worker-protocol.

/// Provider mode for task execution (mirrors worker_protocol::ProviderMode).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    Api,
    Session,
    Local,
}

impl std::fmt::Display for ProviderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderMode::Api => write!(f, "api"),
            ProviderMode::Session => write!(f, "session"),
            ProviderMode::Local => write!(f, "local"),
        }
    }
}

impl std::str::FromStr for ProviderMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "api" => Ok(ProviderMode::Api),
            "session" => Ok(ProviderMode::Session),
            "local" => Ok(ProviderMode::Local),
            other => Err(format!("unknown provider_mode: '{}'", other)),
        }
    }
}

/// Model binding specifying which AI model to use (typed struct, not String).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelBinding {
    pub provider_name: Option<String>,
    pub model_name: Option<String>,
    pub reasoning_effort: Option<String>,
}

// ── SKL-001~005: SkillPackManifest ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillPackManifest {
    pub skill_pack_id: String,
    pub worker_role: String,
    pub description: String,
    pub accepted_task_kinds: Vec<String>,
    pub references: Vec<String>,
    pub scripts: Vec<String>,
    /// SKL-005: Contract describing the expected shape of worker output.
    #[serde(default)]
    pub expected_output_contract: Option<String>,
    /// SKL-012: Semantic version pin for this skill pack.
    #[serde(default)]
    pub version: Option<String>,
    /// SKL-013: Soft deprecation flag. Deprecated packs are filtered from
    /// resolution unless explicitly overridden.
    #[serde(default)]
    pub deprecated: bool,
}

// ── SKL-003/004: WorkerTemplate (typed fields) ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerTemplate {
    pub template_id: String,
    pub role: String,
    pub skill_pack_id: String,
    /// SKL-004: Typed enum instead of bare String.
    pub provider_mode: ProviderMode,
    /// SKL-004: Typed struct instead of bare String.
    pub model_binding: ModelBinding,
    pub allowed_task_kinds: Vec<String>,
}

// ── Skill pack row (database representation) ────────────────────────────

/// Represents a row from the `skill_packs` database table, used as input
/// for `SkillRegistryLoader::load_from_rows`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillPackRow {
    pub skill_pack_id: String,
    pub worker_role: String,
    pub description: String,
    pub accepted_task_kinds: Vec<String>,
    pub references: Vec<String>,
    pub scripts: Vec<String>,
    #[serde(default)]
    pub expected_output_contract: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub deprecated: bool,
}

// ── Skill resolution types ──────────────────────────────────────────────

/// The result of resolving a task to a skill pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillResolution {
    /// The selected skill pack ID.
    pub skill_pack_id: String,
    /// Human-readable reason for the selection.
    pub selection_reason: String,
    /// Which level in the resolution order was used.
    /// One of: "task_override", "node_override", "task_kind",
    /// "phase_restriction", "role_default", "project_default",
    /// "global_fallback", "escalation".
    /// Legacy values "override", "role", "fallback" are still produced
    /// by the original `resolve_skill` / `resolve_skill_with_task_id` methods.
    pub selection_level: String,
}

/// A per-task or per-task-kind override that pins a specific skill pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillOverride {
    /// If set, this override applies only to this specific task.
    pub task_id: Option<String>,
    /// If set, this override applies to all tasks of this kind.
    pub task_kind: Option<String>,
    /// The skill pack to use when this override matches.
    pub skill_pack_id: String,
}

/// A validation error found while checking a skill pack manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillValidationError {
    /// The field that failed validation.
    pub field: String,
    /// Description of the validation failure.
    pub message: String,
}

// ── Skill registry loader ───────────────────────────────────────────────

/// Loads, validates, and resolves skill packs.
pub struct SkillRegistryLoader;

/// SKL-007: Naming convention regex for skill_pack_id.
/// Allowed: lowercase alphanumeric, hyphens, underscores. Must start with a letter.
fn is_valid_skill_pack_id(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let bytes = id.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl SkillRegistryLoader {
    /// Load skill pack manifests from database rows.
    pub fn load_from_rows(rows: &[SkillPackRow]) -> Vec<SkillPackManifest> {
        rows.iter()
            .map(|row| SkillPackManifest {
                skill_pack_id: row.skill_pack_id.clone(),
                worker_role: row.worker_role.clone(),
                description: row.description.clone(),
                accepted_task_kinds: row.accepted_task_kinds.clone(),
                references: row.references.clone(),
                scripts: row.scripts.clone(),
                expected_output_contract: row.expected_output_contract.clone(),
                version: row.version.clone(),
                deprecated: row.deprecated,
            })
            .collect()
    }

    /// SKL-007: Validate a skill pack manifest, returning any errors found.
    /// Strengthened: checks accepted_task_kinds non-empty, worker_role non-empty,
    /// and skill_pack_id naming convention.
    pub fn validate(manifest: &SkillPackManifest) -> Vec<SkillValidationError> {
        let mut errors = Vec::new();

        if manifest.skill_pack_id.is_empty() {
            errors.push(SkillValidationError {
                field: "skill_pack_id".to_string(),
                message: "skill_pack_id must not be empty".to_string(),
            });
        } else if !is_valid_skill_pack_id(&manifest.skill_pack_id) {
            errors.push(SkillValidationError {
                field: "skill_pack_id".to_string(),
                message: "skill_pack_id must start with a lowercase letter and contain only \
                          lowercase alphanumeric characters, hyphens, or underscores"
                    .to_string(),
            });
        }

        if manifest.worker_role.is_empty() {
            errors.push(SkillValidationError {
                field: "worker_role".to_string(),
                message: "worker_role must not be empty".to_string(),
            });
        }

        if manifest.description.is_empty() {
            errors.push(SkillValidationError {
                field: "description".to_string(),
                message: "description must not be empty".to_string(),
            });
        }

        if manifest.accepted_task_kinds.is_empty() {
            errors.push(SkillValidationError {
                field: "accepted_task_kinds".to_string(),
                message: "accepted_task_kinds must contain at least one entry".to_string(),
            });
        }

        errors
    }

    /// SKL-008: Validate a worker template, returning any errors found.
    /// Checks: template_id non-empty, role non-empty, skill_pack_id references
    /// an existing skill pack.
    pub fn validate_template(
        template: &WorkerTemplate,
        known_skill_packs: &[SkillPackManifest],
    ) -> Vec<SkillValidationError> {
        let mut errors = Vec::new();

        if template.template_id.is_empty() {
            errors.push(SkillValidationError {
                field: "template_id".to_string(),
                message: "template_id must not be empty".to_string(),
            });
        }

        if template.role.is_empty() {
            errors.push(SkillValidationError {
                field: "role".to_string(),
                message: "role must not be empty".to_string(),
            });
        }

        if template.skill_pack_id.is_empty() {
            errors.push(SkillValidationError {
                field: "skill_pack_id".to_string(),
                message: "skill_pack_id must not be empty".to_string(),
            });
        } else if !known_skill_packs
            .iter()
            .any(|sp| sp.skill_pack_id == template.skill_pack_id)
        {
            errors.push(SkillValidationError {
                field: "skill_pack_id".to_string(),
                message: format!(
                    "skill_pack_id '{}' does not reference a known skill pack",
                    template.skill_pack_id
                ),
            });
        }

        if template.allowed_task_kinds.is_empty() {
            errors.push(SkillValidationError {
                field: "allowed_task_kinds".to_string(),
                message: "allowed_task_kinds must contain at least one entry".to_string(),
            });
        }

        errors
    }

    /// Match a task to the best skill pack using resolution order:
    /// 1. explicit per-task override (by task_id if provided via `task_id` param)
    /// 2. task-kind mapping (override by task_kind, or skill pack accepting this kind)
    /// 3. role default (skill pack matching worker_role)
    /// 4. global fallback (first available skill pack)
    pub fn resolve_skill(
        task_kind: &str,
        worker_role: &str,
        overrides: &[SkillOverride],
        available: &[SkillPackManifest],
    ) -> Option<SkillResolution> {
        Self::resolve_skill_with_task_id(None, task_kind, worker_role, overrides, available)
    }

    /// Full resolution with an optional explicit task ID for per-task overrides.
    ///
    /// SKL-013: Deprecated packs are excluded from levels 2-4 (task-kind,
    /// role, fallback). They can still be selected via explicit per-task
    /// overrides (level 1) to support migration scenarios.
    pub fn resolve_skill_with_task_id(
        task_id: Option<&str>,
        task_kind: &str,
        worker_role: &str,
        overrides: &[SkillOverride],
        available: &[SkillPackManifest],
    ) -> Option<SkillResolution> {
        // 1. Explicit per-task override (allows deprecated packs)
        if let Some(tid) = task_id {
            for ov in overrides {
                if ov.task_id.as_deref() == Some(tid) {
                    if available.iter().any(|sp| sp.skill_pack_id == ov.skill_pack_id) {
                        return Some(SkillResolution {
                            skill_pack_id: ov.skill_pack_id.clone(),
                            selection_reason: format!(
                                "explicit per-task override for task '{}'",
                                tid
                            ),
                            selection_level: "override".to_string(),
                        });
                    }
                }
            }
        }

        // SKL-013: For levels 2-4, exclude deprecated packs.
        let active: Vec<&SkillPackManifest> = available.iter().filter(|sp| !sp.deprecated).collect();

        // 2. Task-kind mapping: first check overrides keyed by task_kind,
        //    then check skill packs that accept this task kind.
        for ov in overrides {
            if ov.task_id.is_none() && ov.task_kind.as_deref() == Some(task_kind) {
                if active.iter().any(|sp| sp.skill_pack_id == ov.skill_pack_id) {
                    return Some(SkillResolution {
                        skill_pack_id: ov.skill_pack_id.clone(),
                        selection_reason: format!(
                            "task-kind override for kind '{}'",
                            task_kind
                        ),
                        selection_level: "task_kind".to_string(),
                    });
                }
            }
        }

        // Task-kind mapping from active (non-deprecated) skill packs
        for sp in &active {
            if sp.accepted_task_kinds.iter().any(|k| k == task_kind) {
                return Some(SkillResolution {
                    skill_pack_id: sp.skill_pack_id.clone(),
                    selection_reason: format!(
                        "skill pack '{}' accepts task kind '{}'",
                        sp.skill_pack_id, task_kind
                    ),
                    selection_level: "task_kind".to_string(),
                });
            }
        }

        // 3. Role default: match by worker_role (active packs only)
        for sp in &active {
            if sp.worker_role == worker_role {
                return Some(SkillResolution {
                    skill_pack_id: sp.skill_pack_id.clone(),
                    selection_reason: format!(
                        "role default for worker role '{}'",
                        worker_role
                    ),
                    selection_level: "role".to_string(),
                });
            }
        }

        // 4. Global fallback: first active (non-deprecated) skill pack
        active.first().map(|sp| SkillResolution {
            skill_pack_id: sp.skill_pack_id.clone(),
            selection_reason: "global fallback (first available skill pack)".to_string(),
            selection_level: "fallback".to_string(),
        })
    }

    /// Full 8-level skill resolution per SKILL_RESOLUTION_ORDER.md.
    ///
    /// Precedence:
    ///   1. explicit per-task override
    ///   2. explicit node-level override
    ///   3. task-kind mapping
    ///   4. phase-specific restriction
    ///   5. role default
    ///   6. project default
    ///   7. global fallback
    ///   8. no-match -> escalation
    ///
    /// SKL-013: Deprecated packs are excluded from levels 3-7. They can
    /// still be selected via explicit overrides (levels 1-2) to support
    /// migration scenarios.
    pub fn resolve_skill_full(
        task_id: Option<&str>,
        node_id: Option<&str>,
        task_kind: &str,
        worker_role: &str,
        current_phase: Option<&str>,
        task_overrides: &[SkillOverride],
        node_overrides: &[SkillOverride],
        phase_restrictions: &[(String, Vec<String>)], // (phase, allowed_skill_ids)
        project_default: Option<&str>,
        global_fallback: Option<&str>,
        available: &[SkillPackManifest],
    ) -> SkillResolution {
        // Level 1: explicit per-task override (allows deprecated)
        if let Some(tid) = task_id {
            if let Some(ov) = task_overrides
                .iter()
                .find(|o| o.task_id.as_deref() == Some(tid))
            {
                if available.iter().any(|s| s.skill_pack_id == ov.skill_pack_id) {
                    return SkillResolution {
                        skill_pack_id: ov.skill_pack_id.clone(),
                        selection_reason: format!("per-task override for {}", tid),
                        selection_level: "task_override".to_string(),
                    };
                }
            }
        }

        // Level 2: explicit node-level override (allows deprecated)
        if let Some(nid) = node_id {
            if let Some(ov) = node_overrides
                .iter()
                .find(|o| o.task_id.as_deref() == Some(nid))
            {
                if available.iter().any(|s| s.skill_pack_id == ov.skill_pack_id) {
                    return SkillResolution {
                        skill_pack_id: ov.skill_pack_id.clone(),
                        selection_reason: format!("node-level override for {}", nid),
                        selection_level: "node_override".to_string(),
                    };
                }
            }
        }

        // SKL-013: For levels 3-7, exclude deprecated packs.
        let active: Vec<&SkillPackManifest> = available.iter().filter(|s| !s.deprecated).collect();

        // Level 3: task-kind mapping
        if let Some(skill) = active
            .iter()
            .find(|s| s.accepted_task_kinds.iter().any(|k| k == task_kind))
        {
            return SkillResolution {
                skill_pack_id: skill.skill_pack_id.clone(),
                selection_reason: format!("task-kind mapping for '{}'", task_kind),
                selection_level: "task_kind".to_string(),
            };
        }

        // Level 4: phase-specific restriction
        if let Some(phase) = current_phase {
            if let Some((_, allowed)) = phase_restrictions.iter().find(|(p, _)| p == phase) {
                if let Some(skill) = active
                    .iter()
                    .find(|s| allowed.contains(&s.skill_pack_id))
                {
                    return SkillResolution {
                        skill_pack_id: skill.skill_pack_id.clone(),
                        selection_reason: format!("phase restriction for '{}'", phase),
                        selection_level: "phase_restriction".to_string(),
                    };
                }
            }
        }

        // Level 5: role default
        if let Some(skill) = active.iter().find(|s| s.worker_role == worker_role) {
            return SkillResolution {
                skill_pack_id: skill.skill_pack_id.clone(),
                selection_reason: format!("role default for '{}'", worker_role),
                selection_level: "role_default".to_string(),
            };
        }

        // Level 6: project default
        if let Some(pd) = project_default {
            if active.iter().any(|s| s.skill_pack_id == pd) {
                return SkillResolution {
                    skill_pack_id: pd.to_string(),
                    selection_reason: "project default".to_string(),
                    selection_level: "project_default".to_string(),
                };
            }
        }

        // Level 7: global fallback
        if let Some(gf) = global_fallback {
            if active.iter().any(|s| s.skill_pack_id == gf) {
                return SkillResolution {
                    skill_pack_id: gf.to_string(),
                    selection_reason: "global fallback".to_string(),
                    selection_level: "global_fallback".to_string(),
                };
            }
        }

        // Level 8: no match -> escalation
        SkillResolution {
            skill_pack_id: "unresolved".to_string(),
            selection_reason: "no matching skill found -- escalation required".to_string(),
            selection_level: "escalation".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a SkillPackManifest with sensible defaults.
    fn manifest(id: &str, role: &str, kinds: &[&str]) -> SkillPackManifest {
        SkillPackManifest {
            skill_pack_id: id.to_string(),
            worker_role: role.to_string(),
            description: format!("{} skill", id),
            accepted_task_kinds: kinds.iter().map(|k| k.to_string()).collect(),
            references: vec![],
            scripts: vec![],
            expected_output_contract: None,
            version: None,
            deprecated: false,
        }
    }

    fn task_override(task_id: &str, skill: &str) -> SkillOverride {
        SkillOverride {
            task_id: Some(task_id.to_string()),
            task_kind: None,
            skill_pack_id: skill.to_string(),
        }
    }

    /// Build a standard set of available skills for multi-level tests.
    fn standard_available() -> Vec<SkillPackManifest> {
        vec![
            manifest("sk-plan", "planner", &["planning"]),
            manifest("sk-impl", "implementer", &["coding"]),
            manifest("sk-review", "reviewer", &["review"]),
            manifest("sk-fallback", "ops", &["misc"]),
        ]
    }

    // ── Level 1: per-task override ──────────────────────────────────────

    #[test]
    fn level1_task_override_wins() {
        let available = standard_available();
        let task_ov = vec![task_override("task-42", "sk-review")];
        let res = SkillRegistryLoader::resolve_skill_full(
            Some("task-42"),
            None,
            "coding",      // would match sk-impl at level 3
            "implementer", // would match sk-impl at level 5
            None,
            &task_ov,
            &[],
            &[],
            None,
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-review");
        assert_eq!(res.selection_level, "task_override");
    }

    #[test]
    fn level1_task_override_skipped_when_skill_missing() {
        let available = standard_available();
        // Override points to a skill that does not exist
        let task_ov = vec![task_override("task-42", "sk-nonexistent")];
        let res = SkillRegistryLoader::resolve_skill_full(
            Some("task-42"),
            None,
            "coding",
            "implementer",
            None,
            &task_ov,
            &[],
            &[],
            None,
            None,
            &available,
        );
        // Falls through to level 3 (task-kind)
        assert_eq!(res.skill_pack_id, "sk-impl");
        assert_eq!(res.selection_level, "task_kind");
    }

    // ── Level 2: node-level override ────────────────────────────────────

    #[test]
    fn level2_node_override_wins_over_task_kind() {
        let available = standard_available();
        let node_ov = vec![task_override("node-7", "sk-plan")];
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            Some("node-7"),
            "coding", // would match sk-impl at level 3
            "implementer",
            None,
            &[],
            &node_ov,
            &[],
            None,
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-plan");
        assert_eq!(res.selection_level, "node_override");
    }

    // ── Level 3: task-kind mapping ──────────────────────────────────────

    #[test]
    fn level3_task_kind_mapping() {
        let available = standard_available();
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            None,
            "review",
            "ops", // would match sk-fallback at level 5
            None,
            &[],
            &[],
            &[],
            None,
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-review");
        assert_eq!(res.selection_level, "task_kind");
    }

    // ── Level 4: phase-specific restriction ─────────────────────────────

    #[test]
    fn level4_phase_restriction() {
        let available = standard_available();
        let restrictions = vec![(
            "certification".to_string(),
            vec!["sk-review".to_string()],
        )];
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            None,
            "unknown-kind", // no match at level 3
            "nobody",       // no match at level 5
            Some("certification"),
            &[],
            &[],
            &restrictions,
            None,
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-review");
        assert_eq!(res.selection_level, "phase_restriction");
    }

    // ── Level 5: role default ───────────────────────────────────────────

    #[test]
    fn level5_role_default() {
        let available = standard_available();
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            None,
            "unknown-kind",
            "reviewer",
            None,
            &[],
            &[],
            &[],
            None,
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-review");
        assert_eq!(res.selection_level, "role_default");
    }

    // ── Level 6: project default ────────────────────────────────────────

    #[test]
    fn level6_project_default() {
        let available = standard_available();
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            None,
            "unknown-kind",
            "nobody",
            None,
            &[],
            &[],
            &[],
            Some("sk-fallback"),
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-fallback");
        assert_eq!(res.selection_level, "project_default");
    }

    // ── Level 7: global fallback ────────────────────────────────────────

    #[test]
    fn level7_global_fallback() {
        let available = standard_available();
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            None,
            "unknown-kind",
            "nobody",
            None,
            &[],
            &[],
            &[],
            None,
            Some("sk-impl"),
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-impl");
        assert_eq!(res.selection_level, "global_fallback");
    }

    // ── Level 8: escalation ─────────────────────────────────────────────

    #[test]
    fn level8_escalation_when_nothing_matches() {
        let available = standard_available();
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            None,
            "unknown-kind",
            "nobody",
            None,
            &[],
            &[],
            &[],
            None,
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "unresolved");
        assert_eq!(res.selection_level, "escalation");
        assert!(res.selection_reason.contains("escalation"));
    }

    #[test]
    fn level8_escalation_empty_available() {
        let res = SkillRegistryLoader::resolve_skill_full(
            Some("task-1"),
            Some("node-1"),
            "coding",
            "implementer",
            Some("execution"),
            &[task_override("task-1", "sk-gone")],
            &[task_override("node-1", "sk-also-gone")],
            &[("execution".to_string(), vec!["sk-nope".to_string()])],
            Some("sk-nope"),
            Some("sk-nope"),
            &[], // empty
        );
        assert_eq!(res.skill_pack_id, "unresolved");
        assert_eq!(res.selection_level, "escalation");
    }

    // ── Precedence integration: higher levels shadow lower ones ─────────

    #[test]
    fn precedence_task_override_shadows_everything() {
        let available = standard_available();
        let task_ov = vec![task_override("t1", "sk-plan")];
        let node_ov = vec![task_override("n1", "sk-review")];
        let restrictions = vec![(
            "exec".to_string(),
            vec!["sk-fallback".to_string()],
        )];
        let res = SkillRegistryLoader::resolve_skill_full(
            Some("t1"),
            Some("n1"),
            "coding",
            "reviewer",
            Some("exec"),
            &task_ov,
            &node_ov,
            &restrictions,
            Some("sk-impl"),
            Some("sk-fallback"),
            &available,
        );
        assert_eq!(res.selection_level, "task_override");
        assert_eq!(res.skill_pack_id, "sk-plan");
    }

    #[test]
    fn precedence_node_override_shadows_task_kind_and_below() {
        let available = standard_available();
        let node_ov = vec![task_override("n1", "sk-plan")];
        let res = SkillRegistryLoader::resolve_skill_full(
            None, // no task override
            Some("n1"),
            "coding",  // would match sk-impl at level 3
            "reviewer", // would match sk-review at level 5
            None,
            &[],
            &node_ov,
            &[],
            Some("sk-fallback"),
            Some("sk-impl"),
            &available,
        );
        assert_eq!(res.selection_level, "node_override");
        assert_eq!(res.skill_pack_id, "sk-plan");
    }

    // ── Existing API backward compatibility ─────────────────────────────

    #[test]
    fn legacy_resolve_skill_still_works() {
        let available = standard_available();
        let res = SkillRegistryLoader::resolve_skill(
            "coding",
            "implementer",
            &[],
            &available,
        );
        assert!(res.is_some());
        let r = res.unwrap();
        assert_eq!(r.skill_pack_id, "sk-impl");
        assert_eq!(r.selection_level, "task_kind");
    }

    #[test]
    fn legacy_resolve_skill_with_task_id_override() {
        let available = standard_available();
        let overrides = vec![task_override("t1", "sk-review")];
        let res = SkillRegistryLoader::resolve_skill_with_task_id(
            Some("t1"),
            "coding",
            "implementer",
            &overrides,
            &available,
        );
        assert!(res.is_some());
        let r = res.unwrap();
        assert_eq!(r.skill_pack_id, "sk-review");
        assert_eq!(r.selection_level, "override");
    }

    // ── Validation tests ────────────────────────────────────────────────

    #[test]
    fn validate_catches_empty_fields() {
        let bad = SkillPackManifest {
            skill_pack_id: "".to_string(),
            worker_role: "".to_string(),
            description: "".to_string(),
            accepted_task_kinds: vec![],
            references: vec![],
            scripts: vec![],
            expected_output_contract: None,
            version: None,
            deprecated: false,
        };
        let errors = SkillRegistryLoader::validate(&bad);
        assert_eq!(errors.len(), 4);
    }

    #[test]
    fn validate_catches_bad_naming_convention() {
        let bad = SkillPackManifest {
            skill_pack_id: "123-bad".to_string(),
            worker_role: "role".to_string(),
            description: "desc".to_string(),
            accepted_task_kinds: vec!["coding".to_string()],
            references: vec![],
            scripts: vec![],
            expected_output_contract: None,
            version: None,
            deprecated: false,
        };
        let errors = SkillRegistryLoader::validate(&bad);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "skill_pack_id");
    }

    #[test]
    fn validate_accepts_good_naming() {
        let good = manifest("sk-valid-name_01", "role", &["coding"]);
        let errors = SkillRegistryLoader::validate(&good);
        assert!(errors.is_empty());
    }

    #[test]
    fn load_from_rows_round_trips() {
        let rows = vec![SkillPackRow {
            skill_pack_id: "sp-1".to_string(),
            worker_role: "implementer".to_string(),
            description: "test".to_string(),
            accepted_task_kinds: vec!["coding".to_string()],
            references: vec![],
            scripts: vec![],
            expected_output_contract: Some("json_schema".to_string()),
            version: Some("1.0.0".to_string()),
            deprecated: false,
        }];
        let manifests = SkillRegistryLoader::load_from_rows(&rows);
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].skill_pack_id, "sp-1");
        assert_eq!(manifests[0].expected_output_contract, Some("json_schema".to_string()));
        assert_eq!(manifests[0].version, Some("1.0.0".to_string()));
        assert!(!manifests[0].deprecated);
    }

    // ── SKL-008: Worker template validation ─────────────────────────────

    #[test]
    fn validate_template_catches_empty_fields() {
        let known = standard_available();
        let bad = WorkerTemplate {
            template_id: "".to_string(),
            role: "".to_string(),
            skill_pack_id: "".to_string(),
            provider_mode: ProviderMode::Api,
            model_binding: ModelBinding::default(),
            allowed_task_kinds: vec![],
        };
        let errors = SkillRegistryLoader::validate_template(&bad, &known);
        assert_eq!(errors.len(), 4); // template_id, role, skill_pack_id empty, allowed_task_kinds empty
    }

    #[test]
    fn validate_template_catches_unknown_skill_pack() {
        let known = standard_available();
        let bad = WorkerTemplate {
            template_id: "tmpl-1".to_string(),
            role: "coder".to_string(),
            skill_pack_id: "nonexistent-pack".to_string(),
            provider_mode: ProviderMode::Api,
            model_binding: ModelBinding::default(),
            allowed_task_kinds: vec!["coding".to_string()],
        };
        let errors = SkillRegistryLoader::validate_template(&bad, &known);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "skill_pack_id");
        assert!(errors[0].message.contains("nonexistent-pack"));
    }

    #[test]
    fn validate_template_passes_for_valid() {
        let known = standard_available();
        let good = WorkerTemplate {
            template_id: "tmpl-1".to_string(),
            role: "implementer".to_string(),
            skill_pack_id: "sk-impl".to_string(),
            provider_mode: ProviderMode::Session,
            model_binding: ModelBinding {
                provider_name: Some("anthropic".to_string()),
                model_name: Some("claude-4".to_string()),
                reasoning_effort: None,
            },
            allowed_task_kinds: vec!["coding".to_string()],
        };
        let errors = SkillRegistryLoader::validate_template(&good, &known);
        assert!(errors.is_empty());
    }

    // ── SKL-013: Deprecated packs filtered from resolution ──────────────

    #[test]
    fn deprecated_pack_excluded_from_task_kind_resolution() {
        let mut available = standard_available();
        // Deprecate the coding skill
        available[1].deprecated = true;
        let res = SkillRegistryLoader::resolve_skill_full(
            None,
            None,
            "coding",
            "nobody",
            None,
            &[],
            &[],
            &[],
            None,
            None,
            &available,
        );
        // sk-impl is deprecated so it won't match at level 3; falls to escalation
        assert_eq!(res.selection_level, "escalation");
    }

    #[test]
    fn deprecated_pack_still_selectable_via_override() {
        let mut available = standard_available();
        available[1].deprecated = true;
        let task_ov = vec![task_override("t1", "sk-impl")];
        let res = SkillRegistryLoader::resolve_skill_full(
            Some("t1"),
            None,
            "unknown",
            "nobody",
            None,
            &task_ov,
            &[],
            &[],
            None,
            None,
            &available,
        );
        assert_eq!(res.skill_pack_id, "sk-impl");
        assert_eq!(res.selection_level, "task_override");
    }

    // ── ProviderMode / ModelBinding type tests ──────────────────────────

    #[test]
    fn provider_mode_roundtrip() {
        assert_eq!("api".parse::<ProviderMode>().unwrap(), ProviderMode::Api);
        assert_eq!("session".parse::<ProviderMode>().unwrap(), ProviderMode::Session);
        assert_eq!("local".parse::<ProviderMode>().unwrap(), ProviderMode::Local);
        assert!("unknown".parse::<ProviderMode>().is_err());
    }

    #[test]
    fn provider_mode_display() {
        assert_eq!(ProviderMode::Api.to_string(), "api");
        assert_eq!(ProviderMode::Session.to_string(), "session");
        assert_eq!(ProviderMode::Local.to_string(), "local");
    }
}
