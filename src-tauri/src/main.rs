#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

fn parse_semver(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().trim_start_matches(['v', 'V']);
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    // patch may carry a pre-release suffix (e.g. "3-rc1") — take leading digits only.
    let patch_raw = parts.next().unwrap_or("0");
    let patch_digits: String = patch_raw.chars().take_while(|c| c.is_ascii_digit()).collect();
    let patch = patch_digits.parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const TRAY_MENU_SHOW: &str = "show";
const TRAY_MENU_QUIT: &str = "quit";
const STATE_FILE_NAME: &str = "cc-sync.state.json";
const STATE_ENV_VAR: &str = "CC_SYNC_STATE_PATH";

struct TrayState {
    icon: Mutex<Option<TrayIcon>>,
}

#[derive(Serialize)]
struct SkillOption {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    paths: Vec<String>,
}

#[derive(Default)]
struct SkillMetadata {
    display_name: Option<String>,
    description: Option<String>,
}

#[derive(Default)]
struct SkillOptionBuilder {
    display_name: Option<String>,
    description: Option<String>,
    paths: Vec<String>,
}

/* target skills 目录里某个技能的真实磁盘状态。 */
#[derive(Serialize)]
struct TargetSkill {
    name: String,
    kind: String, // "link" = junction/符号链接且目标存在; "copy" = 真实文件夹; "broken" = 失效链接
}

#[derive(Serialize)]
struct UpdateInfo {
    current: String,
    latest: Option<String>,
    has_update: bool,
    download_url: Option<String>,
    release_url: Option<String>,
    notes: Option<String>,
}

fn parse_release(json: &Value, current: &str) -> UpdateInfo {
    let tag = json.get("tag_name").and_then(|v| v.as_str());
    let release_url = json
        .get("html_url")
        .and_then(|v| v.as_str())
        .map(String::from);
    let notes = json.get("body").and_then(|v| v.as_str()).map(String::from);
    let download_url = json
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|assets| {
            assets.iter().find_map(|asset| {
                let name = asset.get("name").and_then(|n| n.as_str())?;
                if name.to_lowercase().ends_with(".zip") {
                    asset
                        .get("browser_download_url")
                        .and_then(|u| u.as_str())
                        .map(String::from)
                } else {
                    None
                }
            })
        });

    let has_update =
        tag.map(|t| is_newer(t, current)).unwrap_or(false) && download_url.is_some();

    UpdateInfo {
        current: current.to_string(),
        latest: tag.map(String::from),
        has_update,
        download_url,
        release_url,
        notes,
    }
}

fn source_project_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .map(|path| path.join("../../.."))
        .and_then(|path| path.canonicalize().ok())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(debug_assertions)]
fn default_config_path() -> PathBuf {
    let portable_config = app_dir().join("cc-sync.config.json");
    let source_config = source_project_root(&portable_config).join("cc-sync.config.json");
    if source_config.exists() {
        return source_config;
    }

    let cwd_config = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("cc-sync.config.json");
    if cwd_config.exists() {
        return cwd_config;
    }

    source_config
}

#[cfg(not(debug_assertions))]
fn default_config_path() -> PathBuf {
    let portable_config = app_dir().join("cc-sync.config.json");
    if portable_config.exists() {
        return portable_config;
    }

    let cwd_config = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("cc-sync.config.json");
    if cwd_config.exists() {
        return cwd_config;
    }

    portable_config
}

fn resolve_project_root(config: &Value, config_path: &Path) -> PathBuf {
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    config
        .get("project_root")
        .and_then(|value| value.as_str())
        .map(|raw| resolve_runtime_path(base, raw))
        .unwrap_or_else(|| source_project_root(config_path))
}

fn load_config_json(config_path: Option<String>) -> Result<(Value, PathBuf, PathBuf), String> {
    let config_path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let raw = fs::read_to_string(&config_path)
        .map_err(|err| format!("failed to read config {}: {}", config_path.display(), err))?;

    let value: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {}", config_path.display(), err))?;

    let project_root = resolve_project_root(&value, &config_path);
    Ok((value, config_path, project_root))
}

fn runtime_config_context(
    config_path: Option<String>,
    config_override: Option<Value>,
) -> Result<(Value, PathBuf, PathBuf, bool), String> {
    if let Some(config) = config_override {
        let resolved_config_path = config_path
            .map(PathBuf::from)
            .unwrap_or_else(default_config_path);
        let project_root = resolve_project_root(&config, &resolved_config_path);
        return Ok((config, resolved_config_path, project_root, true));
    }

    let (config, resolved_config_path, project_root) = load_config_json(config_path)?;
    Ok((config, resolved_config_path, project_root, false))
}

