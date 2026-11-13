use crate::apply::{resolve_action, FieldRule, ResolvedAction};
use crate::error::{ClearMetaError, Result};
use crate::metadata::{self, MetadataField};
use crate::safety;
use exif::{Reader, Tag};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

const WEBP_EXIF: &[u8; 4] = b"EXIF";
const WEBP_XMP: &[u8; 4] = b"XMP ";

pub fn read_image_metadata(path: &Path) -> Result<Vec<MetadataField>> {
    let ext = extension(path)?;
    match ext.as_str() {
        "jpg" | "jpeg" => read_jpeg_exif(path),
        "png" => read_png_metadata(path),
        "webp" => read_webp_metadata(path),
        _ => Err(ClearMetaError::UnsupportedFormat(ext)),
    }
}

pub fn apply_image(input: &Path, output: &Path, rules: &[FieldRule], clear_all: bool) -> Result<()> {
    // 单文件大小预检，避免把超大图片整体读进内存。
    safety::ensure_size_ok(input)?;

    if clear_all {
        let data = std::fs::read(input)?;
        let ext = extension(input)?;
        let cleaned = match ext.as_str() {
            "jpg" | "jpeg" => strip_jpeg_metadata(&data)?,
            "png" => strip_png_metadata(&data)?,
            "webp" => strip_webp_metadata(&data)?,
            _ => return Err(ClearMetaError::UnsupportedFormat(ext)),
        };
        return write_output(input, output, &cleaned);
    }

    let fields = read_image_metadata(input)?;
    let ext = extension(input)?;
    let data = std::fs::read(input)?;

    let strip_exif = should_strip_exif(&fields, rules);
    let strip_xmp = should_strip_xmp(&fields, rules);
    let strip_png_text = collect_png_text_removals(&fields, rules);
    let strip_png_exif = should_strip_png_exif(&fields, rules);

    let cleaned = match ext.as_str() {
        "jpg" | "jpeg" => {
            let mut out = data.clone();
            if strip_exif || strip_xmp {
                out = strip_jpeg_metadata(&out)?;
            }
            out
        }
        "png" => {
            let mut out = data;
            if !strip_png_text.is_empty() {
                out = strip_png_text_chunks(&out, &strip_png_text)?;
            }
            if strip_png_exif {
                out = strip_png_exif_chunk(&out)?;
            }
            if strip_xmp {
                out = strip_png_metadata(&out)?;
            }
            out
        }
        "webp" => {
            let mut out = data;
            if strip_exif {
                out = strip_webp_chunk(&out, WEBP_EXIF)?;
            }
            if strip_xmp {
                out = strip_webp_chunk(&out, WEBP_XMP)?;
            }
            out
        }
        _ => return Err(ClearMetaError::UnsupportedFormat(ext)),
    };

    write_output(input, output, &cleaned)
}

fn should_strip_exif(fields: &[MetadataField], rules: &[FieldRule]) -> bool {
    if matches!(resolve_action("exif:*", rules), ResolvedAction::Remove)
        || matches!(resolve_action("gps:*", rules), ResolvedAction::Remove)
    {
        return true;
    }

    fields.iter().any(|field| {
        (field.key.starts_with("exif:") || field.key.starts_with("gps:"))
            && matches!(resolve_action(&field.key, rules), ResolvedAction::Remove)
    })
}

fn should_strip_xmp(fields: &[MetadataField], rules: &[FieldRule]) -> bool {
    if matches!(resolve_action("xmp:*", rules), ResolvedAction::Remove) {
        return true;
    }

    fields.iter().any(|field| {
        field.key.starts_with("xmp:")
            && matches!(resolve_action(&field.key, rules), ResolvedAction::Remove)
    })
}

fn should_strip_png_exif(fields: &[MetadataField], rules: &[FieldRule]) -> bool {
    fields.iter().any(|field| {
        (field.key.starts_with("exif:") || field.key.starts_with("gps:"))
            && field.group != "PNG 文本"
            && matches!(resolve_action(&field.key, rules), ResolvedAction::Remove)
    })
}

fn collect_png_text_removals(fields: &[MetadataField], rules: &[FieldRule]) -> HashSet<String> {
    let mut keys = HashSet::new();
    if matches!(resolve_action("png:*", rules), ResolvedAction::Remove) {
        for field in fields {
            if field.key.starts_with("png:") {
                keys.insert(field.name.clone());
            }
        }
        return keys;
    }

    for field in fields {
        if field.key.starts_with("png:")
            && matches!(resolve_action(&field.key, rules), ResolvedAction::Remove)
        {
            keys.insert(field.name.clone());
        }
    }
    keys
}

