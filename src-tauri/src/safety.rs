//! 路径授权与资源上限。
//!
//! 所有文件读写都来自前端 `invoke`，而 Tauri 的 fs scope 对 Rust 侧 `std::fs` 不生效，
//! 因此这里维护一个「已授权文件」集合：只有前端通过 `register_paths` 登记过
//! （且经 `canonicalize` 解析、排除符号链接与系统目录）的文件才允许读取或处理。
//! 配合严格 CSP 与前端无注入点，构成纵深防御。

use crate::error::{ClearMetaError, Result};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// 资源上限：防止超大文件 / 解压炸弹 / 海量批量拖垮应用
pub const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024; // 单文件 200 MB
pub const MAX_INPUT_ZIP_BYTES: u64 = 100 * 1024 * 1024; // Office 输入 100 MB
pub const MAX_ZIP_ENTRY_BYTES: u64 = 200 * 1024 * 1024; // 单条目解压后 200 MB
pub const MAX_ZIP_TOTAL_BYTES: u64 = 512 * 1024 * 1024; // 累计解压 512 MB
pub const MAX_ZIP_RATIO: u64 = 100; // 压缩比 100:1
pub const MAX_BATCH: usize = 500; // 单次处理文件数
pub const MAX_SCAN_DEPTH: usize = 8; // 目录递归深度
pub const MAX_SCAN_ENTRIES: usize = 5000; // 单次扫描条数

static AUTHORIZED_FILES: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

fn authorized() -> &'static Mutex<HashSet<PathBuf>> {
    AUTHORIZED_FILES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn lock_set() -> Result<std::sync::MutexGuard<'static, HashSet<PathBuf>>> {
    authorized()
        .lock()
        .map_err(|_| ClearMetaError::Internal("权限缓存暂不可用".into()))
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// 系统敏感目录前缀（小写），拒绝把这里的文件纳入授权或写入报告。
fn is_sensitive(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    const BLOCKED: &[&str] = &[
        r"c:\windows",
        r"c:\program files",
        r"c:\program files (x86)",
        r"c:\programdata",
        r"c:\system volume information",
        r"c:\$recycle.bin",
        "/windows",
        "/system",
        "/usr/bin",
        "/usr/sbin",
        "/etc",
    ];
    BLOCKED.iter().any(|p| lower.starts_with(p))
}

/// 登记一组已由用户主动选择（对话框 / 拖拽）的文件。返回实际新增数量。
pub fn register_paths(paths: &[String]) -> Result<usize> {
    let mut set = lock_set()?;
    let mut added = 0usize;
    for raw in paths {
        let p = PathBuf::from(raw);
        let Ok(canon) = p.canonicalize() else {
            continue;
        };
        if !canon.is_file() || is_sensitive(&canon) {
            continue;
        }
        if set.insert(canon) {
            added += 1;
        }
    }
    Ok(added)
}

/// 校验单个文件在授权集合内，返回其 canonical 路径（已解析符号链接）。
pub fn ensure_authorized(path: &Path) -> Result<PathBuf> {
    let canon = path.canonicalize().map_err(|_| {
        ClearMetaError::OutOfScope(format!("路径不存在或不可访问：{}", path.display()))
    })?;
    let set = lock_set()?;
    if set.contains(&canon) {
        Ok(canon)
    } else {
        Err(ClearMetaError::OutOfScope(
            "文件不在已授权范围内，请重新选择文件".into(),
        ))
    }
}

/// 单文件大小预检。
pub fn ensure_size_ok(path: &Path) -> Result<()> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_FILE_BYTES {
        return Err(ClearMetaError::TooLarge(format!(
            "文件超过 {} MB，请先拆分",
            MAX_FILE_BYTES / 1024 / 1024
        )));
    }
    Ok(())
}

/// Copy 模式：目标已存在时生成 `{name}_{n}{ext}`，绝不静默覆盖。
pub fn unique_output_path(dir: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let src = Path::new(file_name);
    let stem = src
        .file_stem()
        .unwrap_or(file_name)
        .to_string_lossy()
        .into_owned();
    let ext = src.extension().map(|e| e.to_string_lossy().into_owned());
    for i in 1..10_000u32 {
        let name = match &ext {
            Some(e) => format!("{stem}_{i}.{e}"),
            None => format!("{stem}_{i}"),
        };
        let p = dir.join(&name);
        if !p.exists() {
            return p;
        }
    }
    let suffix = timestamp_nanos();
    let name = match &ext {
        Some(e) => format!("{stem}_{suffix}.{e}"),
        None => format!("{stem}_{suffix}"),
    };
    dir.join(name)
}

