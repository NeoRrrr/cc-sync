import { startTransition, useEffect, useRef, useState } from "react";
import { getAvailableSkills, listTargetSkills, loadConfig, openPath, pickFolder, previewSync, runSync, saveConfig } from "./lib/client";
import { dictionaries, loadLanguage, saveLanguage, type Language } from "./i18n";
import { applyTheme, loadTheme, saveTheme, watchSystemTheme, type Theme } from "./theme";
import type { ActivityLog, AvailableSkillOption, Endpoint, PlanOperation, SyncConfig, SyncMode, SyncPlan, SyncScope, TargetSkill } from "./types";
import { SegmentedControl } from "./components/SegmentedControl";
import { Modal } from "./components/Modal";
import { SubpageHeader } from "./components/SubpageHeader";
import { SkillCheckboxList } from "./components/SkillCheckboxList";
import { EndpointCard } from "./components/EndpointCard";
import { SettingsView } from "./components/SettingsView";
import { GearIcon, InfoIcon } from "./components/icons";

const APP_VERSION = "0.1.0";

type View =
  | { name: "main" }
  | { name: "commonSkills" }
  | { name: "logs" }
  | { name: "settings" }
  | { name: "advanced"; target: string };

function now() {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}

function appendLog(
  setLogs: React.Dispatch<React.SetStateAction<ActivityLog[]>>,
  level: ActivityLog["level"],
  message: string
) {
  setLogs((current) => [{ ts: now(), level, message }, ...current].slice(0, 200));
}

function sourceSkillDirs(config: SyncConfig): string[] {
  return config.endpoints[config.sync.source]?.skills_dirs ?? [];
}

/* 按目标聚合同步计划。 */
function groupOperationsByTarget(plan: SyncPlan | null) {
  const grouped = new Map<string, PlanOperation[]>();
  for (const operation of plan?.operations ?? []) {
    const list = grouped.get(operation.target) ?? [];
    list.push(operation);
    grouped.set(operation.target, list);
  }
  return grouped;
}

function summarizeOperations(operations: PlanOperation[]) {
  const summary = { md: 0, skills: 0, docs: 0 };
  for (const operation of operations) {
    if (operation.type === "md") summary.md += 1;
    else if (operation.type === "skills") summary.skills += 1;
    else if (operation.type === "docs") summary.docs += 1;
  }
  return summary;
}

function summarizePlan(plan: SyncPlan | null) {
  return summarizeOperations(plan?.operations ?? []);
}

