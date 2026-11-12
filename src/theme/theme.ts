export type Theme = "dark" | "light";

const STORAGE_KEY = "clear-meta-theme";

export function getInitialTheme(): Theme {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    // localStorage 被禁用或存储损坏时回退默认主题，绝不阻断渲染。
  }
  return "dark";
}

export function applyTheme(theme: Theme) {
  document.documentElement.setAttribute("data-theme", theme);
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // 存储不可用时仅应用当前主题，不影响使用。
  }
}

export function getStoredTheme(): Theme {
  return getInitialTheme();
}
