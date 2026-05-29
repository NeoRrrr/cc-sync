from __future__ import annotations

import copy
import json
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent.parent
DEFAULT_CONFIG_PATH = SCRIPT_DIR / "cc-sync.config.json"

# schema_version 2 = 对等端点模型(任意源→任意目标)。缺失/为 1 = 旧的 claude_*/targets 形状，加载时自动迁移。
SCHEMA_VERSION = 2

DEFAULT_CONFIG: dict[str, Any] = {
    "schema_version": SCHEMA_VERSION,
    "project_root": ".",
    "overwrite_md": True,
    "fallback_to_copy": True,
    "verbose": True,
    # 每个 agent 都是同形端点：既能当源(读 md/skills/docs)，也能当目标(写 md、链/拷 skills/docs)。
    # dir/name 是身份标记，用于显示和"全新源→目标对"的默认替换推导；已有对的替换以 replacements 为准。
    "endpoints": {
        "claude": {
            "label": "Claude Code",
            "dir": ".claude",
            "name": "Claude Code",
            "md": "CLAUDE.md",
            "md_local_candidates": [".claude/CLAUDE.local.md", ".claude/claude.local.md"],
            "skills_dirs": [".claude/skills", "tools/AI/claude/skills"],
            "docs_dirs": [".claude/docs", "tools/AI/claude/docs"],
            "mode": "junction",
        },
        "codex": {
            "label": "Codex",
            "dir": ".codex",
            "name": "Codex",
            "md": "AGENTS.md",
            "md_local_candidates": [],
            "skills_dirs": [".codex/skills"],
            "docs_dirs": [".codex/docs"],
            "mode": "junction",
        },
        "gemini": {
            "label": "Gemini",
            "dir": ".gemini",
            "name": "Gemini",
            "md": "GEMINI.md",
            "md_local_candidates": [],
            "skills_dirs": [".gemini/skills"],
            "docs_dirs": [".gemini/docs"],
            "mode": "junction",
        },
    },
    # 当前同步：选一个源 + 一组目标。
    "sync": {
        "source": "claude",
        "targets": ["codex", "gemini"],
    },
    # 技能选择：通用 + 该目标 extra − 该目标 exclude（语义与旧版一致，仅换键）。
    "skill_selection": {
        "common": ["act-dev", "video-expert", "xlocust-smoke"],
        "extra": {"codex": [], "gemini": []},
        "exclude": {"codex": [], "gemini": []},
    },
    # 显式替换，按 "源->目标" 存。已有对原样保留；全新对在引擎里从 dir/md/name 推导一个基础版。
    "replacements": {
        "claude->codex": {
            ".claude": ".codex",
            "Claude Code": "Codex",
            "CLAUDE.md": "AGENTS.md",
            "CLAUDE.local.md": "AGENTS.md",
            "claude.local.md": "AGENTS.md",
        },
        "claude->gemini": {
            ".claude": ".gemini",
            "Claude Code": "Gemini",
            "CLAUDE.md": "GEMINI.md",
            "CLAUDE.local.md": "GEMINI.md",
            "claude.local.md": "GEMINI.md",
        },
    },
    # runtime.* 仅供 Rust 桌面启动器读取（python_executable / script_path），本引擎不读取。
    "runtime": {
        "python_executable": "python/bin/python.exe",
        "script_path": "sync_from_claude.py",
        "config_path": "cc-sync.config.json",
    },
}