fn resolve_runtime_path(project_root: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        candidate
    } else {
        project_root.join(candidate)
    }
}

fn resolve_runtime_file(runtime_base: &Path, config_path: &Path, raw: &str) -> PathBuf {
    let primary = resolve_runtime_path(runtime_base, raw);
    if primary.exists() {
        return primary;
    }

    let source_candidate = resolve_runtime_path(&source_project_root(config_path), raw);
    if source_candidate.exists() {
        return source_candidate;
    }

    primary
}

fn resolve_python_executable(runtime_base: &Path, config_path: &Path, raw: &str) -> PathBuf {
    let primary = resolve_runtime_path(runtime_base, raw);
    if primary.exists() {
        return primary;
    }

    let source_candidate = resolve_runtime_path(&source_project_root(config_path), raw);
    if source_candidate.exists() {
        return source_candidate;
    }

    if raw.replace('\\', "/").ends_with("python/bin/python.exe") {
        return PathBuf::from("python");
    }

    primary
}

fn clean_frontmatter_value(raw: &str) -> Option<String> {
    let value = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_skill_metadata(skill_md: &Path) -> Option<SkillMetadata> {
    let content = fs::read_to_string(skill_md).ok()?;
    let mut metadata = SkillMetadata::default();
    let mut lines = content.lines();

    if matches!(lines.next().map(str::trim), Some("---")) {
        for line in lines.by_ref() {
            let trimmed = line.trim();
            if trimmed == "---" {
                break;
            }
            let Some((key, value)) = trimmed.split_once(':') else {
                continue;
            };
            match key.trim() {
                "name" => metadata.display_name = clean_frontmatter_value(value),
                "description" => metadata.description = clean_frontmatter_value(value),
                _ => {}
            }
        }
    }

    if metadata.display_name.is_none() {
        metadata.display_name = content.lines().find_map(|line| {
            line.trim()
                .strip_prefix("# ")
                .and_then(clean_frontmatter_value)
        });
    }

    Some(metadata)
}

/* 打开目录，或在资源管理器中定位文件。 */
fn open_path_in_explorer(raw_path: &str) -> Result<(), String> {
    let path = PathBuf::from(raw_path);
    let target = if path.exists() {
        path
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("path does not exist: {}", raw_path))?
    };

    let mut command = Command::new("explorer.exe");
    if target.is_file() {
        command.arg("/select,").arg(&target);
    } else {
        command.arg(&target);
    }

    let status = command
        .status()
        .map_err(|err| format!("failed to open {}: {}", target.display(), err))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "explorer returned non-zero status for {}",
            target.display()
        ))
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, TRAY_MENU_SHOW, "打开 CC Sync", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let app_handle = app.handle().clone();
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("CC Sync")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW => show_main_window(app),
            TRAY_MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(&app_handle);
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    let icon = builder.build(app)?;
    app.state::<TrayState>().icon.lock().unwrap().replace(icon);
    Ok(())
}

