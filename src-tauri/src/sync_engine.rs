use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file, MetadataExt};
use std::path::{Component, Path, PathBuf};
#[cfg(windows)]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub const STATE_FILE_NAME: &str = "cc-sync.state.json";
pub const STATE_ENV_VAR: &str = "CC_SYNC_STATE_PATH";
const SCHEMA_VERSION: i64 = 3;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

pub fn default_config() -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "project_root": "",
        "workspaces": [],
        "overwrite_md": true,
        "fallback_to_copy": true,
        "verbose": true,
        "sources": {
            "md_files": [],
            "skills_dirs": [],
            "docs_dirs": []
        },
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
                "docs_mode": "junction"
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
                "docs_mode": "junction"
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
                "docs_mode": "junction"
            }
        },
        "sync": {
            "targets": ["codex"]
        },
        "skill_selection": {
            "common": [],
            "extra": {
                "codex": [],
                "gemini": []
            },
            "exclude": {
                "codex": [],
                "gemini": []
            }
        },
        "replacements": {
            "codex": {
                ".claude": ".codex",
                "Claude Code": "Codex",
                "CLAUDE.md": "AGENTS.md",
                "CLAUDE.local.md": "AGENTS.md",
                "claude.local.md": "AGENTS.md"
            },
            "gemini": {
                ".claude": ".gemini",
                "Claude Code": "Gemini",
                "CLAUDE.md": "GEMINI.md",
                "CLAUDE.local.md": "GEMINI.md",
                "claude.local.md": "GEMINI.md"
            }
        },
        "preferences": {
            "close_to_tray": false
        }
    })
}

pub fn load_config(config_path: &Path) -> Result<Value, String> {
    if !config_path.exists() {
        return Ok(default_config());
    }

    let raw = fs::read_to_string(config_path)
        .map_err(|err| format!("failed to read config {}: {}", config_path.display(), err))?;
    let loaded: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {}", config_path.display(), err))?;
    if !loaded.is_object() {
        return Err(format!("配置文件必须是 JSON 对象: {}", config_path.display()));
    }

    Ok(normalize_config(
        loaded,
        config_path.parent().unwrap_or_else(|| Path::new(".")),
    ))
}

pub fn normalize_config(loaded: Value, config_base: &Path) -> Value {
    let migrated = if is_legacy_config(&loaded) {
        migrate_legacy(&loaded, config_base)
    } else if is_v2_config(&loaded) {
        migrate_v2(&loaded, config_base)
    } else {
        loaded
    };

    let mut config = deep_merge(default_config(), migrated);
    if let Some(obj) = config.as_object_mut() {
        obj.remove("runtime");
    }
    config
}

pub fn project_root(config: &Value, config_path: &Path) -> PathBuf {
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    config
        .get("project_root")
        .and_then(Value::as_str)
        .and_then(|raw| resolve_path(raw, base))
        .unwrap_or_else(|| base.to_path_buf())
}

pub fn state_path_for_config(config_path: &Path) -> PathBuf {
    std::env::var_os(STATE_ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| config_path.with_file_name(STATE_FILE_NAME))
}

pub fn run_sync(
    config: &Value,
    config_path: &Path,
    scope: &str,
    dry_run: bool,
) -> Result<Value, String> {
    let mut plan = build_plan(config, config_path, scope, None)?;
    let success = plan_errors(&plan).is_empty();

    if dry_run || !success {
        set_plan_footer(&mut plan, config_path, success, false);
        return Ok(plan);
    }

    let mut exec_warnings = Vec::new();
    let operations = plan
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for operation in &operations {
        execute_operation(config, operation, &mut exec_warnings)?;
    }
    update_managed_state(config, config_path, &plan)?;

    if !exec_warnings.is_empty() {
        let warnings = plan
            .get_mut("warnings")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "plan warnings is not an array".to_string())?;
        warnings.extend(exec_warnings.into_iter().map(Value::String));
    }

    set_plan_footer(&mut plan, config_path, true, true);
    Ok(plan)
}

