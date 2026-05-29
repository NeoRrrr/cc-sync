#!/usr/bin/env python3
# -*- coding: utf-8 -*-

from __future__ import annotations

import argparse
import json
import os
import sys
import shutil
import subprocess
from pathlib import Path
from typing import Any

from sync_config import DEFAULT_CONFIG_PATH, PROJECT_ROOT, load_config, write_default_config


CONFIG: dict[str, Any] = {}
CONFIG_BASE = PROJECT_ROOT
OUTPUT_JSON = False


def log(msg: str) -> None:
    if CONFIG.get("verbose", True):
        print(msg, file=sys.stderr if OUTPUT_JSON else sys.stdout)


def ensure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def is_windows() -> bool:
    return os.name == "nt"


def resolve_path(value: str | None, *, base: Path) -> Path | None:
    if not value:
        return None
    path = Path(value)
    if path.is_absolute():
        return path
    return (base / path).resolve()


def resolve_path_list(values: list[str], *, base: Path) -> list[Path]:
    return [resolve_path(value, base=base) for value in values if value]


def remove_path(path: Path) -> None:
    if not path.exists() and not path.is_symlink():
        return

    try:
        if path.is_file() or path.is_symlink():
            path.unlink()
            return

        if is_windows():
            result = subprocess.run(
                ["cmd", "/c", "rmdir", str(path)],
                capture_output=True,
                text=True,
                shell=False,
            )
            if result.returncode == 0:
                return

        shutil.rmtree(path)

    except FileNotFoundError:
        return


def read_text(path: Path) -> str:
    if not path.exists():
        raise FileNotFoundError(f"源文件不存在: {path}")
    return path.read_text(encoding="utf-8")


def write_text(path: Path, content: str, overwrite: bool = True) -> None:
    if path.exists():
        if not overwrite:
            log(f"[skip] 文件已存在: {path}")
            return

        if path.is_file() or path.is_symlink():
            path.unlink()
        else:
            remove_path(path)

    ensure_dir(path.parent)
    path.write_text(content, encoding="utf-8", newline="\n")


def render_text_for_target(content: str, ordered_pairs: list[list[str]]) -> str:
    # ordered_pairs 已按 old 长度降序排好（长的先替换），避免 ".claude" 抢在 "CLAUDE.md" 前面。
    for pair in ordered_pairs:
        old, new = pair[0], pair[1]
        content = content.replace(old, new)
    return content


def merge_md_with_local(
    main_src: Path,
    local_src: Path | None,
    dst: Path,
    ordered_pairs: list[list[str]],
    overwrite: bool = True,
) -> None:
    content = read_text(main_src)

    if local_src and local_src.exists():
        local_content = read_text(local_src)
        content = content.rstrip() + "\n\n---\n\n" + local_content.lstrip()

    content = render_text_for_target(content, ordered_pairs)
    write_text(dst, content, overwrite=overwrite)
    log(f"[md-merge] {main_src} + {local_src} -> {dst}")


def copy_dir(src: Path, dst: Path) -> None:
    if not src.exists():
        raise FileNotFoundError(f"源目录不存在: {src}")

    remove_path(dst)
    shutil.copytree(src, dst)
    log(f"[copy] {dst} <- {src}")


def create_symlink(src: Path, dst: Path) -> None:
    remove_path(dst)
    ensure_dir(dst.parent)
    os.symlink(str(src), str(dst), target_is_directory=True)
    log(f"[symlink] {dst} -> {src}")


def create_junction_windows(src: Path, dst: Path) -> None:
    remove_path(dst)
    ensure_dir(dst.parent)

    cmd = ["cmd", "/c", "mklink", "/J", str(dst), str(src)]
    result = subprocess.run(cmd, capture_output=True, text=True, shell=False)

    if result.returncode != 0:
        err = result.stderr.strip() or result.stdout.strip() or "mklink /J failed"
        raise RuntimeError(err)

    log(f"[junction] {dst} -> {src}")


def link_or_copy_dir(src: Path, dst: Path, mode: str) -> None:
    if not src.exists():
        raise FileNotFoundError(f"源目录不存在: {src}")

    normalized_mode = mode.lower()

    try:
        if normalized_mode == "junction":
            if not is_windows():
                raise RuntimeError("junction 仅支持 Windows")
            create_junction_windows(src, dst)
            return

        if normalized_mode == "symlink":
            create_symlink(src, dst)
            return

        if normalized_mode == "copy":
            copy_dir(src, dst)
            return

        raise ValueError(f"未知 mode: {mode}")

    except Exception as exc:
        if not CONFIG.get("fallback_to_copy", True) or normalized_mode == "copy":
            raise

        log(f"[warn] {normalized_mode} 失败，回退 copy: {dst}, err={exc}")
        copy_dir(src, dst)