fn extension(path: &Path) -> Result<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| ClearMetaError::UnsupportedFormat(String::new()))
}

/// 从切片指定位置安全读取 4 字节大端 u32，越界返回 Corrupted 而非 panic。
fn be_u32_at(data: &[u8], at: usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(at..at.saturating_add(4))
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| ClearMetaError::Corrupted("文件块长度字段越界".into()))?;
    Ok(u32::from_be_bytes(bytes))
}

/// 安全读取 4 字节小端 u32（WebP）。
fn le_u32_at(data: &[u8], at: usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(at..at.saturating_add(4))
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| ClearMetaError::Corrupted("文件块长度字段越界".into()))?;
    Ok(u32::from_le_bytes(bytes))
}

fn write_output(input: &Path, output: &Path, data: &[u8]) -> Result<()> {
    if input == output {
        // 并发安全的临时名，避免多窗口 / 多次调用争用同一临时文件。
        let temp = safety::temp_path(input);
        std::fs::write(&temp, data)?;
        safety::replace_file(&temp, output)?;
    } else {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, data)?;
    }
    Ok(())
}

fn read_jpeg_exif(path: &Path) -> Result<Vec<MetadataField>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let exif = Reader::new()
        .read_from_container(&mut reader)
        .map_err(|e| ClearMetaError::Image(e.to_string()))?;

    Ok(exif_fields_to_metadata(&exif))
}

fn exif_field_key(tag: Tag) -> String {
    if is_gps_tag(tag) {
        format!("gps:{tag:?}")
    } else {
        format!("exif:{tag:?}")
    }
}

fn exif_fields_to_metadata(exif: &exif::Exif) -> Vec<MetadataField> {
    let mut fields = Vec::new();

    for field in exif.fields() {
        let group = if is_gps_tag(field.tag) { "GPS" } else { "EXIF" };
        let name = tag_label(field.tag);
        let key = exif_field_key(field.tag);
        let value = field.display_value().to_string();
        metadata::push_field(&mut fields, &key, group, name, value);
    }

    fields
}

fn is_gps_tag(tag: Tag) -> bool {
    matches!(
        tag,
        Tag::GPSVersionID
            | Tag::GPSLatitudeRef
            | Tag::GPSLatitude
            | Tag::GPSLongitudeRef
            | Tag::GPSLongitude
            | Tag::GPSAltitudeRef
            | Tag::GPSAltitude
            | Tag::GPSTimeStamp
            | Tag::GPSSatellites
            | Tag::GPSStatus
            | Tag::GPSMeasureMode
            | Tag::GPSDOP
            | Tag::GPSSpeedRef
            | Tag::GPSSpeed
            | Tag::GPSTrackRef
            | Tag::GPSTrack
            | Tag::GPSImgDirectionRef
            | Tag::GPSImgDirection
            | Tag::GPSMapDatum
            | Tag::GPSDestLatitudeRef
            | Tag::GPSDestLatitude
            | Tag::GPSDestLongitudeRef
            | Tag::GPSDestLongitude
            | Tag::GPSDestBearingRef
            | Tag::GPSDestBearing
            | Tag::GPSDestDistanceRef
            | Tag::GPSDestDistance
            | Tag::GPSProcessingMethod
            | Tag::GPSAreaInformation
            | Tag::GPSDateStamp
            | Tag::GPSDifferential
            | Tag::GPSHPositioningError
    )
}

fn tag_label(tag: Tag) -> &'static str {
    match tag {
        Tag::Make => "相机品牌",
        Tag::Model => "相机型号",
        Tag::Software => "软件",
        Tag::Artist => "作者",
        Tag::Copyright => "版权",
        Tag::ImageDescription => "描述",
        Tag::DateTime => "修改时间",
        Tag::DateTimeOriginal => "拍摄时间",
        Tag::DateTimeDigitized => "数字化时间",
        Tag::Orientation => "方向",
        Tag::XResolution => "水平分辨率",
        Tag::YResolution => "垂直分辨率",
        Tag::ExposureTime => "曝光时间",
        Tag::FNumber => "光圈",
        Tag::ISOSpeed => "ISO",
        Tag::FocalLength => "焦距",
        Tag::Flash => "闪光灯",
        Tag::WhiteBalance => "白平衡",
        Tag::LensModel => "镜头型号",
        Tag::GPSLatitude => "GPS 纬度",
        Tag::GPSLongitude => "GPS 经度",
        Tag::GPSAltitude => "GPS 海拔",
        Tag::GPSDateStamp => "GPS 日期",
        Tag::GPSProcessingMethod => "GPS 处理方式",
        Tag::ImageWidth => "宽度",
        Tag::ImageLength => "高度",
        _ => "其他",
    }
}

