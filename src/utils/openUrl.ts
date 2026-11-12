import { openUrl } from "@tauri-apps/plugin-opener";

function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

export async function openExternalUrl(url: string): Promise<void> {
  if (isTauri()) {
    try {
      await openUrl(url);
      return;
    } catch {
      // 插件失败时回退到系统浏览器
    }
  }
  window.open(url, "_blank", "noopener,noreferrer");
}
