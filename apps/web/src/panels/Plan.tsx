import { useMemo } from 'react';
import { useUiStore } from '../stores/ui';
import { usePlanGate, useMilestones } from '../api/hooks';
import type { MilestoneNodeResponse } from '../types/generated';

type GateStatusValue = 'satisfied' | 'open' | 'overridden';

function gateColor(status: string): string {
  switch (status as GateStatusValue) {
    case 'satisfied': return '#22c55e';
    case 'open': return '#eab308';
    case 'overridden': return '#f97316';
    default: return '#6b7280';
  }
}

// Tree node derived from flat MilestoneNodeResponse
interface MilestoneTreeNode {
  milestone: MilestoneNodeResponse;
  children: MilestoneTreeNode[];
}

function buildTree(flat: MilestoneNodeResponse[]): MilestoneTreeNode[] {
  const map = new Map<string, MilestoneTreeNode>();
  const roots: MilestoneTreeNode[] = [];

  // Sort by ordering so siblings appear in correct order
  const sorted = [...flat].sort((a, b) => a.ordering - b.ordering);

  for (const ms of sorted) {
    map.set(ms.milestone_id, { milestone: ms, children: [] });
  }

  for (const ms of sorted) {
    const node = map.get(ms.milestone_id)!;
    if (ms.parent_id && map.has(ms.parent_id)) {
      map.get(ms.parent_id)!.children.push(node);
    } else {
      roots.push(node);
    }
  }

  return roots;
}

function MilestoneTree({ nodes }: { nodes: MilestoneTreeNode[] }) {
  return (
    <ul className="milestone-list">
      {nodes.map((n) => {
        const done = n.milestone.status === 'completed' || n.milestone.status === 'done';
        return (
          <li key={n.milestone.milestone_id}>
            <span className={done ? 'milestone-done' : 'milestone-pending'}>
              {done ? '[x]' : '[ ]'} {n.milestone.title}
            </span>
            <span className="text-muted" style={{ fontSize: 11, marginLeft: 8 }}>
              {n.milestone.status}
            </span>
            {n.children.length > 0 && <MilestoneTree nodes={n.children} />}
          </li>
        );
      })}
    </ul>
  );
}

export default function Plan() {
  const selectedObjectiveId = useUiStore((s) => s.selectedObjectiveId);
  const { data: gate, isLoading: gateLoading } = usePlanGate(selectedObjectiveId ?? '');
  const { data: milestones, isLoading: msLoading } = useMilestones(selectedObjectiveId ?? '');

  const tree = useMemo(() => buildTree(milestones ?? []), [milestones]);

  if (!selectedObjectiveId) {
    return (
      <div className="panel">
        <h2>Plan & Gate</h2>
        <p className="text-muted">Select an objective to view its plan gate status.</p>
      </div>
    );
  }

  return (
    <div className="panel">
      <h2>Plan & Gate</h2>

      {/* Gate status */}
      {gateLoading && <p className="text-muted">Loading gate status...</p>}
      {gate && (
        <div className="gate-section">
          <div className="gate-status" style={{ borderLeftColor: gateColor(gate.status) }}>
            <strong>Gate Status:</strong>{' '}
            <span style={{ color: gateColor(gate.status), fontWeight: 700 }}>
              {gate.status.toUpperCase()}
            </span>
          </div>

          <h3>Conditions</h3>
          <table className="data-table">
            <thead>
              <tr>
                <th>Condition</th>
                <th>Status</th>
                <th>Detail</th>
              </tr>
            </thead>
            <tbody>
              {gate.conditions.map((c, i) => (
                <tr key={i}>
                  <td>{c.label}</td>
                  <td>
                    <span className={c.passed ? 'badge-pass' : 'badge-fail'}>
                      {c.passed ? 'PASS' : 'FAIL'}
                    </span>
                  </td>
                  <td>{c.detail}</td>
                </tr>
              ))}
            </tbody>
          </table>

          <div className="gate-meta">
            <p><strong>Unresolved questions:</strong> {gate.unresolved_questions}</p>
            {gate.block_reason && (
              <div className="block-reason">
                <strong>Why is implementation blocked?</strong>
                <p>{gate.block_reason}</p>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Milestones */}
      <h3>Milestones</h3>
      {msLoading && <p className="text-muted">Loading milestones...</p>}
      {milestones && milestones.length === 0 && (
        <p className="text-muted">No milestones defined yet.</p>
      )}
      {tree.length > 0 && <MilestoneTree nodes={tree} />}
    </div>
  );
}
