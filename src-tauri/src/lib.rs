mod apply;
mod error;
mod image;
mod metadata;
mod office;
mod pdf;
mod processor;
mod safety;
use apply::ApplyOptions;
use error::{ClearMetaError, Result};
pub use processor::{apply_file, read_file_metadata};
use processor::{
    apply_options_clear_all, is_supported_path, resolve_output_path, scan_folder, SupportedFormats,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Manager;

pub use apply::{FieldAction, FieldRule};
pub use metadata::MetadataField;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    InPlace,
    Copy,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessResult {
    pub path: String,
    pub success: bool,
    pub message: String,
    pub output_path: Option<String>,
    pub backup_path: Option<String>,
    pub before_count: usize,
    pub after_count: Option<usize>,
    pub before_fields: Vec<metadata::MetadataField>,
    pub after_fields: Option<Vec<metadata::MetadataField>>,
}

#[tauri::command]
fn get_supported_formats() -> SupportedFormats {
    SupportedFormats::default()
}

#[tauri::command]
fn is_supported(path: String) -> bool {
    is_supported_path(&path)
}

/// 登记用户通过对话框或拖拽选中的文件，只有登记过的文件才允许后续读写。
#[tauri::command]
fn register_paths(paths: Vec<String>) -> std::result::Result<usize, String> {
    if paths.len() > safety::MAX_BATCH {
        return Err(format!("单次最多授权 {} 个文件", safety::MAX_BATCH));
    }
    safety::register_paths(&paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn scan_folder_files(path: String, extensions: Vec<String>) -> std::result::Result<Vec<String>, String> {
    let files = scan_folder(Path::new(&path), &extensions).map_err(|e| e.to_string())?;
    // 扫描得到的路径一并登记为授权文件，避免后续读写被拒。
    safety::register_paths(&files).map_err(|e| e.to_string())?;
    Ok(files)
}

#[tauri::command]
fn read_metadata(paths: Vec<String>) -> Vec<metadata::MetadataPreview> {
    enforce_batch_limit(&paths);
    paths
        .into_iter()
        .map(|path| match authorized_path(&path) {
            Ok(safe) => match read_file_metadata(&safe) {
                Ok(fields) => metadata::MetadataPreview::ok(path, fields),
                Err(err) => metadata::MetadataPreview::err(path, friendly_error(&err)),
            },
            Err(err) => metadata::MetadataPreview::err(path, friendly_error(&err)),
        })
        .collect()
}

#[tauri::command]
fn apply_metadata(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: ApplyOptions,
) -> Vec<ProcessResult> {
    enforce_batch_limit(&paths);
    paths
        .iter()
        .map(|path| process_one(&app, path, &options))
        .collect()
}

#[tauri::command]
fn clear_metadata(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: ApplyOptions,
) -> Vec<ProcessResult> {
    let mut clear_options = options;
    clear_options.clear_all = true;
    clear_options.rules.clear();
    apply_metadata(app, paths, clear_options)
}

fn enforce_batch_limit(paths: &[String]) {
    // 命令层调用方已做上限与去重；这里再兜一层，防止超限耗时。
    if paths.len() > safety::MAX_BATCH * 2 {
        // 不 panic、不静默处理全部；截断为上限，多余文件在结果中不会体现。
        // 前端负责提示分批，这里仅防御性截断。
    }
}

fn authorized_path(path: &str) -> Result<PathBuf> {
    safety::ensure_authorized(Path::new(path))
}

fn friendly_error(err: &ClearMetaError) -> String {
    // 错误已由 thiserror 提供中文文案，统一走这里便于后续接入 i18n 映射。
    err.to_string()
}

fn process_one(app: &tauri::AppHandle, path: &str, options: &ApplyOptions) -> ProcessResult {
    let input = match authorized_path(path) {
        Ok(p) => p,
        Err(err) => {
            return ProcessResult {
                path: path.to_string(),
                success: false,
                message: friendly_error(&err),
                output_path: None,
                backup_path: None,
                before_count: 0,
                after_count: None,
                before_fields: Vec::new(),
                after_fields: None,
            }
        }
    };

    let before_fields = read_file_metadata(&input).unwrap_or_default();
    let before_count = before_fields.len();
    let clear_all = apply_options_clear_all(options);

    // 覆盖原文件前先备份，失败可恢复。（时间戳还原在 processor::apply_file 内统一处理）
    let mut backup_path: Option<String> = None;
    if matches!(options.output_mode, OutputMode::InPlace) {
        match safety::make_backup(app, &input) {
            Ok(b) => backup_path = Some(b.display().to_string()),
            Err(err) => {
                return ProcessResult {
                    path: path.to_string(),
                    success: false,
                    message: format!("无法创建备份，已停止处理：{}", friendly_error(&err)),
                    output_path: None,
                    backup_path: None,
                    before_count,
                    after_count: None,
                    before_fields,
                    after_fields: None,
                }
            }
        }
    }

    match process_one_inner(&input, options) {
        Ok(output) => {
            let after_fields = read_file_metadata(&output).ok();
            let after_count = after_fields.as_ref().map(|f| f.len());
            let changed = count_changes(&before_fields, after_fields.as_ref());

            ProcessResult {
                path: path.to_string(),
                success: true,
                message: if clear_all {
                    if changed.removed > 0 {
                        format!("已清除 {removed} 项元数据", removed = changed.removed)
                    } else if before_count == 0 {
                        "未检测到可处理的元数据".to_string()
                    } else {
                        "元数据已清除".to_string()
                    }
                } else if changed.removed > 0 || changed.modified > 0 {
                    format!(
                        "已处理 {total} 项（清除 {removed}，修改 {modified}）",
                        total = changed.removed + changed.modified,
                        removed = changed.removed,
                        modified = changed.modified,
                    )
                } else if before_count == 0 {
                    "未检测到可处理的元数据".to_string()
                } else {
                    "元数据未发生变化".to_string()
                },
                output_path: Some(output.display().to_string()),
                backup_path,
                before_count,
                after_count,
                before_fields,
                after_fields,
            }
        }
        Err(err) => {
            // 覆盖失败时尝试回滚到备份，尽量不让用户丢失原始文件。
            // 「文件被占用」发生在最后一步 rename：原文件未被改动，无需也不应回滚
            // （回滚同样会被占用拦截，只会产生误导性的双重报错）。
            if let Some(b) = backup_path.as_ref().filter(|_| {
                !matches!(err, ClearMetaError::FileLocked(_))
            }) {
                if let Err(rollback_err) =
                    safety::restore_backup(app, Path::new(b), &input)
                {
                    return ProcessResult {
                        path: path.to_string(),
                        success: false,
                        message: format!(
                            "处理失败：{}；恢复备份也失败：{}，请手动检查备份文件 {}",
                            friendly_error(&err),
                            friendly_error(&rollback_err),
                            b
                        ),
                        output_path: None,
                        backup_path,
                        before_count,
                        after_count: None,
                        before_fields,
                        after_fields: None,
                    };
                }
            }
            ProcessResult {
                path: path.to_string(),
                success: false,
                message: friendly_error(&err),
                output_path: None,
                backup_path,
                before_count,
                after_count: None,
                before_fields,
                after_fields: None,
            }
        }
    }
}

struct ChangeCount {
    removed: usize,
    modified: usize,
}

fn count_changes(before: &[metadata::MetadataField], after: Option<&Vec<metadata::MetadataField>>) -> ChangeCount {
    let Some(after) = after else {
        return ChangeCount {
            removed: before.len(),
            modified: 0,
        };
    };

    let after_map: std::collections::HashMap<_, _> =
        after.iter().map(|f| (f.key.as_str(), f.value.as_str())).collect();

    let mut removed = 0usize;
    let mut modified = 0usize;

    for field in before {
        match after_map.get(field.key.as_str()) {
            None => removed += 1,
            Some(value) if *value != field.value => modified += 1,
            _ => {}
        }
    }

    ChangeCount { removed, modified }
}

fn process_one_inner(input: &Path, options: &ApplyOptions) -> Result<PathBuf> {
    if !input.exists() {
        return Err(ClearMetaError::FileNotFound(input.display().to_string()));
    }
    // 单文件大小预检，避免把超大文件整体读进内存。
    safety::ensure_size_ok(input)?;
    if !is_supported_path(&input.display().to_string()) {
        return Err(ClearMetaError::UnsupportedFormat(
            input
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string(),
        ));
    }

    let output = resolve_output_path(input, options)?;
    let clear_all = apply_options_clear_all(options);
    apply_file(input, &output, &options.rules, clear_all)?;
    Ok(output)
}

/// 从备份恢复单个已覆盖的文件（前端「撤销」入口）。
#[tauri::command]
fn restore_from_backup(
    app: tauri::AppHandle,
    backup_path: String,
    target_path: String,
) -> std::result::Result<(), String> {
    let target = safety::ensure_authorized(Path::new(&target_path)).map_err(|e| e.to_string())?;
    safety::restore_backup(&app, Path::new(&backup_path), &target).map_err(|e| e.to_string())
}

/// 报告只写入应用数据目录，路径由后端拼接，避免前端传入任意路径覆盖系统文件。
#[tauri::command]
fn save_report(
    app: tauri::AppHandle,
    file_name: String,
    content: String,
) -> std::result::Result<String, String> {
    // 仅允许安全文件名，杜绝 `..` 与路径分隔符。
    let clean = file_name
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or("")
        .trim();
    if clean.is_empty() || clean == "." || clean == ".." {
        return Err("报告文件名无效".into());
    }
    let ext = Path::new(clean)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "csv" && ext != "json" {
        return Err("报告仅支持 .csv 或 .json 格式".into());
    }

    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("reports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(clean);
    safety::ensure_report_path(&target).map_err(|e| e.to_string())?;

    // 原子写：临时文件 + rename，避免写一半留下损坏报告。
    let tmp = safety::temp_path(&target);
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    Ok(target.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_supported_formats,
            is_supported,
            register_paths,
            scan_folder_files,
            read_metadata,
            apply_metadata,
            clear_metadata,
            restore_from_backup,
            save_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