pub fn build_plan(
    config: &Value,
    config_path: &Path,
    scope: &str,
    target_ids: Option<Vec<String>>,
) -> Result<Value, String> {
    let root = project_root(config, config_path);
    let targets = target_ids.unwrap_or_else(|| string_array(config.pointer("/sync/targets")));
    let md_sources = resolve_path_list(config.pointer("/sources/md_files"), &root);
    let skill_source_roots = resolve_path_list(config.pointer("/sources/skills_dirs"), &root);
    let docs_source_roots = resolve_path_list(config.pointer("/sources/docs_dirs"), &root);
    let managed_state = load_managed_state(&state_path_for_config(config_path));

    let mut operations = Vec::<Value>::new();
    let mut errors = Vec::<String>::new();
    let mut warnings = Vec::<String>::new();
    let mut existing_md_sources = Vec::<PathBuf>::new();
    let mut skill_sources = BTreeMap::<String, PathBuf>::new();
    let mut docs_sources = BTreeMap::<String, PathBuf>::new();

    if matches!(scope, "all" | "md") {
        for md_source in &md_sources {
            if md_source.exists() && md_source.is_file() {
                existing_md_sources.push(md_source.clone());
            } else {
                warnings.push(format!("源 md 不存在，已跳过: {}", md_source.display()));
            }
        }
        if existing_md_sources.is_empty() {
            let reason = if md_sources.is_empty() {
                "未配置任何源 md 文件"
            } else {
                "配置的源 md 文件都不存在"
            };
            if scope == "md" {
                errors.push(reason.to_string());
            } else {
                warnings.push(format!("{}，已跳过 md 同步", reason));
            }
        }
    }

    if matches!(scope, "all" | "skills") {
        let (sources, source_warnings) = collect_named_sources(&skill_source_roots, true);
        skill_sources = sources;
        warnings.extend(source_warnings);
    }

    if matches!(scope, "all" | "docs") {
        let (sources, source_warnings) = collect_named_sources(&docs_source_roots, false);
        docs_sources = sources;
        warnings.extend(source_warnings);
    }

    for target_id in targets {
        let Some(endpoint) = config.pointer(&format!("/endpoints/{}", json_pointer_escape(&target_id))) else {
            errors.push(format!("目标端点不存在: {}", target_id));
            continue;
        };

        let skills_mode = endpoint_sync_mode(endpoint, "skills");
        let docs_mode = endpoint_sync_mode(endpoint, "docs");

        if matches!(scope, "all" | "md") {
            if let Some(target_md) = endpoint_md(endpoint, &root) {
                if !existing_md_sources.is_empty() {
                    operations.push(json!({
                        "type": "md",
                        "target": target_id,
                        "sources": path_strings(&existing_md_sources),
                        "dst": target_md.to_string_lossy().to_string(),
                        "overwrite": config.get("overwrite_md").and_then(Value::as_bool).unwrap_or(true),
                        "replacements": build_replacements(config, endpoint, &target_id)
                    }));
                }
            }
        }

        if matches!(scope, "all" | "skills") {
            if let Some(write_dir) = endpoint_write_dir(endpoint, "skills", &root) {
                let target_skills = get_skills_for_target(config, &target_id);
                for skill_name in &target_skills {
                    let Some(src) = skill_sources.get(skill_name) else {
                        let checked = skill_source_roots
                            .iter()
                            .map(|root| root.join(skill_name).to_string_lossy().to_string())
                            .collect::<Vec<_>>()
                            .join("\n");
                        errors.push(format!("未找到 skill: {}\n已检查:\n{}", skill_name, checked));
                        continue;
                    };
                    operations.push(json!({
                        "type": "skills",
                        "target": target_id,
                        "name": skill_name,
                        "src": src.to_string_lossy().to_string(),
                        "dst": write_dir.join(skill_name).to_string_lossy().to_string(),
                        "mode": skills_mode
                    }));
                }
                if has_existing_root(&skill_source_roots) {
                    operations.extend(build_remove_managed_operations(
                        &managed_state,
                        config,
                        config_path,
                        &target_id,
                        "skills",
                        &write_dir,
                        &target_skills.into_iter().collect(),
                    ));
                }
            }
        }

        if matches!(scope, "all" | "docs") {
            if let Some(write_dir) = endpoint_write_dir(endpoint, "docs", &root) {
                for (name, src) in &docs_sources {
                    operations.push(json!({
                        "type": "docs",
                        "target": target_id,
                        "name": name,
                        "src": src.to_string_lossy().to_string(),
                        "dst": write_dir.join(name).to_string_lossy().to_string(),
                        "mode": docs_mode,
                        "missing_source": false
                    }));
                }
                if has_existing_root(&docs_source_roots) {
                    operations.extend(build_remove_managed_operations(
                        &managed_state,
                        config,
                        config_path,
                        &target_id,
                        "docs",
                        &write_dir,
                        &docs_sources.keys().cloned().collect(),
                    ));
                }
            }
        }
    }

    let uses_symlink = operations.iter().any(|operation| {
        matches!(
            operation.get("type").and_then(Value::as_str),
            Some("skills" | "docs")
        ) && operation
            .get("mode")
            .and_then(Value::as_str)
            .map(|mode| mode.eq_ignore_ascii_case("symlink"))
            .unwrap_or(false)
    });
    if uses_symlink && !can_create_symlink() {
        warnings.push(
            "当前环境无法创建符号链接(Windows 需开启「开发者模式」或以管理员运行)，symlink 模式的项在同步时会改为复制。"
                .to_string(),
        );
    }

    errors.extend(validate_plan_safety(&operations));
    errors = dedupe_strings(errors);

    Ok(json!({
        "source": "inputs",
        "project_root": root.to_string_lossy().to_string(),
        "scope": scope,
        "main_md": existing_md_sources.first().map(|path| path.to_string_lossy().to_string()),
        "local_md": Value::Null,
        "md_sources": path_strings(&existing_md_sources),
        "docs_src": Value::Null,
        "docs_source_roots": path_strings(&docs_source_roots),
        "skill_source_roots": path_strings(&skill_source_roots),
        "state_path": state_path_for_config(config_path).to_string_lossy().to_string(),
        "operations": operations,
        "errors": errors,
        "warnings": warnings
    }))
}

pub fn cleanup_legacy_python_runtime(app_dir: &Path) {
    for file_name in ["sync_agents.py", "sync_config.py", "sync_from_claude.py"] {
        let path = app_dir.join(file_name);
        if path.is_file() {
            let _ = fs::remove_file(path);
        }
    }

    let python_dir = app_dir.join("python");
    let marker = if cfg!(windows) {
        python_dir.join("bin").join("python.exe")
    } else {
        python_dir.join("bin").join("python3")
    };
    if marker.exists() {
        let _ = fs::remove_dir_all(python_dir);
    }
}

fn set_plan_footer(plan: &mut Value, config_path: &Path, success: bool, executed: bool) {
    if let Some(obj) = plan.as_object_mut() {
        obj.insert(
            "config_path".to_string(),
            Value::String(config_path.to_string_lossy().to_string()),
        );
        obj.insert("success".to_string(), Value::Bool(success));
        obj.insert("executed".to_string(), Value::Bool(executed));
    }
}

fn plan_errors(plan: &Value) -> Vec<String> {
    plan.get("errors")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn deep_merge(base: Value, override_value: Value) -> Value {
    match (base, override_value) {
        (Value::Object(mut base_map), Value::Object(override_map)) => {
            for (key, value) in override_map {
                let next = match base_map.remove(&key) {
                    Some(existing) => deep_merge(existing, value),
                    None => value,
                };
                base_map.insert(key, next);
            }
            Value::Object(base_map)
        }
        (_, value) => value,
    }
}

fn is_legacy_config(config: &Value) -> bool {
    if config.get("endpoints").is_some() || config.get("sources").is_some() {
        return false;
    }
    config.get("claude_md").is_some() || config.get("targets").is_some()
}

fn is_v2_config(config: &Value) -> bool {
    config.get("endpoints").is_some() && config.get("sources").is_none()
}

fn config_project_root(config: &Value, config_base: &Path) -> PathBuf {
    config
        .get("project_root")
        .and_then(Value::as_str)
        .and_then(|raw| resolve_path(raw, config_base))
        .unwrap_or_else(|| config_base.to_path_buf())
}

fn existing_md_files(paths: Vec<String>, project_root: &Path) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in paths {
        if raw.is_empty() || seen.contains(&raw) {
            continue;
        }
        let resolved = resolve_path(&raw, project_root).unwrap_or_else(|| project_root.to_path_buf());
        if result.is_empty() || resolved.exists() {
            seen.insert(raw.clone());
            result.push(raw);
        }
    }
    result
}

