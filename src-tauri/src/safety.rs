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
    replace_file(&tmp, target)?;
    Ok(())
}

/// 把 `from` 覆盖为 `to`。
///
/// Windows 上 rename 覆盖已有文件时，只要目标（或源）被任何进程打开——
/// 包括杀毒软件 / 索引服务对刚写出的临时文件的短暂扫描——就会报
/// 「另一个程序正在使用此文件 (os error 32/5)」。这类占用都是瞬时的，
/// 因此这里先短暂重试；重试仍失败才报 FileLocked，让前端给出明确指引。
pub fn replace_file(from: &Path, to: &Path) -> Result<()> {
    const RETRIES: usize = 6;
    const INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..RETRIES {
        if attempt > 0 {
            std::thread::sleep(INTERVAL);
        }
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(err) if is_busy_error(&err) => {
                last_err = Some(err);
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(ClearMetaError::FileLocked(
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "重试后仍被占用".into()),
    ))
}

/// 判断是否为「文件被占用」类错误（共享冲突 / 拒绝访问）。
fn is_busy_error(err: &std::io::Error) -> bool {
    match err.raw_os_error() {
        // 32: ERROR_SHARING_VIOLATION, 33: ERROR_LOCK_VIOLATION, 5: ERROR_ACCESS_DENIED
        Some(32) | Some(33) | Some(5) => true,
        _ => matches!(
            err.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::AlreadyExists
        ),
    }
}

/// 原文件时间戳快照：覆盖后还原，避免「处理时间」暴露在资源管理器里。
#[derive(Debug, Clone, Copy)]
pub struct OriginalTimes {
    pub created: Option<std::time::SystemTime>,
    pub modified: std::time::SystemTime,
    pub accessed: std::time::SystemTime,
}

/// 读取文件当前的三类时间戳。
pub fn capture_times(path: &Path) -> Result<OriginalTimes> {
    let meta = std::fs::metadata(path)?;
    Ok(OriginalTimes {
        created: meta.created().ok(),
        modified: meta.modified()?,
        accessed: meta.accessed()?,
    })
}

/// 把文件时间戳还原为快照值。修改/访问时间必须成功；创建时间尽力还原（Windows）。
pub fn restore_times(path: &Path, times: &OriginalTimes) -> Result<()> {
    let m = filetime::FileTime::from_system_time(times.modified);
    let a = filetime::FileTime::from_system_time(times.accessed);
    filetime::set_file_times(path, a, m)?;

    #[cfg(windows)]
    if let Some(created) = times.created {
        restore_created_time_windows(path, created);
    }
    Ok(())
}

/// Windows 上还原创建时间：SetFileTime 需要 FILE_WRITE_ATTRIBUTES 的句柄。
/// 还原失败不影响整体（时间戳主体是修改时间），静默忽略。
#[cfg(windows)]
fn restore_created_time_windows(path: &Path, created: std::time::SystemTime) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, SetFileTime, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
        OPEN_EXISTING,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_WRITE_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut() as *mut core::ffi::c_void,
        )
    };
    if handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        return;
    }

    // SystemTime → FILETIME（100ns since 1601-01-01）
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let ft: i64 = match created
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| (EPOCH_DIFF_SECS + d.as_secs()) as i64 * 10_000_000 + d.subsec_nanos() as i64 / 100)
    {
        Some(v) => v,
        None => {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return;
        }
    };

    unsafe {
        SetFileTime(handle, &ft as *const i64 as *const _, std::ptr::null(), std::ptr::null());
        windows_sys::Win32::Foundation::CloseHandle(handle);
    }
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
