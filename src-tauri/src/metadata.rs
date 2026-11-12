use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MetadataField {
    pub key: String,
    pub group: String,
    pub name: String,
    pub value: String,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataPreview {
    pub path: String,
    pub success: bool,
    pub message: String,
    pub fields: Vec<MetadataField>,
}

impl MetadataPreview {
    pub fn ok(path: String, fields: Vec<MetadataField>) -> Self {
        let message = if fields.is_empty() {
            "未检测到元数据".to_string()
        } else {
            format!("共 {} 项元数据", fields.len())
        };
        Self {
            path,
            success: true,
            message,
            fields,
        }
    }

    pub fn err(path: String, message: String) -> Self {
        Self {
            path,
            success: false,
            message,
            fields: Vec::new(),
        }
    }
}

pub fn push_field(
    fields: &mut Vec<MetadataField>,
    key: &str,
    group: &str,
    name: &str,
    value: String,
) {
    push_field_with_editable(fields, key, group, name, value, true);
}

pub fn push_field_readonly(
    fields: &mut Vec<MetadataField>,
    key: &str,
    group: &str,
    name: &str,
    value: String,
) {
    push_field_with_editable(fields, key, group, name, value, false);
}

fn push_field_with_editable(
    fields: &mut Vec<MetadataField>,
    key: &str,
    group: &str,
    name: &str,
    value: String,
    editable: bool,
) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
        fields.push(MetadataField {
            key: key.to_string(),
            group: group.to_string(),
            name: name.to_string(),
            value: trimmed.to_string(),
            editable,
        });
    }
}

pub fn remove_xml_element_by_local_name(xml: &str, local_name: &str) -> String {
    // 标签名做元字符转义，避免构造值时被当作正则注入。
    let name = regex::escape(local_name);
    let pattern = format!(
        r#"(?s)<(?:[\w]+:)?{name}(?:\s[^>]*)?>.*?</(?:[\w]+:)?{name}>"#
    );
    let mut result = xml.to_string();
    if let Ok(re) = regex::Regex::new(&pattern) {
        result = re.replace_all(&result, "").into_owned();
    }

    let self_closing = format!(r#"(?s)<(?:[\w]+:)?{name}(?:\s[^>]*)?/>"#);
    if let Ok(re) = regex::Regex::new(&self_closing) {
        result = re.replace_all(&result, "").into_owned();
    }

    result
}

pub fn set_xml_element_by_local_name(xml: &str, local_name: &str, inner_xml: &str) -> String {
    let mut result = remove_xml_element_by_local_name(xml, local_name);
    if let Some(pos) = result.rfind("</") {
        result.insert_str(pos, inner_xml);
    } else {
        result.push_str(inner_xml);
    }
    result
}

pub fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn format_pdf_date(raw: &str) -> String {
    let s = raw.trim_start_matches("D:");
    // 只在前 14 个字节都是 ASCII 数字时才做固定切片，避免多字节字符边界 panic。
    // PDF 日期形如 D:20240612143000+08'00'；非标准值原样返回。
    let digits = s.as_bytes();
    if digits.len() >= 14 && digits[..14].iter().all(u8::is_ascii_digit) {
        // 前 14 字节均为 ASCII，按字节切片一定落在合法字符边界上。
        let year = &s[0..4];
        let month = &s[4..6];
        let day = &s[6..8];
        let hour = &s[8..10];
        let minute = &s[10..12];
        let second = &s[12..14];
        return format!("{year}-{month}-{day} {hour}:{minute}:{second}");
    }
    raw.to_string()
}

pub fn extract_xml_element_text(xml: &str, local_name: &str) -> Option<String> {
    let name = regex::escape(local_name);
    let pattern = format!(r#"(?s)<(?:[\w]+:)?{name}(?:\s[^>]*)?>(.*?)</(?:[\w]+:)?{name}>"#);
    regex::Regex::new(&pattern)
        .ok()?
        .captures(xml)
        .and_then(|cap| cap.get(1))
        .map(|m| decode_xml_entities(&strip_xml_tags(m.as_str())))
        .filter(|text| !text.is_empty())
}

fn strip_xml_tags(input: &str) -> String {
    let re = match regex::Regex::new(r"(?s)<[^>]+>") {
        Ok(re) => re,
        Err(_) => return input.trim().to_string(),
    };
    re.replace_all(input, "").trim().to_string()
}

pub fn extract_xml_attr_elements(xml: &str, tag: &str, attr: &str) -> Vec<(String, String)> {
    let tag = regex::escape(tag);
    let attr = regex::escape(attr);
    let pattern = format!(
        r#"(?s)<{tag}\s[^>]*{attr}="([^"]*)"[^>]*>(.*?)</{tag}>"#
    );
    let Ok(re) = regex::Regex::new(&pattern) else {
        return Vec::new();
    };
    re.captures_iter(xml)
        .filter_map(|cap| {
            let name = cap.get(1)?.as_str().to_string();
            let inner = cap.get(2)?.as_str();
            let value = extract_vt_value(inner).unwrap_or_else(|| inner.trim().to_string());
            if value.is_empty() {
                None
            } else {
                Some((name, value))
            }
        })
        .collect()
}

fn extract_vt_value(inner: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?s)<vt:[^>]+>(.*?)</vt:[^>]+>").ok()?;
    re.captures(inner)
        .and_then(|cap| cap.get(1))
        .map(|m| decode_xml_entities(m.as_str().trim()))
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_date_standard() {
        assert_eq!(
            format_pdf_date("D:20240612143000+08'00'"),
            "2024-06-12 14:30:00"
        );
    }

    #[test]
    fn pdf_date_multibyte_never_panics() {
        // 曾是字节切片 panic：中文导致第 4 字节不是字符边界。
        assert_eq!(format_pdf_date("中文日期测试"), "中文日期测试");
        assert_eq!(format_pdf_date("D:中文20240612143000"), "D:中文20240612143000");
        assert_eq!(format_pdf_date("D:2024"), "D:2024");
    }

    #[test]
    fn xml_local_name_is_escaped() {
        let xml = r#"<dc:creator>张三</dc:creator><cp:lastModifiedBy>李四</cp:lastModifiedBy>"#;
        let out = remove_xml_element_by_local_name(xml, "creator");
        assert!(!out.contains("张三"));
        assert!(out.contains("李四"));
    }

    #[test]
    fn xml_attr_name_is_escaped() {
        let xml = r#"<property name="公司">Acme</property><property name="作者">张三</property>"#;
        let got = extract_xml_attr_elements(xml, "property", "name");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "公司");
    }
}

pub fn extract_xmp_fields(xmp: &str) -> Vec<(String, String, String)> {
    let tags = [
        ("title", "xmp:title", "标题"),
        ("creator", "xmp:creator", "作者"),
        ("subject", "xmp:subject", "主题"),
        ("description", "xmp:description", "描述"),
        ("Keywords", "xmp:Keywords", "关键词"),
        ("Producer", "xmp:Producer", "生成程序"),
        ("CreatorTool", "xmp:CreatorTool", "创建工具"),
        ("CreateDate", "xmp:CreateDate", "创建时间"),
        ("ModifyDate", "xmp:ModifyDate", "修改时间"),
        ("MetadataDate", "xmp:MetadataDate", "元数据日期"),
    ];

    let mut fields = Vec::new();
    for (tag, key, label) in tags {
        if let Some(value) = extract_xml_element_text(xmp, tag) {
            fields.push((key.to_string(), label.to_string(), value));
        }
    }
    fields
}
