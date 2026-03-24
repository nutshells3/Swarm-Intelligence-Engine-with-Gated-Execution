import { useUiStore } from '../stores/ui';

const tabs = [
  { key: 'dashboard' as const, label: 'Dashboard', icon: '\u{1F4CA}' },
  { key: 'chat' as const, label: 'Chat', icon: '\u{1F4AC}' },
  { key: 'plan' as const, label: 'Plan', icon: '\u{1F4CB}' },
  { key: 'tasks' as const, label: 'Tasks', icon: '\u{2699}' },
  { key: 'graph' as const, label: 'Graph', icon: '\u{1F310}' },
  { key: 'branches' as const, label: 'Branches', icon: '\u{1F500}' },
  { key: 'conflicts' as const, label: 'Conflicts', icon: '\u{26A0}' },
  { key: 'certification' as const, label: 'Certs', icon: '\u{2705}' },
  { key: 'loops' as const, label: 'Loops', icon: '\u{1F504}' },
  { key: 'reviews' as const, label: 'Reviews', icon: '\u{1F50D}' },
  { key: 'skills' as const, label: 'Skills', icon: '\u{1F9E9}' },
  { key: 'settings' as const, label: 'Settings', icon: '\u{2699}\u{FE0F}' },
] as const;

export default function Rail() {
  const activeTab = useUiStore((s) => s.activeTab);
  const setActiveTab = useUiStore((s) => s.setActiveTab);
  const toggleInspector = useUiStore((s) => s.toggleInspector);
  const toggleDrawer = useUiStore((s) => s.toggleDrawer);

  return (
    <nav className="rail">
      <div className="rail-tabs">
        {tabs.map((t) => (
          <button
            key={t.key}
            className={`rail-btn ${activeTab === t.key ? 'rail-btn-active' : ''}`}
            onClick={() => setActiveTab(t.key)}
            title={t.label}
          >
            <span className="rail-icon">{t.icon}</span>
            <span className="rail-label">{t.label}</span>
          </button>
        ))}
      </div>
      <div className="rail-bottom">
        <button className="rail-btn" onClick={toggleInspector} title="Toggle Inspector">
          <span className="rail-icon">&#x25E8;</span>
          <span className="rail-label">Inspector</span>
        </button>
        <button className="rail-btn" onClick={toggleDrawer} title="Toggle Drawer">
          <span className="rail-icon">&#x25E5;</span>
          <span className="rail-label">Drawer</span>
        </button>
      </div>
    </nav>
  );
}
