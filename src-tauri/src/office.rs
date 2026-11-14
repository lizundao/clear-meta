use crate::apply::{resolve_action, FieldRule, ResolvedAction};
use crate::error::{ClearMetaError, Result};
use crate::metadata::{self, MetadataField};
use crate::safety;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// 读取 zip 条目的安全封装：单条解压大小、压缩比、累计总量三重上限，
/// 并校验条目名，防止「解压炸弹」与路径穿越式条目名。
fn read_entry_bounded(
    entry: &mut zip::read::ZipFile<'_>,
    total: &mut u64,
) -> Result<Vec<u8>> {
    let name = entry.name().to_string();
    safety::safe_zip_entry_name(&name)?;

    let uncompressed = entry.size();
    let compressed = entry.compressed_size();
    if uncompressed > safety::MAX_ZIP_ENTRY_BYTES {
        return Err(ClearMetaError::ArchiveBomb(format!(
            "条目 {} 解压后过大",
            name
        )));
    }
    // 仅对可压缩数据检查比例；Stored（不压缩）时跳过，避免误判。
    if entry.compression() != zip::CompressionMethod::Stored && compressed > 0 {
        if uncompressed / compressed > safety::MAX_ZIP_RATIO {
            return Err(ClearMetaError::ArchiveBomb(format!(
                "条目 {} 压缩比异常",
                name
            )));
        }
    }
    *total = total.saturating_add(uncompressed);
    if *total > safety::MAX_ZIP_TOTAL_BYTES {
        return Err(ClearMetaError::ArchiveBomb("压缩包解压总量过大".into()));
    }

    // 预留一个小余量，避免声明大小与实际不符时 read_to_end 无限增长。
    let mut raw = Vec::with_capacity((uncompressed as usize).min(1024 * 1024));
    entry.read_to_end(&mut raw)?;
    if raw.len() as u64 > safety::MAX_ZIP_ENTRY_BYTES {
        return Err(ClearMetaError::ArchiveBomb(format!(
            "条目 {} 实际解压体积超限",
            name
        )));
    }
    Ok(raw)
}

/// 打开 Office 文件前先做输入大小预检。
fn open_archive(path: &Path) -> Result<ZipArchive<Cursor<Vec<u8>>>> {
    let len = std::fs::metadata(path)?.len();
    if len > safety::MAX_INPUT_ZIP_BYTES {
        return Err(ClearMetaError::TooLarge(format!(
            "Office 文件超过 {} MB",
            safety::MAX_INPUT_ZIP_BYTES / 1024 / 1024
        )));
    }
    let data = std::fs::read(path)?;
    ZipArchive::new(Cursor::new(data)).map_err(ClearMetaError::Zip)
}

const CORE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
</cp:coreProperties>"#;

const CUSTOM_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">
</Properties>"#;

const CORE_FIELDS: &[(&str, &str, &str)] = &[
    ("creator", "core:creator", "作者"),
    ("lastModifiedBy", "core:lastModifiedBy", "最后修改者"),
    ("title", "core:title", "标题"),
    ("subject", "core:subject", "主题"),
    ("keywords", "core:keywords", "关键词"),
    ("description", "core:description", "描述"),
    ("category", "core:category", "类别"),
    ("contentStatus", "core:contentStatus", "内容状态"),
    ("revision", "core:revision", "修订号"),
    ("created", "core:created", "创建时间"),
    ("modified", "core:modified", "修改时间"),
];

const APP_FIELDS: &[(&str, &str, &str)] = &[
    ("Application", "app:Application", "应用程序"),
    ("AppVersion", "app:AppVersion", "应用版本"),
    ("Company", "app:Company", "公司"),
    ("Manager", "app:Manager", "管理者"),
    ("Template", "app:Template", "模板"),
    ("TotalTime", "app:TotalTime", "总编辑时间(分钟)"),
    ("Pages", "app:Pages", "页数"),
    ("Words", "app:Words", "字数"),
    ("Characters", "app:Characters", "字符数"),
    ("Lines", "app:Lines", "行数"),
    ("Paragraphs", "app:Paragraphs", "段落数"),
    ("Slides", "app:Slides", "幻灯片数"),
    ("Notes", "app:Notes", "备注页数"),
    ("HiddenSlides", "app:HiddenSlides", "隐藏幻灯片"),
    ("PresentationFormat", "app:PresentationFormat", "演示格式"),
    ("SharedDoc", "app:SharedDoc", "共享文档"),
];

