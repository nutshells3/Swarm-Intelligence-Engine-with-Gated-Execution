import { useEvents, useCycles, useMeta } from '../api/hooks';
import { useTaskMetrics, useSaturationMetrics, useCertificationQueue } from '../api/hooks';
import type { EventResponse } from '../types/generated';

const PHASE_ORDER = [
  'intake',
  'conversation_extraction',
  'plan_elaboration',
  'plan_validation',
  'review',
  'decomposition',
  'dispatch',
  'execution',
  'integration',
  'certification_selection',
  'certification',
  'state_update',
  'next_cycle_ready',
];

function phasePct(phase: string): number {
  const idx = PHASE_ORDER.indexOf(phase);
  if (idx < 0) return 0;
  return Math.round(((idx + 1) / PHASE_ORDER.length) * 100);
}

function eventColor(eventKind: string | undefined): string {
  if (!eventKind) return '#38bdf8';
  if (eventKind.includes('fail') || eventKind.includes('error')) return '#ef4444';
  if (eventKind.includes('succeed') || eventKind.includes('complet') || eventKind.includes('approv')) return '#22c55e';
  if (eventKind.includes('warn') || eventKind.includes('block')) return '#eab308';
  return '#38bdf8';
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  return `${Math.floor(diff / 3_600_000)}h ago`;
}

// ---- Section: Cycle Progress ----

function CycleProgress() {
  const { data: cycles } = useCycles();
  const active = cycles?.[0];
  const phase = active?.phase ?? 'idle';
  const pct = active ? phasePct(phase) : 0;
  const cycleIndex = cycles?.length ?? 0;

  return (
    <div className="metric-card">
      <div className="metric-card-header">
        <span className="metric-card-title">Cycle Progress</span>
        <span className="mono" style={{ fontSize: 11, color: '#64748b' }}>
          cycle #{cycleIndex}
        </span>
      </div>
      <div className="progress-bar-track">
        <div
          className="progress-bar-fill"
          style={{ width: `${pct}%` }}
        />
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 4 }}>
        <span style={{ fontSize: 12, color: '#94a3b8' }}>
          {phase.replace(/_/g, ' ')}
        </span>
        <span style={{ fontSize: 12, color: '#94a3b8' }}>{pct}%</span>
      </div>
    </div>
  );
}

// ---- Section: Agent Status ----

function AgentStatus() {
  const { data: saturation } = useSaturationMetrics();
  const { data: meta } = useMeta();
  const running = saturation?.running_tasks ?? 0;
  const total = meta?.active_agents ?? running;
  const dots = Array.from({ length: Math.max(total, running) }, (_, i) => i < running);

  return (
    <div className="metric-card">
      <div className="metric-card-header">
        <span className="metric-card-title">Agents</span>
        <span style={{ fontSize: 12, color: '#94a3b8' }}>
          {running} / {total} active
        </span>
      </div>
      <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 6 }}>
        {dots.map((active, i) => (
          <span
            key={i}
            className="agent-dot"
            style={{ background: active ? '#22c55e' : '#334155' }}
          />
        ))}
      </div>
    </div>
  );
}

// ---- Section: Task Summary ----

function TaskSummary() {
  const { data: metrics } = useTaskMetrics();
  const queued = metrics?.queued ?? 0;
  const running = metrics?.running ?? 0;
  const succeeded = metrics?.succeeded ?? 0;
  const failed = metrics?.failed ?? 0;
  const total = metrics?.total ?? 1;

  const segments = [
    { count: queued, color: '#6b7280', label: 'queued' },
    { count: running, color: '#3b82f6', label: 'running' },
    { count: succeeded, color: '#22c55e', label: 'succeeded' },
    { count: failed, color: '#ef4444', label: 'failed' },
  ];

  return (
    <div className="metric-card">
      <div className="metric-card-header">
        <span className="metric-card-title">Tasks</span>
        <span style={{ fontSize: 12, color: '#94a3b8' }}>{total} total</span>
      </div>
      <div className="task-bar">
        {segments.map((s) => (
          <div
            key={s.label}
            className="task-bar-segment"
            style={{
              width: total > 0 ? `${(s.count / total) * 100}%` : '0%',
              background: s.color,
            }}
            title={`${s.label}: ${s.count}`}
          />
        ))}
      </div>
      <div style={{ display: 'flex', gap: 12, marginTop: 6, flexWrap: 'wrap' }}>
        {segments.map((s) => (
          <span key={s.label} style={{ fontSize: 12, color: s.color }}>
            {s.count} {s.label}
          </span>
        ))}
      </div>
    </div>
  );
}