def get_project_root() -> Path:
    project_root_value = CONFIG.get("project_root", ".")
    return resolve_path(project_root_value, base=CONFIG_BASE) or CONFIG_BASE


def find_first_existing(paths: list[Path]) -> Path | None:
    for path in paths:
        if path.exists():
            return path
    return None


def get_endpoints() -> dict[str, Any]:
    return CONFIG.get("endpoints", {})


def get_endpoint(endpoint_id: str) -> dict[str, Any] | None:
    return get_endpoints().get(endpoint_id)


def get_sync_spec() -> dict[str, Any]:
    return CONFIG.get("sync", {})


def endpoint_md(endpoint: dict[str, Any]) -> Path | None:
    return resolve_path(endpoint.get("md"), base=get_project_root())


def endpoint_local_md(endpoint: dict[str, Any]) -> Path | None:
    return find_first_existing(
        resolve_path_list(endpoint.get("md_local_candidates", []), base=get_project_root())
    )


def endpoint_skill_roots(endpoint: dict[str, Any]) -> list[Path]:
    return resolve_path_list(endpoint.get("skills_dirs", []), base=get_project_root())


def endpoint_skills_write_dir(endpoint: dict[str, Any]) -> Path | None:
    roots = endpoint_skill_roots(endpoint)
    return roots[0] if roots else None


def endpoint_docs_src(endpoint: dict[str, Any]) -> Path | None:
    return find_first_existing(
        resolve_path_list(endpoint.get("docs_dirs", []), base=get_project_root())
    )


def endpoint_docs_write_dir(endpoint: dict[str, Any]) -> Path | None:
    roots = resolve_path_list(endpoint.get("docs_dirs", []), base=get_project_root())
    return roots[0] if roots else None


def get_source_id() -> str:
    return get_sync_spec().get("source", "claude")


def get_source_endpoint() -> dict[str, Any] | None:
    return get_endpoint(get_source_id())


def get_source_skill_roots() -> list[Path]:
    endpoint = get_source_endpoint()
    return endpoint_skill_roots(endpoint) if endpoint else []


def list_available_skills() -> list[str]:
    skills = set()
    for root in get_source_skill_roots():
        if root.exists() and root.is_dir():
            for item in root.iterdir():
                if item.is_dir() and not item.name.startswith("."):
                    skills.add(item.name)
    return sorted(list(skills))


def find_skill_source(skill_name: str) -> Path:
    checked_paths: list[Path] = []

    for root in get_source_skill_roots():
        candidate = root / skill_name
        checked_paths.append(candidate)
        if candidate.exists():
            return candidate

    checked = "\n".join(str(path) for path in checked_paths)
    raise FileNotFoundError(f"未找到 skill: {skill_name}\n已检查:\n{checked}")


def get_skills_for_target(target_id: str) -> list[str]:
    selection = CONFIG.get("skill_selection", {})
    skills = list(selection.get("common", []))
    skills.extend(selection.get("extra", {}).get(target_id, []))

    exclude = set(selection.get("exclude", {}).get(target_id, []))
    result: list[str] = []
    seen: set[str] = set()

    for skill in skills:
        if skill in exclude or skill in seen:
            continue
        seen.add(skill)
        result.append(skill)

    return result


def build_replacements(
    source_endpoint: dict[str, Any],
    target_endpoint: dict[str, Any],
    source_id: str,
    target_id: str,
) -> list[list[str]]:
    """返回按 old 长度降序排好的 [old, new] 列表。
    已有的 源->目标 替换 map 原样用（保留多对一等真实数据）；全新对才从 dir/md/name 推导一个基础版。
    """
    stored = CONFIG.get("replacements", {}).get(f"{source_id}->{target_id}")
    if stored:
        pairs = [(old, new) for old, new in stored.items()]
    else:
        pairs = []
        for key in ("md", "name", "dir"):
            old = source_endpoint.get(key)
            new = target_endpoint.get(key)
            if old and new:
                pairs.append((old, new))

    pairs = [(old, new) for (old, new) in pairs if old and new and old != new]
    pairs.sort(key=lambda pair: len(pair[0]), reverse=True)
    return [[old, new] for (old, new) in pairs]


