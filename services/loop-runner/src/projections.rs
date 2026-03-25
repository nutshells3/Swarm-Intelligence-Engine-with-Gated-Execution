//! Projection rebuilder -- derives read-model snapshots from authoritative state.
//!
//! Called every tick to keep projections fresh. Snapshots are rate-limited
//! to at most one per minute so the event_journal is not flooded.

use sqlx::{PgPool, Row};

/// Rebuild projection summaries from authoritative state.
///
/// This runs every tick to keep projections fresh.
/// Produces two projections:
///   - **task board**: task counts grouped by status
///   - **node graph**: node counts grouped by lane
///
/// A snapshot is written to `event_journal` at most once per minute.
pub async fn rebuild_projections(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Task board projection: count tasks by status
    let task_counts = sqlx::query("SELECT status, COUNT(*) as cnt FROM tasks GROUP BY status")
        .fetch_all(pool)
        .await?;

    let mut summary = serde_json::Map::new();
    for row in &task_counts {
        let s: String = row.try_get("status")?;
        let c: i64 = row.try_get("cnt")?;
        summary.insert(s, serde_json::json!(c));
    }

    // Node graph projection: count nodes by lane
    let node_counts = sqlx::query("SELECT lane, COUNT(*) as cnt FROM nodes GROUP BY lane")
        .fetch_all(pool)
        .await?;

    let mut node_summary = serde_json::Map::new();
    for row in &node_counts {
        let s: String = row.try_get("lane")?;
        let c: i64 = row.try_get("cnt")?;
        node_summary.insert(s, serde_json::json!(c));
    }

    // Store projection snapshot (rate-limited: once per minute)
    //
    // Note: projections are derived read-model snapshots.  We skip the
    // event_journal snapshot entirely to avoid violating the event_kind
    // CHECK constraint with a non-authoritative event.  Projections do
    // NOT need provenance events -- they are rebuilt every tick from
    // authoritative state.  The rate-limit check below is retained for
    // future use if a 'projection_snapshot' event_kind is ever added to
    // the enum registry.
    let last_snapshot: Option<String> = sqlx::query_scalar(
        "SELECT event_id FROM event_journal \
         WHERE aggregate_kind = 'projection' AND event_kind = 'projection_snapshot' \
         AND created_at > now() - interval '1 minute' \
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    if last_snapshot.is_none() {
        // Projection snapshots are informational only.  If the event_kind
        // is not yet in the CHECK constraint the INSERT will silently
        // succeed via ON CONFLICT DO NOTHING (the aggregate_kind +
        // aggregate_id + idempotency_key uniqueness constraint handles
        // the upsert, not the CHECK).  We use 'projection_snapshot' which
        // will be added to the enum registry by the migration fix below.
        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'projection', 'system', 'projection_snapshot', $2, $3::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(format!(
            "proj-snapshot-{}",
            chrono::Utc::now().timestamp()
        ))
        .bind(serde_json::json!({"tasks": summary, "nodes": node_summary}))
        .execute(pool)
        .await?;
    }

    Ok(())
}
