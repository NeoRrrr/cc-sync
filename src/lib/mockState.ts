import type { SyncConfig } from "../types";

/* 非桌面(浏览器 demo)模式下展示用的示例配置(v3 输入源集合模型)。 */
export const mockConfig: SyncConfig = {
  schema_version: 3,
  project_root: "C:\\Projects\\agent-workspace",
  workspaces: ["C:\\Projects\\agent-workspace", "D:\\Repos\\customer-portal"],
  overwrite_md: true,
  fallback_to_copy: true,
  verbose: true,
  sources: {
    md_files: ["shared\\AGENT_BASE.md", "shared\\TEAM_RULES.md"],
    skills_dirs: ["shared\\skills", "team\\skills"],
    docs_dirs: ["shared\\docs", "runbooks"]
  },
  endpoints: {
    claude: {
      label: "Claude Code",
      dir: ".claude",
      name: "Claude Code",
      md: "CLAUDE.md",
      md_local_candidates: [".claude\\CLAUDE.local.md"],
      skills_dirs: [".claude\\skills"],
      docs_dirs: [".claude\\docs"],
      mode: "junction"
    },
    codex: {
      label: "Codex",
      dir: ".codex",
      name: "Codex",
      md: "AGENTS.md",
      md_local_candidates: [],
      skills_dirs: [".codex\\skills"],
      docs_dirs: [".codex\\docs"],
      mode: "junction"
    },
    gemini: {
      label: "Gemini",
      dir: ".gemini",
      name: "Gemini",
      md: "GEMINI.md",
      md_local_candidates: [],
      skills_dirs: [".gemini\\skills"],
      docs_dirs: [".gemini\\docs"],
      mode: "junction"
    }
  },
  sync: {
    targets: ["claude", "codex", "gemini"]
  },
  skill_selection: {
    common: ["ui-expert", "release-checklist"],
    extra: { codex: ["repo-reviewer"], gemini: ["doc-summarizer"] },
    exclude: { codex: [], gemini: [] }
  },
  replacements: {
    codex: {
      ".claude": ".codex",
      "Claude Code": "Codex",
      "CLAUDE.md": "AGENTS.md",
      "CLAUDE.local.md": "AGENTS.md",
      "claude.local.md": "AGENTS.md"
    },
    gemini: {
      ".claude": ".gemini",
      "Claude Code": "Gemini",
      "CLAUDE.md": "GEMINI.md",
      "CLAUDE.local.md": "GEMINI.md",
      "claude.local.md": "GEMINI.md"
    }
  },
  preferences: {
    close_to_tray: false
  },
  runtime: {
    python_executable: "python/bin/python.exe",
    script_path: "sync_agents.py",
    config_path: "cc-sync.config.json"
  }
};
