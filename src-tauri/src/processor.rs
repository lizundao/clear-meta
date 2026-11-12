use crate::apply::{self, ApplyOptions, FieldRule};
use crate::error::{ClearMetaError, Result};
use crate::image;
use crate::metadata::MetadataField;
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

    if is_pdf_ext(&ext) {
        pdf::read_pdf_metadata(input)
    } else if is_office_ext(&ext) {
        office::read_office_metadata(input)
    } else if is_image_ext(&ext) {
        image::read_image_metadata(input)
    } else {
        Err(ClearMetaError::UnsupportedFormat(ext))
    }
}

pub fn apply_file(input: &Path, output: &Path, rules: &[FieldRule], clear_all: bool) -> Result<()> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if is_pdf_ext(&ext) {
        pdf::apply_pdf(input, output, rules, clear_all)
    } else if is_office_ext(&ext) {
        office::apply_office(input, output, rules, clear_all)
    } else if is_image_ext(&ext) {
        image::apply_image(input, output, rules, clear_all)
    } else {
        Err(ClearMetaError::UnsupportedFormat(ext))
    }
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
