/** Tauri 2 只在桌面壳里注入 __TAURI_INTERNALS__；纯浏览器预览下返回 false。 */
export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

/** 浏览器预览下给出明确提示，避免 invoke / dialog 同步抛底层错误。 */
export function requireTauri(): void {
  if (!isTauri()) {
    throw new Error("此操作需在桌面应用中运行，浏览器预览模式下不可用。");
  }
}
