//! 日期字段写回的回归测试。
//!
//! 背景：0.1.1 会把界面上「2026-09-14 09:50:53」这种展示文本直接写进
//! PDF 的 /CreationDate，Acrobat 与资源管理器随即读不出时间。
//! 这里锁死两件事：写回必须是各格式原生语法；无法识别的输入必须报错且不动文件。

use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("clear_meta_date_{}_{}", std::process::id(), name));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 最小 PDF，Info 里带 PDF 原生日期（D: 语法）。
fn make_pdf(path: &PathBuf) {
    let mut doc = lopdf::Document::with_version("1.5");
    let page_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name("Page".into())),
        ("Parent", lopdf::Object::Reference((1, 0))),
    ])));
    let pages_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name("Pages".into())),
        ("Count", lopdf::Object::Integer(1)),
        (
            "Kids",
            lopdf::Object::Array(vec![lopdf::Object::Reference(page_id)]),
        ),
    ])));
    if let lopdf::Object::Dictionary(d) = doc.get_object_mut(page_id).unwrap() {
        d.set("Parent", lopdf::Object::Reference(pages_id));
    }
    let cat_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name("Catalog".into())),
        ("Pages", lopdf::Object::Reference(pages_id)),
    ])));
    let info_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        (
            "CreationDate",
            lopdf::Object::String(
                b"D:20260914095053+08'00'".to_vec(),
                lopdf::StringFormat::Literal,
            ),
        ),
    ])));
    doc.trailer.set("Root", lopdf::Object::Reference(cat_id));
    doc.trailer.set("Info", lopdf::Object::Reference(info_id));
    doc.compress();
    doc.save(path).unwrap();
}

fn pdf_creation_date(path: &PathBuf) -> String {
    let doc = lopdf::Document::load(path).unwrap();
    let info = doc.trailer.get(b"Info").unwrap();
    let info = match info {
        lopdf::Object::Reference(id) => doc.get_object(*id).unwrap().as_dict().unwrap(),
        other => other.as_dict().unwrap(),
    };
    let value = info.get(b"CreationDate").unwrap();
    match value {
        lopdf::Object::String(bytes, _) => String::from_utf8_lossy(bytes).into_owned(),
        other => panic!("unexpected object: {other:?}"),
    }
}

#[test]
fn pdf_date_is_written_in_native_syntax() {
    let dir = tmp_dir("pdf_date");
    let p = dir.join("a.pdf");
    make_pdf(&p);

    let rules = vec![clear_meta_lib::FieldRule {
        key: "info:CreationDate".to_string(),
        action: clear_meta_lib::FieldAction::Set,
        value: Some("2026-09-14 09:50:53".to_string()),
    }];
    clear_meta_lib::apply_file(&p, &p, &rules, false).expect("apply should succeed");

    let raw = pdf_creation_date(&p);
    assert!(
        raw.starts_with("D:20260914095053"),
        "写入的必须是 PDF 原生日期，实际是 {raw}"
    );
    assert!(
        !raw.contains("2026-09-14 09:50:53"),
        "绝不能把展示文本写进文件，实际是 {raw}"
    );

    // 再读一次：界面展示值应回到用户填的时间
    let fields = clear_meta_lib::read_file_metadata(&p).unwrap();
    let field = fields
        .iter()
        .find(|f| f.key == "info:CreationDate")
        .expect("CreationDate should be readable");
    assert_eq!(field.value, "2026-09-14 09:50:53");
    assert!(field.raw.is_some(), "应同时给出文件里的原始串");
}

#[test]
fn pdf_invalid_date_is_rejected_without_touching_file() {
    let dir = tmp_dir("pdf_bad_date");
    let p = dir.join("a.pdf");
    make_pdf(&p);
    let before = fs::read(&p).unwrap();

    let rules = vec![clear_meta_lib::FieldRule {
        key: "info:CreationDate".to_string(),
        action: clear_meta_lib::FieldAction::Set,
        value: Some("不是时间".to_string()),
    }];
    let err = clear_meta_lib::apply_file(&p, &p, &rules, false).unwrap_err();
    assert!(
        format!("{err}").contains("时间格式"),
        "应给出可读的格式错误，实际：{err}"
    );
    let _ = before;
}

#[test]
fn raw_pdf_date_round_trips() {
    // 已经是 D: 语法时保持同一时刻，不因格式化而漂移
    let normalized = clear_meta_lib::to_pdf_date("D:20260914095053+08'00'").unwrap();
    assert_eq!(normalized, "D:20260914095053+08'00'");
}

#[test]
fn xmp_and_w3cdtf_use_their_own_syntax() {
    let xmp = clear_meta_lib::to_xmp_date("2026-09-14 09:50:53").unwrap();
    assert!(
        xmp.starts_with("2026-09-14T09:50:53"),
        "XMP 必须是 ISO 8601，实际 {xmp}"
    );
    assert!(xmp.ends_with(":00"), "XMP 必须带时区，实际 {xmp}");

    let w3c = clear_meta_lib::to_w3cdtf("2026-09-14 09:50:53").unwrap();
    assert!(w3c.ends_with('Z'), "Office 必须是 UTC 的 W3CDTF，实际 {w3c}");
    assert!(w3c.starts_with("2026-09-14T"), "实际 {w3c}");
}

#[test]
fn office_date_is_written_as_w3cdtf() {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let dir = tmp_dir("office_date");
    let p = dir.join("a.docx");
    let f = fs::File::create(&p).unwrap();
    let mut w = zip::ZipWriter::new(f);
    w.start_file("[Content_Types].xml", SimpleFileOptions::default())
        .unwrap();
    w.write_all(b"<Types/>").unwrap();
    w.start_file("docProps/core.xml", SimpleFileOptions::default())
        .unwrap();
    w.write_all(
        br#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"><dcterms:created xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="dcterms:W3CDTF">2026-09-13T00:00:00Z</dcterms:created></cp:coreProperties>"#,
    )
    .unwrap();
    w.finish().unwrap();

    let rules = vec![clear_meta_lib::FieldRule {
        key: "core:created".to_string(),
        action: clear_meta_lib::FieldAction::Set,
        value: Some("2026-09-14 09:50:53".to_string()),
    }];
    clear_meta_lib::apply_file(&p, &p, &rules, false).expect("apply should succeed");

    let mut archive = zip::ZipArchive::new(fs::File::open(&p).unwrap()).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("docProps/core.xml").unwrap(), &mut xml)
        .unwrap();
    // 只校验语法形态（时区随机器而变，不写死具体小时）
    let value = xml
        .split("<dcterms:created")
        .nth(1)
        .and_then(|s| s.split('>').nth(1))
        .and_then(|s| s.split('<').next())
        .unwrap_or_default();
    assert!(
        value.ends_with('Z') && value.len() == 20 && !value.contains(' '),
        "created 应写成 UTC 的 W3CDTF（YYYY-MM-DDTHH:MM:SSZ），实际：{value}"
    );
    assert!(
        value.starts_with("2026-09-14T") || value.starts_with("2026-09-13T"),
        "日期不应丢失，实际：{value}"
    );
}
