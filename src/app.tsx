import { startTransition, useEffect, useRef, useState } from "react";
import {
  exitApp,
  getAvailableSkills,
  hideMainWindow,
  listTargetSkills,
  listenCloseRequested,
  loadConfig,
  openPath,
  pickFile,
  pickFolder,
  previewSync,
  runSync,
  saveConfig,
} from "./lib/client";
import { dictionaries, loadLanguage, saveLanguage, type Language } from "./i18n";
import { applyTheme, loadTheme, saveTheme, watchSystemTheme, type Theme } from "./theme";
import type { ActivityLog, AvailableSkillOption, Endpoint, PlanOperation, SourceConfig, SyncConfig, SyncMode, SyncPlan, SyncScope, TargetSkill } from "./types";
import { SegmentedControl } from "./components/SegmentedControl";
import { Modal } from "./components/Modal";
import { SubpageHeader } from "./components/SubpageHeader";
import { SkillCheckboxList } from "./components/SkillCheckboxList";
import { EndpointCard } from "./components/EndpointCard";
import { SettingsView } from "./components/SettingsView";
import { GearIcon, InfoIcon } from "./components/icons";

const APP_VERSION = "0.1.1";

const WORKSPACE_DEFAULT_SOURCES: SourceConfig = {
  md_files: [],
  skills_dirs: [],
  docs_dirs: [],
};

const WORKSPACE_DEFAULT_ENDPOINTS: Record<string, Partial<Endpoint>> = {
  claude: {
    md: "CLAUDE.md",
    md_local_candidates: [".claude/CLAUDE.local.md", ".claude/claude.local.md"],
    skills_dirs: [".claude/skills"],
    docs_dirs: [".claude/docs"],
  },
  codex: {
    md: "AGENTS.md",
    skills_dirs: [".codex/skills"],
    docs_dirs: [".codex/docs"],
  },
  gemini: {
    md: "GEMINI.md",
    skills_dirs: [".gemini/skills"],
    docs_dirs: [".gemini/docs"],
  },
};

type View =
  | { name: "main" }
  | { name: "sources" }
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
  return config.sources.skills_dirs ?? [];
}

function uniquePaths(paths: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const path of paths) {
    const trimmed = path?.trim();
    if (!trimmed || seen.has(trimmed)) {
      continue;
    }
    seen.add(trimmed);
    result.push(trimmed);
  }
  return result;
}

function normalizeConfig(config: SyncConfig): SyncConfig {
  const projectRoot = config.project_root ?? "";
  const endpoints = Object.fromEntries(
    Object.entries(config.endpoints ?? {}).map(([endpointId, endpoint]) => {
      const mode = endpoint.mode ?? "junction";
      return [
        endpointId,
        {
          ...endpoint,
          mode,
          skills_mode: endpoint.skills_mode ?? mode,
          docs_mode: endpoint.docs_mode ?? mode,
        },
      ];
    })
  ) as Record<string, Endpoint>;

  return {
    ...config,
    project_root: projectRoot,
    workspaces: uniquePaths([projectRoot, ...(config.workspaces ?? [])]),
    sources: {
      md_files: config.sources?.md_files ?? [],
      skills_dirs: config.sources?.skills_dirs ?? [],
      docs_dirs: config.sources?.docs_dirs ?? [],
    },
    preferences: {
      ...(config.preferences ?? {}),
      close_to_tray: config.preferences?.close_to_tray ?? false,
    },
    endpoints,
  };
}

function setWorkspace(config: SyncConfig, workspace: string): SyncConfig {
  const trimmed = workspace.trim();
  return {
    ...config,
    project_root: trimmed,
    workspaces: uniquePaths([trimmed, ...(config.workspaces ?? [])]),
  };
}

function fillEmptyWorkspaceDefaults(config: SyncConfig): SyncConfig {
  const endpoints = Object.fromEntries(
    Object.entries(config.endpoints).map(([endpointId, endpoint]) => {
      const defaults = WORKSPACE_DEFAULT_ENDPOINTS[endpointId] ?? {};
      return [
        endpointId,
        {
          ...endpoint,
          md: endpoint.md || defaults.md || "",
          md_local_candidates: endpoint.md_local_candidates.length ? endpoint.md_local_candidates : defaults.md_local_candidates ?? [],
          skills_dirs: endpoint.skills_dirs.length ? endpoint.skills_dirs : defaults.skills_dirs ?? [],
          docs_dirs: endpoint.docs_dirs.length ? endpoint.docs_dirs : defaults.docs_dirs ?? [],
          skills_mode: endpoint.skills_mode ?? endpoint.mode,
          docs_mode: endpoint.docs_mode ?? endpoint.mode,
        },
      ];
    })
  ) as Record<string, Endpoint>;

  return {
    ...config,
    sources: {
      md_files: config.sources.md_files.length ? config.sources.md_files : WORKSPACE_DEFAULT_SOURCES.md_files,
      skills_dirs: config.sources.skills_dirs.length ? config.sources.skills_dirs : WORKSPACE_DEFAULT_SOURCES.skills_dirs,
      docs_dirs: config.sources.docs_dirs.length ? config.sources.docs_dirs : WORKSPACE_DEFAULT_SOURCES.docs_dirs,
    },
    endpoints,
  };
}

