#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::path::BaseDirectory;
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

fn parse_semver(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().trim_start_matches(['v', 'V']);
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    // patch may carry a pre-release suffix (e.g. "3-rc1") — take leading digits only.
    let patch_raw = parts.next().unwrap_or("0");
    let patch_digits: String = patch_raw
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let patch = patch_digits.parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[cfg(windows)]
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
    platform: String,
    latest: Option<String>,
    has_update: bool,
    download_url: Option<String>,
    release_url: Option<String>,
    notes: Option<String>,
}

fn parse_manifest(json: &Value, current: &str) -> UpdateInfo {
    parse_manifest_for_platform(json, current, current_update_platform())
}

fn current_update_platform() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "linux"
    }
}

fn manifest_entry_for_platform<'a>(json: &'a Value, platform: &str) -> Option<&'a Value> {
    json.get("platforms")
        .and_then(|value| value.as_object())
        .and_then(|platforms| platforms.get(platform))
}

fn manifest_string_field(entry: Option<&Value>, root: &Value, key: &str) -> Option<String> {
    entry
        .and_then(|value| value.get(key))
        .or_else(|| root.get(key))
        .and_then(|value| value.as_str())
        .map(String::from)
}

fn parse_manifest_for_platform(json: &Value, current: &str, platform: &str) -> UpdateInfo {
    // 从 raw.githubusercontent.com 上的 latest.json 解析。用 raw 文件而非 GitHub API:
    // anon API 限额 60/hr 按 IP 共享，公司出口 IP 容易被打满导致检查静默失效;
    // raw 文件不受该限额，公司网也可靠。
    //
    // v0.1.4 之前的 manifest 只有根级 download_url，且这个 URL 指向 Windows
    // portable zip。非 Windows 平台不能回退到根级字段，否则 macOS 会拿到 Windows 包。
    let platform_entry = manifest_entry_for_platform(json, platform);
    let active_entry = platform_entry.or_else(|| {
        if platform == "windows" && json.get("version").is_some() {
            Some(json)
        } else {
            None
        }
    });

    let latest = active_entry.and_then(|entry| entry.get("version").and_then(|v| v.as_str()));
    let download_url = active_entry
        .and_then(|entry| entry.get("download_url"))
        .and_then(|value| value.as_str())
        .map(String::from);
    let release_url = manifest_string_field(active_entry, json, "release_url");
    let notes = manifest_string_field(active_entry, json, "notes");

    let has_update =
        latest.map(|l| is_newer(l, current)).unwrap_or(false) && download_url.is_some();

    UpdateInfo {
        current: current.to_string(),
        platform: platform.to_string(),
        latest: latest.map(String::from),
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

#[cfg(any(windows, test))]
fn build_update_bat(pid: u32, install_dir: &str, staging: &str, cleanup_dir: &str) -> String {
    // 注意:绝不加 /MIR /PURGE;/XF 排除用户的配置与状态，升级永不覆盖它们。
    format!(
        "@echo off\r\n\
chcp 65001 >nul\r\n\
:waitloop\r\n\
tasklist /FI \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul && (timeout /t 1 /nobreak >nul & goto waitloop)\r\n\
robocopy \"{staging}\\CC Sync\" \"{install}\" /E /R:3 /W:1 /XF cc-sync.config.json cc-sync.state.json cc-sync.state.json.tmp *.bak >nul\r\n\
start \"\" \"{install}\\CC Sync.exe\"\r\n\
rmdir /s /q \"{cleanup}\" >nul 2>&1\r\n\
del \"%~f0\"\r\n",
        pid = pid,
        staging = staging,
        install = install_dir,
        cleanup = cleanup_dir,
    )
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

#[cfg(all(not(debug_assertions), target_os = "macos"))]
fn default_config_path() -> PathBuf {
    let cwd_config = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("cc-sync.config.json");
    if cwd_config.exists() {
        return cwd_config;
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("CC Sync")
                .join("cc-sync.config.json")
        })
        .unwrap_or_else(|| app_dir().join("cc-sync.config.json"))
}

#[cfg(all(not(debug_assertions), not(target_os = "macos")))]
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

fn resolve_runtime_file_with_resources(
    app: Option<&tauri::AppHandle>,
    runtime_base: &Path,
    config_path: &Path,
    raw: &str,
) -> PathBuf {
    let primary = resolve_runtime_path(runtime_base, raw);
    if primary.exists() {
        return primary;
    }

    let source_candidate = resolve_runtime_path(&source_project_root(config_path), raw);
    if source_candidate.exists() {
        return source_candidate;
    }

    if let Some(resource) = resolve_bundled_resource(app, raw) {
        return resource;
    }

    primary
}

fn resolve_bundled_resource(app: Option<&tauri::AppHandle>, raw: &str) -> Option<PathBuf> {
    let normalized = raw.replace('\\', "/");
    let resource_name = Path::new(&normalized)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())?;
    app.and_then(|handle| {
        handle
            .path()
            .resolve(resource_name, BaseDirectory::Resource)
            .ok()
    })
    .filter(|path| path.exists())
}

fn default_python_executable() -> &'static str {
    #[cfg(windows)]
    {
        "python/bin/python.exe"
    }
    #[cfg(not(windows))]
    {
        "python3"
    }
}

