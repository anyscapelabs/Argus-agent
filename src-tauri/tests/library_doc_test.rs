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
    assert!(xlsx
        .bytes
        .windows(24)
        .any(|w| w == b"xl/worksheets/sheet1.xml"));

    let pptx = doc::build(
        "pptx",
        "Deck",
        &serde_json::json!({ "slides": [{ "title": "Intro", "bullets": ["one", "two"] }] }),
    )
    .unwrap();
    assert_eq!(&pptx.bytes[0..4], b"PK\x03\x04");
    assert!(pptx.pages == 1);
    assert!(pptx
        .bytes
        .windows(21)
        .any(|w| w == b"ppt/slides/slide1.xml"));
}

#[test]
fn docx_preview_extracts_title_and_body_as_markdown() {
    let docx = doc::build("docx", "Title", &args("line one\nline two")).unwrap();
    let md = doc::preview_docx(&docx.bytes).expect("preview");
    assert!(md.contains("# Title"));
    assert!(md.contains("line one"));
    assert!(md.contains("line two"));
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
    conn.execute_batch(argus_lib::library::schema::MIGRATE)
        .unwrap();
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

#[test]
fn library_download_copies_to_downloads_with_dedupe() {
    let dir = std::env::temp_dir().join(format!("argus-lib-dl-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, seq INTEGER, role TEXT, content TEXT, active INTEGER DEFAULT 1);
         CREATE TABLE summaries (id TEXT PRIMARY KEY, session_id TEXT, covers_to INTEGER, content TEXT);",
    )
    .unwrap();
    conn.execute_batch(argus_lib::library::schema::MIGRATE)
        .unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    let item = argus_lib::library::store::create_bytes(&conn, &dir, "Notes", "txt", b"hello", None)
        .unwrap();
    let first = argus_lib::library::store::download(&conn, &dir, &item.id).unwrap();
    let second = argus_lib::library::store::download(&conn, &dir, &item.id).unwrap();
    assert!(std::path::Path::new(&first.dest).exists());
    assert!(std::path::Path::new(&second.dest).exists());
    assert_ne!(first.dest, second.dest);
    assert_eq!(std::fs::read(&second.dest).unwrap(), b"hello");
    let _ = std::fs::remove_file(&first.dest);
    let _ = std::fs::remove_file(&second.dest);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn library_preview_returns_text_for_text_and_none_for_office() {
    let dir = std::env::temp_dir().join(format!("argus-lib-pv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, seq INTEGER, role TEXT, content TEXT, active INTEGER DEFAULT 1);
         CREATE TABLE summaries (id TEXT PRIMARY KEY, session_id TEXT, covers_to INTEGER, content TEXT);",
    )
    .unwrap();
    conn.execute_batch(argus_lib::library::schema::MIGRATE)
        .unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    let txt =
        argus_lib::library::store::create_bytes(&conn, &dir, "Doc", "md", b"# hi", None).unwrap();
    let pv = argus_lib::library::store::preview(&conn, &dir, &txt.id, 12000).unwrap();
    assert_eq!(pv.text.as_deref(), Some("# hi"));
    assert!(!pv.truncated);
    let long = "w ".repeat(9000);
    let big =
        argus_lib::library::store::create_bytes(&conn, &dir, "Big", "txt", long.as_bytes(), None)
            .unwrap();
    let pv2 = argus_lib::library::store::preview(&conn, &dir, &big.id, 1000).unwrap();
    assert!(pv2.truncated);
    assert_eq!(pv2.text.map(|t| t.chars().count()), Some(1000));
    let pdf = argus_lib::library::store::create_bytes(&conn, &dir, "R", "pdf", b"%PDF-1.4", None)
        .unwrap();
    let pv3 = argus_lib::library::store::preview(&conn, &dir, &pdf.id, 12000).unwrap();
    assert!(pv3.text.is_none());
    assert!(argus_lib::library::store::preview(&conn, &dir, "missing", 12000).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
