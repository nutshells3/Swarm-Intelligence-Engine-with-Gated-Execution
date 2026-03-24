import { useUiStore } from '../stores/ui';
import { useEvents } from '../api/hooks';

const drawerTabs = ['activity', 'logs', 'diff', 'artifacts'] as const;

export default function Drawer() {
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const drawerTab = useUiStore((s) => s.drawerTab);
  const setDrawerTab = useUiStore((s) => s.setDrawerTab);
  const { data: events } = useEvents();

  if (!drawerOpen) return null;

  const recentEvents = (events ?? []).slice(-20).reverse();

  return (
    <div className="drawer">
      <div className="drawer-tabs">
        {drawerTabs.map((tab) => (
          <button
            key={tab}
            className={`drawer-tab-btn ${drawerTab === tab ? 'drawer-tab-active' : ''}`}
            onClick={() => setDrawerTab(tab)}
          >
            {tab.charAt(0).toUpperCase() + tab.slice(1)}
          </button>
        ))}
      </div>
      <div className="drawer-body">
        {drawerTab === 'activity' && (
          <div className="drawer-activity">
            {recentEvents.length === 0 && <p className="text-muted">No events yet.</p>}
            {recentEvents.map((ev, idx) => (
              <div key={ev.event_id ?? idx} className="event-row">
                <span className="event-type">{ev.event_kind}</span>
                <span className="event-time">
                  {new Date(ev.created_at).toLocaleTimeString()}
                </span>
                <span className="event-payload">{JSON.stringify(ev.payload)}</span>
              </div>
            ))}
          </div>
        )}
        {drawerTab === 'logs' && <p className="text-muted">Logs viewer coming soon.</p>}
        {drawerTab === 'diff' && <p className="text-muted">Diff viewer coming soon.</p>}
        {drawerTab === 'artifacts' && <p className="text-muted">Artifacts browser coming soon.</p>}
      </div>
    </div>
  );
}
