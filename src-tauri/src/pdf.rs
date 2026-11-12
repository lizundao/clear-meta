use crate::apply::{resolve_action, FieldRule, ResolvedAction};
use crate::error::{ClearMetaError, Result};
use crate::metadata::{self, MetadataField};
use crate::safety;
use lopdf::{decode_text_string, Dictionary, Document, Object, ObjectId, Stream};
use std::collections::BTreeSet;
use std::path::Path;

const INFO_FIELDS: &[(&[u8], &str, &str, bool)] = &[
    (b"Title", "info:Title", "标题", false),
    (b"Author", "info:Author", "作者", false),
    (b"Subject", "info:Subject", "主题", false),
    (b"Keywords", "info:Keywords", "关键词", false),
    (b"Creator", "info:Creator", "创建程序", false),
    (b"Producer", "info:Producer", "生成程序", false),
    (b"CreationDate", "info:CreationDate", "创建时间", true),
    (b"ModDate", "info:ModDate", "修改时间", true),
];

const XMP_TAG_KEYS: &[(&str, &str)] = &[
    ("title", "xmp:title"),
    ("creator", "xmp:creator"),
    ("subject", "xmp:subject"),
    ("description", "xmp:description"),
    ("Keywords", "xmp:Keywords"),
    ("Producer", "xmp:Producer"),
    ("CreatorTool", "xmp:CreatorTool"),
    ("CreateDate", "xmp:CreateDate"),
    ("ModifyDate", "xmp:ModifyDate"),
    ("MetadataDate", "xmp:MetadataDate"),
];

pub fn read_pdf_metadata(path: &Path) -> Result<Vec<MetadataField>> {
    let doc = Document::load(path).map_err(|e| ClearMetaError::Pdf(e.to_string()))?;
    let mut fields = Vec::new();

    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        match info_ref {
            Object::Reference(info_id) => {
                if let Ok(Object::Dictionary(info)) = doc.get_object(*info_id) {
                    read_info_dictionary(&info, &mut fields);
                }
            }
            Object::Dictionary(info) => read_info_dictionary(&info, &mut fields),
            _ => {}
        }
    }

    let page_count = doc.get_pages().len();
    if page_count > 0 {
        metadata::push_field_readonly(
            &mut fields,
            "doc:pageCount",
            "文档信息",
            "页数",
            page_count.to_string(),
        );
    }

    collect_xmp_metadata(&doc, &mut fields);

    Ok(fields)
}

fn collect_xmp_metadata(doc: &Document, fields: &mut Vec<MetadataField>) {
    let mut parsed = false;

    for object in doc.objects.values() {
        let Object::Stream(stream) = object else {
            continue;
        };
        if !is_xmp_metadata_stream(object) {
            continue;
        }

        let Ok(content) = stream.get_plain_content() else {
            continue;
        };
        let Ok(xmp) = String::from_utf8(content) else {
            continue;
        };

        if !parsed {
            metadata::push_field_readonly(
                fields,
                "xmp:__stream__",
                "XMP",
                "状态",
                "包含 XMP 元数据".to_string(),
            );
            parsed = true;
        }

        for (key, name, value) in metadata::extract_xmp_fields(&xmp) {
            if !fields.iter().any(|f| f.key == key && f.value == value) {
                metadata::push_field(fields, &key, "XMP", &name, value);
            }
        }
    }
}

fn read_info_dictionary(info: &Dictionary, fields: &mut Vec<MetadataField>) {
    for (key, field_key, label, is_date) in INFO_FIELDS {
        if let Ok(value) = info.get(*key) {
            if let Some(text) = object_to_string(value) {
                let display = if *is_date {
                    metadata::format_pdf_date(&text)
                } else {
                    text
                };
                metadata::push_field(fields, field_key, "文档信息", label, display);
            }
        }
    }
}

