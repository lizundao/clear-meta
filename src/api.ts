import { invoke } from "@tauri-apps/api/core";
import { open as dialogOpen } from "@tauri-apps/plugin-dialog";
import { isTauri, requireTauri } from "./utils/tauriEnv";
import type {
  ApplyOptions,
  MetadataPreview,
  ProcessResult,
  SupportedFormats,
} from "./types";

export async function getSupportedFormats(): Promise<SupportedFormats> {
  requireTauri();
  return invoke("get_supported_formats");
}

export async function applyMetadata(
  paths: string[],
  options: ApplyOptions,
): Promise<ProcessResult[]> {
  requireTauri();
  return invoke("apply_metadata", { paths, options });
}

export async function clearMetadata(
  paths: string[],
  options: ApplyOptions,
): Promise<ProcessResult[]> {
  requireTauri();
  return invoke("clear_metadata", { paths, options });
}

export async function isSupported(path: string): Promise<boolean> {
  requireTauri();
  return invoke("is_supported", { path });
}

export async function readMetadata(paths: string[]): Promise<MetadataPreview[]> {
  requireTauri();
  return invoke("read_metadata", { paths });
}

export async function scanFolder(
  path: string,
  extensions: string[],
): Promise<string[]> {
  requireTauri();
  return invoke("scan_folder_files", { path, extensions });
}

/** 登记用户主动选择的文件；只有登记过的文件才允许后端读写。 */
export async function registerPaths(paths: string[]): Promise<number> {
  requireTauri();
  return invoke("register_paths", { paths });
}

/** 从备份恢复一个被覆盖的文件。 */
export async function restoreFromBackup(
  backupPath: string,
  targetPath: string,
): Promise<void> {
  requireTauri();
  await invoke("restore_from_backup", { backupPath, targetPath });
}

/** 报告写入应用数据目录，返回实际保存路径。 */
export async function saveReport(
  fileName: string,
  content: string,
): Promise<string> {
  requireTauri();
  return invoke("save_report", { fileName, content });
}

// —— 对话框封装：浏览器预览下返回 null，不抛底层错误 —— //

export async function pickFilesDialog(
  label: string,
  extensions: string[],
): Promise<string[] | null> {
  if (!isTauri()) return null;
  const selected = await dialogOpen({
    multiple: true,
    filters: [{ name: label, extensions: [...extensions] }],
  });
  if (!selected) return null;
  return Array.isArray(selected) ? selected : [selected];
}

export async function pickDirectoryDialog(): Promise<string | null> {
  if (!isTauri()) return null;
  const selected = await dialogOpen({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}