function setWorkspaceAndFillDefaults(config: SyncConfig, workspace: string): SyncConfig {
  return fillEmptyWorkspaceDefaults(setWorkspace(config, workspace));
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

function targetCount(plan: SyncPlan) {
  return new Set(plan.operations.map((operation) => operation.target)).size;
}

function formatOperation(operation: PlanOperation, index: number) {
  const prefix = `${index + 1}. [${operation.type}] ${operation.target}`;
  if (operation.type === "md") {
    return `${prefix}: ${(operation.sources ?? []).length} files -> ${operation.dst}`;
  }
  if (operation.type === "remove_managed") {
    return `${prefix}: ${operation.kind ?? "item"} ${operation.name ?? ""} -> remove ${operation.dst}`;
  }

  const name = operation.name ? `${operation.name} ` : "";
  const mode = operation.mode ? ` (${operation.mode})` : "";
  return `${prefix}: ${name}${operation.src ?? "(missing source)"} -> ${operation.dst}${mode}`;
}

function formatPlanDetails(plan: SyncPlan, title: string, text: (typeof dictionaries)[Language]["logs"]) {
  const summary = summarizeOperations(plan.operations);
  const lines = [
    title,
    text.planSummary(plan.operations.length, summary.md, summary.skills, summary.docs, targetCount(plan)),
  ];

  if (!plan.operations.length) {
    lines.push(text.noOperations);
  } else {
    lines.push(...plan.operations.map((operation, index) => formatOperation(operation, index)));
  }

  return lines.join("\n");
}

function formatPlanMessages(messages: string[], title: string) {
  return [title, ...messages.map((message, index) => `${index + 1}. ${message}`)].join("\n");
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
  const [showWorkspacePrompt, setShowWorkspacePrompt] = useState(false);
  const [closeDialog, setCloseDialog] = useState<{ remember: boolean } | null>(null);
  const [syncNotice, setSyncNotice] = useState<{ md: number; skills: number; docs: number } | null>(null);
  const [saveNotice, setSaveNotice] = useState<{ level: "success" | "error"; message: string } | null>(null);
  const [availableSkills, setAvailableSkills] = useState<AvailableSkillOption[]>([]);
  const [diskSkills, setDiskSkills] = useState<Record<string, TargetSkill[]>>({});
  const [replacementDraft, setReplacementDraft] = useState("");
  const [sourceDrafts, setSourceDrafts] = useState<Record<keyof SourceConfig, string>>({
    md_files: "",
    skills_dirs: "",
    docs_dirs: "",
  });
  const text = dictionaries[language];
  const configRef = useRef<SyncConfig | null>(null);

  async function refreshAvailableSkills(cfg: SyncConfig) {
    try {
      const skills = cfg.project_root.trim() ? await getAvailableSkills(sourceSkillDirs(cfg), cfg) : [];
      setAvailableSkills(skills);
      appendLog(setLogs, "info", `cc-sync loaded ${skills.length} skills`);
    } catch (err) {
      appendLog(setLogs, "error", `cc-sync failed to load skills: ${String(err)}`);
    }
  }

  /* 扫描每个端点 skills 目录的磁盘真实状态。 */
  async function refreshDiskSkills(cfg: SyncConfig) {
    if (!cfg.project_root.trim()) {
      setDiskSkills({});
      return;
    }
    const pairs = await Promise.all(
      Object.keys(cfg.endpoints).map(async (endpointId) => {
        try {
          return [endpointId, await listTargetSkills(endpointId, cfg)] as const;
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
        const normalized = normalizeConfig(loaded.config);
        setConfig(normalized);
        setMode(loaded.mode);
        setShowWorkspacePrompt(!normalized.project_root.trim());
        appendLog(setLogs, "info", text.logs.configLoaded(loaded.mode));
        void refreshDiskSkills(normalized);
        void refreshAvailableSkills(normalized);
      } catch (error) {
        appendLog(setLogs, "error", text.logs.loadFailed(String(error)));
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    configRef.current = config;
  }, [config]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenCloseRequested(() => {
      const current = configRef.current;
      if (current?.preferences.close_to_tray) {
        void hideMainWindow();
      } else {
        setCloseDialog({ remember: false });
      }
    }).then((cleanup) => {
      if (disposed) {
        cleanup();
      } else {
        unlisten = cleanup;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
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

  /* 进入高级配置子页时，把当前目标的替换规则填入草稿。 */
  useEffect(() => {
    if (view.name !== "advanced" || !config) {
      return;
    }
    const reps = config.replacements[view.target] ?? {};
    setReplacementDraft(
      Object.entries(reps)
        .map(([source, destination]) => `${source} => ${destination}`)
        .join("\n")
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  /* 只有浮层需要锁滚动。 */
  useEffect(() => {
    const open = Boolean(confirmPlan || syncNotice || saveNotice || showHelp || showWorkspacePrompt || closeDialog);
    document.body.style.overflow = open ? "hidden" : "";
    return () => {
      document.body.style.overflow = "";
    };
  }, [confirmPlan, syncNotice, saveNotice, showHelp, showWorkspacePrompt, closeDialog]);

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
        setConfig((current) => {
          if (!current) return current;
          const next = setWorkspaceAndFillDefaults(current, picked);
          void refreshAvailableSkills(next);
          void refreshDiskSkills(next);
          return next;
        });
        setShowWorkspacePrompt(false);
        appendLog(setLogs, "info", `cc-sync workspace set: ${picked}`);
      }
    } catch (error) {
      appendLog(setLogs, "error", `cc-sync pick folder failed: ${String(error)}`);
    }
  }

  function switchWorkspace(path: string) {
    setConfig((current) => {
      if (!current) return current;
      const next = setWorkspaceAndFillDefaults(current, path);
      void refreshAvailableSkills(next);
      void refreshDiskSkills(next);
      return next;
    });
    appendLog(setLogs, "info", `cc-sync workspace switched: ${path}`);
  }

  function deleteCurrentWorkspace() {
    setConfig((current) => {
      if (!current) return current;
      const nextWorkspaces = (current.workspaces ?? []).filter((path) => path !== current.project_root);
      const nextProjectRoot = nextWorkspaces[0] ?? "";
      if (!nextProjectRoot) {
        setShowWorkspacePrompt(true);
      }
      const next = {
        ...current,
        project_root: nextProjectRoot,
        workspaces: nextWorkspaces,
      };
      void refreshAvailableSkills(next);
      void refreshDiskSkills(next);
      return next;
    });
  }

  function setCloseToTrayPreference(value: boolean) {
    setConfig((current) =>
      current
        ? {
            ...current,
            preferences: { ...current.preferences, close_to_tray: value },
          }
        : current
    );
  }

  async function handleMinimizeToTray() {
    if (closeDialog?.remember) {
      setCloseToTrayPreference(true);
    }
    setCloseDialog(null);
    await hideMainWindow();
  }

  async function handleExitApp() {
    await exitApp();
  }

  async function openLocation(path: string, label: string) {
    try {
      await openPath(path);
      appendLog(setLogs, "info", `cc-sync opened ${label}: ${path}`);
    } catch (error) {
      appendLog(setLogs, "error", `cc-sync failed to open ${label}: ${String(error)}`);
    }
  }

  async function handlePreview() {
    if (!config?.project_root.trim()) {
      appendLog(setLogs, "error", text.app.workspaceRequired);
      setShowWorkspacePrompt(true);
      return;
    }
    setBusy(true);
    setSyncNotice(null);
    setConfirmPlan(null);
    appendLog(setLogs, "info", text.logs.syncRequested(scope));
    try {
      const previewPlan = await previewSync(scope, config ?? undefined);
      const previewSucceeded = previewPlan.success ?? !previewPlan.errors.length;
      appendLog(setLogs, previewPlan.errors.length ? "error" : "info", text.logs.previewReady(previewSucceeded));
      appendLog(setLogs, "info", formatPlanDetails(previewPlan, text.logs.previewDetails, text.logs));
      if (previewPlan.warnings?.length) {
        appendLog(setLogs, "info", formatPlanMessages(previewPlan.warnings, text.logs.planWarnings(previewPlan.warnings.length)));
      }
      if (previewPlan.errors.length) {
        appendLog(setLogs, "error", formatPlanMessages(previewPlan.errors, text.logs.planErrors(previewPlan.errors.length)));
      }
      setConfirmPlan(previewPlan);
    } catch (error) {
      appendLog(setLogs, "error", String(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirmRun() {
    if (!config?.project_root.trim()) {
      appendLog(setLogs, "error", text.app.workspaceRequired);
      setShowWorkspacePrompt(true);
      return;
    }
    setBusy(true);
    appendLog(setLogs, "info", text.logs.syncRequested(scope));
    try {
      const nextPlan = await runSync(scope, config ?? undefined);
      startTransition(() => setPlan(nextPlan));
      const syncSucceeded = nextPlan.success ?? !nextPlan.errors.length;
      appendLog(setLogs, nextPlan.errors.length ? "error" : "info", text.logs.syncFinished(syncSucceeded));
      appendLog(setLogs, "info", formatPlanDetails(nextPlan, text.logs.runDetails, text.logs));
      if (nextPlan.warnings?.length) {
        appendLog(setLogs, "info", formatPlanMessages(nextPlan.warnings, text.logs.planWarnings(nextPlan.warnings.length)));
      }
      if (nextPlan.errors.length) {
        appendLog(setLogs, "error", formatPlanMessages(nextPlan.errors, text.logs.planErrors(nextPlan.errors.length)));
      }
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
      const next = {
        ...current,
        endpoints: { ...current.endpoints, [endpointId]: { ...current.endpoints[endpointId], ...patch } },
      };
      void refreshDiskSkills(next);
      return next;
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

  function updateSourceList(kind: keyof SourceConfig, values: string[]) {
    setConfig((current) => {
      if (!current) return current;
      const next = {
        ...current,
        sources: { ...current.sources, [kind]: Array.from(new Set(values.filter(Boolean))) },
      };
      if (kind === "skills_dirs") {
        void refreshAvailableSkills(next);
      }
      return next;
    });
  }

  function addSourcePath(kind: keyof SourceConfig, value: string) {
    const trimmed = value.trim();
    if (!trimmed || !config) return;
    updateSourceList(kind, [...config.sources[kind], trimmed]);
    setSourceDrafts((current) => ({ ...current, [kind]: "" }));
  }

  async function chooseSourcePath(kind: keyof SourceConfig) {
    try {
      const picked = kind === "md_files" ? await pickFile(config?.project_root) : await pickFolder(config?.project_root);
      if (picked) {
        addSourcePath(kind, picked);
      }
    } catch (error) {
      appendLog(setLogs, "error", `cc-sync pick source failed: ${String(error)}`);
    }
  }

  if (!config) {
    return (
      <main className="mx-auto flex min-h-screen max-w-[1140px] flex-col items-center justify-center px-6 font-semibold text-dim">
        {text.app.loading}
      </main>
    );
  }

  const activeConfig = config;
  const hasWorkspace = Boolean(config.project_root.trim());
  const workspaceChoices = uniquePaths([config.project_root, ...(config.workspaces ?? [])]);
  const endpointEntries = Object.entries(config.endpoints) as Array<[string, Endpoint]>;
  const targetEntries = endpointEntries;
  const operationsByTarget = groupOperationsByTarget(plan);
  const confirmOperationsByTarget = groupOperationsByTarget(confirmPlan);
  const syncErrors = plan?.errors.slice(0, 3) ?? [];
  const syncWarnings = plan?.warnings?.slice(0, 3) ?? [];
  const unselectLabel = (skill: string) => `${text.app.cancel} ${skill}`;

  const fieldLabel = "text-[0.78rem] font-bold uppercase tracking-[0.05em] text-dim";
  const sourceEditors: Array<{ kind: keyof SourceConfig; title: string; pickTitle: string }> = [
    { kind: "md_files", title: text.app.mdSources, pickTitle: text.app.chooseFile },
    { kind: "skills_dirs", title: text.app.skillSourceDirs, pickTitle: text.app.chooseFolder },
    { kind: "docs_dirs", title: text.app.docsSourceDirs, pickTitle: text.app.chooseFolder },
  ];

  function renderSourceEditor({ kind, title, pickTitle }: { kind: keyof SourceConfig; title: string; pickTitle: string }) {
    const values = activeConfig.sources[kind] ?? [];
    return (
      <div key={kind} className="flex flex-col gap-3 rounded-xl border border-line bg-subtle p-4">
        <div className="flex items-center justify-between gap-3">
          <p className="m-0 text-base font-extrabold text-main">{title}</p>
          <span className="inline-flex min-h-6 min-w-6 items-center justify-center rounded-full bg-muted px-2 text-[0.74rem] font-extrabold text-dim">
            {values.length}
          </span>
        </div>
        <div className="grid gap-2">
          {values.length ? (
            values.map((value, index) => (
              <div key={`${kind}-${value}`} className="source-path-row">
                <code className="source-path-value" title={value}>
                  {value}
                </code>
                <button type="button" className="mini-action source-action" onClick={() => void openLocation(value, title)} disabled={!hasWorkspace}>
                  {text.app.openLocation}
                </button>
                <button type="button" className="mini-action source-action" onClick={() => updateSourceList(kind, values.filter((_, i) => i !== index))} disabled={!hasWorkspace}>
                  {text.app.remove}
                </button>
              </div>
            ))
          ) : (
            <span className="muted">{text.app.noSources}</span>
          )}
        </div>
        <div className="source-add-row">
          <input
            className="min-w-0"
            value={sourceDrafts[kind]}
            placeholder={text.app.pathPlaceholder}
            onChange={(event) => setSourceDrafts((current) => ({ ...current, [kind]: event.target.value }))}
            disabled={!hasWorkspace}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                addSourcePath(kind, sourceDrafts[kind]);
              }
            }}
          />
          <button type="button" className="mini-action source-action" title={pickTitle} onClick={() => void chooseSourcePath(kind)} disabled={!hasWorkspace}>
            {text.app.chooseFolder}
          </button>
          <button type="button" className="mini-action source-action" onClick={() => addSourcePath(kind, sourceDrafts[kind])} disabled={!hasWorkspace}>
            {text.app.add}
          </button>
        </div>
      </div>
    );
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-[1140px] flex-col gap-6 px-6 pb-14 pt-9">
      {view.name === "main" && (
        <>
          <header className="flex flex-wrap items-center justify-between gap-4 max-md:items-start">
            <div className="flex items-center gap-[14px]">
              <p className="m-0 text-[1.4rem] font-extrabold tracking-[-0.02em] text-primary">{text.app.title}</p>
              <button type="button" className="icon-btn" aria-label={text.app.settings} title={text.app.settings} onClick={() => setView({ name: "settings" })}>
                <GearIcon />
              </button>
              <button type="button" className="icon-btn" aria-label={text.app.help} title={text.app.help} onClick={() => setShowHelp(true)}>
                <InfoIcon />
              </button>
            </div>
            <div className="flex flex-wrap items-center gap-2.5">
              <button type="button" className="utility-action" onClick={() => setView({ name: "sources" })} disabled={!hasWorkspace} title={hasWorkspace ? undefined : text.app.workspaceRequired}>
                {text.app.inputSources}
              </button>
              <button type="button" className="utility-action" onClick={() => setView({ name: "commonSkills" })} disabled={!hasWorkspace} title={hasWorkspace ? undefined : text.app.workspaceRequired}>
                {text.app.commonSkills}
              </button>
              <button type="button" className="utility-action" onClick={() => setView({ name: "logs" })}>
                {text.app.executionLog}
              </button>
              <button type="button" className="primary-action" onClick={handlePreview} disabled={busy || !hasWorkspace} title={hasWorkspace ? undefined : text.app.workspaceRequired}>
                {text.app.runSync}
              </button>
            </div>
          </header>

          <section className="flex flex-wrap items-end gap-7 rounded-2xl border border-line bg-card px-6 py-5 shadow-[var(--shadow-sm)] max-md:flex-col max-md:items-stretch">
            <label className="flex min-w-[300px] flex-1 flex-col gap-2.5">
              <span className={fieldLabel}>{text.app.workspace}</span>
              <div className="grid grid-cols-[minmax(0,1fr)_auto_auto_auto] gap-2.5 max-md:grid-cols-1">
                <select
                  className="min-w-0"
                  title={text.app.workspaceHint}
                  value={config.project_root}
                  onChange={(event) => switchWorkspace(event.target.value)}
                  disabled={!workspaceChoices.length}
                >
                  {workspaceChoices.length ? (
                    workspaceChoices.map((path) => (
                      <option key={path} value={path}>
                        {path}
                      </option>
                    ))
                  ) : (
                    <option value="">{text.app.noWorkspaces}</option>
                  )}
                </select>
                <button type="button" className="mini-action" onClick={() => void chooseWorkspace()}>
                  {text.app.chooseFolder}
                </button>
                <button type="button" className="mini-action" onClick={deleteCurrentWorkspace} disabled={!config.project_root}>
                  {text.app.remove}
                </button>
                <button type="button" className="mini-action" onClick={() => void openLocation(config.project_root, text.app.workspace)} disabled={!config.project_root}>
                  {text.app.openLocation}
                </button>
              </div>
            </label>
            <label className="flex flex-col gap-2.5">
              <span className={fieldLabel}>{text.app.scope}</span>
              <SegmentedControl<SyncScope>
                ariaLabel={text.app.scope}
                value={scope}
                onChange={setScope}
                disabled={!hasWorkspace}
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

          {!!syncWarnings.length && (
            <section className="warning-strip">
              <strong>{text.app.syncWarnings}</strong>
              {syncWarnings.map((warning) => (
                <code key={warning}>{warning}</code>
              ))}
            </section>
          )}

          <section
            className="flex flex-col gap-3.5 rounded-2xl border border-line bg-card px-[22px] py-[18px] shadow-[var(--shadow-sm)]"
            aria-label={text.app.inputSources}
          >
            <div className="flex items-center justify-between gap-3">
              <span className={fieldLabel}>{text.app.inputSources}</span>
              <button type="button" className="mini-action" onClick={() => setView({ name: "sources" })} disabled={!hasWorkspace} title={hasWorkspace ? undefined : text.app.workspaceRequired}>
                {text.app.configure}
              </button>
            </div>
            <div className="grid grid-cols-3 gap-3 max-md:grid-cols-1">
              <div className="rounded-xl border border-line bg-subtle p-4">
                <span className="block text-[0.74rem] font-bold text-dim">{text.app.mdSources}</span>
                <strong className="text-[1.25rem] font-extrabold text-main">{config.sources.md_files.length}</strong>
              </div>
              <div className="rounded-xl border border-line bg-subtle p-4">
                <span className="block text-[0.74rem] font-bold text-dim">{text.app.skillSourceDirs}</span>
                <strong className="text-[1.25rem] font-extrabold text-main">{config.sources.skills_dirs.length}</strong>
              </div>
              <div className="rounded-xl border border-line bg-subtle p-4">
                <span className="block text-[0.74rem] font-bold text-dim">{text.app.docsSourceDirs}</span>
                <strong className="text-[1.25rem] font-extrabold text-main">{config.sources.docs_dirs.length}</strong>
              </div>
            </div>
          </section>

          <section
            className="flex flex-col gap-3.5 rounded-2xl border border-line bg-card px-[22px] py-[18px] shadow-[var(--shadow-sm)]"
            aria-label={text.app.activeCommonSkills}
          >
            <div className="flex items-center justify-between gap-3">
              <span className={fieldLabel}>{text.app.activeCommonSkills}</span>
              <button type="button" className="mini-action" onClick={() => setView({ name: "commonSkills" })} disabled={!hasWorkspace} title={hasWorkspace ? undefined : text.app.workspaceRequired}>
                {text.app.commonSkills}
              </button>
            </div>
            <div className="flex flex-wrap gap-2">
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

          <section className="grid grid-cols-[repeat(auto-fill,minmax(360px,1fr))] gap-5">
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
                  disabled={!hasWorkspace}
                  onToggleTarget={(checked) => toggleTarget(endpointId, checked)}
                  onOpen={(path, label) => void openLocation(path, label)}
                  onAdvanced={() => setView({ name: "advanced", target: endpointId })}
                />
              );
            })}
          </section>
        </>
      )}

      {view.name === "sources" && (
        <section className="flex flex-col gap-5">
          <SubpageHeader title={text.app.inputSources} backLabel={text.app.back} onBack={() => setView({ name: "main" })} />
          <div className="flex flex-col gap-4 rounded-2xl border border-line bg-card p-6 shadow-[var(--shadow-sm)]">
            {sourceEditors.map(renderSourceEditor)}
          </div>
        </section>
      )}

      {view.name === "commonSkills" && (
        <section className="flex flex-col gap-5">
          <SubpageHeader title={text.app.commonSkills} backLabel={text.app.back} onBack={() => setView({ name: "main" })} />
          <div className="flex flex-col gap-6 rounded-2xl border border-line bg-card p-6 shadow-[var(--shadow-sm)]">
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
        <section className="flex flex-col gap-5">
          <SubpageHeader
            title={text.app.executionLog}
            backLabel={text.app.back}
            onBack={() => setView({ name: "main" })}
            actions={
              <span className="inline-flex min-h-[30px] items-center rounded-full bg-primary-soft px-3 text-[0.8rem] font-bold text-primary">
                {logs.length} {text.app.entries}
              </span>
            }
          />
          <div className="grid gap-3">
            {logs.length ? (
              logs.map((entry, index) => (
                <div
                  key={`${entry.ts}-${index}`}
                  className={`grid gap-2.5 rounded-xl border px-4 py-3.5 ${entry.level === "error" ? "border-danger-border bg-danger-bg" : "border-line bg-card"}`}
                >
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-[0.8rem] font-semibold text-dim">{entry.ts}</span>
                    <strong
                      className={`inline-flex min-h-6 min-w-[52px] items-center justify-center rounded-full px-2.5 text-[0.72rem] font-extrabold uppercase tracking-[0.04em] ${entry.level === "error" ? "bg-danger-border text-danger-text" : "bg-muted text-dim"}`}
                    >
                      {entry.level}
                    </strong>
                  </div>
                  <code className="log-message">{entry.message}</code>
                </div>
              ))
            ) : (
              <p className="m-0 py-8 text-center text-dim">{text.app.emptyLog}</p>
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
          closeToTray={config.preferences.close_to_tray}
          onCloseToTray={setCloseToTrayPreference}
          version={APP_VERSION}
          onBack={() => setView({ name: "main" })}
        />
      )}

      {view.name === "advanced" && config.endpoints[view.target] && (
        <section className="flex flex-col gap-5">
          <SubpageHeader
            title={`${config.endpoints[view.target].label || view.target} · ${text.app.advancedConfig}`}
            backLabel={text.app.back}
            onBack={() => setView({ name: "main" })}
          />
          <div className="flex flex-col gap-6 rounded-2xl border border-line bg-card p-6 shadow-[var(--shadow-sm)]">
            <div className="grid gap-4 md:grid-cols-2">
              <div className="flex flex-col items-start gap-3">
                <p className="m-0 text-base font-extrabold text-main">{text.app.skillsSyncMode}</p>
                <SegmentedControl<SyncMode>
                  ariaLabel={text.app.skillsSyncMode}
                  value={config.endpoints[view.target].skills_mode ?? config.endpoints[view.target].mode}
                  onChange={(nextMode) => updateEndpoint(view.target, { skills_mode: nextMode })}
                  options={[
                    { value: "junction", label: text.modeOptions.junction },
                    { value: "symlink", label: text.modeOptions.symlink },
                    { value: "copy", label: text.modeOptions.copy },
                  ]}
                />
              </div>
              <div className="flex flex-col items-start gap-3">
                <p className="m-0 text-base font-extrabold text-main">{text.app.docsSyncMode}</p>
                <SegmentedControl<SyncMode>
                  ariaLabel={text.app.docsSyncMode}
                  value={config.endpoints[view.target].docs_mode ?? config.endpoints[view.target].mode}
                  onChange={(nextMode) => updateEndpoint(view.target, { docs_mode: nextMode })}
                  options={[
                    { value: "junction", label: text.modeOptions.junction },
                    { value: "symlink", label: text.modeOptions.symlink },
                    { value: "copy", label: text.modeOptions.copy },
                  ]}
                />
              </div>
            </div>

            <div className="flex flex-col items-start gap-3">
              <p className="m-0 text-base font-extrabold text-main">{text.app.extraSkills}</p>
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

            <div className="flex flex-col items-start gap-3">
              <p className="m-0 text-base font-extrabold text-main">{text.app.excludeSkills}</p>
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

            <div className="flex flex-col items-start gap-3">
              <p className="m-0 text-base font-extrabold text-main">{text.app.replacements}（{text.app.inputSources} → {view.target}）</p>
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
                  updatePairReplacements(view.target, replacements);
                }}
              />
            </div>
          </div>
        </section>
      )}

      {confirmPlan && (
        <Modal title={text.app.confirmSyncTitle} kicker={text.app.runSync} closeLabel={text.app.close} onClose={() => setConfirmPlan(null)}>
          <p className="m-0 text-[0.9rem] font-bold text-main">{text.app.willSync}</p>
          {Array.from(confirmOperationsByTarget.entries()).map(([targetName, operations]) => {
            const summary = summarizeOperations(operations);
            return (
              <div key={targetName} className="flex flex-col gap-2">
                <strong className="text-[0.95rem] text-main">{targetName}</strong>
                <div className="flex flex-wrap gap-2">
                  <span className="rounded-lg border border-line bg-muted px-3 py-1 text-[0.74rem] font-bold text-main">{text.app.mdTarget} {summary.md}</span>
                  <span className="rounded-lg border border-line bg-muted px-3 py-1 text-[0.74rem] font-bold text-main">{text.app.skillsDir} {summary.skills}</span>
                  <span className="rounded-lg border border-line bg-muted px-3 py-1 text-[0.74rem] font-bold text-main">{text.app.docsDir} {summary.docs}</span>
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

          {!!confirmPlan.warnings?.length && (
            <div className="warning-strip">
              <strong>{text.app.syncWarnings}</strong>
              {confirmPlan.warnings.map((warning) => (
                <code key={warning}>{warning}</code>
              ))}
            </div>
          )}

          {!!confirmPlan.errors.length && <p className="muted">{text.app.previewHasErrors}</p>}

          <div className="mt-1 flex justify-end gap-3">
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
          <p className="m-0 text-[0.95rem] font-semibold text-main">{text.app.helpIntro}</p>
          <ul className="m-0 flex flex-col gap-2.5 pl-5">
            {text.app.helpPoints.map((point, index) => (
              <li key={index} className="text-[0.88rem] leading-relaxed text-dim">{point}</li>
            ))}
          </ul>
        </Modal>
      )}

      {showWorkspacePrompt && !config.project_root.trim() && (
        <Modal
          title={text.app.firstRunWorkspaceTitle}
          kicker={text.app.workspaces}
          closeLabel={text.app.skipForNow}
          showHeaderClose={false}
          size="narrow"
          onClose={() => setShowWorkspacePrompt(false)}
        >
          <p className="m-0 text-[0.95rem] font-semibold text-main">{text.app.firstRunWorkspaceIntro}</p>
          <div className="flex flex-wrap justify-end gap-3">
            <button type="button" className="secondary-action" onClick={() => setShowWorkspacePrompt(false)}>
              {text.app.skipForNow}
            </button>
            <button type="button" className="primary-action" onClick={() => void chooseWorkspace()}>
              {text.app.chooseWorkspaceNow}
            </button>
          </div>
        </Modal>
      )}

      {closeDialog && (
        <Modal
          title={text.app.closeAppTitle}
          kicker={text.app.close}
          closeLabel={text.app.cancel}
          showHeaderClose={false}
          size="narrow"
          onClose={() => setCloseDialog(null)}
        >
          <p className="m-0 text-[0.95rem] font-semibold text-main">{text.app.closeAppIntro}</p>
          <label className="flex items-start gap-3 rounded-xl border border-line bg-subtle p-4">
            <input
              type="checkbox"
              className="mt-1 h-[18px] w-[18px] shrink-0"
              checked={closeDialog.remember}
              onChange={(event) => setCloseDialog((current) => (current ? { ...current, remember: event.target.checked } : current))}
            />
            <span className="text-[0.9rem] font-bold text-main">{text.app.rememberMinimizeOnClose}</span>
          </label>
          <div className="flex flex-wrap justify-end gap-3">
            <button type="button" className="secondary-action" onClick={() => setCloseDialog(null)}>
              {text.app.cancel}
            </button>
            <button type="button" className="secondary-action" onClick={() => void handleExitApp()}>
              {text.app.exitApplication}
            </button>
            <button type="button" className="primary-action" onClick={() => void handleMinimizeToTray()}>
              {text.app.minimizeToTray}
            </button>
          </div>
        </Modal>
      )}

      {syncNotice && (
        <Modal title={text.app.syncSuccessTitle} kicker={text.app.runSync} closeLabel={text.app.close} size="success" onClose={() => setSyncNotice(null)}>
          <div className="grid grid-cols-3 gap-3">
            <div className="grid gap-1.5 rounded-xl border border-line bg-subtle p-4 text-center">
              <span className="text-[0.78rem] font-bold text-dim">{text.app.mdTarget}</span>
              <strong className="text-[1.3rem] font-extrabold text-main">{syncNotice.md}</strong>
            </div>
            <div className="grid gap-1.5 rounded-xl border border-line bg-subtle p-4 text-center">
              <span className="text-[0.78rem] font-bold text-dim">{text.app.skillsDir}</span>
              <strong className="text-[1.3rem] font-extrabold text-main">{syncNotice.skills}</strong>
            </div>
            <div className="grid gap-1.5 rounded-xl border border-line bg-subtle p-4 text-center">
              <span className="text-[0.78rem] font-bold text-dim">{text.app.docsDir}</span>
              <strong className="text-[1.3rem] font-extrabold text-main">{syncNotice.docs}</strong>
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