fn migrate_legacy(loaded: &Value, config_base: &Path) -> Value {
    let project_root = config_project_root(loaded, config_base);
    let mut md_candidates = Vec::new();
    md_candidates.push(
        loaded
            .get("claude_md")
            .and_then(Value::as_str)
            .unwrap_or("CLAUDE.md")
            .to_string(),
    );
    md_candidates.extend(string_array(loaded.get("claude_local_md_candidates")));

    let mut endpoints = Map::new();
    endpoints.insert(
        "claude".to_string(),
        json!({
            "label": "Claude Code",
            "dir": ".claude",
            "name": "Claude Code",
            "md": loaded.get("claude_md").and_then(Value::as_str).unwrap_or("CLAUDE.md"),
            "md_local_candidates": loaded.get("claude_local_md_candidates").cloned().unwrap_or_else(|| json!([])),
            "skills_dirs": loaded.get("claude_skill_sources").cloned().unwrap_or_else(|| json!([])),
            "docs_dirs": loaded.get("claude_docs_sources").cloned().unwrap_or_else(|| json!([])),
            "mode": "junction"
        }),
    );

    let mut replacements = Map::new();
    let mut enabled_targets = Vec::new();
    if let Some(targets) = loaded.get("targets").and_then(Value::as_object) {
        for (target_id, target_cfg) in targets {
            let reps = target_cfg
                .get("text_replacements")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let (dir_token, name_token) = infer_identity(target_id, &reps);
            endpoints.insert(
                target_id.clone(),
                json!({
                    "label": capitalize(target_id),
                    "dir": dir_token,
                    "name": name_token,
                    "md": target_cfg.get("md_target").and_then(Value::as_str).unwrap_or(""),
                    "md_local_candidates": [],
                    "skills_dirs": target_cfg.get("skills_dir").and_then(Value::as_str).map(|value| json!([value])).unwrap_or_else(|| json!([])),
                    "docs_dirs": target_cfg.get("docs_dir").and_then(Value::as_str).map(|value| json!([value])).unwrap_or_else(|| json!([])),
                    "mode": target_cfg.get("mode").and_then(Value::as_str).unwrap_or("junction"),
                    "skills_mode": target_cfg.get("skills_mode").or_else(|| target_cfg.get("mode")).and_then(Value::as_str).unwrap_or("junction"),
                    "docs_mode": target_cfg.get("docs_mode").or_else(|| target_cfg.get("mode")).and_then(Value::as_str).unwrap_or("junction")
                }),
            );
            replacements.insert(target_id.clone(), Value::Object(reps));
            if target_cfg
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                enabled_targets.push(Value::String(target_id.clone()));
            }
        }
    }

    json!({
        "schema_version": SCHEMA_VERSION,
        "project_root": loaded.get("project_root").and_then(Value::as_str).unwrap_or("."),
        "workspaces": loaded.get("workspaces").cloned().unwrap_or_else(|| json!([])),
        "overwrite_md": loaded.get("overwrite_md").and_then(Value::as_bool).unwrap_or(true),
        "fallback_to_copy": loaded.get("fallback_to_copy").and_then(Value::as_bool).unwrap_or(true),
        "verbose": loaded.get("verbose").and_then(Value::as_bool).unwrap_or(true),
        "sources": {
            "md_files": existing_md_files(md_candidates, &project_root),
            "skills_dirs": loaded.get("claude_skill_sources").cloned().unwrap_or_else(|| json!([])),
            "docs_dirs": loaded.get("claude_docs_sources").cloned().unwrap_or_else(|| json!([]))
        },
        "endpoints": Value::Object(endpoints),
        "sync": { "targets": enabled_targets },
        "skill_selection": {
            "common": loaded.get("common_skills").cloned().unwrap_or_else(|| json!([])),
            "extra": loaded.get("target_extra_skills").cloned().unwrap_or_else(|| json!({})),
            "exclude": loaded.get("target_exclude_skills").cloned().unwrap_or_else(|| json!({}))
        },
        "replacements": Value::Object(replacements),
        "preferences": loaded.get("preferences").cloned().unwrap_or_else(|| json!({}))
    })
}

fn migrate_v2(loaded: &Value, config_base: &Path) -> Value {
    let project_root = config_project_root(loaded, config_base);
    let source_id = loaded
        .pointer("/sync/source")
        .and_then(Value::as_str)
        .unwrap_or("claude");
    let source_endpoint = loaded
        .get("endpoints")
        .and_then(|endpoints| endpoints.get(source_id))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut md_candidates = Vec::new();
    md_candidates.push(
        source_endpoint
            .get("md")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    );
    md_candidates.extend(string_array(source_endpoint.get("md_local_candidates")));

    json!({
        "schema_version": SCHEMA_VERSION,
        "project_root": loaded.get("project_root").and_then(Value::as_str).unwrap_or("."),
        "workspaces": loaded.get("workspaces").cloned().unwrap_or_else(|| json!([])),
        "overwrite_md": loaded.get("overwrite_md").and_then(Value::as_bool).unwrap_or(true),
        "fallback_to_copy": loaded.get("fallback_to_copy").and_then(Value::as_bool).unwrap_or(true),
        "verbose": loaded.get("verbose").and_then(Value::as_bool).unwrap_or(true),
        "sources": {
            "md_files": existing_md_files(md_candidates, &project_root),
            "skills_dirs": source_endpoint.get("skills_dirs").cloned().unwrap_or_else(|| json!([])),
            "docs_dirs": source_endpoint.get("docs_dirs").cloned().unwrap_or_else(|| json!([]))
        },
        "endpoints": loaded.get("endpoints").cloned().unwrap_or_else(|| json!({})),
        "sync": { "targets": loaded.pointer("/sync/targets").cloned().unwrap_or_else(|| json!([])) },
        "skill_selection": loaded.get("skill_selection").cloned().unwrap_or_else(|| json!({})),
        "replacements": target_replacements_from_pairs(loaded.get("replacements")),
        "preferences": loaded.get("preferences").cloned().unwrap_or_else(|| json!({}))
    })
}