/// core.xml 里属于时间的标签：写回必须用 W3CDTF（UTC），不能用展示文本。
const OFFICE_DATE_TAGS: &[&str] = &["created", "modified"];

pub fn read_office_metadata(path: &Path) -> Result<Vec<MetadataField>> {
    let mut archive = open_archive(path)?;
    let mut fields = Vec::new();
    let mut custom_xml_count = 0usize;
    let mut total: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(ClearMetaError::Zip)?;
        let name = entry.name().to_string();
        let normalized = name.replace('\\', "/");

        if normalized.starts_with("customXml/") || normalized.contains("/customXml/") {
            custom_xml_count += 1;
            continue;
        }

        let raw = read_entry_bounded(&mut entry, &mut total)?;
        let xml = String::from_utf8_lossy(&raw);

        if is_core_properties(&name) {
            for (tag, key, label) in CORE_FIELDS {
                if let Some(value) = metadata::extract_xml_element_text(&xml, tag) {
                    metadata::push_field(&mut fields, key, "文档属性", label, value);
                }
            }
        } else if is_app_properties(&name) {
            for (tag, key, label) in APP_FIELDS {
                if let Some(value) = metadata::extract_xml_element_text(&xml, tag) {
                    metadata::push_field(&mut fields, key, "应用属性", label, value);
                }
            }
        } else if is_custom_properties(&name) {
            for (prop_name, value) in metadata::extract_xml_attr_elements(&xml, "property", "name") {
                let key = format!("custom:{prop_name}");
                metadata::push_field(&mut fields, &key, "自定义属性", &prop_name, value);
            }
        }
    }

    if custom_xml_count > 0 {
        metadata::push_field(
            &mut fields,
            "office:customXml",
            "自定义 XML",
            "嵌入数据",
            format!("{custom_xml_count} 个 customXml 部件"),
        );
    }

    Ok(fields)
}