/* 某 target 端点的 skill 写入目录。v2 读 endpoints[target].skills_dirs[0]；旧版回退 targets[target].skills_dir。 */
fn target_skills_dir(config: &Value, target: &str) -> Option<String> {
    if let Some(endpoints) = config.get("endpoints").and_then(|v| v.as_object()) {
        return endpoints
            .get(target)
            .and_then(|endpoint| endpoint.get("skills_dirs"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|e| e.as_str())
            .map(String::from);
    }
    config
        .get("targets")
        .and_then(|targets| targets.get(target))
        .and_then(|cfg| cfg.get("skills_dir"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/* 跑引擎的 --emit-config，拿到(必要时已从旧版迁移的)规范化 v2 配置。Python 是唯一迁移入口，避免双实现漂移。 */
fn emit_config(config_path: Option<String>) -> Result<Value, String> {
    let (config, resolved_config_path, project_root) = load_config_json(config_path)?;
    let runtime = config
        .get("runtime")
        .and_then(|value| value.as_object())
        .ok_or_else(|| "missing runtime config".to_string())?;
    let python = runtime
        .get("python_executable")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing runtime.python_executable".to_string())?;
    let script = runtime
        .get("script_path")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing runtime.script_path".to_string())?;
    let runtime_base = resolved_config_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let python_path = resolve_python_executable(runtime_base, &resolved_config_path, python);
    let script_path = resolve_runtime_file(runtime_base, &resolved_config_path, script);

    let output = Command::new(&python_path)
        .current_dir(&project_root)
        .arg(script_path)
        .arg("--config")
        .arg(&resolved_config_path)
        .arg("--emit-config")
        .output()
        .map_err(|err| format!("failed to run --emit-config: {}", err))?;

    if !output.status.success() && output.stdout.is_empty() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim())
        .map_err(|err| format!("failed to parse emit-config output: {}", err))
}

fn write_temp_config(base_config_path: &Path, config: &Value) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_millis();
    let file_name = format!("cc-sync.runtime.{}.json", timestamp);
    let temp_path = std::env::temp_dir().join(file_name);
    let payload = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;

    fs::write(&temp_path, payload + "\n").map_err(|err| {
        format!(
            "failed to write temp config for {}: {}",
            base_config_path.display(),
            err
        )
    })?;
    Ok(temp_path)
}

/* 命令执行只信任磁盘上的配置:python_executable / script_path 永远从磁盘 config 读，
 * 绝不读 webview 传入的 override.runtime.*——否则前端能指定任意可执行文件来启动。
 * 磁盘 config 读不到(如首次运行尚未保存)时退回内置默认，仍然不读 override。 */
fn trusted_runtime_paths(resolved_config_path: &Path) -> (String, String) {
    let on_disk_runtime = fs::read_to_string(resolved_config_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|cfg| cfg.get("runtime").cloned());

    let python = on_disk_runtime
        .as_ref()
        .and_then(|runtime| runtime.get("python_executable"))
        .and_then(|value| value.as_str())
        .unwrap_or("python/bin/python.exe")
        .to_string();
    let script = on_disk_runtime
        .as_ref()
        .and_then(|runtime| runtime.get("script_path"))
        .and_then(|value| value.as_str())
        .unwrap_or("sync_agents.py")
        .to_string();
    (python, script)
}

fn run_sync(
    config_path: Option<String>,
    config_override: Option<Value>,
    scope: String,
    dry_run: bool,
) -> Result<Value, String> {
    let (runtime_config, resolved_config_path, project_root, has_config_override) =
        runtime_config_context(config_path, config_override)?;
    let temp_config_path = if has_config_override {
        Some(write_temp_config(&resolved_config_path, &runtime_config)?)
    } else {
        None
    };
    let effective_config_path = temp_config_path.as_ref().unwrap_or(&resolved_config_path);

    // 安全:可执行文件与脚本只信任磁盘配置，override 仅用于同步内容(sources/endpoints 等)。
    let (python, script) = trusted_runtime_paths(&resolved_config_path);

    let runtime_base = resolved_config_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let python_path = resolve_python_executable(runtime_base, &resolved_config_path, &python);
    let script_path = resolve_runtime_file(runtime_base, &resolved_config_path, &script);

    let mut command = Command::new(&python_path);
    command
        .current_dir(&project_root)
        .env(
            STATE_ENV_VAR,
            resolved_config_path.with_file_name(STATE_FILE_NAME),
        )
        .arg(&script_path)
        .arg("--config")
        .arg(effective_config_path)
        .arg("--scope")
        .arg(scope)
        .arg("--json");

    if dry_run {
        command.arg("--dry-run");
    }

    let output = match command.output() {
        Ok(output) => output,
        Err(err) => {
            if let Some(temp_path) = temp_config_path.as_ref() {
                let _ = fs::remove_file(temp_path);
            }
            return Err(format!(
                "failed to execute sync script: {} (python: {}, script: {})",
                err,
                python_path.display(),
                script_path.display()
            ));
        }
    };

    if let Some(temp_path) = temp_config_path {
        let _ = fs::remove_file(temp_path);
    }

    if !output.status.success() && output.stdout.is_empty() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    serde_json::from_str(&stdout).map_err(|err| format!("failed to parse sync output: {}", err))
}

#[tauri::command]
fn load_config_data(config_path: Option<String>) -> Result<Value, String> {
    // 优先用引擎 --emit-config 拿规范化(必要时已迁移)的配置;引擎不可用(如开发环境无内嵌
    // Python)时回退到原始读取，保证不崩。
    match emit_config(config_path.clone()) {
        Ok(value) => Ok(value),
        Err(emit_err) => match load_config_json(config_path.clone()) {
            Ok((config, _, _)) => Ok(config),
            Err(read_err) => {
                // 关键:只有当配置文件【不存在】时才用内置示例(合法的首次运行)。
                // 文件存在但解析失败时，必须把错误抛给前端，绝不静默用示例顶替——否则
                // 用户会以为加载成功，一旦编辑触发自动保存就会把真实(破损)配置覆盖掉。
                let path = config_path
                    .map(PathBuf::from)
                    .unwrap_or_else(default_config_path);
                if path.exists() {
                    return Err(format!(
                        "配置文件存在但无法加载:{} (engine: {} / read: {})",
                        path.display(),
                        emit_err,
                        read_err
                    ));
                }
                serde_json::from_str(include_str!("../../cc-sync.config.example.json"))
                    .map_err(|err| format!("failed to load default config: {}", err))
            }
        },
    }
}

#[tauri::command]
fn save_config_data(config: Value, config_path: Option<String>) -> Result<(), String> {
    let path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);

    // 首次从旧版升级落盘前，把旧文件备份为 *.v1.bak（仅当仍是旧版且尚无备份时），非破坏性。
    if path.exists() {
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(existing) = serde_json::from_str::<Value>(&raw) {
                let is_legacy = existing
                    .get("schema_version")
                    .and_then(|value| value.as_i64())
                    .map(|version| version < 3)
                    .unwrap_or(true)
                    || existing.get("sources").is_none();
                if is_legacy {
                    let backup = path.with_extension("v1.bak");
                    if !backup.exists() {
                        let _ = fs::copy(&path, &backup);
                    }
                }
            }
        }
    }

    let payload = serde_json::to_string_pretty(&config).map_err(|err| err.to_string())?;

    // 原子写:先写同目录临时文件，再 rename 覆盖。避免写到一半被杀进程时把真实配置截断/损坏。
    // (Windows 上 fs::rename 使用 MOVEFILE_REPLACE_EXISTING，可原子覆盖同卷已存在文件。)
    let mut tmp_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("cc-sync.config.json"));
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);

    fs::write(&tmp_path, payload + "\n")
        .map_err(|err| format!("failed to write {}: {}", tmp_path.display(), err))?;
    fs::rename(&tmp_path, &path).map_err(|err| {
        let _ = fs::remove_file(&tmp_path);
        format!("failed to replace {}: {}", path.display(), err)
    })
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    open_path_in_explorer(&path)
}