fn platform_python_fallback() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from("python")
    }
    #[cfg(not(windows))]
    {
        for candidate in [
            "/usr/bin/python3",
            "/opt/homebrew/bin/python3",
            "/usr/local/bin/python3",
        ] {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return path;
            }
        }
        PathBuf::from("python3")
    }
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
        return platform_python_fallback();
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

#[cfg(target_os = "windows")]
fn open_path_native(target: &Path) -> Result<(), String> {
    let mut command = Command::new("explorer.exe");
    if target.is_file() {
        command.arg("/select,").arg(target);
    } else {
        command.arg(target);
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

#[cfg(target_os = "macos")]
fn open_path_native(target: &Path) -> Result<(), String> {
    let mut command = Command::new("open");
    if target.is_file() {
        command.arg("-R").arg(target);
    } else {
        command.arg(target);
    }
    command
        .spawn()
        .map_err(|err| format!("failed to open {}: {}", target.display(), err))?;
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_path_native(target: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map_err(|err| format!("failed to open {}: {}", target.display(), err))?;
    Ok(())
}

/* 打开目录，或在系统文件管理器中定位文件。 */
fn open_path_in_file_manager(raw_path: &str) -> Result<(), String> {
    let path = PathBuf::from(raw_path);
    let target = if path.exists() {
        path
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("path does not exist: {}", raw_path))?
    };

    open_path_native(&target)
}

#[cfg(target_os = "windows")]
fn open_url_native(url: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map_err(|err| format!("failed to open url {}: {}", url, err))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_url_native(url: &str) -> Result<(), String> {
    Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|err| format!("failed to open url {}: {}", url, err))?;
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_url_native(url: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map_err(|err| format!("failed to open url {}: {}", url, err))?;
    Ok(())
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
fn emit_config(
    config_path: Option<String>,
    app: Option<&tauri::AppHandle>,
) -> Result<Value, String> {
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
    let script_path =
        resolve_runtime_file_with_resources(app, runtime_base, &resolved_config_path, script);

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
        .unwrap_or(default_python_executable())
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
    app: Option<&tauri::AppHandle>,
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
    let script_path =
        resolve_runtime_file_with_resources(app, runtime_base, &resolved_config_path, &script);

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
fn load_config_data(app: tauri::AppHandle, config_path: Option<String>) -> Result<Value, String> {
    // 优先用引擎 --emit-config 拿规范化(必要时已迁移)的配置;引擎不可用(如开发环境无内嵌
    // Python)时回退到原始读取，保证不崩。
    match emit_config(config_path.clone(), Some(&app)) {
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create config dir {}: {}", parent.display(), err))?;
    }

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
    open_path_in_file_manager(&path)
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // 用默认浏览器打开 URL。不要走 open_path——那个按文件路径处理，URL "不存在" 会回退到
    // .parent() 把最后一段(如版本 tag)切掉，打开错误页。
    open_url_native(&url)
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
    app: tauri::AppHandle,
    scope: String,
    config_path: Option<String>,
    config: Option<Value>,
) -> Result<Value, String> {
    run_sync(Some(&app), config_path, config, scope, true)
}

#[tauri::command]
fn run_sync_execute(
    app: tauri::AppHandle,
    scope: String,
    config_path: Option<String>,
    config: Option<Value>,
) -> Result<Value, String> {
    run_sync(Some(&app), config_path, config, scope, false)
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
        let Some(kind) = classify_target_skill(&path, &meta) else {
            continue;
        };
        items.push(TargetSkill {
            name,
            kind: kind.to_string(),
        });
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

#[cfg(windows)]
fn classify_target_skill(path: &Path, meta: &fs::Metadata) -> Option<&'static str> {
    let is_reparse = meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    if is_reparse {
        // junction/符号链接：path.is_dir() 会跟随，目标存在则有效，否则失效
        if path.is_dir() {
            Some("link")
        } else {
            Some("broken")
        }
    } else if meta.is_dir() {
        Some("copy")
    } else {
        None
    }
}

#[cfg(not(windows))]
fn classify_target_skill(path: &Path, meta: &fs::Metadata) -> Option<&'static str> {
    if meta.file_type().is_symlink() {
        if path.is_dir() {
            Some("link")
        } else {
            Some("broken")
        }
    } else if meta.is_dir() {
        Some("copy")
    } else {
        None
    }
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

const MANIFEST_URL: &str = "https://raw.githubusercontent.com/NeoRrrr/cc-sync/main/latest.json";

#[tauri::command]
fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();

    let fetched = tauri::async_runtime::spawn_blocking(|| -> Result<Value, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("cc-sync-updater")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|err| err.to_string())?;
        let resp = client
            .get(MANIFEST_URL)
            .send()
            .map_err(|err| err.to_string())?;
        if !resp.status().is_success() {
            // 网络异常 / 文件暂不可用都走这里
            return Err(format!("manifest status {}", resp.status()));
        }
        resp.json::<Value>().map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?;

    // 失败一律软返回 has_update=false，前端静默，不打扰用户。
    match fetched {
        Ok(json) => Ok(parse_manifest(&json, &current)),
        Err(_) => Ok(UpdateInfo {
            current,
            platform: current_update_platform().to_string(),
            latest: None,
            has_update: false,
            download_url: None,
            release_url: None,
            notes: None,
        }),
    }
}

#[cfg(windows)]
#[tauri::command]
async fn download_and_stage(app: tauri::AppHandle, url: String) -> Result<String, String> {
    let root = std::env::temp_dir().join("cc-sync-update");
    let _ = fs::remove_dir_all(&root); // 清理上一次残留
    fs::create_dir_all(&root).map_err(|err| err.to_string())?;
    let zip_path = root.join("pkg.zip");
    let stage = root.join("stage");

    // 下载(分块读取 + 进度事件)。reqwest::blocking::Response 实现 Read。
    let app_for_dl = app.clone();
    let zip_for_dl = zip_path.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("cc-sync-updater")
            .build()
            .map_err(|err| err.to_string())?;
        let mut resp = client.get(&url).send().map_err(|err| err.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("下载失败：HTTP {}", resp.status()));
        }
        let total = resp.content_length().unwrap_or(0);
        let mut file = fs::File::create(&zip_for_dl).map_err(|err| err.to_string())?;
        let mut downloaded: u64 = 0;
        let mut buf = [0u8; 65536];
        loop {
            let read = std::io::Read::read(&mut resp, &mut buf).map_err(|err| err.to_string())?;
            if read == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buf[..read]).map_err(|err| err.to_string())?;
            downloaded += read as u64;
            let pct = if total > 0 {
                (downloaded * 100 / total) as u32
            } else {
                0
            };
            let _ = app_for_dl.emit("update-progress", pct);
        }
        Ok(())
    })
    .await
    .map_err(|err| err.to_string())??;

    // 解压(PowerShell Expand-Archive，免新依赖)。
    let command = format!(
        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        zip_path.display(),
        stage.display()
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .status()
        .map_err(|err| err.to_string())?;
    if !status.success() {
        return Err("解压更新包失败".to_string());
    }

    // 校验:解压物里必须有 CC Sync\CC Sync.exe。
    let exe = stage.join("CC Sync").join("CC Sync.exe");
    if !exe.exists() {
        return Err("更新包校验失败：未找到 CC Sync.exe".to_string());
    }

    Ok(stage.to_string_lossy().to_string())
}

#[cfg(not(windows))]
#[tauri::command]
async fn download_and_stage(_app: tauri::AppHandle, _url: String) -> Result<String, String> {
    Err("自动升级当前只支持 Windows 便携包，请打开发布页手动下载。".to_string())
}

#[cfg(windows)]
#[tauri::command]
fn apply_update(app: tauri::AppHandle) -> Result<(), String> {
    let install_dir = app_dir(); // 已有:exe 所在目录
    let pid = std::process::id();
    let cleanup = std::env::temp_dir().join("cc-sync-update");
    // 零信任:staging 目录由后端按固定规则重算，不接收前端传入的字符串(避免命令注入)。
    let staging = cleanup.join("stage");
    let bat = build_update_bat(
        pid,
        &install_dir.to_string_lossy(),
        &staging.to_string_lossy(),
        &cleanup.to_string_lossy(),
    );
    let bat_path = std::env::temp_dir().join("cc-sync-apply-update.bat");
    fs::write(&bat_path, bat).map_err(|err| err.to_string())?;

    // detached 启动 helper(用 start 脱离父进程，这样本 app 退出后它仍在跑)。
    Command::new("cmd")
        .args(["/c", "start", "", "/min"])
        .arg(&bat_path)
        .spawn()
        .map_err(|err| err.to_string())?;

    app.exit(0); // 退出，让 helper 能替换被锁的 exe
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
fn apply_update(_app: tauri::AppHandle) -> Result<(), String> {
    Err("自动升级当前只支持 Windows 便携包，请打开发布页手动下载。".to_string())
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
            open_url,
            hide_main_window,
            exit_app,
            run_sync_preview,
            run_sync_execute,
            list_available_skills,
            list_target_skills,
            pick_folder,
            pick_file,
            app_version,
            check_update,
            download_and_stage,
            apply_update
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
    fn parse_manifest_reads_fields_and_flags_update() {
        let json: Value = serde_json::from_str(
            r#"{
            "version": "0.1.4",
            "notes": "notes here",
            "download_url": "https://example.com/p.zip",
            "release_url": "https://github.com/NeoRrrr/cc-sync/releases/tag/v0.1.4"
        }"#,
        )
        .unwrap();

        let info = parse_manifest_for_platform(&json, "0.1.3", "windows");
        assert_eq!(info.platform, "windows");
        assert_eq!(info.latest.as_deref(), Some("0.1.4"));
        assert!(info.has_update);
        assert_eq!(
            info.download_url.as_deref(),
            Some("https://example.com/p.zip")
        );
        assert_eq!(
            info.release_url.as_deref(),
            Some("https://github.com/NeoRrrr/cc-sync/releases/tag/v0.1.4")
        );
        assert_eq!(info.notes.as_deref(), Some("notes here"));
    }

    #[test]
    fn parse_manifest_no_update_when_same_version() {
        let json: Value = serde_json::from_str(
            r#"{
            "version": "0.1.3",
            "download_url": "https://example.com/x.zip"
        }"#,
        )
        .unwrap();
        let info = parse_manifest_for_platform(&json, "0.1.3", "windows");
        assert!(!info.has_update);
    }

    #[test]
    fn parse_manifest_uses_platform_entry_without_cross_platform_download_fallback() {
        let json: Value = serde_json::from_str(
            r#"{
            "version": "0.1.4",
            "notes": "root windows notes",
            "download_url": "https://example.com/windows.zip",
            "release_url": "https://github.com/NeoRrrr/cc-sync/releases/tag/v0.1.4",
            "platforms": {
                "macos": {
                    "version": "0.1.5",
                    "notes": "mac notes",
                    "download_url": "https://example.com/macos.dmg",
                    "release_url": "https://github.com/NeoRrrr/cc-sync/releases/tag/v0.1.5"
                }
            }
        }"#,
        )
        .unwrap();

        let mac = parse_manifest_for_platform(&json, "0.1.4", "macos");
        assert_eq!(mac.platform, "macos");
        assert_eq!(mac.latest.as_deref(), Some("0.1.5"));
        assert_eq!(
            mac.download_url.as_deref(),
            Some("https://example.com/macos.dmg")
        );
        assert!(mac.has_update);

        let linux = parse_manifest_for_platform(&json, "0.1.4", "linux");
        assert_eq!(linux.platform, "linux");
        assert_eq!(linux.latest.as_deref(), None);
        assert_eq!(linux.download_url.as_deref(), None);
        assert!(!linux.has_update);
    }

    #[test]
    fn update_bat_waits_relaunches_and_protects_user_files() {
        let bat = build_update_bat(
            1234,
            r"C:\Users\Admin\Dawn\Trunk\tools\AI\CC Sync",
            r"C:\Temp\cc-sync-update\stage",
            r"C:\Temp\cc-sync-update",
        );
        // 等本进程退出
        assert!(bat.contains("PID eq 1234"));
        // robocopy 覆盖整包，但排除用户配置/状态
        assert!(bat.contains("robocopy"));
        assert!(bat.contains("/XF cc-sync.config.json cc-sync.state.json"));
        // 不能出现 /MIR 或 /PURGE(会删用户文件)
        assert!(!bat.contains("/MIR"));
        assert!(!bat.contains("/PURGE"));
        // 升级后重启
        assert!(bat.contains(r"CC Sync.exe"));
        // 自删
        assert!(bat.contains("del "));
    }
}
