from __future__ import annotations

import copy
import json
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent.parent
DEFAULT_CONFIG_PATH = SCRIPT_DIR / "cc-sync.config.json"

# schema_version 3 = 输入源集合(md_files / skills_dirs / docs_dirs) -> 多个目标端点。
# schema_version 2 = 对等端点模型(任意源→任意目标)。缺失/为 1 = 旧的 claude_*/targets 形状。
# v1/v2 加载时都会自动迁移到 v3。
SCHEMA_VERSION = 3

DEFAULT_CONFIG: dict[str, Any] = {
    "schema_version": SCHEMA_VERSION,
    "project_root": "",
    "workspaces": [],
    "overwrite_md": True,
    "fallback_to_copy": True,
    "verbose": True,
    # 输入源集合：md 按顺序合并，skills/docs 按直接子项名称去重（同名保留第一个）。
    "sources": {
        "md_files": [],
        "skills_dirs": [],
        "docs_dirs": [],
    },
    # 每个 agent 都是一个输出目标端点。dir/name 用于显示与默认替换推导；已有替换以 replacements 为准。
    "endpoints": {
        "claude": {
            "label": "Claude Code",
            "dir": ".claude",
            "name": "Claude Code",
            "md": "CLAUDE.md",
            "md_local_candidates": [],
            "skills_dirs": [],
            "docs_dirs": [],
            "mode": "junction",
            "skills_mode": "junction",
            "docs_mode": "junction",
        },
        "codex": {
            "label": "Codex",
            "dir": ".codex",
            "name": "Codex",
            "md": "AGENTS.md",
            "md_local_candidates": [],
            "skills_dirs": [],
            "docs_dirs": [],
            "mode": "junction",
            "skills_mode": "junction",
            "docs_mode": "junction",
        },
        "gemini": {
            "label": "Gemini",
            "dir": ".gemini",
            "name": "Gemini",
            "md": "GEMINI.md",
            "md_local_candidates": [],
            "skills_dirs": [],
            "docs_dirs": [],
            "mode": "junction",
            "skills_mode": "junction",
            "docs_mode": "junction",
        },
    },
    # 当前同步目标。
    "sync": {
        "targets": ["codex"],
    },
    # 技能选择：通用 + 该目标 extra − 该目标 exclude（语义与旧版一致，仅换键）。
    "skill_selection": {
        "common": [],
        "extra": {"codex": [], "gemini": []},
        "exclude": {"codex": [], "gemini": []},
    },
    # 显式替换，按目标 id 存。旧版 "源->目标" 键加载时会迁移为目标 id。
    "replacements": {
        "codex": {
            ".claude": ".codex",
            "Claude Code": "Codex",
            "CLAUDE.md": "AGENTS.md",
            "CLAUDE.local.md": "AGENTS.md",
            "claude.local.md": "AGENTS.md",
        },
        "gemini": {
            ".claude": ".gemini",
            "Claude Code": "Gemini",
            "CLAUDE.md": "GEMINI.md",
            "CLAUDE.local.md": "GEMINI.md",
            "claude.local.md": "GEMINI.md",
        },
    },
    "preferences": {
        "close_to_tray": False,
    },
    # runtime.* 仅供 Rust 桌面启动器读取（python_executable / script_path），本引擎不读取。
    "runtime": {
        "python_executable": "python/bin/python.exe",
        "script_path": "sync_agents.py",
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
    """旧的 claude_*/targets 形状(无 endpoints/sources)。"""
    if "endpoints" in cfg or "sources" in cfg:
        return False
    return "claude_md" in cfg or "targets" in cfg


def is_v2_config(cfg: dict[str, Any]) -> bool:
    """v2 对等端点模型：有 endpoints，但没有 v3 sources。"""
    return "endpoints" in cfg and "sources" not in cfg


def _config_project_root(cfg: dict[str, Any], config_base: Path) -> Path:
    root = Path(str(cfg.get("project_root", ".")))
    if root.is_absolute():
        return root
    return (config_base / root).resolve()


def _existing_md_files(paths: list[str], *, project_root: Path) -> list[str]:
    result: list[str] = []
    seen: set[str] = set()
    for raw in paths:
        if not raw or raw in seen:
            continue
        candidate = Path(raw)
        resolved = candidate if candidate.is_absolute() else project_root / candidate
        # v2 的 md_local_candidates 是候选项语义，迁移时只保留真实存在的文件，避免升级后误报缺失。
        if not result or resolved.exists():
            result.append(raw)
            seen.add(raw)
    return result


def _target_replacements_from_pairs(replacements: dict[str, Any]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in replacements.items():
        if "->" in key:
            _, target = key.split("->", 1)
            result[target] = value
        else:
            result[key] = value
    return result


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


def migrate_legacy(loaded: dict[str, Any], config_base: Path) -> dict[str, Any]:
    """把旧配置(claude_* + targets)就地转成 v3 输入源集合 + 目标端点模型。"""
    cfg = copy.deepcopy(loaded)
    project_root = _config_project_root(cfg, config_base)
    md_files = _existing_md_files(
        [cfg.get("claude_md", "CLAUDE.md"), *cfg.get("claude_local_md_candidates", [])],
        project_root=project_root,
    )

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
            "skills_mode": target_cfg.get("skills_mode", target_cfg.get("mode", "junction")),
            "docs_mode": target_cfg.get("docs_mode", target_cfg.get("mode", "junction")),
        }
        replacements[target_id] = reps
        if target_cfg.get("enabled", True):
            enabled_targets.append(target_id)

    return {
        "schema_version": SCHEMA_VERSION,
        "project_root": cfg.get("project_root", "."),
        "workspaces": cfg.get("workspaces", []),
        "overwrite_md": cfg.get("overwrite_md", True),
        "fallback_to_copy": cfg.get("fallback_to_copy", True),
        "verbose": cfg.get("verbose", True),
        "sources": {
            "md_files": md_files,
            "skills_dirs": cfg.get("claude_skill_sources", []),
            "docs_dirs": cfg.get("claude_docs_sources", []),
        },
        "endpoints": endpoints,
        "sync": {"targets": enabled_targets},
        "skill_selection": {
            "common": cfg.get("common_skills", []),
            "extra": cfg.get("target_extra_skills", {}),
            "exclude": cfg.get("target_exclude_skills", {}),
        },
        "replacements": replacements,
        "preferences": cfg.get("preferences", {}),
        "runtime": cfg.get("runtime", {}),
    }


def migrate_v2(loaded: dict[str, Any], config_base: Path) -> dict[str, Any]:
    """把 v2 的 source endpoint 抽出来变成 v3 输入源集合。"""
    cfg = copy.deepcopy(loaded)
    project_root = _config_project_root(cfg, config_base)
    source_id = cfg.get("sync", {}).get("source", "claude")
    source_endpoint = cfg.get("endpoints", {}).get(source_id, {})

    return {
        "schema_version": SCHEMA_VERSION,
        "project_root": cfg.get("project_root", "."),
        "workspaces": cfg.get("workspaces", []),
        "overwrite_md": cfg.get("overwrite_md", True),
        "fallback_to_copy": cfg.get("fallback_to_copy", True),
        "verbose": cfg.get("verbose", True),
        "sources": {
            "md_files": _existing_md_files(
                [source_endpoint.get("md", ""), *source_endpoint.get("md_local_candidates", [])],
                project_root=project_root,
            ),
            "skills_dirs": source_endpoint.get("skills_dirs", []),
            "docs_dirs": source_endpoint.get("docs_dirs", []),
        },
        "endpoints": cfg.get("endpoints", {}),
        "sync": {"targets": cfg.get("sync", {}).get("targets", [])},
        "skill_selection": cfg.get("skill_selection", {}),
        "replacements": _target_replacements_from_pairs(cfg.get("replacements", {})),
        "preferences": cfg.get("preferences", {}),
        "runtime": cfg.get("runtime", {}),
    }


def load_config(config_path: str | Path | None = None) -> tuple[dict[str, Any], Path]:
    effective_path = Path(config_path).resolve() if config_path else DEFAULT_CONFIG_PATH
    config = get_default_config()

    if effective_path.exists():
        loaded = json.loads(effective_path.read_text(encoding="utf-8"))
        if not isinstance(loaded, dict):
            raise ValueError(f"配置文件必须是 JSON 对象: {effective_path}")
        # 关键：先把旧文件迁移成 v3，再 deep_merge 到 v3 默认值上，避免新旧键混在一起的 Frankenstein。
        if is_legacy_config(loaded):
            loaded = migrate_legacy(loaded, effective_path.parent)
        elif is_v2_config(loaded):
            loaded = migrate_v2(loaded, effective_path.parent)
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