fn read_png_metadata(path: &Path) -> Result<Vec<MetadataField>> {
    let data = std::fs::read(path)?;
    let mut fields = Vec::new();

    for (name, content) in png_text_chunks(&data)? {
        let key = format!("png:{name}");
        metadata::push_field(&mut fields, &key, "PNG 文本", &name, content);
    }

    if let Some(exif_data) = png_exif_chunk(&data) {
        if let Ok(exif) = Reader::new().read_from_container(&mut Cursor::new(exif_data)) {
            fields.extend(exif_fields_to_metadata(&exif));
        }
    }

    Ok(fields)
}

fn png_text_chunks(data: &[u8]) -> Result<Vec<(String, String)>> {
    let mut result = Vec::new();
    if data.len() < 8 || &data[0..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err(ClearMetaError::Image("无效的 PNG 文件".into()));
    }

    let mut offset = 8;
    while offset + 12 <= data.len() {
        let length = be_u32_at(data, offset)? as usize;
        let chunk_type = &data[offset + 4..offset + 8];
        let chunk_end = offset.saturating_add(12).saturating_add(length);
        if chunk_end > data.len() {
            return Err(ClearMetaError::Corrupted("PNG 块长度越界".into()));
        }

        let chunk_data = &data[offset + 8..offset + 8 + length];
        match chunk_type {
            b"tEXt" => {
                if let Some((key, value)) = parse_png_text(chunk_data) {
                    result.push((key, value));
                }
            }
            b"iTXt" => {
                if let Some((key, value)) = parse_png_itxt(chunk_data) {
                    result.push((key, value));
                }
            }
            b"IEND" => break,
            _ => {}
        }

        offset = chunk_end;
    }

    Ok(result)
}

fn png_exif_chunk(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 8 {
        return None;
    }
    let mut offset = 8;
    while offset + 12 <= data.len() {
        let length = be_u32_at(data, offset).ok()? as usize;
        let chunk_type = data.get(offset + 4..offset + 8)?;
        let chunk_end = offset.saturating_add(12).saturating_add(length);
        if chunk_end > data.len() {
            break;
        }
        if chunk_type == b"eXIf" {
            return Some(data[offset + 8..offset + 8 + length].to_vec());
        }
        if chunk_type == b"IEND" {
            break;
        }
        offset = chunk_end;
    }
    None
}

fn parse_png_text(data: &[u8]) -> Option<(String, String)> {
    let nul = data.iter().position(|&b| b == 0)?;
    let key = String::from_utf8_lossy(&data[..nul]).into_owned();
    let value = String::from_utf8_lossy(&data[nul + 1..]).into_owned();
    Some((key, value))
}

fn parse_png_itxt(data: &[u8]) -> Option<(String, String)> {
    let mut parts = data.split(|&b| b == 0);
    let key = String::from_utf8_lossy(parts.next()?).into_owned();
    let _compression = parts.next()?;
    let _method = parts.next()?;
    let _language = parts.next()?;
    let _translated = parts.next()?;
    let value = String::from_utf8_lossy(parts.next()?).into_owned();
    Some((key, value))
}

fn read_webp_metadata(path: &Path) -> Result<Vec<MetadataField>> {
    let data = std::fs::read(path)?;
    let mut fields = Vec::new();

    if let Some(exif_data) = webp_chunk(&data, WEBP_EXIF) {
        if let Ok(exif) = Reader::new().read_from_container(&mut Cursor::new(exif_data)) {
            fields.extend(exif_fields_to_metadata(&exif));
        }
    }

    if let Some(xmp) = webp_chunk(&data, WEBP_XMP) {
        if let Ok(text) = String::from_utf8(xmp) {
            for (key, name, value) in metadata::extract_xmp_fields(&text) {
                metadata::push_field(&mut fields, &key, "XMP", &name, value);
            }
        }
    }

    Ok(fields)
}

fn webp_chunk(data: &[u8], tag: &[u8; 4]) -> Option<Vec<u8>> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }

    let mut offset = 12;
    while offset + 8 <= data.len() {
        let chunk_tag = &data[offset..offset + 4];
        let size = le_u32_at(data, offset + 4).ok()? as usize;
        let payload_start = offset + 8;
        let payload_end = payload_start.saturating_add(size).saturating_add(size % 2);
        if payload_end > data.len() {
            break;
        }

        if chunk_tag == tag {
            return Some(data[payload_start..payload_start + size].to_vec());
        }

        offset = payload_end;
    }

    None
}

