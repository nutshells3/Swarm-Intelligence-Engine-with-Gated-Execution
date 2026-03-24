import { useNodes } from '../api/hooks';
import { useUiStore } from '../stores/ui';

export default function Graph() {
  const { data: nodes, isLoading } = useNodes();
  const selectNode = useUiStore((s) => s.selectNode);
  const selectedNodeId = useUiStore((s) => s.selectedNodeId);

  return (
    <div className="panel">
      <h2>Dependency Graph</h2>
      <p className="text-muted">
        Interactive dependency graph coming soon (React Flow). For now, nodes are listed below.
      </p>

      {isLoading && <p className="text-muted">Loading nodes...</p>}
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Title</th>
              <th>Lane</th>
              <th>Lifecycle</th>
              <th>Statement</th>
            </tr>
          </thead>
          <tbody>
            {(nodes ?? []).map((node) => (
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
                <td>{node.statement ? (node.statement.length > 60 ? node.statement.slice(0, 60) + '...' : node.statement) : '-'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