#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.hide().map_err(|err| err.to_string())
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn run_sync_preview(
    scope: String,
    config_path: Option<String>,
    config: Option<Value>,
) -> Result<Value, String> {
    run_sync(config_path, config, scope, true)
}

#[tauri::command]
fn run_sync_execute(
    scope: String,
    config_path: Option<String>,
    config: Option<Value>,
) -> Result<Value, String> {
    run_sync(config_path, config, scope, false)
}

/* 扫描给定 skill 目录(由前端按当前内存中的源端点传入，避免依赖磁盘上尚未保存的源选择)。 */
#[tauri::command]
fn list_available_skills(
    dirs: Vec<String>,
    config_path: Option<String>,
    config: Option<Value>,
) -> Result<Vec<SkillOption>, String> {
    let (_, _, project_root, _) = runtime_config_context(config_path, config)?;
    let mut skills = BTreeMap::<String, SkillOptionBuilder>::new();

    let roots: Vec<PathBuf> = dirs
        .iter()
        .map(|raw| resolve_runtime_path(&project_root, raw))
        .collect();

    for root in roots {
        if !root.exists() || !root.is_dir() {
            continue;
        }

        let entries = fs::read_dir(&root)
            .map_err(|err| format!("failed to read skill root {}: {}", root.display(), err))?;

        for entry in entries {
            let entry = entry.map_err(|err| err.to_string())?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|err| err.to_string())?;
            if !file_type.is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }

            let skill_md = path.join("SKILL.md");
            let Some(metadata) = parse_skill_metadata(&skill_md) else {
                continue;
            };

            let option = skills.entry(name).or_default();
            if option.display_name.is_none() {
                option.display_name = metadata.display_name;
            }
            if option.description.is_none() {
                option.description = metadata.description;
            }
            option.paths.push(path.to_string_lossy().to_string());
        }
    }

    Ok(skills
        .into_iter()
        .map(|(name, option)| SkillOption {
            name,
            display_name: option.display_name,
            description: option.description,
            paths: option.paths,
        })
        .collect())
}

/* 列出某个 target 的 skills 目录里实际存在的技能(子目录名)。
 * 用 entry.path().is_dir() 跟随 junction/symlink，能识别同步创建的链接与复制目录。 */
