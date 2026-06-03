# In-App Updater — 设计文档

日期：2026-06-03 · 状态：待实现

## 目标

为便携版（Windows）做一个**半自动一键升级**：

- 主界面启动时**静默检查** GitHub 是否有更新的 release。
- 有新版时，在设置/帮助按钮旁出现一个「⬆」按钮（**只显示图标，不显示版本号**）。
- 点「⬆」→ 确认 → 下载新包 → 关闭并由外部 helper 替换文件 → 重新启动。

## 约束（决定方案的关键）

- **便携包**：没有安装器；逻辑大头在 `python/` + `sync_agents.py`，所以更新必须替换**整包**，官方 Tauri updater（只换 exe、需签名、面向安装版）不适用。
- **公开仓库** `NeoRrrr/cc-sync`：GitHub release API 可匿名读，无需 token。
- **匿名 API 限流 60 次/小时/IP**：公司共享出口 IP 容易打满 → 检查必须**低频 + 失败静默**。
- **Windows 锁定运行中的 exe**：无法覆盖自身 → 必须"app 先退出 → 外部 helper 再替换"。
- **无端到端签名**：下载走 GitHub HTTPS，但不做签名校验（不用官方签名 updater）。内部公开仓库可接受。

## 不做（Out of scope）

- 跨平台（仅 Windows）。
- 全自动后台静默升级 / 定时轮询。
- 下载产物 SHA256 校验（可作为后续可选增强）。

## UI

- **主界面头部**：仅当有新版时，在设置/帮助图标旁显示一个「⬆」图标按钮；无新版 / 限流 / 断网时**不显示任何东西**（不显示版本号、不报错）。
- 点「⬆」→ 确认弹窗：当前版本 → 新版本、release notes、提示"升级会关闭并重新打开 CC Sync"。
- 确认 → 下载进度条 → 完成后提示"即将重启升级"→ app 退出、helper 接管。
- **SettingsView**：仍显示当前版本，但版本号改为从 Rust 取（修掉写死且已过时的 `APP_VERSION = "0.1.1"`，[app.tsx:27](../../src/app.tsx)）。

## 后端命令（Rust，原生 HTTP，绕开 CSP）

- `app_version() -> String`：返回 `app.package_info().version`（来源 Cargo.toml/tauri.conf，单一真相）。也可并入 `check_update` 的 `current` 字段。
- `check_update() -> UpdateInfo`：
  - `GET https://api.github.com/repos/NeoRrrr/cc-sync/releases/latest`，带 `User-Agent` 头（GitHub 必需）。
  - 解析 `tag_name`、zip 资产的 `browser_download_url`、`html_url`、`body`(notes)。
  - **semver 逐段数值比对**（去 `v` 前缀，保证 `0.1.10 > 0.1.9`）。
  - 返回 `{ current, latest, hasUpdate, downloadUrl, releaseUrl, notes }`。
  - **失败（限流/断网/无 release）→ `hasUpdate=false` 软返回，前端静默**。
- `download_and_stage(url) -> String`：
  - 流式下载 zip 到 `%TEMP%\cc-sync-update\pkg.zip`，发 `update-progress` 事件给前端进度条。
  - 解压到 `%TEMP%\cc-sync-update\stage\`，**校验** `stage\CC Sync\CC Sync.exe` 存在。
  - 返回 staging 路径。
- `apply_update(stagingPath)`：写 `update.bat` 到 `%TEMP%`，detached 启动它，然后 app 退出。

## 更新器 helper（`update.bat`）—— 正确性核心

入参：当前 PID、安装目录（exe 父目录）、staging 目录。

1. 轮询等当前 PID 退出（`tasklist /FI "PID eq <pid>"`）。
2. `robocopy "<stage>\CC Sync" "<安装目录>" /E /R:3 /W:1 /XF cc-sync.config.json cc-sync.state.json cc-sync.state.json.tmp *.bak`
   - **`/XF` 排除用户的配置与状态文件；不加 `/PURGE`/`/MIR`，只覆盖新文件，不删用户手放的东西。**
3. `start "" "<安装目录>\CC Sync.exe"`。
4. `rmdir /s /q "<staging>"`；`del` 自身。

## 数据保护（硬性）

升级**永不覆盖** `cc-sync.config.json` / `cc-sync.state.json`（robocopy `/XF`）。不做镜像/清除。

## 边界与失败处理

- 限流 / 断网 / release 无 zip 资产 → 不显示 ⬆，或在确认弹窗里降级为「打开 release 页手动下载」。
- 下载失败 → 提示并提供「打开 release 页」兜底，不卡死。
- 校验失败（解压物里没有 exe）→ 中止，不触发替换。

## 新依赖

Rust 增加一个 HTTP 客户端：`ureq`（轻、阻塞、够用）或 `reqwest`（异步、带流式进度）。倾向 `reqwest` 以支持下载进度。这是本次唯一新依赖。

## 涉及文件

- `src-tauri/Cargo.toml`：加 HTTP 客户端依赖。
- `src-tauri/src/main.rs`：新命令 + helper 写入/启动 + 版本暴露。
- `src/lib/client.ts`：新命令绑定 + `update-progress` 事件监听。
- `src/types.ts`：`UpdateInfo` 类型。
- `src/app.tsx`：⬆ 按钮 + 挂载时静默检查 + 确认弹窗 + 进度；版本号来源改 Rust。
- `src/components/SettingsView.tsx`：版本号来源改 Rust。
- `src/i18n.ts`：相关中英文案。

## 验收

- `cargo check` / `tsc -b` / `npm run build` 通过。
- 手动：临时把本地版本号调低 → 启动出现 ⬆ → 升级流程下载、替换、重启成功；**`cc-sync.config.json` / `cc-sync.state.json` 保持不变**；限流/断网时不显示 ⬆ 也不报错。
