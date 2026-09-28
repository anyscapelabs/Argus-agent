use argus_lib::library::store;

fn lib() -> (rusqlite::Connection, std::path::PathBuf) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    store::migrate(&conn).unwrap();
    let dir = std::env::temp_dir().join(format!("argus-lib-dup-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    (conn, dir)
}

// The bug from the Nkrumah session: the model re-issued the same doc.create
// four times, and each one wrote a real file. Four documents, four cards, and
// the only difference between them was a `-2` suffix the store invented.
#[test]
fn the_same_bytes_do_not_become_two_documents() {
    let (conn, dir) = lib();
    let body = b"# Nkrumah\n\nWho he was.";

    let a = store::create_bytes(&conn, &dir, "Nkrumah", "docx", body, Some("s1")).unwrap();
    let b = store::create_bytes(&conn, &dir, "Nkrumah", "docx", body, Some("s1")).unwrap();

    assert_eq!(a.id, b.id, "the same call must not mint a second document");
    assert_eq!(a.path, b.path, "{a:?} / {b:?}");

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM library", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "one call, one row");
}

// A different document under the same name is a different document. Deduping
// on the name alone would throw away work the model actually did.
#[test]
fn different_content_is_still_a_different_document() {
    let (conn, dir) = lib();

    let a = store::create_bytes(&conn, &dir, "Notes", "docx", b"first", Some("s1")).unwrap();
    let b = store::create_bytes(&conn, &dir, "Notes", "docx", b"second", Some("s1")).unwrap();

    assert_ne!(a.id, b.id);
}

// Two sessions asking for the same file are two deliberate acts. Collapsing
// them hides a document one of them expected to find.
#[test]
fn the_same_bytes_in_two_sessions_are_two_documents() {
    let (conn, dir) = lib();
    let body = b"shared";

    let a = store::create_bytes(&conn, &dir, "Shared", "docx", body, Some("s1")).unwrap();
    let b = store::create_bytes(&conn, &dir, "Shared", "docx", body, Some("s2")).unwrap();

    assert_ne!(a.id, b.id, "a session boundary is a real boundary");
}

// A library written before the sha column existed has to keep working.
#[test]
fn a_library_without_the_sha_column_migrates() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE library (
           id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL,
           ext TEXT NOT NULL, path TEXT NOT NULL UNIQUE, session_id TEXT,
           sz INTEGER NOT NULL DEFAULT 0,
           created_at TEXT NOT NULL DEFAULT (datetime('now')));",
    )
    .unwrap();

    store::migrate(&conn).unwrap();

    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('library') WHERE name = 'sha'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "migrate must add sha to a table that predates it");
}
