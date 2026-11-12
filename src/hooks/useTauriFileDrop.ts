import { useEffect } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Tauri 2 只在桌面壳里注入 __TAURI_INTERNALS__；纯浏览器预览下直接跳过。 */
function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

export function useTauriFileDrop(
  onDrop: (paths: string[]) => void,
  onHover?: (active: boolean) => void,
) {
  useEffect(() => {
    // 浏览器预览 / SSR 环境没有 Tauri 注入，注册监听会同步抛错。
    if (!isTauri()) return;

    let unlisten: (() => void) | undefined;
    let cancelled = false;

    try {
      getCurrentWebviewWindow()
        .onDragDropEvent((event) => {
          const { type } = event.payload;
          if (type === "over" || type === "enter") {
            onHover?.(true);
          } else if (type === "leave") {
            onHover?.(false);
          } else if (type === "drop") {
            onHover?.(false);
            onDrop(event.payload.paths);
          }
        })
        .then((fn) => {
          if (cancelled) fn();
          else unlisten = fn;
        })
        .catch(console.error);
    } catch (error) {
      // 即便检测通过，注册失败也不应让整个界面崩溃。
      console.warn("拖拽监听注册失败：", error);
    }

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onDrop, onHover]);
}
