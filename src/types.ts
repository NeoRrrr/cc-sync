export type SyncMode = "junction" | "symlink" | "copy";
export type SyncScope = "all" | "md" | "skills" | "docs";

export type RuntimeConfig = {
  python_executable: string;
  script_path: string;
  config_path: string;
};

/* 对等端点：每个 agent 同形，既能当源也能当目标。 */
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
  source: string;
  targets: string[];
};

export type SkillSelection = {
  common: string[];
  extra: Record<string, string[]>;
  exclude: Record<string, string[]>;
};

export type SyncConfig = {
  schema_version: number;
  project_root: string;
  overwrite_md: boolean;
  fallback_to_copy: boolean;
  verbose: boolean;
  endpoints: Record<string, Endpoint>;
  sync: SyncSpec;
  skill_selection: SkillSelection;
  replacements: Record<string, Record<string, string>>;
  runtime: RuntimeConfig;
};

export type PlanOperation = {
  type: "md" | "skills" | "docs";
  target: string;
  name?: string;
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
  docs_src: string | null;
  skill_source_roots: string[];
  operations: PlanOperation[];
  errors: string[];
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
  paths: string[];
};

/* target skills 目录里某技能的真实磁盘状态。 */
export type TargetSkill = {
  name: string;
  kind: "link" | "copy" | "broken";
};
