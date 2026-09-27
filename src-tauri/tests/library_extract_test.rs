use argus_lib::library::{doc, extract};

// A docx a real word processor wrote, not one Argus built. The old reader
// only understood stored zip entries and returned nothing for every one of
// these.
fn deflated_docx(body: &str) -> Vec<u8> {
    let xml = format!(
        r#"<?xml version="1.0"?><w:document><w:body>
           <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Quarterly</w:t></w:r></w:p>
           <w:p><w:r><w:t>{body}</w:t></w:r></w:p>
           </w:body></w:document>"#
    );

    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("word/document.xml", opts).unwrap();
        std::io::Write::write_all(&mut zip, xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    buf
}

#[test]
fn a_deflated_docx_is_read_where_a_stored_one_was_not() {
    let bytes = deflated_docx("Revenue grew by eleven percent.");

    // Prove the fixture is the hard case, not the easy one.
    assert_ne!(&bytes[8..10], &[0, 0], "entry must be deflate, not stored");

    let text = extract::text(&bytes, "docx").expect("a deflated docx must read");
    assert!(text.contains("Quarterly"), "{text}");
    assert!(text.contains("eleven percent"), "{text}");
    assert!(
        text.starts_with("# Quarterly"),
        "headings stay markdown: {text}"
    );
}

#[test]
fn a_docx_argus_built_itself_still_reads() {
    let built = doc::build(
        "docx",
        "Notes",
        &serde_json::json!({ "content": "plain body" }),
    )
    .unwrap();
    let text = extract::text(&built.bytes, &built.ext).expect("our own docx must read");
    assert!(text.contains("plain body"), "{text}");
}

#[test]
fn a_pdf_argus_built_itself_reads_back() {
    let built = doc::build(
        "pdf",
        "Report",
        &serde_json::json!({ "content": "the finding" }),
    )
    .unwrap();
    let text = extract::text(&built.bytes, "pdf").expect("a generated pdf must read");
    assert!(text.contains("finding"), "{text}");
}

#[test]
fn a_spreadsheet_becomes_a_table() {
    let rows = serde_json::json!({ "rows": [["region", "total"], ["north", "120"]] });
    let built = doc::build("xlsx", "", &rows).unwrap();
    let text = extract::text(&built.bytes, "xlsx").expect("xlsx must read");
    assert!(text.contains("region"), "{text}");
    assert!(text.contains("north"), "{text}");
    assert!(text.contains("| ---"), "{text}");
}

#[test]
fn a_deck_keeps_its_slides_in_order() {
    let slides = serde_json::json!({
        "slides": [
            { "title": "Opening", "bullets": ["first point"] },
            { "title": "Closing", "bullets": ["last point"] }
        ]
    });
    let built = doc::build("pptx", "", &slides).unwrap();
    let text = extract::text(&built.bytes, "pptx").expect("pptx must read");

    let opening = text.find("first point").expect("slide one must be there");
    let closing = text.find("last point").expect("slide two must be there");
    assert!(opening < closing, "slides must stay in order: {text}");
}

#[test]
fn a_source_file_is_text_and_nobody_had_to_list_it() {
    let src = b"fn main() { println!(\"hi\"); }";
    let text = extract::text(src, "rs").expect("code is text");
    assert!(text.contains("fn main"));
}

#[test]
fn a_binary_file_says_it_has_no_text() {
    // A real binary, not just invalid bytes.
    let png = [
        0x89u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xd8, 0xff,
    ];
    assert!(extract::text(&png, "png").is_none());
    assert!(extract::text(&png, "bin").is_none());
}

#[test]
fn an_empty_or_missing_file_is_never_an_empty_string() {
    assert!(extract::text(b"", "txt").is_none());
    assert!(extract::text(b"   \n  ", "txt").is_none());
}

#[test]
fn a_file_too_big_to_be_worth_reading_is_refused() {
    let big = vec![b'a'; extract::MAX_BYTES + 1];
    assert!(extract::text(&big, "txt").is_none());
}

// The tool the model calls. Extraction is only worth anything if the model can
// reach it by id, so the tool gets its own tests.
fn lib() -> (rusqlite::Connection, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("argus-libread-{}", uuid::Uuid::new_v4()));
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

    (conn, dir)
}

fn call(conn: &rusqlite::Connection, dir: &std::path::Path, id: &str) -> Result<String, String> {
    argus_lib::tools::library::read(conn, dir, &serde_json::json!({ "id": id }))
}

#[test]
fn reading_by_id_returns_the_text_named_by_its_header() {
    let (conn, dir) = lib();
    let item =
        argus_lib::library::store::create_bytes(&conn, &dir, "Notes", "md", b"the finding", None)
            .unwrap();

    let out = call(&conn, &dir, &item.id).expect("the id the attachment listed must read");
    assert!(out.contains("Notes (md"), "the file names itself: {out}");
    assert!(out.contains("the finding"), "{out}");
}

#[test]
fn an_unreadable_file_is_an_error_naming_it_not_an_empty_string() {
    let (conn, dir) = lib();
    let png = [
        0x89u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xd8, 0xff,
    ];
    let item =
        argus_lib::library::store::create_bytes(&conn, &dir, "Photo", "png", &png, None).unwrap();

    // An empty result would read as a blank file. A refusal reads as a refusal.
    let err = call(&conn, &dir, &item.id).unwrap_err();
    assert!(err.contains("no text"), "{err}");
}

#[test]
fn an_id_the_library_never_had_is_refused_rather_than_answered() {
    let (conn, dir) = lib();
    let err = call(&conn, &dir, "not-a-real-id").unwrap_err();
    assert!(err.contains("not-a-real-id"), "{err}");
}

#[test]
fn a_long_file_is_cut_at_the_limit_and_says_so() {
    let (conn, dir) = lib();
    let long = "abcdefghij".repeat(2000);
    let item =
        argus_lib::library::store::create_bytes(&conn, &dir, "Long", "txt", long.as_bytes(), None)
            .unwrap();

    let out = argus_lib::tools::library::read(
        &conn,
        &dir,
        &serde_json::json!({ "id": item.id, "max_chars": 500 }),
    )
    .unwrap();

    assert!(out.contains("truncated at 500 characters"), "{out}");
}

#[test]
fn the_picker_payload_deserializes_the_camel_case_the_frontend_sends() {
    // Tauri rewrites the top-level command args, not the fields inside a
    // struct. A payload that does not deserialize is a picker that silently
    // adds nothing, which is exactly how this reached a screenshot.
    let item: argus_lib::library::schema::NewLibItem = serde_json::from_value(serde_json::json!({
        "sourcePath": "/home/u/shot.png",
        "name": "shot",
        "sessionId": "s1",
    }))
    .expect("camelCase must deserialize");

    assert_eq!(item.source_path, "/home/u/shot.png");
    assert_eq!(item.session_id.as_deref(), Some("s1"));
}
