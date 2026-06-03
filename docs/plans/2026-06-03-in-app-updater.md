# In-App Updater Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a semi-automatic one-click updater to the portable Windows build: silent launch-time check against the public GitHub releases API, an "⬆" button when a newer release exists, download + helper-based whole-package swap that preserves the user's config/state, then relaunch.

**Architecture:** All network + filesystem work runs in the Rust backend (native, so the webview CSP is not involved). The frontend shows an "⬆" icon only when an update exists, a confirm dialog, and a download progress bar. Because Windows locks the running `.exe`, the actual file replacement is done by a generated `update.bat` that waits for the app to exit, robocopies the new package over the install dir (excluding the user's `cc-sync.config.json` / `cc-sync.state.json`), then relaunches.

**Tech Stack:** Tauri 2 (Rust), React 18 + TypeScript, `reqwest` (blocking) for HTTP, Windows `robocopy` / `powershell Expand-Archive` (no new extraction crate).

**Spec:** `docs/specs/2026-06-03-in-app-updater.md`

---

## File structure

- `src-tauri/Cargo.toml` — add `reqwest` dependency.
- `src-tauri/src/main.rs` — add: `parse_semver`/`is_newer` (version compare), `UpdateInfo` + `parse_release`, `build_update_bat`, and commands `app_version`, `check_update`, `download_and_stage`, `apply_update`; register them in `invoke_handler`. Pure functions get `#[cfg(test)]` unit tests in the same file.
- `src/types.ts` — `UpdateInfo` type.
- `src/lib/client.ts` — bindings: `appVersion`, `checkUpdate`, `downloadAndStage`, `applyUpdate`, `onUpdateProgress`.
- `src/app.tsx` — launch-time check, "⬆" header button, confirm/progress dialog, version sourced from Rust (replaces the stale hard-coded `APP_VERSION`).
- `src/components/SettingsView.tsx` — receive version from Rust-sourced state (already takes a `version` prop).
- `src/i18n.ts` — update-related strings (zh + en).

---

## Task 1: Add the reqwest dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add reqwest under `[dependencies]`**

In `src-tauri/Cargo.toml`, add to the `[dependencies]` block:

```toml
reqwest = { version = "0.12", features = ["blocking", "json"] }
```

(Default TLS on Windows uses schannel — no OpenSSL needed.)

- [ ] **Step 2: Verify it resolves/compiles**

Run: `cd src-tauri && cargo check`
Expected: Finishes successfully (downloads + compiles reqwest the first time; may take a few minutes).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "Add reqwest dependency for updater"
```

---

## Task 2: Version comparison (TDD, pure)

**Files:**
- Modify: `src-tauri/src/main.rs` (add functions + test module)

- [ ] **Step 1: Write the failing tests**

Add to the end of `src-tauri/src/main.rs`:

```rust
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test is_newer`
Expected: FAIL — `cannot find function is_newer`.

- [ ] **Step 3: Implement the functions**

Add near the top of `src-tauri/src/main.rs` (after the `use` lines, before `main`):

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test is_newer newer_`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "Add semver compare for update check"
```

---

## Task 3: Parse GitHub release JSON into UpdateInfo (TDD, pure)

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write the failing test**

Add inside `mod tests`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test parse_release`
Expected: FAIL — `cannot find function parse_release` / `UpdateInfo`.

- [ ] **Step 3: Implement UpdateInfo + parse_release**

Add near the other structs in `src-tauri/src/main.rs`:

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test parse_release`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "Parse GitHub release into UpdateInfo"
```

---

## Task 4: `app_version` and `check_update` commands

**Files:**
- Modify: `src-tauri/src/main.rs`

No automated test (network). Verified by `cargo check` + manual run later.

- [ ] **Step 1: Add the commands**

Add to `src-tauri/src/main.rs` (anywhere among the other `#[tauri::command]` functions):

```rust
const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/NeoRrrr/cc-sync/releases/latest";

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
            .get(RELEASES_LATEST_URL)
            .header("Accept", "application/vnd.github+json")
            .send()
            .map_err(|err| err.to_string())?;
        if !resp.status().is_success() {
            // 限流(403)/无 release(404)/网络异常都走这里
            return Err(format!("github api status {}", resp.status()));
        }
        resp.json::<Value>().map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?;

    // 失败一律软返回 has_update=false，前端静默，不打扰用户。
    match fetched {
        Ok(json) => Ok(parse_release(&json, &current)),
        Err(_) => Ok(UpdateInfo {
            current,
            latest: None,
            has_update: false,
            download_url: None,
            release_url: None,
            notes: None,
        }),
    }
}
```

- [ ] **Step 2: Register the commands**

In `main()`, add `app_version,` and `check_update,` to the `tauri::generate_handler![ ... ]` list.

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Success.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "Add app_version and check_update commands"
```

---

## Task 5: `download_and_stage` command

**Files:**
- Modify: `src-tauri/src/main.rs`

Downloads the zip with progress events, extracts via PowerShell (no new crate), validates the package.

- [ ] **Step 1: Add the command**

```rust
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
            let pct = if total > 0 { (downloaded * 100 / total) as u32 } else { 0 };
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
```

- [ ] **Step 2: Register the command**

Add `download_and_stage,` to `tauri::generate_handler![ ... ]`.

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Success.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "Add download_and_stage command with progress events"
```

---

## Task 6: `build_update_bat` (TDD) + `apply_update` command

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write the failing test**

Add inside `mod tests`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test update_bat`
Expected: FAIL — `cannot find function build_update_bat`.

- [ ] **Step 3: Implement `build_update_bat`**

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test update_bat`
Expected: PASS.

- [ ] **Step 5: Implement `apply_update` command**

```rust
#[tauri::command]
fn apply_update(app: tauri::AppHandle, staging: String) -> Result<(), String> {
    let install_dir = app_dir(); // 已有:exe 所在目录
    let pid = std::process::id();
    let cleanup = std::env::temp_dir().join("cc-sync-update");
    let bat = build_update_bat(
        pid,
        &install_dir.to_string_lossy(),
        &staging,
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
```

- [ ] **Step 6: Register the command + verify**

Add `apply_update,` to `tauri::generate_handler![ ... ]`. Run: `cd src-tauri && cargo check` → Success.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "Add update.bat helper and apply_update command"
```

---

## Task 7: Frontend types + client bindings

**Files:**
- Modify: `src/types.ts`
- Modify: `src/lib/client.ts`

- [ ] **Step 1: Add the `UpdateInfo` type**

Append to `src/types.ts`:

```ts
export type UpdateInfo = {
  current: string;
  latest: string | null;
  has_update: boolean;
  download_url: string | null;
  release_url: string | null;
  notes: string | null;
};
```

- [ ] **Step 2: Add client bindings**

Append to `src/lib/client.ts` (it already has `inTauri()`, `invoke<T>()`, and an event-listener pattern in `listenCloseRequested`):

```ts
import type { UpdateInfo } from "../types";

/* 取当前 app 版本(来源 Rust package_info，单一真相)。 */
export async function appVersion(): Promise<string | null> {
  if (!inTauri()) {
    return null;
  }
  try {
    return await invoke<string>("app_version");
  } catch {
    return null;
  }
}

/* 静默检查更新。失败(限流/断网)时返回 null，调用方应忽略。 */
export async function checkUpdate(): Promise<UpdateInfo | null> {
  if (!inTauri()) {
    return null;
  }
  try {
    return await invoke<UpdateInfo>("check_update");
  } catch {
    return null;
  }
}

/* 下载并暂存更新包，返回 staging 路径。 */
export async function downloadAndStage(url: string): Promise<string> {
  return invoke<string>("download_and_stage", { url });
}

/* 写 helper 并退出 app，由 helper 完成替换与重启。 */
export async function applyUpdate(staging: string): Promise<void> {
  await invoke("apply_update", { staging });
}

/* 监听下载进度(0-100)。返回取消监听的函数。 */
export async function onUpdateProgress(handler: (pct: number) => void): Promise<() => void> {
  if (!inTauri()) {
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<number>("update-progress", (event) => handler(event.payload));
}
```

(Note: `UpdateInfo`/`SyncConfig` etc. are imported at the top of the file via the existing `import type { ... } from "../types"`; add `UpdateInfo` to that import instead of a second import line if the linter prefers a single import.)

- [ ] **Step 3: Type-check**

Run (repo root): `npx tsc -b`
Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add src/types.ts src/lib/client.ts
git commit -m "Add updater types and client bindings"
```

---

## Task 8: Frontend UI — version source fix, "⬆" button, check, dialog, progress

**Files:**
- Modify: `src/app.tsx`
- Modify: `src/components/SettingsView.tsx` (only if it currently uses the hard-coded constant directly — it takes a `version` prop, so usually just the value passed in `app.tsx` changes)

- [ ] **Step 1: Replace the stale version constant with Rust-sourced state**

In `src/app.tsx`:
- Delete the line `const APP_VERSION = "0.1.1";` (line ~27).
- Add state near the other `useState` declarations:

```tsx
const [appVersionStr, setAppVersionStr] = useState<string>("");
const [update, setUpdate] = useState<UpdateInfo | null>(null);
const [updateBusy, setUpdateBusy] = useState(false);
const [updateProgress, setUpdateProgress] = useState<number>(0);
const [showUpdate, setShowUpdate] = useState(false);
```

- Add `UpdateInfo` to the `../types` import and `appVersion, checkUpdate, downloadAndStage, applyUpdate, onUpdateProgress` to the `./lib/client` import.
- Where the settings view is rendered (`version={APP_VERSION}` at line ~946), change to `version={appVersionStr || update?.current || ""}`.

- [ ] **Step 2: Check for updates on mount**

Add a `useEffect` (alongside the existing config-load effect):

```tsx
useEffect(() => {
  void (async () => {
    const v = await appVersion();
    if (v) setAppVersionStr(v);
    const info = await checkUpdate(); // 失败返回 null，静默
    if (info?.has_update) setUpdate(info);
  })();
}, []);
```

- [ ] **Step 3: Add the "⬆" button to the header**

In the main header's left button group (`src/app.tsx` ~line 713, right after the help `InfoIcon` button), add:

```tsx
{update?.has_update && (
  <button
    type="button"
    className="icon-btn"
    aria-label={text.app.updateAvailable(update.latest ?? "")}
    title={text.app.updateAvailable(update.latest ?? "")}
    onClick={() => setShowUpdate(true)}
  >
    ⬆
  </button>
)}
```

- [ ] **Step 4: Add the confirm + progress dialog**

Add near the other modals at the bottom of the main render (follow the existing `Modal` usage pattern in the file — match its props to the existing modals):

```tsx
{showUpdate && update && (
  <Modal onClose={() => (updateBusy ? undefined : setShowUpdate(false))}>
    <h3>{text.app.updateTitle(update.latest ?? "")}</h3>
    {update.notes && <pre className="update-notes">{update.notes}</pre>}
    <p>{text.app.updateWarning}</p>
    {updateBusy && <p>{text.app.updateDownloading(updateProgress)}</p>}
    <div className="flex gap-2">
      <button
        type="button"
        className="primary-action"
        disabled={updateBusy || !update.download_url}
        onClick={async () => {
          if (!update.download_url) return;
          setUpdateBusy(true);
          const stop = await onUpdateProgress(setUpdateProgress);
          try {
            const staging = await downloadAndStage(update.download_url);
            await applyUpdate(staging); // app 将退出并由 helper 接管
          } catch (err) {
            appendLog(setLogs, "error", String(err));
            setUpdateBusy(false);
            stop();
          }
        }}
      >
        {text.app.updateNow}
      </button>
      <button
        type="button"
        className="utility-action"
        disabled={updateBusy}
        onClick={() => update.release_url && void openPath(update.release_url)}
      >
        {text.app.updateOpenPage}
      </button>
      <button type="button" className="utility-action" disabled={updateBusy} onClick={() => setShowUpdate(false)}>
        {text.app.cancel ?? "取消"}
      </button>
    </div>
  </Modal>
)}
```

(If `Modal`'s API differs — e.g. it needs a `title`/`open` prop — match the existing modal call sites in `app.tsx`. The `openPath` binding already exists in `client.ts` and opens a URL/path via the OS.)

- [ ] **Step 5: Type-check + build**

Run (repo root): `npx tsc -b` → exit 0.
Run: `npm run build` → succeeds.

- [ ] **Step 6: Commit**

```bash
git add src/app.tsx src/components/SettingsView.tsx
git commit -m "Add updater UI: arrow button, check on launch, confirm + progress"
```

---

## Task 9: i18n strings

**Files:**
- Modify: `src/i18n.ts`

- [ ] **Step 1: Add to the `Texts` `app` type**

In the `app: { ... }` type block, add:

```ts
updateAvailable: (version: string) => string;
updateTitle: (version: string) => string;
updateWarning: string;
updateDownloading: (pct: number) => string;
updateNow: string;
updateOpenPage: string;
```

(If `cancel` is not already present in the `app` type, add `cancel: string;` too.)

- [ ] **Step 2: Add the zh strings**

In the Chinese `app: { ... }` block:

```ts
updateAvailable: (version) => `发现新版本 ${version}，点击升级`,
updateTitle: (version) => `升级到 ${version}`,
updateWarning: "升级会关闭并重新打开 CC Sync。你的配置和同步状态不会被覆盖。",
updateDownloading: (pct) => `下载中… ${pct}%`,
updateNow: "立即升级",
updateOpenPage: "打开发布页",
```

(Add `cancel: "取消",` if missing.)

- [ ] **Step 3: Add the en strings**

In the English `app: { ... }` block:

```ts
updateAvailable: (version) => `New version ${version} available — click to update`,
updateTitle: (version) => `Update to ${version}`,
updateWarning: "Updating will close and reopen CC Sync. Your config and sync state are preserved.",
updateDownloading: (pct) => `Downloading… ${pct}%`,
updateNow: "Update now",
updateOpenPage: "Open release page",
```

(Add `cancel: "Cancel",` if missing.)

- [ ] **Step 4: Type-check**

Run (repo root): `npx tsc -b` → exit 0 (this also confirms the `text.app.*` usages in Task 8 resolve).

- [ ] **Step 5: Commit**

```bash
git add src/i18n.ts
git commit -m "Add updater i18n strings"
```

---

## Task 10: Full build verification + manual acceptance

**Files:** none (verification only)

- [ ] **Step 1: Full static verification**

```bash
cd src-tauri && cargo test && cargo check && cd ..
npx tsc -b
npm run build
```
Expected: all pass / exit 0.

- [ ] **Step 2: Manual smoke test of the happy path**

To simulate an available update without publishing, temporarily lower the local version:
- In `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, set version to `0.1.2` (below the published `0.1.3`).
- Run `npm run tauri:dev`.
- Expect: the "⬆" button appears in the header (because GitHub's latest `0.1.3` > local `0.1.2`).
- Click it → confirm dialog shows notes + warning → click 立即升级 → progress advances → app closes; the helper replaces files and relaunches.
- After relaunch, confirm the app is running and the window renders.
- **Revert the temporary version change** when done.

- [ ] **Step 3: Verify data protection (critical)**

Before the manual upgrade in Step 2, note the contents/timestamps of `cc-sync.config.json` and `cc-sync.state.json` in the install dir. After the upgrade + relaunch, confirm **both files are unchanged** (the updater must never overwrite them).

- [ ] **Step 4: Verify graceful degradation**

- Disconnect network (or rely on the shared-IP rate limit) and launch: the "⬆" button must **not** appear and no error is shown.

- [ ] **Step 5: Commit any fixes**

If Steps 2–4 surfaced issues, fix and commit. Otherwise the feature is complete.

---

## Notes for the implementer

- **Single source of version truth:** the current version comes from `app.package_info().version` (Cargo.toml/tauri.conf), never a hard-coded TS constant. The old `APP_VERSION = "0.1.1"` was stale — removing it is part of Task 8.
- **CSP is not touched:** all HTTP happens in Rust, so the webview CSP added earlier needs no change.
- **Repo coordinates are hard-coded** in `RELEASES_LATEST_URL` (`NeoRrrr/cc-sync`). If the repo moves, update that constant.
- **No end-to-end signature** on the downloaded zip (HTTPS from GitHub only). Acceptable for an internal public-repo tool; a future enhancement could verify a SHA256 published in the release notes.
