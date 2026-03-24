import { useCertificationQueueProjection, useCertificationQueue } from '../api/hooks';
import { useUiStore } from '../stores/ui';
import type {
  CertificationQueueItem,
  CertificationQueueEntryResponse,
} from '../types/generated';

function statusColor(status: string): string {
  switch (status) {
    case 'pending': return '#eab308';
    case 'passed': return '#22c55e';
    case 'failed': return '#ef4444';
    default: return '#94a3b8';
  }
}

function ProjectionSection({ items, pendingCount }: { items: CertificationQueueItem[]; pendingCount: number }) {
  const selectNode = useUiStore((s) => s.selectNode);
  const selectedNodeId = useUiStore((s) => s.selectedNodeId);

  return (
    <div style={{ marginBottom: 16 }}>
      <h3>
        Queue Overview{' '}
        <span className="text-muted" style={{ fontWeight: 400, fontSize: 13 }}>
          ({pendingCount} pending)
        </span>
      </h3>
      {items.length === 0 && (
        <p className="text-muted">No items in the certification queue.</p>
      )}
      {items.length > 0 && (
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>Node ID</th>
                <th>Title</th>
                <th>Lane</th>
                <th>Lifecycle</th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => (
                <tr
                  key={item.node_id}
                  className={`task-row ${selectedNodeId === item.node_id ? 'selected' : ''}`}
                  onClick={() => selectNode(item.node_id)}
                  style={{ cursor: 'pointer' }}
                >
                  <td className="mono">{item.node_id.slice(0, 8)}</td>
                  <td>{item.title}</td>
                  <td>{item.lane}</td>
                  <td>{item.lifecycle}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function DetailSection({ entries }: { entries: CertificationQueueEntryResponse[] }) {
  if (entries.length === 0) return null;

  const pending = entries.filter((e) => e.queue_status === 'pending');
  const passed = entries.filter((e) => e.queue_status === 'passed');
  const failed = entries.filter((e) => e.queue_status === 'failed');
  const other = entries.filter(
    (e) => e.queue_status !== 'pending' && e.queue_status !== 'passed' && e.queue_status !== 'failed',
  );

  function renderGroup(label: string, items: CertificationQueueEntryResponse[], color: string) {
    if (items.length === 0) return null;
    return (
      <div style={{ marginBottom: 12 }}>
        <h4 style={{ color, marginBottom: 4 }}>
          {label} ({items.length})
        </h4>
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>Submission</th>
                <th>Candidate</th>
                <th>Node</th>
                <th>Claim</th>
                <th>Status</th>
                <th>Retries</th>
                <th>Submitted</th>
              </tr>
            </thead>
            <tbody>
              {items.map((entry) => (
                <tr key={entry.submission_id}>
                  <td className="mono">{entry.submission_id.slice(0, 8)}</td>
                  <td className="mono">{entry.candidate_id.slice(0, 8)}</td>
                  <td className="mono">{entry.node_id.slice(0, 8)}</td>
                  <td>{entry.claim_summary.length > 50 ? entry.claim_summary.slice(0, 50) + '...' : entry.claim_summary}</td>
                  <td>
                    <span
                      className="status-badge"
                      style={{ backgroundColor: statusColor(entry.queue_status), color: '#fff' }}
                    >
                      {entry.queue_status}
                    </span>
                  </td>
                  <td>{entry.retry_count}</td>
                  <td>{new Date(entry.submitted_at).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    );
  }

  return (
    <div>
      <h3>Submissions</h3>
      {renderGroup('Pending', pending, '#eab308')}
      {renderGroup('Passed', passed, '#22c55e')}
      {renderGroup('Failed', failed, '#ef4444')}
      {renderGroup('Other', other, '#94a3b8')}
    </div>
  );
}

export default function Certification() {
  const { data: projection, isLoading: projLoading, error: projError } = useCertificationQueueProjection();
  const { data: entries, isLoading: entriesLoading } = useCertificationQueue();

  const isLoading = projLoading || entriesLoading;

  return (
    <div className="panel">
      <h2>Certification Queue</h2>

      {isLoading && <p className="text-muted">Loading certification data...</p>}
      {projError && <p style={{ color: '#ef4444' }}>Error: {String(projError)}</p>}

      {/* Summary bar */}
      {entries && (
        <div style={{ display: 'flex', gap: 16, marginBottom: 12, flexWrap: 'wrap' }}>
          <span style={{ fontSize: 13, color: '#eab308' }}>
            Pending: {entries.filter((e) => e.queue_status === 'pending').length}
          </span>
          <span style={{ fontSize: 13, color: '#22c55e' }}>
            Passed: {entries.filter((e) => e.queue_status === 'passed').length}
          </span>
          <span style={{ fontSize: 13, color: '#ef4444' }}>
            Failed: {entries.filter((e) => e.queue_status === 'failed').length}
          </span>
          <span style={{ fontSize: 13, color: '#94a3b8' }}>
            Total: {entries.length}
          </span>
        </div>
      )}

      {projection && (
        <ProjectionSection
          items={projection.items}
          pendingCount={projection.pending_count}
        />
      )}

      {entries && <DetailSection entries={entries} />}
    </div>
  );
}