fn target_replacements_from_pairs(replacements: Option<&Value>) -> Value {
    let mut result = Map::new();
    if let Some(items) = replacements.and_then(Value::as_object) {
        for (key, value) in items {
            if let Some((_, target)) = key.split_once("->") {
                result.insert(target.to_string(), value.clone());
            } else {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(result)
}

fn infer_identity(target_id: &str, replacements: &Map<String, Value>) -> (String, String) {
    let mut dir_token = None;
    let mut name_token = None;
    for (old, new) in replacements {
        if old == ".claude" {
            dir_token = new.as_str().map(String::from);
        } else if old == "Claude Code" {
            name_token = new.as_str().map(String::from);
        }
    }
    (
        dir_token.unwrap_or_else(|| format!(".{}", target_id)),
        name_token.unwrap_or_else(|| capitalize(target_id)),
    )
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn path_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

fn resolve_path(raw: &str, base: &Path) -> Option<PathBuf> {
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(base.join(path))
    }
}

fn resolve_path_list(value: Option<&Value>, base: &Path) -> Vec<PathBuf> {
    string_array(value)
        .into_iter()
        .filter_map(|raw| resolve_path(&raw, base))
        .collect()
}

fn endpoint_md(endpoint: &Value, root: &Path) -> Option<PathBuf> {
    endpoint.get("md").and_then(Value::as_str).and_then(|raw| resolve_path(raw, root))
}

fn endpoint_write_dir(endpoint: &Value, kind: &str, root: &Path) -> Option<PathBuf> {
    endpoint
        .get(format!("{}_dirs", kind))
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .and_then(|raw| resolve_path(raw, root))
}

fn endpoint_sync_mode(endpoint: &Value, kind: &str) -> String {
    let legacy = endpoint
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("junction");
    endpoint
        .get(format!("{}_mode", kind))
        .and_then(Value::as_str)
        .unwrap_or(legacy)
        .to_string()
}

fn collect_named_sources(
    roots: &[PathBuf],
    require_skill_md: bool,
) -> (BTreeMap<String, PathBuf>, Vec<String>) {
    let mut entries = BTreeMap::<String, PathBuf>::new();
    let mut warnings = Vec::new();

    for root in roots {
        if !root.exists() || !root.is_dir() {
            warnings.push(format!("源目录不存在，已跳过: {}", root.display()));
            continue;
        }

        let mut children = match fs::read_dir(root) {
            Ok(items) => items.filter_map(Result::ok).collect::<Vec<_>>(),
            Err(_) => {
                warnings.push(format!("源目录不存在，已跳过: {}", root.display()));
                continue;
            }
        };
        children.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

        for child in children {
            let path = child.path();
            let name = child.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if require_skill_md {
                if !path.is_dir() || !path.join("SKILL.md").is_file() {
                    continue;
                }
            } else if !(path.is_dir() || path.is_file()) {
                continue;
            }

            if let Some(existing) = entries.get(&name) {
                warnings.push(format!(
                    "同名源已忽略: {} ({}, 已使用 {})",
                    name,
                    path.display(),
                    existing.display()
                ));
                continue;
            }
            entries.insert(name, path);
        }
    }

    (entries, warnings)
}

fn get_skills_for_target(config: &Value, target_id: &str) -> Vec<String> {
    let selection = config.get("skill_selection").unwrap_or(&Value::Null);
    let mut skills = string_array(selection.get("common"));
    skills.extend(string_array(
        selection
            .get("extra")
            .and_then(|extra| extra.get(target_id)),
    ));
    let exclude = string_array(
        selection
            .get("exclude")
            .and_then(|exclude| exclude.get(target_id)),
    )
    .into_iter()
    .collect::<BTreeSet<_>>();

    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for skill in skills {
        if exclude.contains(&skill) || seen.contains(&skill) {
            continue;
        }
        seen.insert(skill.clone());
        result.push(skill);
    }
    result
}

fn build_replacements(config: &Value, endpoint: &Value, target_id: &str) -> Vec<Vec<String>> {
    let replacements = config.get("replacements").unwrap_or(&Value::Null);
    let legacy_source = config
        .pointer("/sync/source")
        .and_then(Value::as_str)
        .unwrap_or("claude");
    let stored = replacements
        .get(target_id)
        .or_else(|| replacements.get(format!("{}->{}", legacy_source, target_id)));

    let mut pairs = Vec::<(String, String)>::new();
    if let Some(items) = stored.and_then(Value::as_object) {
        for (old, new) in items {
            if let Some(new) = new.as_str() {
                pairs.push((old.clone(), new.to_string()));
            }
        }
    } else {
        let name = endpoint.get("name").and_then(Value::as_str).unwrap_or("");
        let dir = endpoint.get("dir").and_then(Value::as_str).unwrap_or("");
        let md = endpoint.get("md").and_then(Value::as_str).unwrap_or("");
        pairs.extend([
            ("Claude Code".to_string(), name.to_string()),
            (".claude".to_string(), dir.to_string()),
            ("CLAUDE.md".to_string(), md.to_string()),
            ("CLAUDE.local.md".to_string(), md.to_string()),
            ("claude.local.md".to_string(), md.to_string()),
        ]);
    }

    pairs.retain(|(old, new)| !old.is_empty() && !new.is_empty() && old != new);
    pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    pairs.into_iter().map(|(old, new)| vec![old, new]).collect()
}

fn has_existing_root(roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| root.exists() && root.is_dir())
}

fn build_remove_managed_operations(
    state: &Value,
    config: &Value,
    config_path: &Path,
    target_id: &str,
    kind: &str,
    write_dir: &Path,
    desired_names: &BTreeSet<String>,
) -> Vec<Value> {
    let mut operations = Vec::new();
    let workspace_key = project_root(config, config_path).to_string_lossy().to_string();
    let Some(items) = state
        .pointer(&format!(
            "/workspaces/{}/targets/{}/{}",
            json_pointer_escape(&workspace_key),
            json_pointer_escape(target_id),
            json_pointer_escape(kind)
        ))
        .and_then(Value::as_object)
    else {
        return operations;
    };

    for (name, entry) in items {
        if desired_names.contains(name) || !entry.is_object() {
            continue;
        }
        let dst = entry
            .get("dst")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| write_dir.join(name));
        if normalized_path(dst.parent().unwrap_or_else(|| Path::new("")))
            != normalized_path(write_dir)
        {
            continue;
        }
        operations.push(json!({
            "type": "remove_managed",
            "target": target_id,
            "kind": kind,
            "name": name,
            "dst": dst.to_string_lossy().to_string(),
            "base": write_dir.to_string_lossy().to_string(),
            "expected_src": entry.get("src").cloned().unwrap_or(Value::Null),
            "expected_mode": entry.get("mode").cloned().unwrap_or(Value::Null),
            "expected_fingerprint": entry.get("fingerprint").cloned().unwrap_or(Value::Null)
        }));
    }

    operations
}

fn json_pointer_escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            result.push(value);
        }
    }
    result
}

