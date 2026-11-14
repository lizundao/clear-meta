use crate::apply::{self, ApplyOptions, FieldRule};
use crate::error::{ClearMetaError, Result};
use crate::image;
use crate::metadata::{self, MetadataField};
use crate::office;
use crate::pdf;
use crate::safety;
use serde::Serialize;
use std::path::{Path, PathBuf};

const PDF_EXTENSIONS: &[&str] = &["pdf"];
const OFFICE_EXTENSIONS: &[&str] = &["docx", "xlsx", "pptx", "docm", "xlsm", "pptm"];
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

#[derive(Debug, Clone, Serialize)]
pub struct SupportedFormats {
    pub pdf: Vec<String>,
    pub office: Vec<String>,
    pub image: Vec<String>,
}

impl Default for SupportedFormats {
    fn default() -> Self {
        Self {
            pdf: PDF_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            office: OFFICE_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            image: IMAGE_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

pub fn is_supported_path(path: &str) -> bool {
    extension_of(path)
        .map(|ext| is_pdf_ext(&ext) || is_office_ext(&ext) || is_image_ext(&ext))
        .unwrap_or(false)
}

pub fn scan_folder(dir: &Path, extensions: &[String]) -> Result<Vec<String>> {
    if !dir.is_dir() {
        return Err(ClearMetaError::FileNotFound(format!(
            "目录不存在: {}",
            dir.display()
        )));
    }

    let allowed: Vec<String> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();
    let mut files = Vec::new();
    let mut truncated = false;

    // 限制深度与条数；单个不可读子目录跳过而非丢弃整批结果。
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .max_depth(safety::MAX_SCAN_DEPTH)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if files.len() >= safety::MAX_SCAN_ENTRIES {
            truncated = true;
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if allowed.contains(&ext) && is_supported_path(&path.display().to_string()) {
                files.push(path.display().to_string());
            }
        }
    }

    files.sort();
    if truncated {
        // 已收集的部分仍然返回，前端会提示分批；不静默丢弃。
        eprintln!("目录扫描达到 {} 条上限，结果已截断", safety::MAX_SCAN_ENTRIES);
    }
    Ok(files)
}

pub fn read_file_metadata(input: &Path) -> Result<Vec<MetadataField>> {
    if !input.exists() {
        return Err(ClearMetaError::FileNotFound(
            input.display().to_string(),
        ));
    }

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // 文件属性（资源管理器「属性」里的那套时间）放最前面展示：
    // 用户核对时间时第一反应就是右键属性，两套时间摆在一起才对得上。
    let mut fields = read_fs_attributes(input);
    let meta_fields = if is_pdf_ext(&ext) {
        pdf::read_pdf_metadata(input)
    } else if is_office_ext(&ext) {
        office::read_office_metadata(input)
    } else if is_image_ext(&ext) {
        image::read_image_metadata(input)
    } else {
        Err(ClearMetaError::UnsupportedFormat(ext))
    };
    fields.extend(meta_fields?);
    Ok(fields)
}

/// 读取文件系统层面的创建 / 修改 / 访问时间。
/// 这些时间不属于文档元数据：不会被清除规则触碰，也全部标为只读，
/// 编辑器（editable = false）与规则引擎都会跳过。
pub fn read_fs_attributes(path: &Path) -> Vec<MetadataField> {
    let mut fields = Vec::new();
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return fields,
    };

    if let Ok(created) = meta.created() {
        metadata::push_field_readonly(
            &mut fields,
            "fs.created",
            FS_GROUP,
            "创建时间",
            format_system_time(created),
        );
    }
    if let Ok(modified) = meta.modified() {
        metadata::push_field_readonly(
            &mut fields,
            "fs.modified",
            FS_GROUP,
            "修改时间",
            format_system_time(modified),
        );
    }
    if let Ok(accessed) = meta.accessed() {
        metadata::push_field_readonly(
            &mut fields,
            "fs.accessed",
            FS_GROUP,
            "访问时间",
            format_system_time(accessed),
        );
    }
    fields
}

/// 文件属性分组名，前端据此在对比表 / 统计中排除。
pub const FS_GROUP: &str = "文件属性";

fn format_system_time(t: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn apply_file(input: &Path, output: &Path, rules: &[FieldRule], clear_all: bool) -> Result<()> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // 原地覆盖时保留原文件时间戳：处理动作本身不应留在资源管理器的
    // 修改时间 / 创建时间里。还原只在覆盖成功后进行，失败仅记录不阻断。
    let in_place = input == output;
    let times = if in_place {
        safety::capture_times(input).ok()
    } else {
        None
    };

    let result = if is_pdf_ext(&ext) {
        pdf::apply_pdf(input, output, rules, clear_all)
    } else if is_office_ext(&ext) {
        office::apply_office(input, output, rules, clear_all)
    } else if is_image_ext(&ext) {
        image::apply_image(input, output, rules, clear_all)
    } else {
        Err(ClearMetaError::UnsupportedFormat(ext))
    };

    if let (true, Some(t)) = (in_place, times) {
        if result.is_ok() {
            if let Err(err) = safety::restore_times(output, &t) {
                eprintln!("时间戳还原失败 {}: {}", output.display(), err);
            }
        }
    }

    result
}

pub fn resolve_output_path(input: &Path, options: &ApplyOptions) -> Result<PathBuf> {
    match options.output_mode {
        crate::OutputMode::InPlace => Ok(input.to_path_buf()),
        crate::OutputMode::Copy => {
            let dir = options
                .output_dir
                .as_ref()
                .ok_or_else(|| ClearMetaError::Office("未指定输出目录".into()))?;
            let dir_path = PathBuf::from(dir);
            if !dir_path.is_dir() {
                return Err(ClearMetaError::FileNotFound(format!(
                    "输出目录不存在: {dir}"
                )));
            }
            let file_name = input
                .file_name()
                .ok_or_else(|| ClearMetaError::Office("无效的文件路径".into()))?;
            // 目标已存在时自动加序号，绝不静默覆盖用户已有文件。
            Ok(safety::unique_output_path(&dir_path, file_name))
        }
    }
}

pub fn apply_options_clear_all(options: &ApplyOptions) -> bool {
    apply::should_clear_all(&options.rules, options.clear_all)
}

fn extension_of(path: &str) -> Option<String> {
    PathBuf::from(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

fn is_pdf_ext(ext: &str) -> bool {
    PDF_EXTENSIONS.contains(&ext)
}

fn is_office_ext(ext: &str) -> bool {
    OFFICE_EXTENSIONS.contains(&ext)
}

fn is_image_ext(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext)
}