pub fn apply_office(input: &Path, output: &Path, rules: &[FieldRule], clear_all: bool) -> Result<()> {
    if clear_all {
        return clear_office_all(input, output);
    }

    let mut archive = open_archive(input)?;

    if output != input {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let target = if input == output {
        safety::temp_path(input)
    } else {
        output.to_path_buf()
    };

    let skip_custom_xml = matches!(
        resolve_action("office:customXml", rules),
        ResolvedAction::Remove
    );

    let output_file = File::create(&target)?;
    let mut writer = ZipWriter::new(output_file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut total: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(ClearMetaError::Zip)?;
        let name = entry.name().to_string();

        if skip_custom_xml && should_skip_entry(&name) {
            continue;
        }

        let raw = read_entry_bounded(&mut entry, &mut total)?;

        let content = if is_core_properties(&name) {
            apply_core_xml(&raw, rules)?
        } else if is_app_properties(&name) {
            apply_app_xml(&raw, rules)
        } else if is_custom_properties(&name) {
            apply_custom_xml(&raw, rules)
        } else {
            raw
        };

        writer
            .start_file(name, options)
            .map_err(|e| ClearMetaError::Office(e.to_string()))?;
        writer
            .write_all(&content)
            .map_err(|e| ClearMetaError::Office(e.to_string()))?;
    }

    writer
        .finish()
        .map_err(|e| ClearMetaError::Office(e.to_string()))?;

    if input == output {
        drop(archive);
        safety::replace_file(&target, output)?;
    }

    Ok(())
}

fn clear_office_all(input: &Path, output: &Path) -> Result<()> {
    let mut archive = open_archive(input)?;

    if output != input {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let target = if input == output {
        safety::temp_path(input)
    } else {
        output.to_path_buf()
    };

    let output_file = File::create(&target)?;
    let mut writer = ZipWriter::new(output_file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut total: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(ClearMetaError::Zip)?;
        let name = entry.name().to_string();

        if should_skip_entry(&name) {
            continue;
        }

        let raw = read_entry_bounded(&mut entry, &mut total)?;

        let content = if is_core_properties(&name) {
            CORE_XML.as_bytes().to_vec()
        } else if is_app_properties(&name) {
            sanitize_app_xml(&raw)
        } else if is_custom_properties(&name) {
            CUSTOM_XML.as_bytes().to_vec()
        } else {
            raw
        };

        writer
            .start_file(name, options)
            .map_err(|e| ClearMetaError::Office(e.to_string()))?;
        writer
            .write_all(&content)
            .map_err(|e| ClearMetaError::Office(e.to_string()))?;
    }

    writer
        .finish()
        .map_err(|e| ClearMetaError::Office(e.to_string()))?;

    if input == output {
        drop(archive);
        safety::replace_file(&target, output)?;
    }

    Ok(())
}

fn apply_core_xml(original: &[u8], rules: &[FieldRule]) -> Result<Vec<u8>> {
    let mut xml = String::from_utf8_lossy(original).into_owned();
    if xml.trim().is_empty() {
        xml = CORE_XML.to_string();
    }

    for (tag, key, _) in CORE_FIELDS {
        match resolve_action(key, rules) {
            ResolvedAction::Keep => {}
            ResolvedAction::Remove => {
                xml = metadata::remove_xml_element_by_local_name(&xml, tag);
            }
            ResolvedAction::Set(value) => {
                // created / modified 是 dcterms:W3CDTF（UTC 的 ISO 8601），
                // 写“2026-09-14 09:50:53”这种展示文本会让 Office 识别成无效时间。
                let stored = if OFFICE_DATE_TAGS.contains(tag) {
                    metadata::to_w3cdtf(value)
                        .ok_or_else(|| ClearMetaError::InvalidDate(value.to_string()))?
                } else {
                    value.to_string()
                };
                let escaped = metadata::escape_xml_text(&stored);
                let inner = core_element_xml(tag, &escaped);
                xml = metadata::remove_xml_element_by_local_name(&xml, tag);
                if let Some(pos) = xml.rfind("</cp:coreProperties>") {
                    xml.insert_str(pos, &inner);
                }
            }
        }
    }

    Ok(xml.into_bytes())
}

fn core_element_xml(tag: &str, value: &str) -> String {
    match tag {
        "creator" => format!("<dc:creator>{value}</dc:creator>"),
        "lastModifiedBy" => format!("<cp:lastModifiedBy>{value}</cp:lastModifiedBy>"),
        "title" => format!("<dc:title>{value}</dc:title>"),
        "subject" => format!("<dc:subject>{value}</dc:subject>"),
        "keywords" => format!("<cp:keywords>{value}</cp:keywords>"),
        "description" => format!("<dc:description>{value}</dc:description>"),
        "category" => format!("<cp:category>{value}</cp:category>"),
        "contentStatus" => format!("<cp:contentStatus>{value}</cp:contentStatus>"),
        "revision" => format!("<cp:revision>{value}</cp:revision>"),
        "created" => format!(
            r#"<dcterms:created xsi:type="dcterms:W3CDTF">{value}</dcterms:created>"#
        ),
        "modified" => format!(
            r#"<dcterms:modified xsi:type="dcterms:W3CDTF">{value}</dcterms:modified>"#
        ),
        _ => format!("<{tag}>{value}</{tag}>"),
    }
}

fn apply_app_xml(original: &[u8], rules: &[FieldRule]) -> Vec<u8> {
    let mut xml = String::from_utf8_lossy(original).into_owned();

    for (tag, key, _) in APP_FIELDS {
        match resolve_action(key, rules) {
            ResolvedAction::Keep => {}
            ResolvedAction::Remove => {
                xml = remove_app_element(&xml, tag);
            }
            ResolvedAction::Set(value) => {
                let escaped = metadata::escape_xml_text(value);
                xml = remove_app_element(&xml, tag);
                if let Some(pos) = xml.rfind("</Properties>") {
                    xml.insert_str(pos, &format!("<{tag}>{escaped}</{tag}>"));
                }
            }
        }
    }

    xml.into_bytes()
}

fn apply_custom_xml(original: &[u8], rules: &[FieldRule]) -> Vec<u8> {
    if matches!(resolve_action("custom:*", rules), ResolvedAction::Remove) {
        return CUSTOM_XML.as_bytes().to_vec();
    }

    let xml = String::from_utf8_lossy(original);
    let properties = metadata::extract_xml_attr_elements(&xml, "property", "name");
    if properties.is_empty() {
        return original.to_vec();
    }

    let mut result = xml.to_string();
    for (prop_name, _) in properties {
        let key = format!("custom:{prop_name}");
        match resolve_action(&key, rules) {
            ResolvedAction::Keep => {}
            ResolvedAction::Remove => {
                result = remove_custom_property(&result, &prop_name);
            }
            ResolvedAction::Set(value) => {
                result = set_custom_property(&result, &prop_name, value);
            }
        }
    }

    result.into_bytes()
}

fn remove_custom_property(xml: &str, name: &str) -> String {
    // 属性名来自被处理文件，必须转义，否则含正则元字符时 Regex::new 失败会「原样返回」
    // 导致该敏感属性残留（脱敏不彻底）。
    let escaped_name = regex::escape(name);
    let pattern = format!(
        r#"(?s)<property[^>]*name="{escaped_name}"[^>]*>.*?</property>"#
    );
    match regex::Regex::new(&pattern) {
        Ok(re) => re.replace_all(xml, "").into_owned(),
        // 极端情况下正则仍无法构建，宁可整块清空也不要泄露自定义属性。
        Err(_) => CUSTOM_XML.to_string(),
    }
}

fn set_custom_property(xml: &str, name: &str, value: &str) -> String {
    let mut result = remove_custom_property(xml, name);
    let escaped_name = metadata::escape_xml_text(name);
    let escaped = metadata::escape_xml_text(value);
    let block = format!(
        r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-0800200C9A66}}" pid="0" name="{escaped_name}"><vt:lpwstr>{escaped}</vt:lpwstr></property>"#
    );
    if let Some(pos) = result.rfind("</Properties>") {
        result.insert_str(pos, &block);
    }
    result
}