def deep_merge(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    merged = copy.deepcopy(base)
    for key, value in override.items():
        if isinstance(merged.get(key), dict) and isinstance(value, dict):
            merged[key] = deep_merge(merged[key], value)
        else:
            merged[key] = value
    return merged


def get_default_config() -> dict[str, Any]:
    return copy.deepcopy(DEFAULT_CONFIG)


def is_legacy_config(cfg: dict[str, Any]) -> bool:
    """旧的 claude_*/targets 形状(无 endpoints、schema_version 不是 2)。"""
    if cfg.get("schema_version") == SCHEMA_VERSION or "endpoints" in cfg:
        return False
    return "claude_md" in cfg or "targets" in cfg


def _infer_identity(target_id: str, replacements: dict[str, str]) -> tuple[str, str]:
    """从旧 text_replacements 反推一个端点的 dir/name 标记，仅用于显示与全新对的默认推导。"""
    dir_token = None
    name_token = None
    for old, new in (replacements or {}).items():
        if old == ".claude":
            dir_token = new
        elif old == "Claude Code":
            name_token = new
    return dir_token or ("." + target_id), name_token or target_id.capitalize()


def migrate_legacy(loaded: dict[str, Any]) -> dict[str, Any]:
    """把旧配置(claude_* + targets)就地转成 v2 端点模型。替换 map 原样保留，绝不丢信息。"""
    cfg = copy.deepcopy(loaded)

    endpoints: dict[str, Any] = {
        "claude": {
            "label": "Claude Code",
            "dir": ".claude",
            "name": "Claude Code",
            "md": cfg.get("claude_md", "CLAUDE.md"),
            "md_local_candidates": cfg.get("claude_local_md_candidates", []),
            "skills_dirs": cfg.get("claude_skill_sources", []),
            "docs_dirs": cfg.get("claude_docs_sources", []),
            "mode": "junction",
        }
    }

    replacements: dict[str, Any] = {}
    enabled_targets: list[str] = []

    for target_id, target_cfg in cfg.get("targets", {}).items():
        reps = target_cfg.get("text_replacements", {}) or {}
        dir_token, name_token = _infer_identity(target_id, reps)
        endpoints[target_id] = {
            "label": target_id.capitalize(),
            "dir": dir_token,
            "name": name_token,
            "md": target_cfg.get("md_target", ""),
            "md_local_candidates": [],
            "skills_dirs": [target_cfg["skills_dir"]] if target_cfg.get("skills_dir") else [],
            "docs_dirs": [target_cfg["docs_dir"]] if target_cfg.get("docs_dir") else [],
            "mode": target_cfg.get("mode", "junction"),
        }
        replacements[f"claude->{target_id}"] = reps
        if target_cfg.get("enabled", True):
            enabled_targets.append(target_id)

    return {
        "schema_version": SCHEMA_VERSION,
        "project_root": cfg.get("project_root", "."),
        "overwrite_md": cfg.get("overwrite_md", True),
        "fallback_to_copy": cfg.get("fallback_to_copy", True),
        "verbose": cfg.get("verbose", True),
        "endpoints": endpoints,
        "sync": {"source": "claude", "targets": enabled_targets},
        "skill_selection": {
            "common": cfg.get("common_skills", []),
            "extra": cfg.get("target_extra_skills", {}),
            "exclude": cfg.get("target_exclude_skills", {}),
        },
        "replacements": replacements,
        "runtime": cfg.get("runtime", {}),
    }


def load_config(config_path: str | Path | None = None) -> tuple[dict[str, Any], Path]:
    effective_path = Path(config_path).resolve() if config_path else DEFAULT_CONFIG_PATH
    config = get_default_config()

    if effective_path.exists():
        loaded = json.loads(effective_path.read_text(encoding="utf-8"))
        if not isinstance(loaded, dict):
            raise ValueError(f"配置文件必须是 JSON 对象: {effective_path}")
        # 关键：先把旧文件迁移成 v2，再 deep_merge 到 v2 默认值上，避免新旧键混在一起的 Frankenstein。
        if is_legacy_config(loaded):
            loaded = migrate_legacy(loaded)
        config = deep_merge(config, loaded)

    return config, effective_path


def write_default_config(path: str | Path | None = None, *, overwrite: bool = False) -> Path:
    effective_path = Path(path).resolve() if path else DEFAULT_CONFIG_PATH
    if effective_path.exists() and not overwrite:
        return effective_path

    effective_path.parent.mkdir(parents=True, exist_ok=True)
    effective_path.write_text(
        json.dumps(DEFAULT_CONFIG, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return effective_path
