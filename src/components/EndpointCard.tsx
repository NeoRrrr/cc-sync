import type { ReactNode } from "react";
import type { Dictionary } from "../i18n";
import type { Endpoint, TargetSkill } from "../types";
import { compactPath } from "../lib/format";

/* 一个目标端点的卡片：是否同步到它(target 开关) + 它磁盘上实际有哪些技能(链接/复制/失效)。 */
export function EndpointCard({
  id,
  endpoint,
  isTarget,
  diskSkills,
  diskKnown,
  summary,
  hasSyncResult,
  text,
  onToggleTarget,
  onOpen,
  onAdvanced,
}: {
  id: string;
  endpoint: Endpoint;
  isTarget: boolean;
  diskSkills: TargetSkill[];
  diskKnown: boolean;
  summary: { md: number; skills: number; docs: number };
  hasSyncResult: boolean;
  text: Dictionary;
  onToggleTarget: (checked: boolean) => void;
  onOpen: (path: string, label: string) => void;
  onAdvanced: () => void;
}) {
  const paths: Array<{ key: string; label: string; value: string }> = [
    { key: "md", label: text.app.mdTarget, value: endpoint.md },
    { key: "skills", label: text.app.skillsDir, value: endpoint.skills_dirs[0] ?? "" },
    { key: "docs", label: text.app.docsDir, value: endpoint.docs_dirs[0] ?? "" },
  ];

  const kindOrder: Record<TargetSkill["kind"], number> = { link: 0, copy: 1, broken: 2 };
  const sortedSkills = [...diskSkills].sort(
    (a, b) => kindOrder[a.kind] - kindOrder[b.kind] || a.name.localeCompare(b.name)
  );
  const legend = (["link", "copy", "broken"] as const)
    .map((kind) => ({ kind, count: diskSkills.filter((s) => s.kind === kind).length }))
    .filter((entry) => entry.count > 0);
  const kindLabel: Record<TargetSkill["kind"], string> = {
    link: text.app.skillLink,
    copy: text.app.skillCopy,
    broken: text.app.skillBroken,
  };

  let skillBody: ReactNode;
  if (sortedSkills.length) {
    skillBody = sortedSkills.map((skill) => (
      <span key={skill.name} className={`skill-chip skill-${skill.kind}`} title={kindLabel[skill.kind]}>
        {skill.name}
      </span>
    ));
  } else {
    skillBody = <span className="muted">{diskKnown ? text.app.noSkills : "…"}</span>;
  }

  return (
    <article className={isTarget ? "target-card" : "target-card muted-card"}>
      <header className="target-head">
        <div className="target-title">
          <div className="target-avatar">{(endpoint.label || id).slice(0, 1).toUpperCase()}</div>
          <h3>{endpoint.label || id}</h3>
        </div>
        <label className="switch">
          <input type="checkbox" checked={isTarget} onChange={(event) => onToggleTarget(event.target.checked)} />
          <span>{isTarget ? text.app.syncTarget : text.app.notSyncTarget}</span>
        </label>
      </header>

      <div className="target-tags">
        <span className="tag">{text.modeOptions[endpoint.mode]}</span>
      </div>

      {hasSyncResult && (
        <div className="preview-summary">
          <span className="preview-chip">{text.app.mdTarget} {summary.md}</span>
          <span className="preview-chip">{text.app.skillsDir} {summary.skills}</span>
          <span className="preview-chip">{text.app.docsDir} {summary.docs}</span>
        </div>
      )}

      <div className="path-list">
        {paths.map((path) => (
          <div key={path.key} className="path-row">
            <span>{path.label}</span>
            <div className="path-main">
              <code title={path.value}>{compactPath(path.value)}</code>
              <button
                type="button"
                className="mini-action"
                onClick={() => onOpen(path.value, `${id}-${path.label}`)}
              >
                {text.app.openLocation}
              </button>
            </div>
          </div>
        ))}
      </div>

      <div className="skill-group">
        <div className="skill-group-head">
          <span className="skill-group-title">{text.app.skills} · {diskSkills.length}</span>
          {legend.length > 0 && (
            <div className="skill-legend">
              {legend.map((entry) => (
                <span key={entry.kind} className={`legend-${entry.kind}`}>
                  {kindLabel[entry.kind]} {entry.count}
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="skill-list">{skillBody}</div>
      </div>

      <div className="target-actions">
        <button type="button" className="ghost" onClick={onAdvanced}>
          {text.app.advancedConfig}
        </button>
      </div>
    </article>
  );
}
