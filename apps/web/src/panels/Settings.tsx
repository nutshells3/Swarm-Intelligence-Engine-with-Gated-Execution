import { useState } from 'react';
import { usePolicies, useUpdateCertificationConfig } from '../api/hooks';
import type { PolicySnapshotResponse } from '../types/generated';

// ---- Section: Policy Detail ----

function PolicyDetail({ policy }: { policy: PolicySnapshotResponse }) {
  const payload = policy.policy_payload as Record<string, unknown> | null;

  return (
    <div className="metric-card" style={{ marginBottom: 12 }}>
      <div className="metric-card-header">
        <span className="metric-card-title mono">{policy.policy_id.slice(0, 12)}</span>
        <span style={{ fontSize: 11, color: '#64748b' }}>rev {policy.revision}</span>
      </div>
      {payload && (
        <pre style={{ fontSize: 12, color: '#94a3b8', whiteSpace: 'pre-wrap', marginTop: 8 }}>
          {JSON.stringify(payload, null, 2)}
        </pre>
      )}
    </div>
  );
}

// ---- Section: Certification Config Editor ----

function CertificationConfigEditor() {
  const { data: policies } = usePolicies();
  const updateConfig = useUpdateCertificationConfig();

  // Find certification policy from the list
  const certPolicy = policies?.find(
    (p) => p.policy_id === 'certification' || p.policy_id.startsWith('cert'),
  );
  const certPayload = certPolicy?.policy_payload as Record<string, unknown> | null;

  const [enabled, setEnabled] = useState<boolean>(
    (certPayload?.enabled as boolean) ?? false,
  );
  const [frequency, setFrequency] = useState<string>(
    (certPayload?.frequency as string) ?? 'per_cycle',
  );
  const [routing, setRouting] = useState<string>(
    (certPayload?.routing as string) ?? 'round_robin',
  );

  function handleSave(e: React.FormEvent) {
    e.preventDefault();
    updateConfig.mutate({ enabled, frequency, routing });
  }

  return (
    <div className="metric-card" style={{ marginBottom: 12 }}>
      <div className="metric-card-header">
        <span className="metric-card-title">Certification Config</span>
      </div>
      <form onSubmit={handleSave} style={{ marginTop: 8 }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <label style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 13, color: '#e2e8f0' }}>
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
            />
            Enabled
          </label>

          <label style={{ fontSize: 13, color: '#94a3b8' }}>
            Frequency
            <select
              value={frequency}
              onChange={(e) => setFrequency(e.target.value)}
              style={{ marginLeft: 8 }}
            >
              <option value="per_cycle">Per Cycle</option>
              <option value="per_task">Per Task</option>
              <option value="on_demand">On Demand</option>
            </select>
          </label>

          <label style={{ fontSize: 13, color: '#94a3b8' }}>
            Routing
            <select
              value={routing}
              onChange={(e) => setRouting(e.target.value)}
              style={{ marginLeft: 8 }}
            >
              <option value="round_robin">Round Robin</option>
              <option value="least_loaded">Least Loaded</option>
              <option value="sticky">Sticky</option>
            </select>
          </label>

          <button type="submit" disabled={updateConfig.isPending} style={{ alignSelf: 'flex-start' }}>
            {updateConfig.isPending ? 'Saving...' : 'Save Certification Config'}
          </button>

          {updateConfig.isError && (
            <p style={{ color: '#ef4444', fontSize: 12 }}>
              Error: {updateConfig.error.message}
            </p>
          )}
          {updateConfig.isSuccess && (
            <p style={{ color: '#22c55e', fontSize: 12 }}>Saved.</p>
          )}
        </div>
      </form>
    </div>
  );
}

// ---- Section: Policy Summary Table ----

function PolicySummaryTable({ policies }: { policies: PolicySnapshotResponse[] }) {
  return (
    <div className="metric-card" style={{ marginBottom: 12 }}>
      <div className="metric-card-header">
        <span className="metric-card-title">Policy Snapshots</span>
        <span style={{ fontSize: 11, color: '#64748b' }}>{policies.length} total</span>
      </div>
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th>Policy ID</th>
              <th>Revision</th>
              <th>Payload Summary</th>
            </tr>
          </thead>
          <tbody>
            {policies.map((p) => {
              const payload = p.policy_payload as Record<string, unknown> | null;
              const keys = payload ? Object.keys(payload).join(', ') : '-';
              return (
                <tr key={p.policy_id}>
                  <td className="mono">{p.policy_id}</td>
                  <td>{p.revision}</td>
                  <td style={{ fontSize: 12, color: '#94a3b8' }}>{keys}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ---- Main Settings Panel ----

export default function Settings() {
  const { data: policies, isLoading } = usePolicies();

  return (
    <div className="panel">
      <h2>Settings</h2>

      {isLoading && <p className="text-muted">Loading policies...</p>}

      <CertificationConfigEditor />

      {policies && policies.length > 0 && (
        <PolicySummaryTable policies={policies} />
      )}

      {policies && policies.length === 0 && (
        <p className="text-muted">No policy snapshots found.</p>
      )}
    </div>
  );
}
