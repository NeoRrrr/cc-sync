#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const TRAY_MENU_SHOW: &str = "show";
const TRAY_MENU_QUIT: &str = "quit";

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
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        })
}

fn default_config_path() -> PathBuf {
    let portable_config = app_dir().join("cc-sync.config.json");
    if portable_config.exists() {
        return portable_config;
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cc-sync.config.json")
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
    let config_path = config_path.map(PathBuf::from).unwrap_or_else(default_config_path);
    let raw = fs::read_to_string(&config_path)
        .map_err(|err| format!("failed to read config {}: {}", config_path.display(), err))?;

    let value: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {}", config_path.display(), err))?;

    let project_root = resolve_project_root(&value, &config_path);
    Ok((value, config_path, project_root))
}

fn resolve_runtime_path(project_root: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        candidate
    } else {
        project_root.join(candidate)
    }
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
        Err(format!("explorer returned non-zero status for {}", target.display()))
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
    let runtime_base = resolved_config_path.parent().unwrap_or_else(|| Path::new("."));

    let output = Command::new(resolve_runtime_path(runtime_base, python))
        .current_dir(&project_root)
        .arg(resolve_runtime_path(runtime_base, script))
        .arg("--config")
        .arg(&resolved_config_path)
        .arg("--emit-config")
        .output()
        .map_err(|err| format!("failed to run --emit-config: {}", err))?;

    if !output.status.success() && output.stdout.is_empty() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|err| format!("failed to parse emit-config output: {}", err))
}

fn write_temp_config(base_config_path: &Path, config: &Value) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_millis();
    let file_name = format!("cc-sync.runtime.{}.json", timestamp);
    let temp_path = std::env::temp_dir().join(file_name);
    let payload = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;

    fs::write(&temp_path, payload + "\n")
        .map_err(|err| format!("failed to write temp config for {}: {}", base_config_path.display(), err))?;
    Ok(temp_path)
}

fn run_sync(
    config_path: Option<String>,
    config_override: Option<Value>,
    scope: String,
    dry_run: bool,
) -> Result<Value, String> {
    let (loaded_config, resolved_config_path, _) = load_config_json(config_path)?;
    let has_config_override = config_override.is_some();
    let runtime_config = config_override.unwrap_or(loaded_config);
    let project_root = resolve_project_root(&runtime_config, &resolved_config_path);
    let temp_config_path = if has_config_override {
        Some(write_temp_config(&resolved_config_path, &runtime_config)?)
    } else {
        None
    };
    let effective_config_path = temp_config_path
        .as_ref()
        .unwrap_or(&resolved_config_path);
    let runtime = runtime_config
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

    let runtime_base = resolved_config_path.parent().unwrap_or_else(|| Path::new("."));

    let mut command = Command::new(resolve_runtime_path(runtime_base, python));
    command
        .current_dir(&project_root)
        .arg(resolve_runtime_path(runtime_base, script))
        .arg("--config")
        .arg(effective_config_path)
        .arg("--scope")
        .arg(scope)
        .arg("--json");

    if dry_run {
        command.arg("--dry-run");
    }

    let output = command
        .output()
        .map_err(|err| format!("failed to execute sync script: {}", err))?;

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
    // 通过引擎 --emit-config 拿规范化 v2(必要时已迁移)；引擎不可用时回退到原始读取，保证不崩。
    match emit_config(config_path.clone()) {
        Ok(value) => Ok(value),
        Err(_) => {
            let (config, _, _) = load_config_json(config_path)?;
            Ok(config)
        }
    }
}

#[tauri::command]
fn save_config_data(config: Value, config_path: Option<String>) -> Result<(), String> {
    let path = config_path.map(PathBuf::from).unwrap_or_else(default_config_path);

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
    fs::write(&path, payload + "\n")
        .map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    open_path_in_explorer(&path)
}

#[tauri::command]
fn run_sync_preview(scope: String, config_path: Option<String>, config: Option<Value>) -> Result<Value, String> {
    run_sync(config_path, config, scope, true)
}

#[tauri::command]
fn run_sync_execute(scope: String, config_path: Option<String>, config: Option<Value>) -> Result<Value, String> {
    run_sync(config_path, config, scope, false)
}

/* 扫描给定 skill 目录(由前端按当前内存中的源端点传入，避免依赖磁盘上尚未保存的源选择)。 */
#[tauri::command]
fn list_available_skills(dirs: Vec<String>, config_path: Option<String>) -> Result<Vec<SkillOption>, String> {
    let (_, _, project_root) = load_config_json(config_path)?;
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
fn list_target_skills(target: String, config_path: Option<String>) -> Result<Vec<TargetSkill>, String> {
    let (config, _, project_root) = load_config_json(config_path)?;
    let skills_dir_raw = match target_skills_dir(&config, &target) {
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
            if path.is_dir() { "link" } else { "broken" }
        } else if meta.is_dir() {
            "copy"
        } else {
            continue; // 普通文件，忽略
        };
        items.push(TargetSkill { name, kind: kind.to_string() });
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

/* 弹原生文件夹选择窗口，返回所选目录(取消则返回 null)。default_path 用于预定位到当前工作区。 */
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle, default_path: Option<String>) -> Result<Option<String>, String> {
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
async fn pick_file(app: tauri::AppHandle, default_path: Option<String>) -> Result<Option<String>, String> {
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
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
        });
}
