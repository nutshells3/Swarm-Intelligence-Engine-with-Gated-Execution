import { useMeta } from '../api/hooks';

export default function TopBar() {
  const { data: meta, isLoading, isError } = useMeta();

  return (
    <header className="topbar">
      <div className="topbar-brand">Swarm IDE</div>
      <div className="topbar-stats">
        {isLoading && <span className="topbar-pill">Loading...</span>}
        {isError && <span className="topbar-pill pill-error">API offline</span>}
        {meta && (
          <>
            <span className="topbar-pill">
              <strong>Service:</strong> {meta.service}
            </span>
            <span className="topbar-pill">
              <strong>DB:</strong> {meta.database_backend}
            </span>
            <span className="topbar-pill">
              <strong>Agents:</strong> {meta.active_agents}
            </span>
            <span className="topbar-pill">
              <strong>Queue:</strong> {meta.queue_length}
            </span>
          </>
        )}
      </div>
    </header>
  );
}
