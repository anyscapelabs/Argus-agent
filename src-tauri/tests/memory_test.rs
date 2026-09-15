use argus_lib::memory::schema::NewMemory;
use argus_lib::memory::store;

fn test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, seq INTEGER, role TEXT, content TEXT, active INTEGER DEFAULT 1);
         CREATE TABLE summaries (id TEXT PRIMARY KEY, session_id TEXT, covers_to INTEGER, content TEXT);",
    )
    .unwrap();
    store::migrate(&conn).unwrap();
    conn
}

#[test]
fn saves_and_searches_memories() {
    let conn = test_db();
    let m = store::save(
        &conn,
        &NewMemory {
            content: "user prefers dark mode in the terminal".into(),
            kind: Some("preference".into()),
            importance: Some(3),
            session_id: None,
        },
    )
    .unwrap();
    assert_eq!(m.kind, "preference");
    let hits = store::search(&conn, "dark mode terminal", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, m.id);
}

#[test]
fn rejects_bad_kinds_and_empty_content() {
    let conn = test_db();
    let m = store::save(
        &conn,
        &NewMemory {
            content: "project uses sqlite fts5".into(),
            kind: Some("weird".into()),
            importance: Some(9),
            session_id: None,
        },
    )
    .unwrap();
    assert_eq!(m.kind, "fact");
    assert_eq!(m.importance, 5);
    let err = store::save(
        &conn,
        &NewMemory {
            content: "   ".into(),
            kind: None,
            importance: None,
            session_id: None,
        },
    );
    assert!(err.is_err());
}

#[test]
fn links_form_graph_edges() {
    let conn = test_db();
    let a = store::save(
        &conn,
        &NewMemory {
            content: "big data analytics module uses aktunotes".into(),
            kind: Some("project".into()),
            importance: None,
            session_id: None,
        },
    )
    .unwrap();
    let b = store::save(
        &conn,
        &NewMemory {
            content: "aktunotes pdfs live on the desktop folder".into(),
            kind: Some("fact".into()),
            importance: None,
            session_id: None,
        },
    )
    .unwrap();
    store::link(&conn, &a.id, &b.id, "uses").unwrap();
    let g = store::load_graph(&conn, 50).unwrap();
    assert!(g.nodes.len() >= 2);
    assert!(g.edges.iter().any(|e| e.relation == "uses"));
}

#[test]
fn message_update_and_delete_keep_fts_usable() {
    let conn = test_db();
    conn.execute(
        "INSERT INTO messages (id, session_id, seq, role, content) VALUES ('m9', 's9', 1, 'assistant', 'drafting the quarterly report now')",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE messages SET content = 'finished the quarterly report draft' WHERE id = 'm9'",
        [],
    )
    .unwrap();
    let hits = store::recall(&conn, "quarterly report draft", 8).unwrap();
    assert!(hits.iter().any(|h| h.source == "message"));
    conn.execute("DELETE FROM messages WHERE id = 'm9'", [])
        .unwrap();
    let hits = store::recall(&conn, "quarterly report draft", 8).unwrap();
    assert!(!hits.iter().any(|h| h.source == "message"));
}

#[test]
fn recall_spans_messages_summaries_files_and_graph() {
    let conn = test_db();
    conn.execute(
        "INSERT INTO messages (id, session_id, seq, role, content) VALUES ('m1', 's1', 1, 'user', 'where are aktunotes big data pyq papers')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO summaries (id, session_id, covers_to, content) VALUES ('s1', 's1', 4, 'researched aktunotes pyq sources')",
        [],
    )
    .unwrap();
    store::index_file(&conn, "f1", Some("s1"), "aktunotes kds601 big data analytics pyq pdf").unwrap();
    let mem = store::save(
        &conn,
        &NewMemory {
            content: "aktunotes is the approved pyq source".into(),
            kind: Some("decision".into()),
            importance: Some(5),
            session_id: Some("s1".into()),
        },
    )
    .unwrap();
    let hits = store::recall(&conn, "aktunotes pyq", 12).unwrap();
    let sources: Vec<String> = hits.iter().map(|h| h.source.clone()).collect();
    assert!(sources.contains(&"memory".to_string()));
    assert!(sources.contains(&"message".to_string()));
    assert!(sources.contains(&"summary".to_string()));
    assert!(sources.contains(&"file".to_string()));
    let _ = mem;
}