def build_plan(
    source_id: str | None = None,
    target_ids: list[str] | None = None,
    scope: str = "all",
) -> dict[str, Any]:
    project_root = get_project_root()
    spec = get_sync_spec()
    source_id = source_id or spec.get("source", "claude")
    if target_ids is None:
        target_ids = list(spec.get("targets", []))

    operations: list[dict[str, Any]] = []
    errors: list[str] = []

    source_endpoint = get_endpoint(source_id)
    if source_endpoint is None:
        return {
            "source": source_id,
            "project_root": str(project_root),
            "scope": scope,
            "main_md": None,
            "local_md": None,
            "docs_src": None,
            "skill_source_roots": [],
            "operations": [],
            "errors": [f"源端点不存在: {source_id}"],
        }

    main_md = endpoint_md(source_endpoint)
    local_md = endpoint_local_md(source_endpoint)
    docs_src = endpoint_docs_src(source_endpoint)
    skill_source_roots = endpoint_skill_roots(source_endpoint)

    if main_md is None:
        errors.append(f"源 {source_id} 的 md 未配置")
    elif not main_md.exists():
        errors.append(f"源 md 不存在: {main_md}")

    for target_id in target_ids:
        if target_id == source_id:
            errors.append(f"源与目标相同，已跳过: {target_id}")
            continue

        target_endpoint = get_endpoint(target_id)
        if target_endpoint is None:
            errors.append(f"目标端点不存在: {target_id}")
            continue

        mode = target_endpoint.get("mode", "junction")

        if scope in {"all", "md"} and main_md:
            target_md = endpoint_md(target_endpoint)
            if target_md:
                operations.append(
                    {
                        "type": "md",
                        "target": target_id,
                        "main_src": str(main_md),
                        "local_src": str(local_md) if local_md else None,
                        "dst": str(target_md),
                        "overwrite": bool(CONFIG.get("overwrite_md", True)),
                        "replacements": build_replacements(source_endpoint, target_endpoint, source_id, target_id),
                    }
                )

        if scope in {"all", "skills"}:
            write_dir = endpoint_skills_write_dir(target_endpoint)
            if write_dir:
                for skill_name in get_skills_for_target(target_id):
                    try:
                        src = find_skill_source(skill_name)
                    except FileNotFoundError as exc:
                        errors.append(str(exc))
                        continue

                    operations.append(
                        {
                            "type": "skills",
                            "target": target_id,
                            "name": skill_name,
                            "src": str(src),
                            "dst": str(write_dir / skill_name),
                            "mode": mode,
                        }
                    )

        if scope in {"all", "docs"}:
            write_dir = endpoint_docs_write_dir(target_endpoint)
            if write_dir:
                if docs_src is None:
                    # 源 docs 缺失视为计划期错误（与 skills 分支一致），dry-run 与真跑一致、避免半残写入。
                    errors.append(f"源 docs 目录不存在，已跳过 {target_id} 的 docs 同步")
                else:
                    operations.append(
                        {
                            "type": "docs",
                            "target": target_id,
                            "name": "docs",
                            "src": str(docs_src),
                            "dst": str(write_dir),
                            "mode": mode,
                            "missing_source": False,
                        }
                    )

    return {
        "source": source_id,
        "project_root": str(project_root),
        "scope": scope,
        "main_md": str(main_md) if main_md else None,
        "local_md": str(local_md) if local_md else None,
        "docs_src": str(docs_src) if docs_src else None,
        "skill_source_roots": [str(path) for path in skill_source_roots],
        "operations": operations,
        "errors": errors,
    }


