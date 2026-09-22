use argus_lib::learning::{prompt_context, record_correction, record_vote, store};

fn setup() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    store::migrate(&conn).unwrap();
    conn
}

fn assistant_msg(conn: &rusqlite::Connection, sid: &str, content: &str) -> String {
    conn.execute(
        "INSERT INTO sessions (id, title) VALUES (?1, 't') ON CONFLICT(id) DO NOTHING",
        [sid],
    )
    .unwrap();

    argus_lib::sessions::store::add_msg(
        conn,
        &argus_lib::sessions::schema::NewMsg {
            session_id: sid.into(),
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
    .unwrap()
    .id
}

#[test]
fn upsert_blends_inferred_confidence_and_counts_evidence() {
    let conn = setup();

    store::upsert_preference(&conn, "global", "style", "formality", "casual", 0.6, false).unwrap();
    store::upsert_preference(&conn, "global", "style", "formality", "casual", 0.8, false).unwrap();

    let prefs = store::list_preferences(&conn, Some("global")).unwrap();
    assert_eq!(prefs.len(), 1);
    assert_eq!(prefs[0].evidence_count, 2);
    assert!(!prefs[0].explicit);
    assert!(
        (prefs[0].confidence - 0.67).abs() < 0.01,
        "{}",
        prefs[0].confidence
    );
}

#[test]
fn explicit_preference_wins_and_sticks() {
    let conn = setup();

    store::upsert_preference(&conn, "global", "style", "verbosity", "terse", 0.6, false).unwrap();
    store::upsert_preference(&conn, "global", "style", "verbosity", "detailed", 0.9, true).unwrap();

    let prefs = store::list_preferences(&conn, Some("global")).unwrap();
    assert_eq!(prefs[0].value, "detailed");
    assert!(prefs[0].explicit);
    assert_eq!(prefs[0].confidence, 0.9);

    store::upsert_preference(&conn, "global", "style", "verbosity", "terse", 0.5, false).unwrap();
    let prefs = store::list_preferences(&conn, Some("global")).unwrap();
    assert_eq!(prefs[0].value, "terse");
    assert!(prefs[0].explicit);
    assert_eq!(prefs[0].confidence, 0.9, "explicit confidence never decays");
}

#[test]
fn prompt_context_gates_on_confidence_and_caps_output() {
    let conn = setup();

    assert_eq!(prompt_context(&conn).unwrap(), "");

    store::upsert_preference(&conn, "global", "style", "formality", "casual", 0.5, false).unwrap();
    assert_eq!(
        prompt_context(&conn).unwrap(),
        "",
        "low confidence stays out"
    );

    store::upsert_preference(&conn, "global", "style", "formality", "casual", 0.9, true).unwrap();
    let ctx = prompt_context(&conn).unwrap();
    assert!(ctx.contains("<learned-preferences>"));
    assert!(ctx.contains("formality: casual"));
    assert!(ctx.contains("</learned-preferences>"));
}

#[test]
fn examples_keep_only_strongest_twelve() {
    let conn = setup();

    for i in 0..15 {
        store::add_example(
            &conn,
            "global",
            "writing",
            &format!("example {i}"),
            0.1 + i as f64 * 0.05,
            "test",
        )
        .unwrap();
    }

    let kept = store::examples(&conn, "global", "writing", 20).unwrap();
    assert_eq!(kept.len(), 12);

    let contents: Vec<&str> = kept.iter().map(|e| e.content.as_str()).collect();
    for dropped in ["example 0", "example 1", "example 2"] {
        assert!(!contents.contains(&dropped), "{contents:?}");
    }
    for held in ["example 12", "example 13", "example 14"] {
        assert!(contents.iter().any(|c| *c == held), "{contents:?}");
    }
}

#[test]
fn events_queue_and_mark_processed() {
    let conn = setup();

    let id = store::record_event(&conn, Some("s1"), Some("m1"), "vote", "{}").unwrap();
    assert_eq!(store::pending(&conn, 4).unwrap().len(), 1);

    store::mark_processed(&conn, &id).unwrap();
    assert!(store::pending(&conn, 4).unwrap().is_empty());
}

#[test]
fn votes_record_only_valid_assistant_votes() {
    let conn = setup();
    let mid = assistant_msg(&conn, "s1", "dear sir, formal email here");

    assert!(record_vote(&conn, "s1", &mid, None).is_ok());
    assert!(record_vote(&conn, "s1", &mid, Some("meh")).is_ok());
    assert_eq!(store::pending(&conn, 4).unwrap().len(), 0);

    assert!(record_vote(&conn, "nope", &mid, Some("down")).is_ok());
    assert!(record_vote(&conn, "s1", &mid, Some("down")).is_ok());
    assert!(record_vote(&conn, "s1", "missing", Some("up")).is_err());

    let pending = store::pending(&conn, 4).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, "vote");
    assert!(pending[0].payload.contains("\"down\""));
}

#[test]
fn corrections_validate_message_and_content() {
    let conn = setup();
    let mid = assistant_msg(&conn, "s1", "Dear Sir, I hope this finds you well.");

    assert!(record_correction(&conn, "s1", &mid, "   ").is_err());
    assert!(record_correction(&conn, "s1", "missing", "Hi.").is_err());
    assert!(record_correction(&conn, "other", &mid, "Hi.").is_err());

    assert!(record_correction(&conn, "s1", &mid, "Hi John, just following up.").is_ok());

    let pending = store::pending(&conn, 4).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, "correction");
    assert!(pending[0].payload.contains("Hi John"));
}