fn normalized_path(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut cleaned = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                cleaned.pop();
            }
            _ => cleaned.push(component.as_os_str()),
        }
    }
    let mut value = cleaned.to_string_lossy().replace('\\', "/");
    while value.ends_with('/') && value.len() > 1 {
        value.pop();
    }
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

fn resolved_normalized_path(path: &Path) -> String {
    fs::canonicalize(path)
        .map(|path| normalized_path(&path))
        .unwrap_or_else(|_| normalized_path(path))
}

fn is_path_within(child: &Path, parent: &Path) -> bool {
    let child_norm = normalized_path(child);
    let parent_norm = normalized_path(parent);
    if child_norm == parent_norm {
        return true;
    }
    let prefix = if parent_norm.ends_with('/') {
        parent_norm
    } else {
        parent_norm + "/"
    };
    child_norm.starts_with(&prefix)
}

fn validate_src_dst_safety(
    src: &Path,
    dst: &Path,
    label: &str,
    resolve_paths: bool,
) -> Vec<String> {
    if normalized_path(src) == normalized_path(dst) {
        return vec![format!("{} 源和目标相同，已阻止: {} -> {}", label, src.display(), dst.display())];
    }

    let mut errors = Vec::new();
    if is_path_within(dst, src) {
        errors.push(format!("{} 目标位于源路径内部，已阻止: {} -> {}", label, src.display(), dst.display()));
    }
    if is_path_within(src, dst) {
        errors.push(format!("{} 源位于目标路径内部，已阻止: {} -> {}", label, src.display(), dst.display()));
    }

    if resolve_paths {
        let src_resolved = PathBuf::from(resolved_normalized_path(src));
        let dst_resolved = PathBuf::from(resolved_normalized_path(dst));
        if normalized_path(&src_resolved) == normalized_path(&dst_resolved) {
            errors.push(format!("{} 源和目标解析后相同，已阻止: {} -> {}", label, src.display(), dst.display()));
            return errors;
        }
        if is_path_within(&dst_resolved, &src_resolved) {
            errors.push(format!("{} 目标解析后位于源路径内部，已阻止: {} -> {}", label, src.display(), dst.display()));
        }
        if is_path_within(&src_resolved, &dst_resolved) {
            errors.push(format!("{} 源解析后位于目标路径内部，已阻止: {} -> {}", label, src.display(), dst.display()));
        }
    }

    errors
}

fn validate_plan_safety(operations: &[Value]) -> Vec<String> {
    let mut errors = Vec::new();
    for operation in operations {
        let op_type = operation.get("type").and_then(Value::as_str).unwrap_or("");
        let target = operation.get("target").and_then(Value::as_str).unwrap_or("");
        let name = operation.get("name").and_then(Value::as_str).unwrap_or("");
        let label = format!("{}:{}:{}", op_type, target, name);

        match op_type {
            "md" => {
                let dst = PathBuf::from(operation.get("dst").and_then(Value::as_str).unwrap_or(""));
                for src in string_array(operation.get("sources")) {
                    errors.extend(validate_src_dst_safety(&PathBuf::from(src), &dst, &label, true));
                }
            }
            "skills" | "docs" => {
                if let Some(src) = operation.get("src").and_then(Value::as_str) {
                    let src = PathBuf::from(src);
                    let dst = PathBuf::from(operation.get("dst").and_then(Value::as_str).unwrap_or(""));
                    let resolve_paths = !(is_reparse_point(&dst)
                        || (op_type == "docs"
                            && dst.parent().map(is_reparse_point).unwrap_or(false)));
                    errors.extend(validate_src_dst_safety(&src, &dst, &label, resolve_paths));
                }
            }
            "remove_managed" => {
                let base = PathBuf::from(operation.get("base").and_then(Value::as_str).unwrap_or(""));
                let dst = PathBuf::from(operation.get("dst").and_then(Value::as_str).unwrap_or(""));
                if normalized_path(&base) == normalized_path(&dst) {
                    errors.push(format!("{} 清理目标不能是目录本身，已阻止: {}", label, dst.display()));
                } else if normalized_path(dst.parent().unwrap_or_else(|| Path::new("")))
                    != normalized_path(&base)
                {
                    errors.push(format!("{} 清理目标必须是目标目录的直接子项，已阻止: {}", label, dst.display()));
                } else if !is_path_within(&dst, &base) {
                    errors.push(format!("{} 清理目标不在目标目录内，已阻止: {}", label, dst.display()));
                }
            }
            _ => {}
        }
    }
    dedupe_strings(errors)
}