fn object_to_string(object: &Object) -> Option<String> {
    match object {
        Object::String(_, _) => decode_pdf_string(object),
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

fn decode_pdf_string(object: &Object) -> Option<String> {
    if let Ok(text) = decode_text_string(object) {
        if !text.contains('\u{FFFD}') {
            return Some(text);
        }
    }

    let Object::String(bytes, _) = object else {
        return None;
    };

    decode_utf16_be(bytes).or_else(|| Some(String::from_utf8_lossy(bytes).into_owned()))
}

fn decode_utf16_be(bytes: &[u8]) -> Option<String> {
    let data = if bytes.starts_with(&[0xFE, 0xFF]) {
        &bytes[2..]
    } else {
        bytes
    };

    if data.len() < 2 || !data.len().is_multiple_of(2) {
        return None;
    }

    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();

    String::from_utf16(&units).ok()
}

pub fn apply_pdf(input: &Path, output: &Path, rules: &[FieldRule], clear_all: bool) -> Result<()> {
    if clear_all {
        let mut doc = Document::load(input).map_err(|e| ClearMetaError::Pdf(e.to_string()))?;
        remove_info_dictionary(&mut doc);
        remove_catalog_metadata(&mut doc);
        remove_metadata_streams(&mut doc);
        return save_pdf(&mut doc, input, output);
    }

    let mut doc = Document::load(input).map_err(|e| ClearMetaError::Pdf(e.to_string()))?;
    apply_info_dictionary(&mut doc, rules)?;
    apply_xmp_streams(&mut doc, rules)?;
    save_pdf(&mut doc, input, output)
}

fn save_pdf(doc: &mut Document, input: &Path, output: &Path) -> Result<()> {
    if input == output {
        let temp = safety::temp_path(input);
        doc.save(&temp)
            .map_err(|e| ClearMetaError::Pdf(e.to_string()))?;
        // 覆盖前复验：临时文件能被重新加载，才替换原文件，避免写出损坏 PDF。
        let check = Document::load(&temp).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            ClearMetaError::Corrupted(format!("输出 PDF 复验失败：{e}"))
        })?;
        if check.get_pages().len() == 0 {
            let _ = std::fs::remove_file(&temp);
            return Err(ClearMetaError::Corrupted("输出 PDF 页数为 0，已放弃覆盖".into()));
        }
        std::fs::rename(&temp, output)?;
    } else {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        doc.save(output)
            .map_err(|e| ClearMetaError::Pdf(e.to_string()))?;
    }
    Ok(())
}

fn apply_info_dictionary(doc: &mut Document, rules: &[FieldRule]) -> Result<()> {
    let info_id = match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => Some(*id),
        Ok(Object::Dictionary(_)) => None,
        _ => return Ok(()),
    };

    let Some(info_id) = info_id else {
        let mut should_create = false;
        for (_, field_key, _, _) in INFO_FIELDS {
            if !matches!(resolve_action(field_key, rules), ResolvedAction::Keep) {
                should_create = true;
                break;
            }
        }
        if !should_create {
            return Ok(());
        }
        let info_id = doc.new_object_id();
        doc.objects.insert(info_id, Object::Dictionary(Dictionary::new()));
        doc.trailer.set("Info", Object::Reference(info_id));
        return apply_info_dictionary(doc, rules);
    };

    let Object::Dictionary(info) = doc.get_object_mut(info_id).map_err(|e| {
        ClearMetaError::Pdf(e.to_string())
    })? else {
        return Ok(());
    };

    let mut remove_all = true;
    for (pdf_key, field_key, _, _) in INFO_FIELDS {
        match resolve_action(field_key, rules) {
            ResolvedAction::Keep => {
                if info.get(*pdf_key).is_ok() {
                    remove_all = false;
                }
            }
            ResolvedAction::Remove => {
                info.remove(pdf_key);
            }
            ResolvedAction::Set(value) => {
                remove_all = false;
                info.set(
                    *pdf_key,
                    Object::String(value.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                );
            }
        }
    }

    if remove_all || info.is_empty() {
        doc.trailer.remove(b"Info");
        doc.objects.remove(&info_id);
    }

    Ok(())
}

