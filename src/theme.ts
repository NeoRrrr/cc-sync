export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "cc-sync-theme";

export function loadTheme(): Theme {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") {
      return raw;
    }
  } catch {
    // localStorage 不可用时退回默认
  }
  return "system";
}

export function saveTheme(theme: Theme): void {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // 忽略写入失败
  }
}

/* 把 "system" 解析为实际的 light/dark。 */
export function resolveTheme(theme: Theme): "light" | "dark" {
  if (theme === "system") {
    const prefersDark =
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches;
    return prefersDark ? "dark" : "light";
  }
  return theme;
}

/* 把主题写到 <html data-theme>，CSS 据此切换变量。 */
export function applyTheme(theme: Theme): void {
  if (typeof document === "undefined") {
    return;
  }
  document.documentElement.dataset.theme = resolveTheme(theme);
}

/* 当主题为 system 时订阅系统配色变化；返回取消订阅函数。 */
export function watchSystemTheme(theme: Theme, onChange: () => void): () => void {
  if (theme !== "system" || typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return () => {};
  }
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}
