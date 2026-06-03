import type { AvailableSkillOption, SyncConfig, SyncPlan, SyncScope, TargetSkill, UpdateInfo } from "../types";
import { mockConfig } from "./mockState";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

function inTauri() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

async function invoke<T>(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export async function listenCloseRequested(handler: () => void): Promise<() => void> {
  if (!inTauri()) {
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen("cc-sync-close-requested", handler);
}

export async function loadConfig(): Promise<{ config: SyncConfig; mode: "desktop" | "demo"; error?: string }> {
  if (!inTauri()) {
    return { config: structuredClone(mockConfig), mode: "demo" };
  }

  try {
    const config = await invoke<SyncConfig>("load_config_data");
    return { config, mode: "desktop" };
  } catch (error) {
    // 配置文件存在但加载失败时,后端会返回错误(而不是静默顶替示例配置)。这里退回
    // demo(只读)模式并把原因带出去,避免在 desktop 模式下被自动保存覆盖掉真实配置。
    return { config: structuredClone(mockConfig), mode: "demo", error: String(error) };
  }
}

export async function saveConfig(config: SyncConfig): Promise<void> {
  if (!inTauri()) {
    throw new Error("当前不是桌面模式，无法写入本地配置");
  }

  await invoke("save_config_data", { config });
}

export async function hideMainWindow(): Promise<void> {
  if (!inTauri()) {
    return;
  }
  await invoke("hide_main_window");
}

export async function exitApp(): Promise<void> {
  if (!inTauri()) {
    return;
  }
  await invoke("exit_app");
}

/* dry-run 预览：返回将要执行的计划但不写盘。 */
export async function previewSync(scope: SyncScope, config?: SyncConfig): Promise<SyncPlan> {
  if (!inTauri()) {
    throw new Error("当前不是桌面模式，无法预览同步");
  }

  return invoke<SyncPlan>("run_sync_preview", { scope, config });
}

export async function runSync(scope: SyncScope, config?: SyncConfig): Promise<SyncPlan> {
  if (!inTauri()) {
    throw new Error("当前不是桌面模式，无法执行真实同步");
  }

  return invoke<SyncPlan>("run_sync_execute", { scope, config });
}

/* 弹原生文件夹选择窗口，返回所选目录(取消返回 null)。 */
export async function pickFolder(defaultPath?: string): Promise<string | null> {
  if (!inTauri()) {
    return null;
  }
  return invoke<string | null>("pick_folder", { defaultPath });
}

/* 弹原生文件选择窗口，返回所选文件(取消返回 null)。 */
export async function pickFile(defaultPath?: string): Promise<string | null> {
  if (!inTauri()) {
    return null;
  }
  return invoke<string | null>("pick_file", { defaultPath });
}

/* 打开目录或在资源管理器中定位文件。 */
export async function openPath(path: string): Promise<void> {
  if (!inTauri()) {
    throw new Error("当前不是桌面模式，无法打开本地路径");
  }

  await invoke("open_path", { path });
}

/* 列出某个 target 的 skills 目录里实际存在的技能及其磁盘状态(链接/复制/失效)。 */
export async function listTargetSkills(target: string, config?: SyncConfig): Promise<TargetSkill[]> {
  if (!inTauri()) {
    return [];
  }
  return invoke<TargetSkill[]>("list_target_skills", { target, config });
}

/* 扫描给定源技能目录里有哪些可选技能（dirs = 当前源端点的 skills_dirs）。 */
export async function getAvailableSkills(dirs: string[], config?: SyncConfig): Promise<AvailableSkillOption[]> {
  if (!inTauri()) {
    return [
      {
        name: "ui-expert",
        display_name: "ui-expert",
        description: "前端界面、交互细节、响应式布局和视觉一致性检查。",
        paths: ["tools/AI/claude/skills/ui-expert"]
      },
      {
        name: "battle-expert",
        display_name: "battle-expert",
        description: "战斗逻辑、录像回放、结算链路和战斗相关问题排查。",
        paths: ["tools/AI/claude/skills/battle-expert"]
      },
      {
        name: "demo-skill-1",
        display_name: "demo-skill-1",
        description: "示例技能，用于非桌面模式下预览技能说明展示效果。",
        paths: ["tools/AI/claude/skills/demo-skill-1"]
      },
      {
        name: "demo-skill-2",
        display_name: "demo-skill-2",
        description: "另一个示例技能，模拟较长描述在列表中的截断效果。",
        paths: ["tools/AI/claude/skills/demo-skill-2"]
      }
    ];
  }

  return invoke<AvailableSkillOption[]>("list_available_skills", { dirs, config });
}

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
export async function applyUpdate(): Promise<void> {
  await invoke("apply_update");
}

/* 监听下载进度(0-100)。返回取消监听的函数。 */
export async function onUpdateProgress(handler: (pct: number) => void): Promise<() => void> {
  if (!inTauri()) {
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<number>("update-progress", (event) => handler(event.payload));
}