fn strip_jpeg_metadata(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() < 2 || input[0] != 0xFF || input[1] != 0xD8 {
        return Err(ClearMetaError::Image("无效的 JPEG 文件".into()));
    }

    let mut output = Vec::with_capacity(input.len());
    output.extend_from_slice(&input[0..2]);
    let mut i = 2;

    while i < input.len() {
        if input[i] != 0xFF {
            output.extend_from_slice(&input[i..]);
            break;
        }

        let marker_start = i;
        i += 1;
        while i < input.len() && input[i] == 0xFF {
            i += 1;
        }
        if i >= input.len() {
            break;
        }

        let marker = input[i];
        i += 1;

        if marker == 0xD9 {
            output.extend_from_slice(&input[marker_start..]);
            break;
        }

        if marker == 0xDA {
            output.extend_from_slice(&input[marker_start..]);
            break;
        }

        if i + 1 >= input.len() {
            break;
        }

        let segment_len = u16::from_be_bytes([input[i], input[i + 1]]) as usize;
        if segment_len < 2 || marker_start + 2 + segment_len > input.len() {
            return Err(ClearMetaError::Image("JPEG 段结构损坏".into()));
        }

        let segment_end = marker_start.saturating_add(2).saturating_add(segment_len);
        let payload_start = marker_start + 4;

        let skip = match marker {
            0xE1 if payload_start + 4 <= input.len()
                && &input[payload_start..payload_start + 4] == b"Exif" =>
            {
                true
            }
            0xE1 if payload_start + 29 <= input.len()
                && &input[payload_start..payload_start + 29]
                    == b"http://ns.adobe.com/xap/1.0/\0" =>
            {
                true
            }
            0xED | 0xFE => true,
            _ => false,
        };

        if skip {
            i = segment_end;
            continue;
        }

        output.extend_from_slice(&input[marker_start..segment_end]);
        i = segment_end;
    }

    Ok(output)
}

fn strip_png_metadata(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() < 8 || &input[0..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err(ClearMetaError::Image("无效的 PNG 文件".into()));
    }

    let mut output = Vec::with_capacity(input.len());
    output.extend_from_slice(&input[0..8]);

    let skip_chunks: [&[u8]; 4] = [b"tEXt", b"iTXt", b"zTXt", b"eXIf"];
    let mut offset = 8;

    while offset + 12 <= input.len() {
        let length = be_u32_at(input, offset)? as usize;
        let chunk_type = &input[offset + 4..offset + 8];
        let chunk_end = offset.saturating_add(12).saturating_add(length);
        if chunk_end > input.len() {
            return Err(ClearMetaError::Corrupted("PNG 块结构损坏".into()));
        }

        if !skip_chunks.contains(&chunk_type) {
            output.extend_from_slice(&input[offset..chunk_end]);
        }

        if chunk_type == b"IEND" {
            break;
        }

        offset = chunk_end;
    }

    Ok(output)
}

fn strip_png_text_chunks(input: &[u8], keys: &HashSet<String>) -> Result<Vec<u8>> {
    if keys.is_empty() {
        return Ok(input.to_vec());
    }

    let mut output = Vec::with_capacity(input.len());
    output.extend_from_slice(&input[0..8]);
    let mut offset = 8;

    while offset + 12 <= input.len() {
        let length = be_u32_at(input, offset)? as usize;
        let chunk_type = &input[offset + 4..offset + 8];
        let chunk_end = offset.saturating_add(12).saturating_add(length);
        if chunk_end > input.len() {
            return Err(ClearMetaError::Corrupted("PNG 文本块长度越界".into()));
        }

        let chunk_data = &input[offset + 8..offset + 8 + length];
        let skip = match chunk_type {
            b"tEXt" => parse_png_text(chunk_data)
                .map(|(key, _)| keys.contains(&key))
                .unwrap_or(false),
            b"iTXt" => parse_png_itxt(chunk_data)
                .map(|(key, _)| keys.contains(&key))
                .unwrap_or(false),
            _ => false,
        };

        if !skip {
            output.extend_from_slice(&input[offset..chunk_end]);
        }

        if chunk_type == b"IEND" {
            break;
        }

        offset = chunk_end;
    }

    Ok(output)
}

fn strip_png_exif_chunk(input: &[u8]) -> Result<Vec<u8>> {
    strip_png_chunks(input, &[b"eXIf"])
}