fn should_skip_entry(name: &str) -> bool {
    let normalized = name.replace('\\', "/");
    normalized.starts_with("customXml/")
        || normalized.contains("/customXml/")
        || normalized.starts_with("docMetadata/")
        || normalized.contains("/docMetadata/")
}

fn is_core_properties(name: &str) -> bool {
    name.replace('\\', "/").ends_with("docProps/core.xml")
}

fn is_app_properties(name: &str) -> bool {
    name.replace('\\', "/").ends_with("docProps/app.xml")
}

fn is_custom_properties(name: &str) -> bool {
    name.replace('\\', "/").ends_with("docProps/custom.xml")
}

fn sanitize_app_xml(original: &[u8]) -> Vec<u8> {
    let mut xml = String::from_utf8_lossy(original).into_owned();
    for tag in APP_METADATA_TAGS {
        xml = remove_app_element(&xml, tag);
    }
    xml.into_bytes()
}

fn remove_app_element(xml: &str, tag: &str) -> String {
    metadata::remove_xml_element_by_local_name(xml, tag)
}

const APP_METADATA_TAGS: &[&str] = &[
    "Application",
    "AppVersion",
    "Company",
    "Manager",
    "HyperlinkBase",
    "Template",
    "TotalTime",
    "LastAuthor",
    "Revision",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_property_with_regex_metachars_is_removed() {
        // 属性名含正则元字符时曾导致 Regex::new 失败、属性残留（脱敏不彻底）。
        let xml = r#"<Properties><property name="部门(1)">研发</property><property name="作者">张三</property></Properties>"#;
        let out = remove_custom_property(xml, "部门(1)");
        assert!(!out.contains("研发"));
        assert!(out.contains("张三"));
    }

    #[test]
    fn custom_property_name_is_xml_escaped_when_set() {
        let xml = r#"<Properties></Properties>"#;
        let out = set_custom_property(xml, "A&B", "<x>");
        assert!(out.contains("A&amp;B"));
        assert!(out.contains("&lt;x&gt;"));
    }

    #[test]
    fn zip_entry_name_rejects_traversal() {
        assert!(safety::safe_zip_entry_name("../evil.xml").is_err());
        assert!(safety::safe_zip_entry_name("docProps/core.xml").is_ok());
    }
}
