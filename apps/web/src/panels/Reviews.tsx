import { useState } from 'react';
import { useReviews, useApproveReview, useReviewDigest } from '../api/hooks';
import type { ReviewResponse } from '../types/generated';

// ---- Review kind tabs for REV-016/017/018/019 ----
//
// All review_kind values come from review-governance::ReviewKind
// (packages/review-governance/src/artifacts.rs). The generated TS
// types are in apps/web/src/types/generated.ts::ReviewResponse.

type ReviewFilter = 'all' | 'planning' | 'architecture' | 'direction' | 'milestone' | 'implementation';

const REVIEW_FILTERS: { key: ReviewFilter; label: string }[] = [
  { key: 'all', label: 'All Reviews' },
  { key: 'planning', label: 'Planning' },
  { key: 'architecture', label: 'Architecture' },
  { key: 'direction', label: 'Dev Direction' },
  { key: 'milestone', label: 'Milestone' },
  { key: 'implementation', label: 'Implementation' },
];

// ---- Helpers ----

/** Format a JSONB conditions value as a readable string. */
function formatConditions(conditions: unknown): string {
  if (!conditions) return '-';
  if (Array.isArray(conditions)) {
    if (conditions.length === 0) return '-';
    return conditions.map((c) => (typeof c === 'string' ? c : JSON.stringify(c))).join(', ');
  }
  if (typeof conditions === 'object') return JSON.stringify(conditions);
  return String(conditions);
}

// Status values align with review-governance::ReviewStatus enum:
// scheduled, in_progress, submitted, integrated, approved,
// changes_requested, superseded, cancelled.
function statusColor(status: string): string {
  switch (status) {
    case 'approved':
    case 'integrated':
      return '#22c55e';
    case 'changes_requested':
      return '#ef4444';
    case 'scheduled':
      return '#eab308';
    case 'in_progress':
    case 'submitted':
      return '#3b82f6';
    case 'superseded':
    case 'cancelled':
      return '#9ca3af';
    default:
      return '#6b7280';
  }
}

// ---- Section: Review Table (REV-016/017/018) ----

function ReviewTable({
  reviews,
  onApprove,
  approvingId,
}: {
  reviews: ReviewResponse[];
  onApprove: (id: string) => void;
  approvingId: string | null;
}) {
  return (
    <div className="table-scroll">
      <table className="data-table">
        <thead>
          <tr>
            <th>ID</th>
            <th>Kind</th>
            <th>Target Ref</th>
            <th>Status</th>
            <th>Score / Verdict</th>
            <th>Findings</th>
            <th>Conditions</th>
            <th>Approval Effect</th>
            <th>Auto</th>
            <th>Recorded</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {reviews.map((r) => (
            <tr key={r.review_id}>
              <td className="mono">{r.review_id.slice(0, 8)}</td>
              <td>{r.review_kind}</td>
              <td className="mono">{r.target_ref.slice(0, 12)}</td>
              <td>
                <span
                  className="status-badge"
                  style={{ backgroundColor: statusColor(r.status) }}
                >
                  {r.status}
                </span>
              </td>
              <td>{r.score_or_verdict ?? '-'}</td>
              <td style={{ fontSize: 12, maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {r.findings_summary || '-'}
              </td>
              <td style={{ fontSize: 12, maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {formatConditions(r.conditions)}
              </td>
              <td style={{ fontSize: 12, color: '#94a3b8' }}>{r.approval_effect ?? '-'}</td>
              <td style={{ fontSize: 12, textAlign: 'center' }}>
                {r.is_auto_approval ? 'Y' : '-'}
              </td>
              <td>{new Date(r.recorded_at).toLocaleString()}</td>
              <td>
                {r.status !== 'approved' && r.status !== 'superseded' && r.status !== 'cancelled' && r.status !== 'integrated' && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onApprove(r.review_id);
                    }}
                    disabled={approvingId === r.review_id}
                    style={{ fontSize: 12 }}
                  >
                    {approvingId === r.review_id ? 'Approving...' : 'Approve'}
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ---- Main Reviews Panel (IDE-011 / IDE-012 / IDE-013) ----

export default function Reviews() {
  const { data: reviews, isLoading } = useReviews();
  const approveReview = useApproveReview();
  const digestMutation = useReviewDigest();
  const [activeFilter, setActiveFilter] = useState<ReviewFilter>('all');
  const [approvingId, setApprovingId] = useState<string | null>(null);
  const [digestObjectiveId, setDigestObjectiveId] = useState('');

  function handleApprove(reviewId: string) {
    setApprovingId(reviewId);
    approveReview.mutate(
      { reviewId },
      {
        onSettled: () => setApprovingId(null),
      },
    );
  }

  function handleGenerateDigest() {
    if (!digestObjectiveId.trim()) return;
    digestMutation.mutate({ objectiveId: digestObjectiveId.trim() });
  }

  const filtered = (reviews ?? []).filter((r) => {
    if (activeFilter === 'all') return true;
    return r.review_kind === activeFilter;
  });

  return (
    <div className="panel">
      <h2>Reviews</h2>

      {/* Sub-filter tabs */}
      <div style={{ display: 'flex', gap: 4, marginBottom: 12 }}>
        {REVIEW_FILTERS.map((f) => (
          <button
            key={f.key}
            className={`rail-btn ${activeFilter === f.key ? 'rail-btn-active' : ''}`}
            onClick={() => setActiveFilter(f.key)}
            style={{ fontSize: 13, padding: '4px 12px' }}
          >
            {f.label}
          </button>
        ))}
      </div>

      {isLoading && <p className="text-muted">Loading reviews...</p>}

      {!isLoading && filtered.length === 0 && (
        <p className="text-muted">
          No {activeFilter === 'all' ? '' : activeFilter + ' '}reviews found.
        </p>
      )}

      {filtered.length > 0 && (
        <ReviewTable
          reviews={filtered}
          onApprove={handleApprove}
          approvingId={approvingId}
        />
      )}

      {/* REV-019: Human digest summary */}
      <div style={{ marginTop: 20, padding: '12px 0', borderTop: '1px solid #334155' }}>
        <h3 style={{ margin: '0 0 8px' }}>Review Digest (REV-019)</h3>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <input
            type="text"
            placeholder="Objective ID"
            value={digestObjectiveId}
            onChange={(e) => setDigestObjectiveId(e.target.value)}
            style={{ flex: 1, maxWidth: 320, fontSize: 13, padding: '4px 8px' }}
          />
          <button
            onClick={handleGenerateDigest}
            disabled={digestMutation.isPending || !digestObjectiveId.trim()}
            style={{ fontSize: 13, padding: '4px 12px' }}
          >
            {digestMutation.isPending ? 'Generating...' : 'Generate Digest'}
          </button>
        </div>

        {digestMutation.isError && (
          <p style={{ color: '#ef4444', fontSize: 13, marginTop: 6 }}>
            Error: {digestMutation.error?.message ?? 'Unknown error'}
          </p>
        )}

        {digestMutation.data && (
          <pre style={{
            marginTop: 10,
            padding: 12,
            backgroundColor: '#1e293b',
            borderRadius: 6,
            fontSize: 12,
            whiteSpace: 'pre-wrap',
            lineHeight: 1.5,
            maxHeight: 400,
            overflow: 'auto',
          }}>
            {digestMutation.data.digest}
          </pre>
        )}
      </div>
    </div>
  );
}
