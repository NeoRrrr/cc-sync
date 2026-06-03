import type { ReactNode } from "react";
import type { Dictionary } from "../i18n";
import type { Endpoint, TargetSkill } from "../types";
import { compactPath } from "../lib/format";
import claudeIcon from "../assets/agent-icons/claude.svg";
import codexIcon from "../assets/agent-icons/codex.svg";
import geminiIcon from "../assets/agent-icons/gemini.svg";

const endpointIcons: Record<string, string> = {
  claude: claudeIcon,
  codex: codexIcon,
  gemini: geminiIcon,
};

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
  disabled = false,
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
  disabled?: boolean;
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
  const kindLabel: Record<TargetSkill["kind"], string> = {
    link: text.app.skillLink,
    copy: text.app.skillCopy,
    broken: text.app.skillBroken,
  };
  const legendColor: Record<TargetSkill["kind"], string> = {
    link: "text-primary",
    copy: "text-dim",
    broken: "text-danger-text",
  };
  const legend = (["link", "copy", "broken"] as const)
    .map((kind) => ({ kind, count: diskSkills.filter((s) => s.kind === kind).length }))
    .filter((entry) => entry.count > 0);
  const endpointIcon = endpointIcons[id];

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
    <article
      className={`flex flex-col gap-[18px] rounded-2xl border border-line bg-card p-6 shadow-[var(--shadow-sm)] transition-[box-shadow,border-color,transform] duration-200 hover:-translate-y-0.5 hover:border-line-strong hover:shadow-[var(--shadow-md)] ${isTarget ? "" : "opacity-55"}`}
    >
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="grid h-10 w-10 place-items-center rounded-xl border border-line bg-white shadow-[var(--shadow-sm)]">
            {endpointIcon ? (
              <img src={endpointIcon} alt="" aria-hidden="true" className="h-6 w-6 object-contain" />
            ) : (
              <span className="text-[1.1rem] font-extrabold text-primary">{(endpoint.label || id).slice(0, 1).toUpperCase()}</span>
            )}
          </div>
          <h3 className="m-0 text-[1.1rem] font-extrabold text-main">{endpoint.label || id}</h3>
        </div>
        <label className={`flex items-center gap-2 ${disabled ? "cursor-not-allowed" : "cursor-pointer"}`}>
          <input
            type="checkbox"
            checked={isTarget}
            onChange={(event) => onToggleTarget(event.target.checked)}
            className="h-4 w-4 cursor-pointer disabled:cursor-not-allowed"
            disabled={disabled}
          />
          <span className="text-[0.82rem] font-bold text-dim">{isTarget ? text.app.syncTarget : text.app.notSyncTarget}</span>
        </label>
      </header>

      <div className="flex flex-wrap gap-2">
        <span className="rounded-full border border-primary-soft-border bg-primary-soft px-3 py-1 text-[0.74rem] font-bold text-primary">
          {text.app.skillsDir} · {text.modeOptions[endpoint.skills_mode ?? endpoint.mode]}
        </span>
        <span className="rounded-full border border-primary-soft-border bg-primary-soft px-3 py-1 text-[0.74rem] font-bold text-primary">
          {text.app.docsDir} · {text.modeOptions[endpoint.docs_mode ?? endpoint.mode]}
        </span>
      </div>

      {hasSyncResult && (
        <div className="flex flex-wrap gap-2">
          <span className="rounded-lg border border-line bg-muted px-3 py-1 text-[0.74rem] font-bold text-main">{text.app.mdTarget} {summary.md}</span>
          <span className="rounded-lg border border-line bg-muted px-3 py-1 text-[0.74rem] font-bold text-main">{text.app.skillsDir} {summary.skills}</span>
          <span className="rounded-lg border border-line bg-muted px-3 py-1 text-[0.74rem] font-bold text-main">{text.app.docsDir} {summary.docs}</span>
        </div>
      )}

      <div className="flex flex-col">
        {paths.map((path) => {
          const hasPath = Boolean(path.value.trim());
          return (
            <div key={path.key} className="border-t border-line py-[11px] first:border-t-0 first:pt-0">
              <span className="mb-1.5 block text-[0.7rem] font-bold uppercase tracking-[0.04em] text-dim">{path.label}</span>
              <div className="flex items-center justify-between gap-2.5">
                <code className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap bg-transparent text-[0.8rem] text-main [font-family:var(--font-mono)]" title={path.value}>
                  {compactPath(path.value)}
                </code>
                <button type="button" className="mini-action" onClick={() => onOpen(path.value, `${id}-${path.label}`)} disabled={disabled || !hasPath}>
                  {text.app.openLocation}
                </button>
              </div>
            </div>
          );
        })}
      </div>

      <div className="grid gap-2">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <span className="text-[0.78rem] font-bold text-dim">{text.app.skills} · {diskSkills.length}</span>
          {legend.length > 0 && (
            <div className="flex flex-wrap gap-3">
              {legend.map((entry) => (
                <span key={entry.kind} className={`text-[0.72rem] font-bold ${legendColor[entry.kind]}`}>
                  {kindLabel[entry.kind]} {entry.count}
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="flex flex-wrap gap-2">{skillBody}</div>
      </div>

      <div className="flex justify-end">
        <button type="button" className="ghost" onClick={onAdvanced} disabled={disabled}>
          {text.app.advancedConfig}
        </button>
      </div>
    </article>
  );
}
