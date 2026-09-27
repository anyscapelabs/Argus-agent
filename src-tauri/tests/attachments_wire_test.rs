use argus_lib::library::schema::MIGRATE as LIB_MIGRATE;

// The whole chain a file walks: dropped on the input, recorded on the message
// by the path it came from, and finally split on the wire into an image the
// model can see and a document whose text it already has.

fn db(dir: &std::path::Path) -> rusqlite::Connection {
    let _ = dir;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    conn.execute_batch(LIB_MIGRATE).unwrap();
    conn
}

fn session(conn: &rusqlite::Connection) -> String {
    use argus_lib::sessions::schema::NewSession;
    argus_lib::sessions::store::create_session(
        conn,
        &NewSession {
            title: "files".into(),
            model_id: Some("mock/test".into()),
            permission: Some("never".into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap()
    .id
}

fn say(conn: &rusqlite::Connection, sid: &str, content: &str, attachments: Option<&str>) {
    use argus_lib::sessions::schema::NewMsg;
    argus_lib::sessions::store::add_msg(
        conn,
        &NewMsg {
            session_id: sid.into(),
            role: "user".into(),
            content: content.into(),
            model_id: None,
            provider_id: None,
            tok_in: None,
            tok_out: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: attachments.map(str::to_string),
        },
    )
    .unwrap();
}

// A one pixel png, so the image is a real image and not a text file named .png.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

fn lib_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("argus-att-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// A picked file stays where it is. The picker never copies it anywhere.
fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> String {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn an_image_and_a_document_both_reach_the_model_in_one_turn() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    let img = write(&dir, "shot.png", PNG);
    let doc = write(&dir, "Notes.md", b"# the finding\n\neleven percent.");

    say(
        &conn,
        &sid,
        "what do you make of these",
        Some(
            &serde_json::json!([
                { "path": img, "name": "shot.png", "kind": "image", "sz": PNG.len() },
                { "path": doc, "name": "Notes.md", "kind": "doc", "sz": 33 }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().expect("the user message must be on the wire");

    // The model can see the image, so it rides along as a path.
    assert_eq!(m.images.len(), 1, "images: {:?}", m.images);
    assert!(m.images[0].ends_with("shot.png"), "{}", m.images[0]);

    // The document's text comes with it. Making the model call a tool to learn
    // what the user just attached spends a turn to arrive where it already was.
    assert!(m.content.contains("eleven percent"), "{}", m.content);
    assert!(m.content.contains("--- Notes.md ---"), "{}", m.content);
    assert!(
        !m.content.contains("library.read"),
        "nothing to open: {}",
        m.content
    );
    assert!(
        m.content.starts_with("what do you make of these"),
        "{}",
        m.content
    );
}

#[test]
fn a_document_with_no_readable_text_is_named_rather_than_dropped() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    // Mostly replacement characters: a file the extractor will not claim is
    // text. It still has to reach the model as something it can open.
    let junk = vec![0xffu8; 64 * 1024];
    let doc = write(&dir, "Scan.bin", &junk);

    say(
        &conn,
        &sid,
        "what is this",
        Some(
            &serde_json::json!([
                { "path": doc, "name": "Scan.bin", "kind": "doc", "sz": junk.len() }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    assert!(m.content.contains("Scan.bin"), "{}", m.content);
    assert!(
        m.content.contains(&doc),
        "the path is what opens it: {}",
        m.content
    );
    assert!(m.content.contains("fs.read"), "{}", m.content);
    assert!(!m.content.contains("library.read"), "{}", m.content);
}

#[test]
fn a_file_the_library_has_lost_is_skipped_rather_than_failing_the_turn() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    say(
        &conn,
        &sid,
        "read these",
        Some(
            &serde_json::json!([
                { "path": "/nowhere/missing.md", "name": "missing.md", "kind": "doc", "sz": 10 }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    // It is still named, so the model learns the file could not be opened
    // rather than being handed a turn that silently lost it.
    assert!(m.content.contains("missing.md"), "{}", m.content);
    assert!(m.images.is_empty());
}

#[test]
fn a_message_of_only_files_still_carries_the_note() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    let doc = write(&dir, "Notes.md", b"body");
    say(
        &conn,
        &sid,
        "",
        Some(
            &serde_json::json!([{ "path": doc, "name": "Notes.md", "kind": "doc", "sz": 4 }])
                .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    assert!(m.content.starts_with("<attachments>"), "{}", m.content);
    assert!(
        !m.content.starts_with("\n"),
        "no leading blank: {:?}",
        m.content
    );
}

#[test]
fn a_message_written_before_attachments_existed_still_projects() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    say(&conn, &sid, "an old message", None);

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    assert_eq!(m.content, "an old message");
    assert!(m.images.is_empty());
}

#[test]
fn a_document_past_the_old_cut_arrives_whole() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    // Well past where the inline copy used to stop at 20 000 characters, and
    // past what a 12 000 character read returned.
    let big = "the quick brown fox jumps over the lazy dog\n".repeat(1000);
    assert!(big.len() > 40_000, "{} chars", big.len());

    let doc = write(&dir, "Big.md", big.as_bytes());

    say(
        &conn,
        &sid,
        "review it",
        Some(
            &serde_json::json!([
                { "path": doc, "name": "Big.md", "kind": "doc", "sz": big.len() }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    // Every line is here. A cut at 20 000 would have kept 573 of the 1000.
    assert_eq!(
        m.content.matches("lazy dog").count(),
        1000,
        "the file arrived whole: {}",
        &m.content[..200.min(m.content.len())]
    );
    assert!(
        !m.content.contains("cut at"),
        "nothing was cut: {}",
        m.content
    );
    assert!(
        !m.content.contains("fs.read"),
        "no read needed: {}",
        m.content
    );
}

#[test]
fn a_file_too_big_for_one_message_is_named_not_halved() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    // Over the 8 MB read cap, so it cannot be inlined at all.
    let huge = vec![b'x'; 9_000_000];
    let doc = write(&dir, "Huge.txt", &huge);

    say(
        &conn,
        &sid,
        "read it",
        Some(
            &serde_json::json!([
                { "path": doc, "name": "Huge.txt", "kind": "doc", "sz": huge.len() }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    // Named, with the path that opens it, and carrying not one character of a
    // file the model might mistake for a whole one.
    assert!(m.content.contains("Huge.txt"), "{}", m.content);
    assert!(m.content.contains(&doc), "{}", m.content);
    assert!(m.content.contains("fs.read"), "{}", m.content);
    assert!(
        !m.content.contains("xxxx"),
        "no partial text: {}",
        &m.content[..200.min(m.content.len())]
    );
}

#[test]
fn a_row_written_before_paths_still_reads_through_the_library() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    // The shape the picker used to write: a library id, no path.
    let doc = argus_lib::library::store::create_bytes(
        &conn,
        &dir,
        "Old",
        "md",
        b"written the old way",
        None,
    )
    .unwrap()
    .id;

    say(
        &conn,
        &sid,
        "look at this",
        Some(
            &serde_json::json!([
                { "id": doc, "name": "Old.md", "kind": "doc", "sz": 19 }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    assert!(
        m.content.contains("written the old way"),
        "history is not broken by a schema change: {}",
        m.content
    );
}
