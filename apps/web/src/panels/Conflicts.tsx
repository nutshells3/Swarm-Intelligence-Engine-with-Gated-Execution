import { useConflicts } from '../api/hooks';
import { useUiStore } from '../stores/ui';
import type { ConflictResponse, CompetingArtifactLink } from '../types/generated';

function statusColor(status: string): string {
  switch (status) {
    case 'open': return '#eab308';
    case 'under_adjudication': return '#f97316';
    case 'resolved': return '#22c55e';
    case 'superseded': return '#6b7280';
    case 'dismissed': return '#9ca3af';
    default: return '#94a3b8';
  }
}

function classLabel(cls: string): string {
  switch (cls) {
    case 'divergence': return 'Divergence';
    case 'decomposition': return 'Decomposition';
    case 'evidence': return 'Evidence';
    case 'review_disagreement': return 'Review Disagreement';
    case 'mainline_integration': return 'Mainline Integration';
    default: return cls;
  }
}

function ArtifactList({ artifacts }: { artifacts: CompetingArtifactLink[] }) {
  const selectNode = useUiStore((s) => s.selectNode);

  if (artifacts.length === 0) {
    return <span className="text-muted">No competing artifacts</span>;
  }

  return (
    <ul style={{ margin: 0, paddingLeft: 16 }}>
      {artifacts.map((a, i) => (
        <li key={i} style={{ fontSize: 12, marginBottom: 2 }}>
          <span
            className="mono"
            style={{ cursor: 'pointer', color: '#38bdf8' }}
            onClick={() => selectNode(a.node_id)}
          >
            {a.node_id.slice(0, 8)}
          </span>
          {' '}{a.artifact_summary}
          <span className="text-muted" style={{ marginLeft: 4 }}>
            ({a.artifact_hash.slice(0, 8)})
          </span>
        </li>
      ))}
    </ul>
  );
}

function ConflictCard({ conflict }: { conflict: ConflictResponse }) {
  return (
    <div
      className="metric-card"
      style={{
        borderLeft: `3px solid ${statusColor(conflict.status)}`,
        marginBottom: 8,
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
        <span className="mono" style={{ fontSize: 12 }}>{conflict.conflict_id.slice(0, 12)}</span>
        <span
          className="status-badge"
          style={{ backgroundColor: statusColor(conflict.status), color: '#fff', fontSize: 11 }}
        >
          {conflict.status}
        </span>
      </div>

      <div style={{ marginBottom: 4 }}>
        <span style={{ fontSize: 12, color: '#94a3b8', marginRight: 8 }}>
          Class: <strong style={{ color: '#e2e8f0' }}>{classLabel(conflict.conflict_class)}</strong>
        </span>
        <span style={{ fontSize: 12, color: '#94a3b8' }}>
          Trigger: <strong style={{ color: '#e2e8f0' }}>{conflict.trigger.replace(/_/g, ' ')}</strong>
        </span>
      </div>

      <p style={{ fontSize: 13, color: '#cbd5e1', margin: '4px 0' }}>{conflict.description}</p>

      {conflict.blocks_promotion && (
        <div style={{ fontSize: 12, color: '#ef4444', fontWeight: 600, marginBottom: 4 }}>
          Blocks promotion
        </div>
      )}

      <div style={{ marginTop: 6 }}>
        <strong style={{ fontSize: 12, color: '#94a3b8' }}>Competing artifacts:</strong>
        <ArtifactList artifacts={conflict.competing_artifacts} />
      </div>

      <div style={{ fontSize: 11, color: '#64748b', marginTop: 4 }}>
        Created: {new Date(conflict.created_at).toLocaleString()}
      </div>
    </div>
  );
}

export default function Conflicts() {
  const { data: conflicts, isLoading, error } = useConflicts();

  const openConflicts = (conflicts ?? []).filter(
    (c) => c.status === 'open' || c.status === 'under_adjudication',
  );
  const resolvedConflicts = (conflicts ?? []).filter(
    (c) => c.status !== 'open' && c.status !== 'under_adjudication',
  );

  return (
    <div className="panel">
      <h2>Conflict Queue</h2>

      {isLoading && <p className="text-muted">Loading conflicts...</p>}
      {error && <p style={{ color: '#ef4444' }}>Error loading conflicts: {String(error)}</p>}

      {conflicts && conflicts.length === 0 && (
        <p className="text-muted">No conflicts recorded.</p>
      )}

      {openConflicts.length > 0 && (
        <>
          <h3 style={{ color: '#eab308' }}>
            Open ({openConflicts.length})
          </h3>
          {openConflicts.map((c) => (
            <ConflictCard key={c.conflict_id} conflict={c} />
          ))}
        </>
      )}

      {resolvedConflicts.length > 0 && (
        <>
          <h3 style={{ color: '#22c55e', marginTop: 16 }}>
            Resolved / Closed ({resolvedConflicts.length})
          </h3>
          {resolvedConflicts.map((c) => (
            <ConflictCard key={c.conflict_id} conflict={c} />
          ))}
        </>
      )}
    </div>
  );
}