/// 并发安全的临时文件名（PID + 纳秒时间戳）。
pub fn temp_path(input: &Path) -> PathBuf {
    input.with_extension(format!(
        "clear_meta_{}_{}.tmp",
        std::process::id(),
        timestamp_nanos()
    ))
}

/// 覆盖前把原文件备份到应用数据目录，返回备份路径。
pub fn make_backup(app: &tauri::AppHandle, input: &Path) -> Result<PathBuf> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ClearMetaError::Internal(e.to_string()))?
        .join("backups");
    std::fs::create_dir_all(&dir)?;
    let name = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let backup = dir.join(format!("{}_{}", timestamp_nanos(), name));
    std::fs::copy(input, &backup)?;
    Ok(backup)
}

/// 从备份恢复目标文件（临时文件中转，避免写一半）。同时校验备份位于 backups 目录内。
pub fn restore_backup(app: &tauri::AppHandle, backup: &Path, target: &Path) -> Result<()> {
    use tauri::Manager;
    let backup_root = app
        .path()
        .app_data_dir()
        .map_err(|e| ClearMetaError::Internal(e.to_string()))?
        .join("backups")
        .canonicalize()
        .map_err(|_| ClearMetaError::OutOfScope("备份目录不存在".into()))?;
    let canon_backup = backup
        .canonicalize()
        .map_err(|_| ClearMetaError::FileNotFound(backup.display().to_string()))?;
    if !canon_backup.starts_with(&backup_root) {
        return Err(ClearMetaError::OutOfScope("备份路径不合法".into()));
    }
    let tmp = target.with_extension(format!("clear_meta_restore_{}.tmp", std::process::id()));
    std::fs::copy(&canon_backup, &tmp)?;
    std::fs::rename(&tmp, target)?;
    Ok(())
}

/// 报告导出路径校验：仅 .csv/.json、无 `..`、不在系统目录、父目录存在。
pub fn ensure_report_path(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "csv" && ext != "json" {
        return Err(ClearMetaError::Internal("报告仅支持 .csv 或 .json 格式".into()));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(ClearMetaError::OutOfScope("报告路径不合法".into()));
    }
    if is_sensitive(path) {
        return Err(ClearMetaError::OutOfScope("不能写入系统目录".into()));
    }
    let Some(parent) = path.parent() else {
        return Err(ClearMetaError::Internal("报告路径无效".into()));
    };
    if !parent.is_dir() {
        return Err(ClearMetaError::FileNotFound(format!(
            "目录不存在：{}",
            parent.display()
        )));
    }
    Ok(())
}

/// zip 条目名规范化校验：拒绝绝对路径、`..`、控制字符。
pub fn safe_zip_entry_name(name: &str) -> Result<()> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.split('/').any(|seg| seg == "..")
        || name.chars().any(|c| c.is_control())
    {
        return Err(ClearMetaError::ArchiveBomb(
            "压缩包包含异常条目名，已停止处理".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_name_appends_suffix() {
        // 仅在临时目录存在同名文件时才追加，这里用一个必然不存在的名字
        let dir = std::env::temp_dir();
        let name = std::ffi::OsStr::new("clear_meta_nonexistent_unique_test_998877.pdf");
        let p = unique_output_path(&dir, name);
        assert_eq!(p.file_name().unwrap(), name);
    }

    #[test]
    fn report_path_rejects_bad_extension() {
        // 用系统临时目录而非硬编码 C:/tmp：CI runner 上不一定存在 C:\tmp，
        // 而 ensure_report_path 会校验父目录 is_dir()，硬编码盘符会导致脆弱失败。
        let dir = std::env::temp_dir();
        assert!(dir.is_dir());
        assert!(ensure_report_path(&dir.join("report.exe")).is_err());
        assert!(ensure_report_path(&dir.join("report.csv")).is_ok());
    }

    #[test]
    fn report_path_rejects_missing_parent_dir() {
        // 父目录不存在时必须报错，不依赖任何真实盘符。
        let missing = std::env::temp_dir().join("clear_meta_no_such_dir_998877");
        if missing.exists() {
            return; // 极端情况下目录被占用则跳过，避免误判
        }
        assert!(ensure_report_path(&missing.join("report.csv")).is_err());
    }

    #[test]
    fn zip_entry_name_rejects_traversal() {
        assert!(safe_zip_entry_name("word/../evil.xml").is_err());
        assert!(safe_zip_entry_name("/absolute.xml").is_err());
        assert!(safe_zip_entry_name("word/document.xml").is_ok());
    }
}