fn apply_xmp_streams(doc: &mut Document, rules: &[FieldRule]) -> Result<()> {
    let remove_stream = matches!(resolve_action("xmp:__stream__", rules), ResolvedAction::Remove)
        || matches!(resolve_action("xmp:*", rules), ResolvedAction::Remove);

    let mut stream_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, object)| {
            if is_xmp_metadata_stream(object) {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    if remove_stream {
        for id in stream_ids {
            remove_xmp_stream(doc, id);
        }
        return Ok(());
    }

    for id in stream_ids.drain(..) {
        let Object::Stream(stream) = doc.get_object_mut(id).map_err(|e| {
            ClearMetaError::Pdf(e.to_string())
        })? else {
            continue;
        };

        let content = stream.get_plain_content().map_err(|e| ClearMetaError::Pdf(e.to_string()))?;
        let Ok(mut xmp) = String::from_utf8(content) else {
            continue;
        };

        let mut changed = false;
        for (tag, field_key) in XMP_TAG_KEYS {
            match resolve_action(field_key, rules) {
                ResolvedAction::Keep => {}
                ResolvedAction::Remove => {
                    let updated = metadata::remove_xml_element_by_local_name(&xmp, tag);
                    if updated != xmp {
                        xmp = updated;
                        changed = true;
                    }
                }
                ResolvedAction::Set(value) => {
                    let escaped = metadata::escape_xml_text(value);
                    let inner = format!("<{tag}>{escaped}</{tag}>");
                    xmp = metadata::set_xml_element_by_local_name(&xmp, tag, &inner);
                    changed = true;
                }
            }
        }

        if changed {
            let mut new_stream = Stream::new(stream.dict.clone(), xmp.into_bytes());
            let stream_len = i64::try_from(new_stream.content.len())
                .map_err(|_| ClearMetaError::Pdf("XMP 流长度溢出".into()))?;
            new_stream.dict.set("Length", Object::Integer(stream_len));
            if let Ok(Object::Stream(existing)) = doc.get_object_mut(id) {
                *existing = new_stream;
            }
        }
    }

    Ok(())
}

fn remove_xmp_stream(doc: &mut Document, stream_id: ObjectId) {
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };

    if let Some(catalog_id) = catalog_id {
        if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(catalog_id) {
            if catalog.get(b"Metadata").ok().and_then(|o| o.as_reference().ok())
                == Some(stream_id)
            {
                catalog.remove(b"Metadata");
            }
        }
    }

    doc.objects.remove(&stream_id);
}

fn remove_info_dictionary(doc: &mut Document) {
    if let Ok(Object::Reference(info_id)) = doc.trailer.get(b"Info") {
        doc.objects.remove(info_id);
    }
    doc.trailer.remove(b"Info");
}

fn remove_catalog_metadata(doc: &mut Document) {
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return,
    };

    let metadata_keys: [&[u8]; 2] = [b"Metadata", b"PieceInfo"];
    let mut refs_to_remove = Vec::new();

    if let Ok(Object::Dictionary(catalog)) = doc.get_object(catalog_id) {
        for key in metadata_keys {
            if let Ok(Object::Reference(ref_id)) = catalog.get(key) {
                refs_to_remove.push(*ref_id);
            }
        }
    }

    if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(catalog_id) {
        for key in metadata_keys {
            catalog.remove(key);
        }
    }

    for ref_id in refs_to_remove {
        doc.objects.remove(&ref_id);
    }
}

fn remove_metadata_streams(doc: &mut Document) {
    let mut to_remove: BTreeSet<ObjectId> = BTreeSet::new();

    for (id, object) in &doc.objects {
        if is_xmp_metadata_stream(object) {
            to_remove.insert(*id);
        }
    }

    for id in to_remove {
        doc.objects.remove(&id);
    }
}

fn is_xmp_metadata_stream(object: &Object) -> bool {
    match object {
        Object::Stream(stream) => {
            if stream.dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Metadata") {
                return true;
            }
            if stream.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"XML") {
                return true;
            }
            false
        }
        Object::Dictionary(dict) => {
            dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"XML")
                && dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Metadata")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmp_stream_detection() {
        let mut stream = Stream::new(Dictionary::new(), Vec::new());
        stream.dict.set("Type", Object::Name(b"Metadata".to_vec()));
        assert!(is_xmp_metadata_stream(&Object::Stream(stream)));

        let plain = Stream::new(Dictionary::new(), Vec::new());
        assert!(!is_xmp_metadata_stream(&Object::Stream(plain)));
    }
}
