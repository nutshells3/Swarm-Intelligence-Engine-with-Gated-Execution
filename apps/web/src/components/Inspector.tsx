import { useUiStore } from '../stores/ui';
import { useObjective, useTasks, useNodes } from '../api/hooks';

function PropertyTable({ data }: { data: Record<string, unknown> }) {
  return (
    <table className="prop-table">
      <tbody>
        {Object.entries(data).map(([key, value]) => (
          <tr key={key}>
            <td className="prop-key">{key}</td>
            <td className="prop-val">
              {typeof value === 'object' ? JSON.stringify(value, null, 2) : String(value ?? '')}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export default function Inspector() {
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);
  const selectedObjectiveId = useUiStore((s) => s.selectedObjectiveId);
  const selectedTaskId = useUiStore((s) => s.selectedTaskId);
  const selectedNodeId = useUiStore((s) => s.selectedNodeId);

  const { data: objective } = useObjective(selectedObjectiveId ?? '');
  const { data: tasks } = useTasks();
  const { data: nodes } = useNodes();

  if (!inspectorOpen) return null;

  const selectedTask = tasks?.find((t) => t.task_id === selectedTaskId);
  const selectedNode = nodes?.find((n) => n.node_id === selectedNodeId);

  const item = selectedTask ?? selectedNode ?? objective;

  return (
    <aside className="inspector">
      <div className="inspector-header">
        <strong>Inspector</strong>
      </div>
      <div className="inspector-body">
        {!item && <p className="text-muted">Select an objective, task, or node to inspect.</p>}
        {item && <PropertyTable data={item as unknown as Record<string, unknown>} />}
      </div>
    </aside>
  );
}
