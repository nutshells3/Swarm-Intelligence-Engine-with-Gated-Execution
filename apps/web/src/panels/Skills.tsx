import { useSkillPacks, useWorkerTemplates } from '../api/hooks';
import type {
  SkillPackResponse,
  WorkerTemplateResponse,
} from '../types/generated';

// ---- Section: Skill Pack List ----

function SkillPackList({ packs }: { packs: SkillPackResponse[] }) {
  return (
    <div className="metric-card" style={{ marginBottom: 12 }}>
      <div className="metric-card-header">
        <span className="metric-card-title">Skill Packs</span>
        <span style={{ fontSize: 11, color: '#64748b' }}>{packs.length} total</span>
      </div>
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Worker Role</th>
              <th>Description</th>
              <th>Accepted Task Kinds</th>
              <th>Created</th>
            </tr>
          </thead>
          <tbody>
            {packs.map((sp) => {
              const kinds = formatUnknownList(sp.accepted_task_kinds);
              return (
                <tr key={sp.skill_pack_id}>
                  <td className="mono">{sp.skill_pack_id.slice(0, 8)}</td>
                  <td>{sp.worker_role}</td>
                  <td style={{ maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis' }}>
                    {sp.description}
                  </td>
                  <td style={{ fontSize: 12, color: '#94a3b8' }}>{kinds}</td>
                  <td>{new Date(sp.created_at).toLocaleDateString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ---- Section: Worker Template List ----

function WorkerTemplateList({ templates }: { templates: WorkerTemplateResponse[] }) {
  return (
    <div className="metric-card" style={{ marginBottom: 12 }}>
      <div className="metric-card-header">
        <span className="metric-card-title">Worker Templates</span>
        <span style={{ fontSize: 11, color: '#64748b' }}>{templates.length} total</span>
      </div>
      <div className="table-scroll">
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Role</th>
              <th>Skill Pack</th>
              <th>Provider Mode</th>
              <th>Model Binding</th>
              <th>Allowed Task Kinds</th>
              <th>Created</th>
            </tr>
          </thead>
          <tbody>
            {templates.map((t) => {
              const kinds = formatUnknownList(t.allowed_task_kinds);
              return (
                <tr key={t.template_id}>
                  <td className="mono">{t.template_id.slice(0, 8)}</td>
                  <td>{t.role}</td>
                  <td className="mono">{t.skill_pack_id.slice(0, 8)}</td>
                  <td>{t.provider_mode}</td>
                  <td>{t.model_binding}</td>
                  <td style={{ fontSize: 12, color: '#94a3b8' }}>{kinds}</td>
                  <td>{new Date(t.created_at).toLocaleDateString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ---- Helpers ----

function formatUnknownList(value: unknown): string {
  if (Array.isArray(value)) return value.join(', ');
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return '-';
  return JSON.stringify(value);
}

// ---- Main Skills Panel ----

export default function Skills() {
  const { data: skillPacks, isLoading: packsLoading } = useSkillPacks();
  const { data: templates, isLoading: templatesLoading } = useWorkerTemplates();

  return (
    <div className="panel">
      <h2>Skills & Templates</h2>

      {packsLoading && <p className="text-muted">Loading skill packs...</p>}
      {skillPacks && skillPacks.length > 0 && <SkillPackList packs={skillPacks} />}
      {skillPacks && skillPacks.length === 0 && (
        <p className="text-muted">No skill packs registered.</p>
      )}

      {templatesLoading && <p className="text-muted">Loading worker templates...</p>}
      {templates && templates.length > 0 && <WorkerTemplateList templates={templates} />}
      {templates && templates.length === 0 && (
        <p className="text-muted">No worker templates registered.</p>
      )}
    </div>
  );
}
