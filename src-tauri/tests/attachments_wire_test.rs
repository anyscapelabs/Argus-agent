use argus_lib::library::schema::MIGRATE as LIB_MIGRATE;

// The whole chain a file walks: dropped on the input, copied into the library,
// recorded on the message, and finally split on the wire into an image the
// model can see and a document it is told to open.

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

// The library appends the extension itself, so the name is a stem -- the same
// contract the picker goes through.
fn add(
    conn: &rusqlite::Connection,
    dir: &std::path::Path,
    stem: &str,
    ext: &str,
    bytes: &[u8],
) -> String {
    argus_lib::library::store::create_bytes(conn, dir, stem, ext, bytes, None)
        .unwrap()
        .id
}

#[test]
fn an_image_goes_inline_and_a_document_gets_named() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    let img = add(&conn, &dir, "shot", "png", PNG);
    let doc = add(
        &conn,
        &dir,
        "Notes",
        "md",
        b"# the finding\n\neleven percent.",
    );

    say(
        &conn,
        &sid,
        "what do you make of these",
        Some(
            &serde_json::json!([
                { "id": img, "name": "shot.png", "kind": "image", "sz": PNG.len() },
                { "id": doc, "name": "Notes.md", "kind": "doc", "sz": 33 }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().expect("the user message must be on the wire");

    // The model can see the image, so it rides along as a path.
    assert_eq!(m.images.len(), 1, "images: {:?}", m.images);
    assert!(m.images[0].ends_with("shot.png"), "{}", m.images[0]);

    // It cannot see the document, so it has to be told it is there.
    assert!(m.content.contains("Notes.md"), "{}", m.content);
    assert!(m.content.contains("library.read"), "{}", m.content);
    assert!(
        !m.content.contains("eleven percent"),
        "a document must not be inlined: {}",
        m.content
    );
    assert!(
        m.content.starts_with("what do you make of these"),
        "{}",
        m.content
    );
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
                { "id": "gone", "name": "missing.md", "kind": "doc", "sz": 10 }
            ])
            .to_string(),
        ),
    );

    let p = argus_lib::prompt::project(&conn, &sid, &dir).unwrap();
    let m = p.msgs.last().unwrap();

    // The id is still listed, so the model learns it could not be opened
    // rather than being handed a turn that silently lost a file.
    assert!(m.content.contains("missing.md"), "{}", m.content);
    assert!(m.images.is_empty());
}

#[test]
fn a_message_of_only_files_still_carries_the_note() {
    let dir = lib_dir();
    let conn = db(&dir);
    let sid = session(&conn);

    let doc = add(&conn, &dir, "Notes", "md", b"body");
    say(
        &conn,
        &sid,
        "",
        Some(
            &serde_json::json!([{ "id": doc, "name": "Notes.md", "kind": "doc", "sz": 4 }])
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
