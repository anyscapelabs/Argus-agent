use argus_lib::sessions::browser_import::SKIP_DIRS;
use argus_lib::sessions::ext_install::unpacked_id;
use argus_lib::sessions::schema::Session;

use std::path::Path;

#[test]
fn session_deserializes_from_ui_row() {
    let js = serde_json::json!({
        "id": "628a",
        "title": "t",
        "status": "live",
        "model_id": null,
        "permission": "never",
        "folder_id": null,
        "created_at": "2026-09-09 06:21:23",
        "updated_at": "2026-09-09 06:21:23",
        "ctx_tokens": 0,
        "compact_seq": 0,
        "compactions": 0
    });

    let s: Result<Session, _> = serde_json::from_value(js);
    assert!(s.is_ok(), "{s:?}");
}

#[test]
fn skips_cache_dirs_wherever_they_live() {
    assert!(SKIP_DIRS.contains(&"Cache"));
    assert!(SKIP_DIRS.contains(&"Service Worker"));
    assert!(SKIP_DIRS.contains(&"component_crx_cache"));
}

#[test]
fn id_is_32_ap_letters() {
    let dir = Path::new("/tmp/argus-extension");
    let id = unpacked_id(dir);

    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| ('a'..='p').contains(&c)));
}

fn dupe_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    for id in ["s1", "s2", "s3"] {
        conn.execute("INSERT INTO sessions (id, title) VALUES (?1, 't')", [id])
            .unwrap();
    }
    conn
}

fn user_msg(conn: &rusqlite::Connection, session: &str, content: &str) {
    use argus_lib::sessions::schema::NewMsg;
    argus_lib::sessions::store::add_msg(
        conn,
        &NewMsg {
            session_id: session.into(),
            role: "user".into(),
            content: content.into(),
            model_id: None,
            provider_id: None,
            tok_in: None,
            tok_out: None,
            tool_calls: None,
            tool_call_id: None,
        },
    )
    .unwrap();
}

fn assistant_msg(conn: &rusqlite::Connection, session: &str, content: &str) {
    use argus_lib::sessions::schema::NewMsg;
    argus_lib::sessions::store::add_msg(
        conn,
        &NewMsg {
            session_id: session.into(),
            role: "assistant".into(),
            content: content.into(),
            model_id: None,
            provider_id: None,
            tok_in: None,
            tok_out: None,
            tool_calls: None,
            tool_call_id: None,
        },
    )
    .unwrap();
}

#[test]
fn unreplied_identical_send_counts_as_duplicate() {
    use argus_lib::sessions::store::has_unreplied_duplicate;

    let conn = dupe_db();
    user_msg(&conn, "s1", "tell me how to add the extension");
    assert!(has_unreplied_duplicate(&conn, "s1", "tell me how to add the extension").unwrap());
    assert!(has_unreplied_duplicate(&conn, "s1", "  tell me how to add the extension\n").unwrap());
}

#[test]
fn duplicate_detection_ignores_replied_other_and_empty() {
    use argus_lib::sessions::store::has_unreplied_duplicate;

    let conn = dupe_db();
    assert!(!has_unreplied_duplicate(&conn, "s1", "hello").unwrap());

    user_msg(&conn, "s1", "hello");
    assistant_msg(&conn, "s1", "hi there");
    assert!(!has_unreplied_duplicate(&conn, "s1", "hello").unwrap());

    user_msg(&conn, "s2", "hello");
    assert!(!has_unreplied_duplicate(&conn, "s2", "different").unwrap());
    assert!(!has_unreplied_duplicate(&conn, "s2", "").unwrap());
    assert!(!has_unreplied_duplicate(&conn, "s2", "   ").unwrap());

    user_msg(
        &conn,
        "s3",
        "<tool-result tool=\"x\" status=\"ok\">y</tool-result>",
    );
    assert!(!has_unreplied_duplicate(
        &conn,
        "s3",
        "<tool-result tool=\"x\" status=\"ok\">y</tool-result>"
    )
    .unwrap());
}

#[test]
fn duplicate_window_expires_after_sixty_seconds() {
    use argus_lib::sessions::store::has_unreplied_duplicate;

    let conn = dupe_db();
    user_msg(&conn, "s1", "hello");
    conn.execute(
        "UPDATE messages SET created_at = datetime('now', '-120 seconds') WHERE session_id = 's1'",
        [],
    )
    .unwrap();
    assert!(!has_unreplied_duplicate(&conn, "s1", "hello").unwrap());
}

#[test]
fn mark_final_flags_only_the_final_turn() {
    use argus_lib::sessions::schema::NewMsg;

    let conn = dupe_db();
    user_msg(&conn, "s1", "do the thing");

    let first = argus_lib::sessions::store::add_msg(
        &conn,
        &NewMsg {
            session_id: "s1".into(),
            role: "assistant".into(),
            content: "working on it <action tool=\"grep\">{\"pattern\":\"x\"}</action>".into(),
            model_id: None,
            provider_id: None,
            tok_in: None,
            tok_out: None,
            tool_calls: None,
            tool_call_id: None,
        },
    )
    .unwrap();

    assistant_msg(&conn, "s1", "all done, here are the findings");

    let last_id: String = conn
        .query_row(
            "SELECT id FROM messages WHERE session_id = 's1' AND role = 'assistant' ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    argus_lib::sessions::store::mark_final(&conn, &last_id).unwrap();

    let kinds: Vec<(String, Option<String>)> = conn
        .prepare("SELECT id, kind FROM messages WHERE session_id = 's1' AND role = 'assistant' ORDER BY seq")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(kinds.len(), 2);
    assert_eq!(kinds[0].0, first.id);
    assert_eq!(kinds[0].1, None);
    assert_eq!(kinds[1].1, Some("final".to_string()));
}