fn is_reparse_point(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return path.is_symlink();
    };
    #[cfg(windows)]
    {
        meta.file_type().is_symlink() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

fn can_create_symlink() -> bool {
    let base = std::env::temp_dir();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    let target = base.join(format!("cc-sync-symcheck-target-{}-{}.tmp", std::process::id(), stamp));
    let link = base.join(format!("cc-sync-symcheck-link-{}-{}.tmp", std::process::id(), stamp));
    if fs::write(&target, "x").is_err() {
        return true;
    }
    let ok = create_file_symlink(&target, &link).is_ok();
    let _ = remove_path(&link);
    let _ = fs::remove_file(&target);
    ok
}

fn execute_operation(config: &Value, operation: &Value, exec_warnings: &mut Vec<String>) -> Result<(), String> {
    match operation.get("type").and_then(Value::as_str).unwrap_or("") {
        "md" => {
            let sources = string_array(operation.get("sources"))
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            let dst = PathBuf::from(operation.get("dst").and_then(Value::as_str).unwrap_or(""));
            let replacements = operation
                .get("replacements")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            merge_md_files(
                &sources,
                &dst,
                &replacements,
                operation.get("overwrite").and_then(Value::as_bool).unwrap_or(true),
            )
        }
        "remove_managed" => {
            let dst = PathBuf::from(operation.get("dst").and_then(Value::as_str).unwrap_or(""));
            if !dst.exists() && !dst.is_symlink() {
                return Ok(());
            }
            let safety_errors = validate_plan_safety(&[operation.clone()]);
            if !safety_errors.is_empty() {
                return Err(safety_errors.join("\n"));
            }
            let expected = operation.get("expected_fingerprint").unwrap_or(&Value::Null);
            if expected.is_null() {
                return Ok(());
            }
            let current = fingerprint_path(&dst)?;
            if &current != expected {
                return Ok(());
            }
            remove_path(&dst)
        }
        "skills" | "docs" => {
            let op_type = operation.get("type").and_then(Value::as_str).unwrap_or("");
            let Some(src) = operation.get("src").and_then(Value::as_str) else {
                return Err(format!("{} 源目录不存在", op_type));
            };
            let src = PathBuf::from(src);
            let dst = PathBuf::from(operation.get("dst").and_then(Value::as_str).unwrap_or(""));
            if op_type == "docs" {
                ensure_real_item_parent(&dst)?;
            }
            let label = format!(
                "{}:{}:{}",
                op_type,
                operation.get("target").and_then(Value::as_str).unwrap_or(""),
                operation.get("name").and_then(Value::as_str).unwrap_or("")
            );
            let safety_errors =
                validate_src_dst_safety(&src, &dst, &label, !is_reparse_point(&dst));
            if !safety_errors.is_empty() {
                return Err(safety_errors.join("\n"));
            }
            link_or_copy_path(
                config,
                &src,
                &dst,
                operation.get("mode").and_then(Value::as_str).unwrap_or("junction"),
                exec_warnings,
            )
        }
        other => Err(format!("未知 operation type: {}", other)),
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("源文件不存在: {}", path.display()));
    }
    fs::read_to_string(path).map_err(|err| format!("failed to read {}: {}", path.display(), err))
}

fn write_text(path: &Path, content: &str, overwrite: bool) -> Result<(), String> {
    if path.exists() || path.is_symlink() {
        if !overwrite {
            return Ok(());
        }
        remove_path(path)?;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create dir {}: {}", parent.display(), err))?;
    }
    fs::write(path, content).map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

fn merge_md_files(
    sources: &[PathBuf],
    dst: &Path,
    replacements: &[Value],
    overwrite: bool,
) -> Result<(), String> {
    let mut parts = Vec::new();
    for src in sources {
        let text = read_text(src)?;
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    let mut content = parts.join("\n\n---\n\n");
    content.push('\n');
    for pair in replacements {
        let Some(items) = pair.as_array() else {
            continue;
        };
        let Some(old) = items.first().and_then(Value::as_str) else {
            continue;
        };
        let Some(new) = items.get(1).and_then(Value::as_str) else {
            continue;
        };
        content = content.replace(old, new);
    }
    write_text(dst, &content, overwrite)
}

fn ensure_real_item_parent(dst: &Path) -> Result<(), String> {
    let Some(parent) = dst.parent() else {
        return Ok(());
    };
    if is_reparse_point(parent) {
        remove_path(parent)?;
    }
    fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create dir {}: {}", parent.display(), err))
}

fn link_or_copy_path(
    config: &Value,
    src: &Path,
    dst: &Path,
    mode: &str,
    exec_warnings: &mut Vec<String>,
) -> Result<(), String> {
    if src.is_dir() {
        return link_or_copy_dir(config, src, dst, mode, exec_warnings);
    }
    if !src.exists() {
        return Err(format!("源路径不存在: {}", src.display()));
    }
    if mode.eq_ignore_ascii_case("symlink") {
        match create_file_symlink(src, dst) {
            Ok(()) => return Ok(()),
            Err(err) => {
                if !config.get("fallback_to_copy").and_then(Value::as_bool).unwrap_or(true) {
                    return Err(err);
                }
                exec_warnings.push(fallback_note("symlink", dst, &err));
            }
        }
    }
    copy_file(src, dst)
}

fn link_or_copy_dir(
    config: &Value,
    src: &Path,
    dst: &Path,
    mode: &str,
    exec_warnings: &mut Vec<String>,
) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("源目录不存在: {}", src.display()));
    }
    let normalized = mode.to_lowercase();
    let result = match normalized.as_str() {
        "junction" => create_junction(src, dst),
        "symlink" => create_dir_symlink(src, dst),
        "copy" => copy_dir(src, dst),
        _ => Err(format!("未知 mode: {}", mode)),
    };
    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            if !config.get("fallback_to_copy").and_then(Value::as_bool).unwrap_or(true)
                || normalized == "copy"
            {
                return Err(err);
            }
            exec_warnings.push(fallback_note(&normalized, dst, &err));
            copy_dir(src, dst)
        }
    }
}

fn fallback_note(mode: &str, dst: &Path, err: &str) -> String {
    let hint = if mode == "symlink" && err.contains("1314") {
        "(Windows 需开启「开发者模式」或以管理员运行才能创建符号链接)".to_string()
    } else {
        format!("(原因: {})", err)
    };
    format!("{} 不可用，已改为复制{}: {}", mode, hint, dst.display())
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("源文件不存在: {}", src.display()));
    }
    remove_path(dst)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create dir {}: {}", parent.display(), err))?;
    }
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|err| format!("failed to copy {} -> {}: {}", src.display(), dst.display(), err))
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("源目录不存在: {}", src.display()));
    }
    remove_path(dst)?;
    copy_dir_recursive(src, dst)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst)
        .map_err(|err| format!("failed to create dir {}: {}", dst.display(), err))?;
    for entry in fs::read_dir(src).map_err(|err| format!("failed to read {}: {}", src.display(), err))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let child_src = entry.path();
        let child_dst = dst.join(entry.file_name());
        let meta = fs::symlink_metadata(&child_src).map_err(|err| err.to_string())?;
        if meta.file_type().is_symlink() {
            if child_src.is_dir() {
                copy_dir_recursive(&child_src, &child_dst)?;
            } else {
                copy_file(&child_src, &child_dst)?;
            }
        } else if meta.is_dir() {
            copy_dir_recursive(&child_src, &child_dst)?;
        } else if meta.is_file() {
            copy_file(&child_src, &child_dst)?;
        }
    }
    Ok(())
}