def execute_operation(operation: dict[str, Any]) -> None:
    if operation["type"] == "md":
        merge_md_with_local(
            Path(operation["main_src"]),
            Path(operation["local_src"]) if operation.get("local_src") else None,
            Path(operation["dst"]),
            operation.get("replacements", []),
            overwrite=bool(operation.get("overwrite", True)),
        )
        return

    if operation["type"] in {"skills", "docs"}:
        src = operation.get("src")
        if not src:
            raise FileNotFoundError(f"{operation['type']} 源目录不存在")
        link_or_copy_dir(
            Path(src),
            Path(operation["dst"]),
            operation.get("mode", "junction"),
        )
        return

    raise ValueError(f"未知 operation type: {operation['type']}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Sync Claude markdown, docs, and skills to other agent targets."
    )
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG_PATH, help="Path to a JSON config file.")
    parser.add_argument("--dry-run", action="store_true", help="Preview the sync plan without writing files.")
    parser.add_argument("--json", action="store_true", help="Print the sync plan as JSON.")
    parser.add_argument(
        "--scope",
        choices=["all", "md", "skills", "docs"],
        default="all",
        help="Limit sync to md, skills, docs, or all.",
    )
    parser.add_argument(
        "--source",
        default=None,
        help="Source endpoint id. Defaults to config sync.source.",
    )
    parser.add_argument(
        "--target",
        action="append",
        default=[],
        help="Target endpoint id. Repeatable. Defaults to config sync.targets.",
    )
    parser.add_argument(
        "--emit-config",
        action="store_true",
        help="Load + migrate the config and print the normalized v2 JSON, then exit.",
    )
    parser.add_argument(
        "--write-default-config",
        action="store_true",
        help="Write the default JSON config file and exit.",
    )
    parser.add_argument(
        "--force-overwrite-config",
        action="store_true",
        help="Overwrite config when used with --write-default-config.",
    )
    parser.add_argument(
        "--list-skills",
        action="store_true",
        help="List available skills from the configured sources and exit.",
    )
    return parser.parse_args()


def main() -> int:
    global CONFIG, CONFIG_BASE, OUTPUT_JSON

    # 强制 stdout/stderr 使用 UTF-8。Windows 下重定向到管道时 print 默认用本地代码页(如 GBK)，
    # 而 Rust 桌面壳按严格 UTF-8 解析 stdout，含中文(如错误信息)会解析失败。统一 UTF-8 即对齐契约。
    for _stream in (sys.stdout, sys.stderr):
        try:
            _stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass

    args = parse_args()
    OUTPUT_JSON = bool(args.json)

    if args.write_default_config:
        saved_path = write_default_config(args.config, overwrite=args.force_overwrite_config)
        if args.json:
            print(json.dumps({"config_path": str(saved_path)}, ensure_ascii=False, indent=2))
        else:
            print(f"[done] wrote config: {saved_path}")
        return 0

    CONFIG, config_path = load_config(args.config)
    CONFIG_BASE = config_path.parent

    if args.emit_config:
        # 把(可能已从旧版迁移的)规范化 v2 配置打印出来，供 Rust 桌面壳作为单一迁移入口读取。
        print(json.dumps(CONFIG, ensure_ascii=False, indent=2))
        return 0

    if args.list_skills:
        skills = list_available_skills()
        if args.json:
            print(json.dumps(skills, ensure_ascii=False))
        else:
            for skill in skills:
                print(skill)
        return 0

    source_id = args.source or None
    target_ids = list(args.target) if args.target else None
    plan = build_plan(source_id=source_id, target_ids=target_ids, scope=args.scope)
    success = not plan["errors"]

    if args.dry_run:
        if args.json:
            payload = dict(plan)
            payload["config_path"] = str(config_path)
            payload["success"] = success
            payload["executed"] = False
            print(json.dumps(payload, indent=2, ensure_ascii=False))
            return 0 if success else 1

        log(f"[info] project_root = {plan['project_root']}")
        for operation in plan["operations"]:
            if operation["type"] == "md":
                log(
                    f"[dry-run][md] {operation['target']}: "
                    f"{operation['main_src']} + {operation['local_src']} -> {operation['dst']}"
                )
            else:
                log(
                    f"[dry-run][{operation['type']}] {operation['target']}:{operation['name']} "
                    f"{operation['src']} -> {operation['dst']} ({operation['mode']})"
                )
        for error in plan["errors"]:
            print(f"[error] {error}")
        log("[done] dry-run finished")
        return 0 if success else 1

    if not success:
        if args.json:
            payload = dict(plan)
            payload["config_path"] = str(config_path)
            payload["success"] = False
            payload["executed"] = False
            print(json.dumps(payload, indent=2, ensure_ascii=False))
            return 1

        for error in plan["errors"]:
            print(f"[error] {error}")
        return 1

    log(f"[info] PROJECT_ROOT = {plan['project_root']}")
    for operation in plan["operations"]:
        execute_operation(operation)
    log("[done] sync finished")

    if args.json:
        payload = dict(plan)
        payload["config_path"] = str(config_path)
        payload["success"] = True
        payload["executed"] = True
        print(json.dumps(payload, indent=2, ensure_ascii=False))

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