export default function App() {
  const [language, setLanguage] = useState<Language>(() => loadLanguage());
  const [theme, setTheme] = useState<Theme>(() => loadTheme());
  const [config, setConfig] = useState<SyncConfig | null>(null);
  const [mode, setMode] = useState<"desktop" | "demo">("desktop");
  const [plan, setPlan] = useState<SyncPlan | null>(null);
  const [confirmPlan, setConfirmPlan] = useState<SyncPlan | null>(null);
  const [view, setView] = useState<View>({ name: "main" });
  const [scope, setScope] = useState<SyncScope>("all");
  const [busy, setBusy] = useState(false);
  const [logs, setLogs] = useState<ActivityLog[]>([]);
  const [showHelp, setShowHelp] = useState(false);
  const [syncNotice, setSyncNotice] = useState<{ md: number; skills: number; docs: number } | null>(null);
  const [saveNotice, setSaveNotice] = useState<{ level: "success" | "error"; message: string } | null>(null);
  const [availableSkills, setAvailableSkills] = useState<AvailableSkillOption[]>([]);
  const [diskSkills, setDiskSkills] = useState<Record<string, TargetSkill[]>>({});
  const [replacementDraft, setReplacementDraft] = useState("");
  const text = dictionaries[language];

  async function refreshAvailableSkills(cfg: SyncConfig) {
    try {
      const skills = await getAvailableSkills(sourceSkillDirs(cfg));
      setAvailableSkills(skills);
      appendLog(setLogs, "info", `cc-sync-ui loaded ${skills.length} skills`);
    } catch (err) {
      appendLog(setLogs, "error", `cc-sync-ui failed to load skills: ${String(err)}`);
    }
  }

  /* 扫描每个端点 skills 目录的磁盘真实状态。 */
  async function refreshDiskSkills(cfg: SyncConfig) {
    const pairs = await Promise.all(
      Object.keys(cfg.endpoints).map(async (endpointId) => {
        try {
          return [endpointId, await listTargetSkills(endpointId)] as const;
        } catch {
          return [endpointId, [] as TargetSkill[]] as const;
        }
      })
    );
    setDiskSkills(Object.fromEntries(pairs));
  }

  useEffect(() => {
    void (async () => {
      try {
        const loaded = await loadConfig();
        setConfig(loaded.config);
        setMode(loaded.mode);
        appendLog(setLogs, "info", text.logs.configLoaded(loaded.mode));
        void refreshDiskSkills(loaded.config);
        void refreshAvailableSkills(loaded.config);
      } catch (error) {
        appendLog(setLogs, "error", text.logs.loadFailed(String(error)));
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* 主题：应用到 <html>、持久化、跟随系统。 */
  useEffect(() => {
    applyTheme(theme);
    saveTheme(theme);
    return watchSystemTheme(theme, () => applyTheme(theme));
  }, [theme]);

  useEffect(() => {
    saveLanguage(language);
  }, [language]);

  /* 进入高级配置子页时，把当前 源->目标 的替换规则填入草稿。 */
  useEffect(() => {
    if (view.name !== "advanced" || !config) {
      return;
    }
    const pair = `${config.sync.source}->${view.target}`;
    const reps = config.replacements[pair] ?? {};
    setReplacementDraft(
      Object.entries(reps)
        .map(([source, destination]) => `${source} => ${destination}`)
        .join("\n")
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  /* 只有浮层需要锁滚动。 */
  useEffect(() => {
    const open = Boolean(confirmPlan || syncNotice || saveNotice || showHelp);
    document.body.style.overflow = open ? "hidden" : "";
    return () => {
      document.body.style.overflow = "";
    };
  }, [confirmPlan, syncNotice, saveNotice, showHelp]);

  useEffect(() => {
    if (!saveNotice || saveNotice.level !== "success") {
      return;
    }
    const timer = window.setTimeout(() => setSaveNotice(null), 1600);
    return () => window.clearTimeout(timer);
  }, [saveNotice]);

  /* 自动保存：编辑后防抖写盘。仅桌面模式，跳过加载后首次结算。 */
  const autosaveReadyRef = useRef(false);
  useEffect(() => {
    if (!config || mode !== "desktop") {
      return;
    }
    if (!autosaveReadyRef.current) {
      autosaveReadyRef.current = true;
      return;
    }
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          await saveConfig(config);
          appendLog(setLogs, "info", text.logs.configSaved);
        } catch (error) {
          appendLog(setLogs, "error", String(error));
          setSaveNotice({ level: "error", message: `${text.app.saveFailed}: ${String(error)}` });
        }
      })();
    }, 600);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config, mode]);

  async function chooseWorkspace() {
    try {
      const picked = await pickFolder(config?.project_root);
      if (picked) {
        setConfig((current) => (current ? { ...current, project_root: picked } : current));
        appendLog(setLogs, "info", `cc-sync-ui workspace set: ${picked}`);
      }
    } catch (error) {
      appendLog(setLogs, "error", `cc-sync-ui pick folder failed: ${String(error)}`);
    }
  }

  async function openLocation(path: string, label: string) {
    try {
      await openPath(path);
      appendLog(setLogs, "info", `cc-sync-ui opened ${label}: ${path}`);
    } catch (error) {
      appendLog(setLogs, "error", `cc-sync-ui failed to open ${label}: ${String(error)}`);
    }
  }

  async function handlePreview() {
    setBusy(true);
    setSyncNotice(null);
    setConfirmPlan(null);
    appendLog(setLogs, "info", text.logs.syncRequested(scope));
    try {
      const previewPlan = await previewSync(scope, config ?? undefined);
      const previewSucceeded = previewPlan.success ?? !previewPlan.errors.length;
      appendLog(setLogs, previewPlan.errors.length ? "error" : "info", text.logs.previewReady(previewSucceeded));
      setConfirmPlan(previewPlan);
    } catch (error) {
      appendLog(setLogs, "error", String(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirmRun() {
    setBusy(true);
    appendLog(setLogs, "info", text.logs.syncRequested(scope));
    try {
      const nextPlan = await runSync(scope, config ?? undefined);
      startTransition(() => setPlan(nextPlan));
      const syncSucceeded = nextPlan.success ?? !nextPlan.errors.length;
      appendLog(setLogs, nextPlan.errors.length ? "error" : "info", text.logs.syncFinished(syncSucceeded));
      setConfirmPlan(null);
      if (syncSucceeded) {
        setSyncNotice(summarizePlan(nextPlan));
      }
      if (config) {
        void refreshDiskSkills(config);
      }
    } catch (error) {
      appendLog(setLogs, "error", String(error));
    } finally {
      setBusy(false);
    }
  }

  function setSource(sourceId: string) {
    setConfig((current) => {
      if (!current) return current;
      const next: SyncConfig = {
        ...current,
        sync: {
          source: sourceId,
          targets: current.sync.targets.filter((id) => id !== sourceId),
        },
      };
      // 源变了 → 重扫该源的可选技能。
      void refreshAvailableSkills(next);
      return next;
    });
  }

  function toggleTarget(targetId: string, on: boolean) {
    setConfig((current) => {
      if (!current) return current;
      const targets = on
        ? Array.from(new Set([...current.sync.targets, targetId]))
        : current.sync.targets.filter((id) => id !== targetId);
      return { ...current, sync: { ...current.sync, targets } };
    });
  }

  function updateEndpoint(endpointId: string, patch: Partial<Endpoint>) {
    setConfig((current) => {
      if (!current) return current;
      return {
        ...current,
        endpoints: { ...current.endpoints, [endpointId]: { ...current.endpoints[endpointId], ...patch } },
      };
    });
  }

  function updateSkillSelection(kind: "extra" | "exclude", targetId: string, value: string[]) {
    setConfig((current) => {
      if (!current) return current;
      return {
        ...current,
        skill_selection: {
          ...current.skill_selection,
          [kind]: { ...current.skill_selection[kind], [targetId]: value },
        },
      };
    });
  }

  function updatePairReplacements(pair: string, value: Record<string, string>) {
    setConfig((current) => {
      if (!current) return current;
      return { ...current, replacements: { ...current.replacements, [pair]: value } };
    });
  }

  if (!config) {
    return <main className="workspace loading">{text.app.loading}</main>;
  }

  const sourceId = config.sync.source;
  const endpointEntries = Object.entries(config.endpoints) as Array<[string, Endpoint]>;
  const targetEntries = endpointEntries.filter(([id]) => id !== sourceId);
  const operationsByTarget = groupOperationsByTarget(plan);
  const confirmOperationsByTarget = groupOperationsByTarget(confirmPlan);
  const syncErrors = plan?.errors.slice(0, 3) ?? [];
  const unselectLabel = (skill: string) => `${text.app.cancel} ${skill}`;

  return (
    <main className="workspace">
      {view.name === "main" && (
        <>
          <header className="topbar">
            <div className="brand-block">
              <p className="brand-title">{text.app.title}</p>
              <button type="button" className="icon-btn" aria-label={text.app.settings} title={text.app.settings} onClick={() => setView({ name: "settings" })}>
                <GearIcon />
              </button>
              <button type="button" className="icon-btn" aria-label={text.app.help} title={text.app.help} onClick={() => setShowHelp(true)}>
                <InfoIcon />
              </button>
            </div>
            <div className="topbar-actions">
              <button type="button" className="utility-action" onClick={() => setView({ name: "commonSkills" })}>
                {text.app.commonSkills}
              </button>
              <button type="button" className="utility-action" onClick={() => setView({ name: "logs" })}>
                {text.app.executionLog}
              </button>
              <button type="button" className="primary-action" onClick={handlePreview} disabled={busy}>
                {text.app.runSync}
              </button>
            </div>
          </header>

          <section className="control-bar">
            <label className="control-field workspace-field">
              <span>{text.app.workspace}</span>
              <div className="field-inline">
                <input
                  title={text.app.workspaceHint}
                  value={config.project_root}
                  onChange={(event) => setConfig((current) => (current ? { ...current, project_root: event.target.value } : current))}
                />
                <button type="button" className="mini-action" onClick={() => void chooseWorkspace()}>
                  {text.app.chooseFolder}
                </button>
                <button type="button" className="mini-action" onClick={() => void openLocation(config.project_root, text.app.workspace)}>
                  {text.app.openLocation}
                </button>
              </div>
            </label>
            <label className="control-field">
              <span>{text.app.source}</span>
              <SegmentedControl<string>
                ariaLabel={text.app.source}
                value={sourceId}
                onChange={setSource}
                options={endpointEntries.map(([id, endpoint]) => ({ value: id, label: endpoint.label || id }))}
              />
            </label>
            <label className="control-field">
              <span>{text.app.scope}</span>
              <SegmentedControl<SyncScope>
                ariaLabel={text.app.scope}
                value={scope}
                onChange={setScope}
                options={[
                  { value: "all", label: "all" },
                  { value: "md", label: "md" },
                  { value: "skills", label: "skills" },
                  { value: "docs", label: "docs" },
                ]}
              />
            </label>
          </section>

          {!!syncErrors.length && (
            <section className="error-strip">
              <strong>{text.app.syncErrors}</strong>
              {syncErrors.map((error) => (
                <code key={error}>{error}</code>
              ))}
            </section>
          )}

          <section className="common-skill-panel" aria-label={text.app.activeCommonSkills}>
            <div className="common-skill-head">
              <span className="section-label">{text.app.activeCommonSkills}</span>
              <button type="button" className="mini-action" onClick={() => setView({ name: "commonSkills" })}>
                {text.app.commonSkills}
              </button>
            </div>
            <div className="skill-list">
              {config.skill_selection.common.length ? (
                config.skill_selection.common.map((skill) => (
                  <span key={`common-${skill}`} className="skill-chip">
                    {skill}
                  </span>
                ))
              ) : (
                <span className="muted">{text.app.noSkills}</span>
              )}
            </div>
          </section>

          <section className="target-stack">
            {targetEntries.map(([endpointId, endpoint]) => {
              const targetOperations = operationsByTarget.get(endpointId) ?? [];
              return (
                <EndpointCard
                  key={endpointId}
                  id={endpointId}
                  endpoint={endpoint}
                  isTarget={config.sync.targets.includes(endpointId)}
                  diskSkills={diskSkills[endpointId] ?? []}
                  diskKnown={endpointId in diskSkills}
                  summary={summarizeOperations(targetOperations)}
                  hasSyncResult={targetOperations.length > 0}
                  text={text}
                  onToggleTarget={(checked) => toggleTarget(endpointId, checked)}
                  onOpen={(path, label) => void openLocation(path, label)}
                  onAdvanced={() => setView({ name: "advanced", target: endpointId })}
                />
              );
            })}
          </section>
        </>
      )}

      {view.name === "commonSkills" && (
        <section className="subpage">
          <SubpageHeader title={text.app.commonSkills} backLabel={text.app.back} onBack={() => setView({ name: "main" })} />
          <div className="subpage-body">
            <SkillCheckboxList
              availableSkills={availableSkills}
              selectedSkills={config.skill_selection.common ?? []}
              skillSourcesLabel={text.app.skillSources}
              selectedSummaryLabel={text.app.selectedSkills}
              emptySelectionLabel={text.app.noSkills}
              unselectLabel={unselectLabel}
              onChange={(next) => setConfig((current) => (current ? { ...current, skill_selection: { ...current.skill_selection, common: next } } : current))}
            />
          </div>
        </section>
      )}

      {view.name === "logs" && (
        <section className="subpage">
          <SubpageHeader
            title={text.app.executionLog}
            backLabel={text.app.back}
            onBack={() => setView({ name: "main" })}
            actions={<span className="panel-count">{logs.length} {text.app.entries}</span>}
          />
          <div className="log-list">
            {logs.length ? (
              logs.map((entry, index) => (
                <div key={`${entry.ts}-${index}`} className={entry.level === "error" ? "log-entry error" : "log-entry"}>
                  <div className="log-meta">
                    <span className="log-time">{entry.ts}</span>
                    <strong className="log-level">{entry.level}</strong>
                  </div>
                  <code className="log-message">{entry.message}</code>
                </div>
              ))
            ) : (
              <p className="logs-empty">{text.app.emptyLog}</p>
            )}
          </div>
        </section>
      )}

      {view.name === "settings" && (
        <SettingsView
          text={text}
          language={language}
          onLanguage={setLanguage}
          theme={theme}
          onTheme={setTheme}
          version={APP_VERSION}
          onBack={() => setView({ name: "main" })}
        />
      )}

      {view.name === "advanced" && config.endpoints[view.target] && (
        <section className="subpage">
          <SubpageHeader
            title={`${config.endpoints[view.target].label || view.target} · ${text.app.advancedConfig}`}
            backLabel={text.app.back}
            onBack={() => setView({ name: "main" })}
          />
          <div className="subpage-body">
            <div className="settings-section">
              <p className="settings-label">{text.app.mode}</p>
              <SegmentedControl<SyncMode>
                ariaLabel={text.app.mode}
                value={config.endpoints[view.target].mode}
                onChange={(nextMode) => updateEndpoint(view.target, { mode: nextMode })}
                options={[
                  { value: "junction", label: text.modeOptions.junction },
                  { value: "symlink", label: text.modeOptions.symlink },
                  { value: "copy", label: text.modeOptions.copy },
                ]}
              />
            </div>

            <div className="settings-section">
              <p className="settings-label">{text.app.extraSkills}</p>
              <SkillCheckboxList
                availableSkills={availableSkills}
                selectedSkills={config.skill_selection.extra[view.target] ?? []}
                skillSourcesLabel={text.app.skillSources}
                selectedSummaryLabel={text.app.selectedSkills}
                emptySelectionLabel={text.app.noSkills}
                unselectLabel={unselectLabel}
                onChange={(next) => updateSkillSelection("extra", view.target, next)}
              />
            </div>

            <div className="settings-section">
              <p className="settings-label">{text.app.excludeSkills}</p>
              <SkillCheckboxList
                availableSkills={availableSkills}
                selectedSkills={config.skill_selection.exclude[view.target] ?? []}
                skillSourcesLabel={text.app.skillSources}
                selectedSummaryLabel={text.app.selectedSkills}
                emptySelectionLabel={text.app.noSkills}
                unselectLabel={unselectLabel}
                onChange={(next) => updateSkillSelection("exclude", view.target, next)}
              />
            </div>

            <div className="settings-section">
              <p className="settings-label">{text.app.replacements}（{sourceId} → {view.target}）</p>
              <textarea
                rows={8}
                placeholder={text.app.replacementPlaceholder}
                value={replacementDraft}
                onChange={(event) => {
                  const nextValue = event.target.value;
                  setReplacementDraft(nextValue);
                  const replacements = nextValue
                    .split(/\r?\n/)
                    .map((line) => line.trim())
                    .filter(Boolean)
                    .reduce<Record<string, string>>((record, line) => {
                      const [source, destination] = line.split(/\s*=>\s*/, 2);
                      if (source && destination) {
                        record[source] = destination;
                      }
                      return record;
                    }, {});
                  updatePairReplacements(`${sourceId}->${view.target}`, replacements);
                }}
              />
            </div>
          </div>
        </section>
      )}

      {confirmPlan && (
        <Modal title={text.app.confirmSyncTitle} kicker={text.app.runSync} closeLabel={text.app.close} onClose={() => setConfirmPlan(null)}>
          <p className="field-label">{text.app.willSync}</p>
          {Array.from(confirmOperationsByTarget.entries()).map(([targetName, operations]) => {
            const summary = summarizeOperations(operations);
            return (
              <div key={targetName} className="confirm-target">
                <strong>{targetName}</strong>
                <div className="preview-summary">
                  <span className="preview-chip">{text.app.mdTarget} {summary.md}</span>
                  <span className="preview-chip">{text.app.skillsDir} {summary.skills}</span>
                  <span className="preview-chip">{text.app.docsDir} {summary.docs}</span>
                </div>
              </div>
            );
          })}

          {!confirmOperationsByTarget.size && !confirmPlan.errors.length && <p className="muted">{text.app.noSkills}</p>}

          {!!confirmPlan.errors.length && (
            <div className="error-strip">
              <strong>{text.app.syncErrors}</strong>
              {confirmPlan.errors.map((error) => (
                <code key={error}>{error}</code>
              ))}
            </div>
          )}

          {!!confirmPlan.errors.length && <p className="muted">{text.app.previewHasErrors}</p>}

          <div className="modal-actions">
            <button type="button" className="secondary-action" onClick={() => setConfirmPlan(null)}>
              {text.app.cancel}
            </button>
            <button type="button" className="primary-action" onClick={handleConfirmRun} disabled={busy || !!confirmPlan.errors.length}>
              {text.app.confirmRun}
            </button>
          </div>
        </Modal>
      )}

      {showHelp && (
        <Modal title={text.app.helpTitle} kicker={text.app.title} closeLabel={text.app.close} size="narrow" onClose={() => setShowHelp(false)}>
          <p className="help-intro">{text.app.helpIntro}</p>
          <ul className="help-list">
            {text.app.helpPoints.map((point, index) => (
              <li key={index}>{point}</li>
            ))}
          </ul>
        </Modal>
      )}

      {syncNotice && (
        <Modal title={text.app.syncSuccessTitle} kicker={text.app.runSync} closeLabel={text.app.close} size="success" onClose={() => setSyncNotice(null)}>
          <div className="success-summary">
            <div className="success-card">
              <span>{text.app.mdTarget}</span>
              <strong>{syncNotice.md}</strong>
            </div>
            <div className="success-card">
              <span>{text.app.skillsDir}</span>
              <strong>{syncNotice.skills}</strong>
            </div>
            <div className="success-card">
              <span>{text.app.docsDir}</span>
              <strong>{syncNotice.docs}</strong>
            </div>
          </div>
        </Modal>
      )}

      {saveNotice && (
        <Modal
          title={saveNotice.level === "error" ? text.app.saveFailed : text.app.saveSucceeded}
          kicker={text.app.saveConfig}
          closeLabel={text.app.close}
          size="toast"
          onClose={() => setSaveNotice(null)}
        >
          {saveNotice.level === "error" ? <code className="log-message">{saveNotice.message}</code> : undefined}
        </Modal>
      )}
    </main>
  );
}