// ---- Section: Gate Status ----

function GateStatus() {
  const { data: metrics } = useTaskMetrics();
  const succeeded = metrics?.succeeded ?? 0;
  const total = metrics?.total ?? 0;
  const satisfied = total > 0 && succeeded === total;

  return (
    <div className="metric-card">
      <div
        className="gate-line"
        style={{ color: satisfied ? '#22c55e' : '#eab308' }}
      >
        GATE: {satisfied ? 'SATISFIED' : 'OPEN'} ({succeeded}/{total})
      </div>
    </div>
  );
}

// ---- Section: Event Feed ----

function EventFeed() {
  const { data: events } = useEvents();
  const feed = (events ?? []).slice(0, 15);

  return (
    <div className="metric-card event-feed-card">
      <div className="metric-card-header">
        <span className="metric-card-title">Live Events</span>
        <span style={{ fontSize: 11, color: '#64748b' }}>{feed.length} recent</span>
      </div>
      <div className="event-feed">
        {feed.length === 0 && (
          <p className="text-muted" style={{ padding: '8px 0' }}>No events yet.</p>
        )}
        {feed.map((ev: EventResponse, idx: number) => (
          <div key={ev.event_id ?? idx} className="event-feed-row">
            <span className="event-feed-time">{timeAgo(ev.created_at)}</span>
            <span
              className="event-feed-kind"
              style={{ color: eventColor(ev.event_kind) }}
            >
              {ev.event_kind ?? 'event'}
            </span>
            <span className="event-feed-desc">
              {describeEvent(ev)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function describeEvent(ev: EventResponse): string {
  const p = ev.payload ?? {};
  if (p.task_id) return `task ${String(p.task_id).slice(0, 8)}`;
  if (p.node_id) return `node ${String(p.node_id).slice(0, 8)}`;
  if (p.cycle_id) return `cycle ${String(p.cycle_id).slice(0, 8)}`;
  if (p.objective_id) return `obj ${String(p.objective_id).slice(0, 8)}`;
  return '';
}

// ---- Section: Metrics ----

function MetricsSummary() {
  const { data: certQueue } = useCertificationQueue();
  const { data: saturation } = useSaturationMetrics();

  const pending = certQueue?.filter((c) => c.queue_status === 'pending' || c.queue_status === 'processing' || c.queue_status === 'submitted').length ?? 0;
  const completed = certQueue?.filter((c) => c.queue_status === 'completed' || c.queue_status === 'acknowledged').length ?? 0;
  const errored = certQueue?.filter((c) => c.queue_status === 'error' || c.queue_status === 'transport_error' || c.queue_status === 'timed_out').length ?? 0;
  const queuePressure = saturation?.queue_pressure ?? 0;

  return (
    <div className="metric-card">
      <div className="metric-card-header">
        <span className="metric-card-title">Metrics</span>
      </div>
      <div style={{ display: 'flex', gap: 16, flexWrap: 'wrap', marginTop: 4 }}>
        <span style={{ fontSize: 12, color: '#94a3b8' }}>
          Queue pressure: <strong style={{ color: '#e2e8f0' }}>
            {Number.isFinite(queuePressure) ? queuePressure.toFixed(1) : '--'}
          </strong>
        </span>
        <span style={{ fontSize: 12, color: '#94a3b8' }}>
          Certs: <span style={{ color: '#eab308' }}>{pending} pending</span>
          {' / '}
          <span style={{ color: '#22c55e' }}>{completed} completed</span>
          {' / '}
          <span style={{ color: '#ef4444' }}>{errored} errored</span>
        </span>
      </div>
    </div>
  );
}

// ---- Main Dashboard ----

export default function Dashboard() {
  return (
    <div className="panel">
      <h2>Dashboard</h2>
      <div className="dashboard">
        <div className="dashboard-top">
          <CycleProgress />
          <AgentStatus />
        </div>
        <div className="dashboard-mid">
          <TaskSummary />
          <GateStatus />
          <MetricsSummary />
        </div>
        <EventFeed />
      </div>
    </div>
  );
}
