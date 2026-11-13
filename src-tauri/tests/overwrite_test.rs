use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("clear_meta_test_{}_{}", std::process::id(), name));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 构造一个最小 docx（zip + docProps/core.xml）
fn make_docx(path: &PathBuf) {
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;
    let f = fs::File::create(path).unwrap();
    let mut w = ZipWriter::new(f);
    w.start_file("[Content_Types].xml", SimpleFileOptions::default()).unwrap();
    w.write_all(b"<Types/>").unwrap();
    w.start_file("docProps/core.xml", SimpleFileOptions::default()).unwrap();
    w.write_all(br#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"><dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/">tester</dc:creator></cp:coreProperties>"#).unwrap();
    w.finish().unwrap();
}

#[test]
fn office_inplace_overwrite_should_work() {
    let dir = tmp_dir("office");
    let p = dir.join("a.docx");
    make_docx(&p);

    // 先像应用一样读一次元数据（模拟 UI 预览流程）
    let fields = clear_meta_lib::read_file_metadata(&p).unwrap();
    assert!(!fields.is_empty());

    // 覆盖原文件
    clear_meta_lib::apply_file(&p, &p, &[], true).unwrap_or_else(|e| {
        panic!("in-place overwrite failed: {e}");
    });
    assert!(p.exists());
}

#[test]
fn image_inplace_overwrite_should_work() {
    let dir = tmp_dir("image");
    let p = dir.join("a.png");
    let png: &[u8] = &[
        0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A,0,0,0,13,0x49,0x48,0x44,0x52,
        0,0,0,1,0,0,0,1,8,6,0,0,0,0x1F,0x15,0xC4,0x89,0,0,0,10,0x49,0x44,0x41,0x54,
        0x78,0x9C,0x63,0,1,0,0,5,0,1,0x0D,0x0A,0x2D,0xB4,0,0,0,0,0x49,0x45,0x4E,0x44,
        0xAE,0x42,0x60,0x82,
    ];
    fs::write(&p, png).unwrap();
    clear_meta_lib::apply_file(&p, &p, &[], true).unwrap_or_else(|e| panic!("in-place overwrite failed: {e}"));
}

#[test]
fn pdf_inplace_overwrite_should_work() {
    let dir = tmp_dir("pdf");
    let p = dir.join("a.pdf");
    let mut doc = lopdf::Document::with_version("1.5");
    let page_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name("Page".into())),
        ("Parent", lopdf::Object::Reference((1, 0))), // 占位，稍后回填
    ])));
    let pages_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name("Pages".into())),
        ("Count", lopdf::Object::Integer(1)),
        ("Kids", lopdf::Object::Array(vec![lopdf::Object::Reference(page_id)])),
    ])));
    // 回填 Parent
    if let lopdf::Object::Dictionary(d) = doc.get_object_mut(page_id).unwrap() {
        d.set("Parent", lopdf::Object::Reference(pages_id));
    }
    let cat_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name("Catalog".into())),
        ("Pages", lopdf::Object::Reference(pages_id)),
    ])));
    let info_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter([
        ("Title", lopdf::Object::String("secret".as_bytes().to_vec(), lopdf::StringFormat::Literal)),
        ("Author", lopdf::Object::String("tester".as_bytes().to_vec(), lopdf::StringFormat::Literal)),
    ])));
    doc.trailer.set("Root", lopdf::Object::Reference(cat_id));
    doc.trailer.set("Info", lopdf::Object::Reference(info_id));
    doc.compress();
    doc.save(&p).unwrap();

    let fields = clear_meta_lib::read_file_metadata(&p).unwrap();
    assert!(!fields.is_empty(), "pdf metadata should be read");
    clear_meta_lib::apply_file(&p, &p, &[], true).unwrap_or_else(|e| panic!("in-place overwrite failed: {e}"));
}

/// 覆盖后必须还原原文件的修改时间（用户核心诉求：不能暴露处理时间）。
#[test]
fn overwrite_preserves_modified_time() {
    let dir = tmp_dir("mtime");
    let p = dir.join("a.docx");
    make_docx(&p);

    // 把修改时间设到过去，确保和「现在」有可见差异
    let past = filetime::FileTime::from_unix_time(1_700_000_000, 0);
    filetime::set_file_mtime(&p, past).unwrap();

    clear_meta_lib::apply_file(&p, &p, &[], true).unwrap();

    let mtime = std::fs::metadata(&p).unwrap().modified().unwrap();
    let expected = filetime::FileTime::from_unix_time(1_700_000_000, 0);
    let got = filetime::FileTime::from_system_time(mtime);
    assert_eq!(got, expected, "modified time must be preserved after in-place overwrite");
}

/// 文件被外部程序独占打开时：重试后给出 FileLocked 明确报错，且原文件完好。
#[cfg(windows)]
#[test]
fn locked_file_reports_friendly_error() {
    use std::os::windows::fs::OpenOptionsExt;

    let dir = tmp_dir("locked");
    let p = dir.join("a.docx");
    make_docx(&p);
    let before = std::fs::read(&p).unwrap();

    // 以「不允许共享删除/写入」方式持有文件，模拟另一个程序占用
    let _guard = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1) // FILE_SHARE_READ only
        .open(&p)
        .unwrap();

    let err = clear_meta_lib::apply_file(&p, &p, &[], true).unwrap_err();
    drop(_guard);
    let msg = err.to_string();
    assert!(msg.contains("占用"), "message should mention occupation: {msg}");
    // 原文件必须完好
    let after = std::fs::read(&p).unwrap();
    assert_eq!(before, after, "original file must stay intact when lock prevents overwrite");
}