fn create_file_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    remove_path(dst)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create dir {}: {}", parent.display(), err))?;
    }
    #[cfg(windows)]
    {
        symlink_file(src, dst).map_err(|err| err.to_string())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst).map_err(|err| err.to_string())
    }
}

fn create_dir_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    remove_path(dst)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create dir {}: {}", parent.display(), err))?;
    }
    #[cfg(windows)]
    {
        symlink_dir(src, dst).map_err(|err| err.to_string())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst).map_err(|err| err.to_string())
    }
}

#[cfg(windows)]
fn create_junction(src: &Path, dst: &Path) -> Result<(), String> {
    remove_path(dst)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create dir {}: {}", parent.display(), err))?;
    }
    let output = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(dst)
        .arg(src)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if err.is_empty() { out } else { err })
    }
}

#[cfg(not(windows))]
fn create_junction(_src: &Path, _dst: &Path) -> Result<(), String> {
    Err("junction 仅支持 Windows".to_string())
}

fn remove_path(path: &Path) -> Result<(), String> {
    if !path.exists() && !path.is_symlink() {
        return Ok(());
    }

    let meta = fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    if meta.file_type().is_symlink() {
        #[cfg(windows)]
        {
            if path.is_dir() {
                return remove_dir_reparse(path);
            }
        }
        return fs::remove_file(path).map_err(|err| err.to_string());
    }

    #[cfg(windows)]
    {
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return remove_dir_reparse(path);
        }
    }

    if meta.is_file() {
        return fs::remove_file(path).map_err(|err| err.to_string());
    }
    if meta.is_dir() {
        match fs::remove_dir(path) {
            Ok(()) => return Ok(()),
            Err(_) => {}
        }
        return fs::remove_dir_all(path).map_err(|err| err.to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn remove_dir_reparse(path: &Path) -> Result<(), String> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let status = Command::new("cmd")
                .args(["/c", "rmdir"])
                .arg(path)
                .status()
                .map_err(|err| err.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("failed to remove reparse point {}", path.display()))
            }
        }
    }
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|err| format!("failed to open {}: {}", path.display(), err))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|err| err.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn fingerprint_path(path: &Path) -> Result<Value, String> {
    if !path.exists() && !path.is_symlink() {
        return Ok(json!({"kind": "missing"}));
    }

    if is_reparse_point(path) {
        let target = fs::canonicalize(path)
            .or_else(|_| fs::read_link(path))
            .ok()
            .map(|target| normalized_path(&target));
        return Ok(json!({"kind": "link", "target": target}));
    }

    let meta = fs::metadata(path).map_err(|err| err.to_string())?;
    if meta.is_file() {
        return Ok(json!({
            "kind": "file",
            "size": meta.len(),
            "sha256": hash_file(path)?
        }));
    }
    if meta.is_dir() {
        let mut files = Vec::new();
        collect_dir_entries(path, path, &mut files)?;
        files.sort_by_key(|(relative, _)| relative.to_lowercase());

        let mut digest = Sha256::new();
        let mut file_count = 0usize;
        for (relative, child) in files {
            digest.update(relative.as_bytes());
            digest.update(b"\0");
            if is_reparse_point(&child) {
                let target = fs::canonicalize(&child)
                    .or_else(|_| fs::read_link(&child))
                    .ok()
                    .map(|target| normalized_path(&target))
                    .unwrap_or_default();
                digest.update(format!("link:{}", target).as_bytes());
            } else if child.is_file() {
                digest.update(hash_file(&child)?.as_bytes());
            }
            file_count += 1;
        }
        return Ok(json!({
            "kind": "dir",
            "files": file_count,
            "sha256": format!("{:x}", digest.finalize())
        }));
    }
    Ok(json!({"kind": "other"}))
}

fn collect_dir_entries(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|err| format!("failed to read {}: {}", dir.display(), err))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|err| err.to_string())?;
        if meta.is_dir() && !meta.file_type().is_symlink() {
            collect_dir_entries(root, &path, files)?;
            continue;
        }
        if meta.is_file() || meta.file_type().is_symlink() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, path));
        }
    }
    Ok(())
}

fn load_managed_state(path: &Path) -> Value {
    if !path.exists() {
        return json!({"schema_version": 1, "workspaces": {}});
    }
    let Ok(raw) = fs::read_to_string(path) else {
        return json!({"schema_version": 1, "workspaces": {}});
    };
    let Ok(mut state) = serde_json::from_str::<Value>(&raw) else {
        return json!({"schema_version": 1, "workspaces": {}});
    };
    if !state.is_object() {
        return json!({"schema_version": 1, "workspaces": {}});
    }
    let obj = state.as_object_mut().unwrap();
    obj.entry("schema_version").or_insert(json!(1));
    obj.entry("workspaces").or_insert_with(|| json!({}));
    state
}

fn save_managed_state(path: &Path, state: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create state dir {}: {}", parent.display(), err))?;
    }
    let temp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|value| value.to_str()).unwrap_or("")
    ));
    let payload = serde_json::to_string_pretty(state).map_err(|err| err.to_string())?;
    fs::write(&temp_path, payload + "\n")
        .map_err(|err| format!("failed to write {}: {}", temp_path.display(), err))?;
    fs::rename(&temp_path, path).map_err(|err| {
        let _ = fs::remove_file(&temp_path);
        format!("failed to replace {}: {}", path.display(), err)
    })
}

