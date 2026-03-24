import { useState } from 'react';
import { useObjectives, useCreateObjective } from '../api/hooks';
import { useUiStore } from '../stores/ui';

export default function ObjectiveChat() {
  const { data: objectives, isLoading } = useObjectives();
  const createObjective = useCreateObjective();
  const selectObjective = useUiStore((s) => s.selectObjective);
  const selectedObjectiveId = useUiStore((s) => s.selectedObjectiveId);

  const [summary, setSummary] = useState('');

  function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!summary.trim()) return;
    createObjective.mutate(
      { summary: summary.trim(), idempotency_key: crypto.randomUUID() },
      { onSuccess: () => { setSummary(''); } },
    );
  }

  return (
    <div className="panel">
      <h2>Objectives</h2>

      {/* Create form */}
      <form className="create-form" onSubmit={handleCreate}>
        <input
          type="text"
          placeholder="Summary"
          value={summary}
          onChange={(e) => setSummary(e.target.value)}
        />
        <button type="submit" disabled={createObjective.isPending}>
          {createObjective.isPending ? 'Creating...' : 'Create Objective'}
        </button>
      </form>

      {/* Objectives list */}
      {isLoading && <p className="text-muted">Loading objectives...</p>}
      <div className="objective-list">
        {(objectives ?? []).map((obj) => (
          <div
            key={obj.objective_id}
            className={`objective-card ${selectedObjectiveId === obj.objective_id ? 'selected' : ''}`}
            onClick={() => selectObjective(obj.objective_id)}
          >
            <div className="objective-summary">{obj.summary}</div>
            <div className="objective-meta">
              <span className={`status-badge status-${obj.planning_status ?? 'unknown'}`}>{obj.planning_status ?? 'unknown'}</span>
              <span className="text-muted">{new Date(obj.created_at).toLocaleDateString()}</span>
            </div>
          </div>
        ))}
      </div>

      {/* Chat placeholder */}
      <div className="chat-placeholder">
        <h3>Chat</h3>
        <p className="text-muted">Chat messages will appear here once wired to the backend.</p>
      </div>

      {/* Extracted items placeholder */}
      <div className="extracted-placeholder">
        <div className="extracted-card">
          <h4>Constraints</h4>
          <p className="text-muted">Extracted constraints will appear here.</p>
        </div>
        <div className="extracted-card">
          <h4>Decisions</h4>
          <p className="text-muted">Extracted decisions will appear here.</p>
        </div>
        <div className="extracted-card">
          <h4>Open Questions</h4>
          <p className="text-muted">Extracted questions will appear here.</p>
        </div>
      </div>
    </div>
  );
}
