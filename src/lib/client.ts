import type { AvailableSkillOption, SyncConfig, SyncPlan, SyncScope, TargetSkill } from "../types";
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

export async function loadConfig(): Promise<{ config: SyncConfig; mode: "desktop" | "demo" }> {
  if (!inTauri()) {
    return { config: structuredClone(mockConfig), mode: "demo" };
  }

  try {
    const config = await invoke<SyncConfig>("load_config_data");
    return { config, mode: "desktop" };
  } catch {
    return { config: structuredClone(mockConfig), mode: "demo" };
  }
}

export async function saveConfig(config: SyncConfig): Promise<void> {
  if (!inTauri()) {
    throw new Error("当前不是桌面模式，无法写入本地配置");
  }

  await invoke("save_config_data", { config });
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

/* 打开目录或在资源管理器中定位文件。 */
export async function openPath(path: string): Promise<void> {
  if (!inTauri()) {
    throw new Error("当前不是桌面模式，无法打开本地路径");
  }

  await invoke("open_path", { path });
}

/* 列出某个 target 的 skills 目录里实际存在的技能及其磁盘状态(链接/复制/失效)。 */
export async function listTargetSkills(target: string): Promise<TargetSkill[]> {
  if (!inTauri()) {
    return [];
  }
  return invoke<TargetSkill[]>("list_target_skills", { target });
}

/* 扫描给定源技能目录里有哪些可选技能（dirs = 当前源端点的 skills_dirs）。 */
export async function getAvailableSkills(dirs: string[]): Promise<AvailableSkillOption[]> {
  if (!inTauri()) {
    return [
      { name: "ui-expert", paths: ["tools/AI/claude/skills/ui-expert"] },
      { name: "battle-expert", paths: ["tools/AI/claude/skills/battle-expert"] },
      { name: "demo-skill-1", paths: ["tools/AI/claude/skills/demo-skill-1"] },
      { name: "demo-skill-2", paths: ["tools/AI/claude/skills/demo-skill-2"] }
    ];
  }

  return invoke<AvailableSkillOption[]>("list_available_skills", { dirs });
}
