use argus_lib::library::doc;

fn args(content: &str) -> serde_json::Value {
    serde_json::json!({ "content": content })
}

#[test]
fn rejects_images_and_unknown_kinds() {
    assert!(doc::kind_of("photo.png").is_none());
    assert!(doc::kind_of("shot.jpg").is_none());
    assert!(doc::kind_of("archive.zip").is_none());
    assert_eq!(doc::kind_of("notes.pdf"), Some("pdf"));
    assert_eq!(doc::kind_of("report.docx"), Some("docx"));
    assert_eq!(doc::kind_of("deck.pptx"), Some("pptx"));
    assert_eq!(doc::kind_of("data.xlsx"), Some("xlsx"));
    let err = doc::build("png", "t", &args("hi"));
    assert!(err.is_err());
}

#[test]
fn builds_text_and_csv_bytes() {
    let md = doc::build("md", "Title", &args("# hello")).unwrap();
    assert_eq!(md.ext, "md");
    assert!(md.bytes.starts_with(b"# hello"));

    let rows = serde_json::json!({ "rows": [["a", "b"], ["1", "x,y"]] });
    let csv = doc::build("csv", "", &rows).unwrap();
    let text = String::from_utf8(csv.bytes).unwrap();
    assert!(text.contains("\"x,y\""));
}

#[test]
fn builds_pdf_with_header_and_pages() {
    let long = "word ".repeat(2000);
    let pdf = doc::build("pdf", "Report", &args(&long)).unwrap();
    assert_eq!(pdf.ext, "pdf");
    assert!(pdf.bytes.starts_with(b"%PDF-1.4"));
    assert!(pdf.pages >= 2);
    let tail = String::from_utf8_lossy(&pdf.bytes[pdf.bytes.len() - 20..]);
    assert!(tail.contains("%%EOF"));
}

#[test]
fn builds_zip_office_formats() {
    let docx = doc::build("docx", "Title", &args("line one\nline two")).unwrap();
    assert_eq!(&docx.bytes[0..4], b"PK\x03\x04");
    assert!(docx.bytes.windows(17).any(|w| w == b"word/document.xml"));

    let xlsx = doc::build(
        "xlsx",
        "",
        &serde_json::json!({ "rows": [["name", "n"], ["a", "1"]] }),
    )
    .unwrap();
    assert_eq!(&xlsx.bytes[0..4], b"PK\x03\x04");
    assert!(xlsx.bytes.windows(24).any(|w| w == b"xl/worksheets/sheet1.xml"));

    let pptx = doc::build(
        "pptx",
        "Deck",
        &serde_json::json!({ "slides": [{ "title": "Intro", "bullets": ["one", "two"] }] }),
    )
    .unwrap();
    assert_eq!(&pptx.bytes[0..4], b"PK\x03\x04");
    assert!(pptx.pages == 1);
    assert!(pptx.bytes.windows(21).any(|w| w == b"ppt/slides/slide1.xml"));
}

#[test]
fn library_create_bytes_roundtrip() {
    let dir = std::env::temp_dir().join(format!("argus-lib-doc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, seq INTEGER, role TEXT, content TEXT, active INTEGER DEFAULT 1);
         CREATE TABLE summaries (id TEXT PRIMARY KEY, session_id TEXT, covers_to INTEGER, content TEXT);",
    )
    .unwrap();
    conn.execute_batch(argus_lib::library::schema::MIGRATE).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    let item = argus_lib::library::store::create_bytes(
        &conn,
        &dir,
        "Budget Sheet",
        "csv",
        b"a,b\n1,2",
        None,
    )
    .unwrap();
    assert_eq!(item.ext, "csv");
    assert_eq!(item.kind, "sheet");
    assert!(dir.join(&item.path).exists());
    let _ = std::fs::remove_dir_all(&dir);
}
