import { useLoops, useCycles } from '../api/hooks';
import type {
  LoopResponse,
  CycleResponse,
} from '../types/generated';

// ---- Section: Loop Row with Cycles ----

function LoopRow({ loop, cycles }: { loop: LoopResponse; cycles: CycleResponse[] }) {
  const loopCycles = cycles.filter((c) => c.loop_id === loop.loop_id);

  return (
    <div className="metric-card" style={{ marginBottom: 12 }}>
      <div className="metric-card-header">
        <span className="metric-card-title">
          Loop <span className="mono">{loop.loop_id.slice(0, 8)}</span>
        </span>
        <span style={{ fontSize: 11, color: '#64748b' }}>
          cycle #{loop.cycle_index} &middot; track: {loop.active_track}
        </span>
      </div>

      <div style={{ fontSize: 12, color: '#94a3b8', marginTop: 4 }}>
        Objective: <span className="mono">{loop.objective_id.slice(0, 8)}</span>
        &nbsp;&middot;&nbsp;
        Created: {new Date(loop.created_at).toLocaleString()}
      </div>

      {loopCycles.length > 0 ? (
        <div className="table-scroll" style={{ marginTop: 8 }}>
          <table className="data-table">
            <thead>
              <tr>
                <th>Cycle ID</th>
                <th>Phase</th>
                <th>Created</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {loopCycles.map((c) => (
                <tr key={c.cycle_id}>
                  <td className="mono">{c.cycle_id.slice(0, 8)}</td>
                  <td>
                    <span className="status-badge" style={{ backgroundColor: phaseColor(c.phase) }}>
                      {c.phase.replace(/_/g, ' ')}
                    </span>
                  </td>
                  <td>{new Date(c.created_at).toLocaleString()}</td>
                  <td>{new Date(c.updated_at).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="text-muted" style={{ marginTop: 6 }}>No cycles in this loop.</p>
      )}
    </div>
  );
}

// ---- Helpers ----

function phaseColor(phase: string): string {
  if (phase === 'completed') return '#22c55e';
  if (phase === 'execution' || phase === 'dispatch') return '#3b82f6';
  if (phase === 'review' || phase === 'certification') return '#eab308';
  if (phase.includes('fail') || phase.includes('error')) return '#ef4444';
  return '#6b7280';
}

// ---- Main Loop History Panel ----

export default function LoopHistory() {
  const { data: loops, isLoading: loopsLoading } = useLoops();
  const { data: cycles, isLoading: cyclesLoading } = useCycles();

  const isLoading = loopsLoading || cyclesLoading;
  const sortedLoops = [...(loops ?? [])].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );

  return (
    <div className="panel">
      <h2>Loop History</h2>

      {isLoading && <p className="text-muted">Loading loop history...</p>}

      {!isLoading && sortedLoops.length === 0 && (
        <p className="text-muted">No loops recorded yet.</p>
      )}

      {sortedLoops.map((loop) => (
        <LoopRow key={loop.loop_id} loop={loop} cycles={cycles ?? []} />
      ))}
    </div>
  );
}