fn update_managed_state(config: &Value, config_path: &Path, plan: &Value) -> Result<(), String> {
    let mut touched = BTreeMap::<String, BTreeSet<String>>::new();
    let mut next_items = BTreeMap::<String, BTreeMap<String, Map<String, Value>>>::new();

    for operation in plan.get("operations").and_then(Value::as_array).into_iter().flatten() {
        match operation.get("type").and_then(Value::as_str).unwrap_or("") {
            "skills" | "docs" => {
                let target = operation.get("target").and_then(Value::as_str).unwrap_or("").to_string();
                let kind = operation.get("type").and_then(Value::as_str).unwrap_or("").to_string();
                touched.entry(target.clone()).or_default().insert(kind.clone());
                if let Some(entry) = managed_entry_for_operation(operation)? {
                    if let Some(name) = operation.get("name").and_then(Value::as_str) {
                        next_items
                            .entry(target)
                            .or_default()
                            .entry(kind)
                            .or_default()
                            .insert(name.to_string(), entry);
                    }
                }
            }
            "remove_managed" => {
                let target = operation.get("target").and_then(Value::as_str).unwrap_or("").to_string();
                let kind = operation.get("kind").and_then(Value::as_str).unwrap_or("").to_string();
                touched.entry(target).or_default().insert(kind);
            }
            _ => {}
        }
    }

    if touched.is_empty() {
        return Ok(());
    }

    let state_path = state_path_for_config(config_path);
    let mut state = load_managed_state(&state_path);
    let workspace_key = project_root(config, config_path).to_string_lossy().to_string();
    let state_obj = state.as_object_mut().unwrap();
    let workspaces = state_obj
        .entry("workspaces")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "state.workspaces is not an object".to_string())?;
    let workspace = workspaces
        .entry(workspace_key)
        .or_insert_with(|| json!({"targets": {}}))
        .as_object_mut()
        .ok_or_else(|| "state workspace is not an object".to_string())?;
    let targets = workspace
        .entry("targets")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "state targets is not an object".to_string())?;

    for (target_id, kinds) in touched {
        let target = targets
            .entry(target_id.clone())
            .or_insert_with(|| json!({"skills": {}, "docs": {}}))
            .as_object_mut()
            .ok_or_else(|| "state target is not an object".to_string())?;
        for kind in kinds {
            let items = next_items
                .get(&target_id)
                .and_then(|by_kind| by_kind.get(&kind))
                .cloned()
                .unwrap_or_default();
            target.insert(kind, Value::Object(items));
        }
    }

    save_managed_state(&state_path, &state)
}

fn managed_entry_for_operation(operation: &Value) -> Result<Option<Value>, String> {
    let Some(src) = operation.get("src").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(dst) = operation.get("dst").and_then(Value::as_str) else {
        return Ok(None);
    };
    let dst_path = PathBuf::from(dst);
    if !dst_path.exists() && !dst_path.is_symlink() {
        return Ok(None);
    }
    Ok(Some(json!({
        "src": src,
        "dst": dst,
        "mode": operation.get("mode").cloned().unwrap_or(Value::Null),
        "fingerprint": fingerprint_path(&dst_path)?
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drops_legacy_runtime() {
        let config = normalize_config(
            json!({
                "schema_version": 3,
                "sources": {},
                "runtime": {"python_executable": "python/bin/python.exe"}
            }),
            Path::new("."),
        );
        assert!(config.get("runtime").is_none());
    }

    #[test]
    fn replacements_sort_longest_old_first() {
        let config = default_config();
        let endpoint = config.pointer("/endpoints/codex").unwrap();
        let replacements = build_replacements(&config, endpoint, "codex");
        let first = replacements.first().unwrap().first().unwrap();
        assert_eq!(first, "CLAUDE.local.md");
    }

    #[test]
    fn run_sync_copies_md_skills_docs_and_updates_state() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let root = std::env::temp_dir().join(format!(
            "cc-sync-rust-engine-test-{}-{}",
            std::process::id(),
            stamp
        ));
        let source = root.join("source");
        let skill = source.join("skills").join("sample-skill");
        let docs = source.join("docs");
        fs::create_dir_all(&skill).unwrap();
        fs::create_dir_all(&docs).unwrap();
        fs::write(source.join("BASE.md"), "Use Claude Code with CLAUDE.md.").unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: Sample\n---\n# Sample\n").unwrap();
        fs::write(docs.join("guide.md"), "guide").unwrap();

        let config_path = root.join("cc-sync.config.json");
        let config = json!({
            "schema_version": 3,
            "project_root": root.to_string_lossy(),
            "overwrite_md": true,
            "fallback_to_copy": true,
            "verbose": true,
            "sources": {
                "md_files": ["source/BASE.md"],
                "skills_dirs": ["source/skills"],
                "docs_dirs": ["source/docs"]
            },
            "endpoints": {
                "codex": {
                    "label": "Codex",
                    "dir": ".codex",
                    "name": "Codex",
                    "md": "AGENTS.md",
                    "md_local_candidates": [],
                    "skills_dirs": [".codex/skills"],
                    "docs_dirs": [".codex/docs"],
                    "mode": "copy",
                    "skills_mode": "copy",
                    "docs_mode": "copy"
                }
            },
            "sync": {"targets": ["codex"]},
            "skill_selection": {
                "common": ["sample-skill"],
                "extra": {"codex": []},
                "exclude": {"codex": []}
            },
            "replacements": {
                "codex": {
                    "Claude Code": "Codex",
                    "CLAUDE.md": "AGENTS.md"
                }
            },
            "preferences": {"close_to_tray": false}
        });

        let preview = run_sync(&config, &config_path, "all", true).unwrap();
        assert_eq!(preview.get("success").and_then(Value::as_bool), Some(true));
        assert_eq!(
            preview
                .get("operations")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(3)
        );

        let executed = run_sync(&config, &config_path, "all", false).unwrap();
        assert_eq!(executed.get("executed").and_then(Value::as_bool), Some(true));
        assert_eq!(
            fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            "Use Codex with AGENTS.md.\n"
        );
        assert!(root.join(".codex/skills/sample-skill/SKILL.md").is_file());
        assert!(root.join(".codex/docs/guide.md").is_file());
        assert!(root.join("cc-sync.state.json").is_file());

        let _ = fs::remove_dir_all(root);
    }
}