fn strip_png_chunks(input: &[u8], skip_types: &[&[u8; 4]]) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len());
    output.extend_from_slice(&input[0..8]);
    let mut offset = 8;

    while offset + 12 <= input.len() {
        let length = be_u32_at(input, offset)? as usize;
        let chunk_type = &input[offset + 4..offset + 8];
        let chunk_end = offset.saturating_add(12).saturating_add(length);
        if chunk_end > input.len() {
            return Err(ClearMetaError::Corrupted("PNG 块长度越界".into()));
        }

        if !skip_types.iter().any(|t| *t == chunk_type) {
            output.extend_from_slice(&input[offset..chunk_end]);
        }

        if chunk_type == b"IEND" {
            break;
        }

        offset = chunk_end;
    }

    Ok(output)
}

fn strip_webp_metadata(input: &[u8]) -> Result<Vec<u8>> {
    let out = strip_webp_chunk(input, WEBP_EXIF)?;
    strip_webp_chunk(&out, WEBP_XMP)
}

fn strip_webp_chunk(input: &[u8], tag: &[u8; 4]) -> Result<Vec<u8>> {
    if input.len() < 12 || &input[0..4] != b"RIFF" || &input[8..12] != b"WEBP" {
        return Err(ClearMetaError::Image("无效的 WebP 文件".into()));
    }

    let mut output = Vec::with_capacity(input.len());
    output.extend_from_slice(&input[0..4]);
    output.extend_from_slice(&[0, 0, 0, 0]);
    output.extend_from_slice(&input[8..12]);

    let mut offset = 12;

    while offset + 8 <= input.len() {
        let chunk_tag = &input[offset..offset + 4];
        let size = le_u32_at(input, offset + 4)? as usize;
        let chunk_end = offset.saturating_add(8).saturating_add(size).saturating_add(size % 2);
        if chunk_end > input.len() {
            return Err(ClearMetaError::Corrupted("WebP 块长度越界".into()));
        }

        if chunk_tag != tag {
            output.extend_from_slice(&input[offset..chunk_end]);
        }

        offset = chunk_end;
    }

    let riff_size = u32::try_from(output.len().saturating_sub(8))
        .map_err(|_| ClearMetaError::Corrupted("WebP 文件过大".into()))?;
    output[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_roundtrip_header() {
        // 合法的 APP0 段：长度字段 16（含自身 2 字节），随后补齐 14 字节 payload。
        let mut input = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        input.extend_from_slice(b"JFIF\0");
        input.extend_from_slice(&[1, 1, 0, 0, 1, 0, 1, 0, 0]); // 共 14 字节 payload
        input.extend_from_slice(&[0xFF, 0xD9]); // EOI
        let out = strip_jpeg_metadata(&input).unwrap();
        assert_eq!(&out[0..2], &[0xFF, 0xD8]);
        // APP0 段被保留（不是元数据段）
        assert!(out.windows(2).any(|w| w == [0xFF, 0xE0]));
    }

    #[test]
    fn truncated_png_returns_err_instead_of_silent_output() {
        // 声明长度 256，但块头完整（≥12 字节）而 payload 不足：必须报错。
        let mut input = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG 签名
        input.extend_from_slice(&[0, 0, 1, 0]); // length = 256
        input.extend_from_slice(b"tEXt"); // 4 字节类型
        input.extend_from_slice(&[1, 2, 3, 4]); // 4 字节 CRC 占位，凑满 12 字节块头
        input.extend_from_slice(&[9, 9, 9]); // payload 远不足 256 字节
        // offset=8, 8+12=20 <= 23（进入循环），chunk_end=8+12+256=276 > 23 → Err
        assert!(strip_png_metadata(&input).is_err());
        assert!(strip_png_text_chunks(&input, &std::collections::HashSet::from(["Comment".into()])).is_err());
    }

    #[test]
    fn be_u32_at_out_of_range_is_err() {
        assert!(be_u32_at(&[1, 2], 0).is_err());
        assert_eq!(be_u32_at(&[0, 0, 1, 0], 0).unwrap(), 256);
    }

    #[test]
    fn truncated_webp_returns_err() {
        let mut input = Vec::new();
        input.extend_from_slice(b"RIFF");
        input.extend_from_slice(&[0, 0, 0, 0]);
        input.extend_from_slice(b"WEBP");
        input.extend_from_slice(b"EXIF");
        input.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x7F]); // 超大 size
        assert!(strip_webp_chunk(&input, WEBP_EXIF).is_err());
    }
}