#[tauri::command]
fn list_target_skills(
    target: String,
    config_path: Option<String>,
    config: Option<Value>,
) -> Result<Vec<TargetSkill>, String> {
    let (runtime_config, _, project_root, _) = runtime_config_context(config_path, config)?;
    let skills_dir_raw = match target_skills_dir(&runtime_config, &target) {
        Some(raw) => raw,
        None => return Ok(Vec::new()),
    };

    let dir = resolve_runtime_path(&project_root, &skills_dir_raw);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        let is_reparse = meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        let kind = if is_reparse {
            // junction/符号链接：path.is_dir() 会跟随，目标存在则有效，否则失效
            if path.is_dir() {
                "link"
            } else {
                "broken"
            }
        } else if meta.is_dir() {
            "copy"
        } else {
            continue; // 普通文件，忽略
        };
        items.push(TargetSkill {
            name,
            kind: kind.to_string(),
        });
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

/* 弹原生文件夹选择窗口，返回所选目录(取消则返回 null)。default_path 用于预定位到当前工作区。 */
#[tauri::command]
async fn pick_folder(
    app: tauri::AppHandle,
    default_path: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut builder = app.dialog().file();
    if let Some(start) = default_path {
        let start_path = PathBuf::from(&start);
        if start_path.is_dir() {
            builder = builder.set_directory(start_path);
        }
    }
    let picked = builder.blocking_pick_folder();
    Ok(picked.map(|file_path| file_path.to_string()))
}

/* 弹原生文件选择窗口，返回所选文件(取消则返回 null)。 */
#[tauri::command]
async fn pick_file(
    app: tauri::AppHandle,
    default_path: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut builder = app.dialog().file();
    if let Some(start) = default_path {
        let start_path = PathBuf::from(&start);
        if start_path.is_dir() {
            builder = builder.set_directory(start_path);
        } else if let Some(parent) = start_path.parent() {
            builder = builder.set_directory(parent);
        }
    }
    let picked = builder.blocking_pick_file();
    Ok(picked.map(|file_path| file_path.to_string()))
}

fn main() {
    tauri::Builder::default()
        .manage(TrayState {
            icon: Mutex::new(None),
        })
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_config_data,
            save_config_data,
            open_path,
            hide_main_window,
            exit_app,
            run_sync_preview,
            run_sync_execute,
            list_available_skills,
            list_target_skills,
            pick_folder,
            pick_file
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } = event
            {
                if label == "main" {
                    api.prevent_close();
                    let _ = app.emit("cc-sync-close-requested", ());
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_detects_patch_and_double_digits() {
        assert!(is_newer("v0.1.4", "0.1.3"));
        assert!(is_newer("0.1.10", "0.1.9")); // numeric, not string compare
        assert!(is_newer("v1.0.0", "0.9.9"));
    }

    #[test]
    fn newer_is_false_for_same_or_older() {
        assert!(!is_newer("0.1.3", "0.1.3"));
        assert!(!is_newer("v0.1.2", "0.1.3"));
    }

    #[test]
    fn newer_is_false_for_garbage() {
        assert!(!is_newer("not-a-version", "0.1.3"));
        assert!(!is_newer("0.1.4", ""));
    }

    #[test]
    fn parse_release_picks_zip_asset_and_flags_update() {
        let json: Value = serde_json::from_str(r#"{
            "tag_name": "v0.1.4",
            "html_url": "https://github.com/NeoRrrr/cc-sync/releases/tag/v0.1.4",
            "body": "notes here",
            "assets": [
                {"name": "CC.Sync.portable.zip", "browser_download_url": "https://example.com/p.zip"},
                {"name": "other.txt", "browser_download_url": "https://example.com/o.txt"}
            ]
        }"#).unwrap();

        let info = parse_release(&json, "0.1.3");
        assert_eq!(info.latest.as_deref(), Some("v0.1.4"));
        assert!(info.has_update);
        assert_eq!(info.download_url.as_deref(), Some("https://example.com/p.zip"));
        assert_eq!(info.release_url.as_deref(), Some("https://github.com/NeoRrrr/cc-sync/releases/tag/v0.1.4"));
        assert_eq!(info.notes.as_deref(), Some("notes here"));
    }

    #[test]
    fn parse_release_no_update_when_same_version() {
        let json: Value = serde_json::from_str(r#"{
            "tag_name": "v0.1.3",
            "assets": [{"name": "x.zip", "browser_download_url": "https://example.com/x.zip"}]
        }"#).unwrap();
        let info = parse_release(&json, "0.1.3");
        assert!(!info.has_update);
    }
}
