import { useBranchMainlineProjection } from '../api/hooks';
import { useUiStore } from '../stores/ui';
import type { BranchMainlineItem } from '../types/generated';

const LANE_META: Record<string, { label: string; color: string }> = {
  branch: { label: 'Branch', color: '#a78bfa' },
  mainline_candidate: { label: 'Mainline Candidate', color: '#38bdf8' },
  mainline: { label: 'Mainline', color: '#22c55e' },
  blocked: { label: 'Blocked', color: '#ef4444' },
  archived: { label: 'Archived', color: '#6b7280' },
  integration: { label: 'Integration', color: '#f59e0b' },
  implementation: { label: 'Implementation', color: '#8b5cf6' },
  planning: { label: 'Planning', color: '#06b6d4' },
  review: { label: 'Review', color: '#eab308' },
  verification: { label: 'Verification', color: '#14b8a6' },
};

function LaneGroup({ laneKey, items }: { laneKey: string; items: BranchMainlineItem[] }) {
  const selectNode = useUiStore((s) => s.selectNode);
  const selectedNodeId = useUiStore((s) => s.selectedNodeId);
  const meta = LANE_META[laneKey] ?? { label: laneKey, color: '#94a3b8' };

  return (
    <div className="lane-group" style={{ marginBottom: 16 }}>
      <h3 style={{ color: meta.color, marginBottom: 4 }}>
        {meta.label}{' '}
        <span className="text-muted" style={{ fontWeight: 400 }}>({items.length})</span>
      </h3>
      {items.length === 0 && (
        <p className="text-muted" style={{ fontSize: 12, paddingLeft: 8 }}>None</p>
      )}
      {items.length > 0 && (
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>ID</th>
                <th>Title</th>
                <th>Lane</th>
                <th>Lifecycle</th>
              </tr>
            </thead>
            <tbody>
              {items.map((node) => (
                <tr
                  key={node.node_id}
                  className={`task-row ${selectedNodeId === node.node_id ? 'selected' : ''}`}
                  onClick={() => selectNode(node.node_id)}
                  style={{ cursor: 'pointer' }}
                >
                  <td className="mono">{node.node_id.slice(0, 8)}</td>
                  <td>{node.title}</td>
                  <td>{node.lane}</td>
                  <td>{node.lifecycle}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

export default function BranchMainline() {
  const { data: projection, isLoading, error } = useBranchMainlineProjection();

  return (
    <div className="panel">
      <h2>Branch / Mainline</h2>

      {isLoading && <p className="text-muted">Loading branch-mainline projection...</p>}
      {error && <p style={{ color: '#ef4444' }}>Error: {String(error)}</p>}

      {projection && (
        <>
          <LaneGroup laneKey="mainline" items={projection.mainline} />
          <LaneGroup laneKey="mainline_candidate" items={projection.mainline_candidate} />
          <LaneGroup laneKey="branch" items={projection.branch} />
          <LaneGroup laneKey="blocked" items={projection.blocked} />
        </>
      )}
    </div>
  );
}
