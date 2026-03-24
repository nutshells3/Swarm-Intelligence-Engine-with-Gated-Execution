import { useTaskBoardProjection } from '../api/hooks';
import { useUiStore } from '../stores/ui';
import type { TaskBoardItem } from '../types/generated';

type TaskStatusValue = 'queued' | 'running' | 'succeeded' | 'failed';

function statusColor(status: string): string {
  switch (status as TaskStatusValue) {
    case 'queued': return '#6b7280';
    case 'running': return '#3b82f6';
    case 'succeeded': return '#22c55e';
    case 'failed': return '#ef4444';
    default: return '#94a3b8';
  }
}

function LaneSection({ title, items, color }: { title: string; items: TaskBoardItem[]; color: string }) {
  const selectTask = useUiStore((s) => s.selectTask);
  const selectedTaskId = useUiStore((s) => s.selectedTaskId);

  if (items.length === 0) return null;

  return (
    <div className="task-lane">
      <h3 style={{ color, marginBottom: 4 }}>
        {title} <span className="text-muted" style={{ fontWeight: 400 }}>({items.length})</span>
      </h3>
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Node</th>
              <th>Worker Role</th>
              <th>Status</th>
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {items.map((task) => (
              <tr
                key={task.task_id}
                className={`task-row ${selectedTaskId === task.task_id ? 'selected' : ''}`}
                onClick={() => selectTask(task.task_id)}
                style={{ cursor: 'pointer' }}
              >
                <td className="mono">{task.task_id.slice(0, 8)}</td>
                <td className="mono">{task.node_id ? task.node_id.slice(0, 8) : '-'}</td>
                <td>{task.worker_role}</td>
                <td>
                  <span
                    className="status-badge"
                    style={{ backgroundColor: statusColor(task.status), color: '#fff' }}
                  >
                    {task.status}
                  </span>
                </td>
                <td>{new Date(task.updated_at).toLocaleString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export default function Tasks() {
  const { data: board, isLoading, error } = useTaskBoardProjection();

  return (
    <div className="panel">
      <h2>Task Board</h2>

      {isLoading && <p className="text-muted">Loading task board...</p>}
      {error && <p style={{ color: '#ef4444' }}>Error loading task board: {String(error)}</p>}

      {board && (
        <>
          {/* Summary bar */}
          <div style={{ display: 'flex', gap: 16, marginBottom: 12, flexWrap: 'wrap' }}>
            <span style={{ fontSize: 13, color: '#6b7280' }}>Queued: {board.summary.queued}</span>
            <span style={{ fontSize: 13, color: '#3b82f6' }}>Running: {board.summary.running}</span>
            <span style={{ fontSize: 13, color: '#22c55e' }}>Succeeded: {board.summary.succeeded}</span>
            <span style={{ fontSize: 13, color: '#ef4444' }}>Failed: {board.summary.failed}</span>
            <span style={{ fontSize: 13, color: '#94a3b8' }}>Total: {board.summary.total}</span>
          </div>

          <LaneSection title="Running" items={board.running} color="#3b82f6" />
          <LaneSection title="Queued" items={board.queued} color="#6b7280" />
          <LaneSection title="Failed" items={board.failed} color="#ef4444" />
          <LaneSection title="Succeeded" items={board.succeeded} color="#22c55e" />
        </>
      )}
    </div>
  );
}
