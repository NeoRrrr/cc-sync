export type SyncMode = "junction" | "symlink" | "copy";
export type SyncScope = "all" | "md" | "skills" | "docs";

export type RuntimeConfig = {
  python_executable: string;
  script_path: string;
  config_path: string;
};

export type SourceConfig = {
  md_files: string[];
  skills_dirs: string[];
  docs_dirs: string[];
};

/* 输出端点：每个 agent 的写入目标。 */
export type Endpoint = {
  label: string;
  dir: string;
  name: string;
  md: string;
  md_local_candidates: string[];
  skills_dirs: string[];
  docs_dirs: string[];
  mode: SyncMode;
};

export type SyncSpec = {
  source?: string;
  targets: string[];
};

export type SkillSelection = {
  common: string[];
  extra: Record<string, string[]>;
  exclude: Record<string, string[]>;
};

export type PreferencesConfig = {
  close_to_tray: boolean;
};

export type SyncConfig = {
  schema_version: number;
  project_root: string;
  workspaces: string[];
  overwrite_md: boolean;
  fallback_to_copy: boolean;
  verbose: boolean;
  sources: SourceConfig;
  endpoints: Record<string, Endpoint>;
  sync: SyncSpec;
  skill_selection: SkillSelection;
  replacements: Record<string, Record<string, string>>;
  preferences: PreferencesConfig;
  runtime: RuntimeConfig;
};

export type PlanOperation = {
  type: "md" | "skills" | "docs";
  target: string;
  name?: string;
  sources?: string[];
  main_src?: string;
  local_src?: string | null;
  src?: string | null;
  dst: string;
  mode?: SyncMode;
  overwrite?: boolean;
  missing_source?: boolean;
  replacements?: [string, string][];
};

export type SyncPlan = {
  source?: string;
  project_root: string;
  scope: SyncScope;
  main_md: string | null;
  local_md: string | null;
  md_sources?: string[];
  docs_src: string | null;
  docs_source_roots?: string[];
  skill_source_roots: string[];
  operations: PlanOperation[];
  errors: string[];
  warnings?: string[];
  config_path?: string;
  success?: boolean;
  executed?: boolean;
};

export type ActivityLog = {
  ts: string;
  level: "info" | "error";
  message: string;
};

export type AvailableSkillOption = {
  name: string;
  display_name?: string | null;
  description?: string | null;
  paths: string[];
};

/* target skills 目录里某技能的真实磁盘状态。 */
export type TargetSkill = {
  name: string;
  kind: "link" | "copy" | "broken";
};
