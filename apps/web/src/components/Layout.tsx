import { useUiStore } from '../stores/ui';
import TopBar from './TopBar';
import Rail from './Rail';
import Inspector from './Inspector';
import Drawer from './Drawer';
import Dashboard from '../panels/Dashboard';
import ObjectiveChat from '../panels/ObjectiveChat';
import Plan from '../panels/Plan';
import Tasks from '../panels/Tasks';
import Graph from '../panels/Graph';
import BranchMainline from '../panels/BranchMainline';
import Conflicts from '../panels/Conflicts';
import Certification from '../panels/Certification';
import Settings from '../panels/Settings';
import Skills from '../panels/Skills';
import LoopHistory from '../panels/LoopHistory';
import Reviews from '../panels/Reviews';

function MainPanel() {
  const activeTab = useUiStore((s) => s.activeTab);
  switch (activeTab) {
    case 'dashboard':
      return <Dashboard />;
    case 'chat':
      return <ObjectiveChat />;
    case 'plan':
      return <Plan />;
    case 'tasks':
      return <Tasks />;
    case 'graph':
      return <Graph />;
    case 'branches':
      return <BranchMainline />;
    case 'conflicts':
      return <Conflicts />;
    case 'certification':
      return <Certification />;
    case 'settings':
      return <Settings />;
    case 'skills':
      return <Skills />;
    case 'loops':
      return <LoopHistory />;
    case 'reviews':
      return <Reviews />;
  }
}

export default function Layout() {
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);
  const drawerOpen = useUiStore((s) => s.drawerOpen);

  const gridCols = `200px 1fr ${inspectorOpen ? '300px' : '0px'}`;
  const gridRows = `48px 1fr ${drawerOpen ? '200px' : '0px'}`;

  return (
    <div
      className="cockpit"
      style={{
        display: 'grid',
        gridTemplateColumns: gridCols,
        gridTemplateRows: gridRows,
        height: '100vh',
        width: '100vw',
        overflow: 'hidden',
      }}
    >
      {/* Top bar spans all columns */}
      <div style={{ gridColumn: '1 / -1', gridRow: '1' }}>
        <TopBar />
      </div>

      {/* Left rail */}
      <div style={{ gridColumn: '1', gridRow: '2 / -1', overflow: 'auto' }}>
        <Rail />
      </div>

      {/* Main panel */}
      <div style={{ gridColumn: '2', gridRow: '2', overflow: 'auto' }}>
        <MainPanel />
      </div>

      {/* Right inspector */}
      {inspectorOpen && (
        <div style={{ gridColumn: '3', gridRow: '2', overflow: 'auto' }}>
          <Inspector />
        </div>
      )}

      {/* Bottom drawer spans main + inspector */}
      {drawerOpen && (
        <div style={{ gridColumn: '2 / -1', gridRow: '3', overflow: 'auto' }}>
          <Drawer />
        </div>
      )}
    </div>
  );
}
