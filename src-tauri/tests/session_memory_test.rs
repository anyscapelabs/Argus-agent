use argus_lib::memory::schema::NewSessionMemory;
use argus_lib::memory::session_memory;

fn test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    conn
}

fn smem(session: &str, task: &str, summary: &str) -> NewSessionMemory {
    NewSessionMemory {
        session_id: session.into(),
        task: task.into(),
        summary: summary.into(),
        outcome: "ok".into(),
        importance: 2,
    }
}

#[test]
fn saves_and_validates_session_memories() {
    let conn = test_db();
    let e =
        session_memory::save_session_memory(&conn, &smem("s1", "deploy backend", "ran compose"))
            .unwrap();
    assert_eq!(e.task, "deploy backend");
    assert_eq!(e.outcome, "ok");
    assert_eq!(e.importance, 2);

    let bad = NewSessionMemory {
        session_id: "s1".into(),
        task: "   ".into(),
        ..Default::default()
    };
    assert!(session_memory::save_session_memory(&conn, &bad).is_err());

    let fetched = session_memory::get_session_memory(&conn, &e.id).unwrap();
    assert_eq!(fetched.summary, "ran compose");
}

#[test]
fn session_link_bridges_fts_gaps() {
    let conn = test_db();
    let a = session_memory::save_session_memory(
        &conn,
        &smem("s1", "fix flaky deploy pipeline", "retried with backoff"),
    )
    .unwrap();
    let b = session_memory::save_session_memory(
        &conn,
        &smem(
            "s1",
            "completely unrelated gardening notes",
            "planted tomatoes",
        ),
    )
    .unwrap();
    assert!(session_memory::add_session_edge(&conn, &a.id, &b.id, "led_to").unwrap());
    assert!(!session_memory::add_session_edge(&conn, &a.id, &b.id, "led_to").unwrap());
    assert!(session_memory::add_session_edge(&conn, "", &b.id, "x").is_err());

    let hits = session_memory::recall_session_memory(&conn, "flaky deploy pipeline", 5).unwrap();
    assert_eq!(hits[0].id, a.id);
    assert_eq!(hits[0].distance, 0);
    let bridged = hits
        .iter()
        .find(|h| h.id == b.id)
        .expect("graph hop must surface");
    assert!(bridged.distance > 0, "{bridged:?}");
}

#[test]
fn recall_empty_query_returns_nothing() {
    let conn = test_db();
    let hits = session_memory::recall_session_memory(&conn, "   ", 5).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn decay_fades_old_session_memories_with_floor() {
    let conn = test_db();
    let e = session_memory::save_session_memory(
        &conn,
        &smem("s1", "old deploy runbook", "used compose"),
    )
    .unwrap();
    conn.execute(
        "UPDATE session_memory SET created_at = datetime('now', '-60 days') WHERE id = ?1",
        [e.id.clone()],
    )
    .unwrap();
    // raise importance so decay has room to move
    conn.execute(
        "UPDATE session_memory SET importance = 3 WHERE id = ?1",
        [e.id.clone()],
    )
    .unwrap();

    assert_eq!(session_memory::decay_session_memory(&conn, 30).unwrap(), 1);
    assert_eq!(
        session_memory::get_session_memory(&conn, &e.id)
            .unwrap()
            .importance,
        2
    );

    let fresh =
        session_memory::save_session_memory(&conn, &smem("s1", "fresh task", "did thing")).unwrap();
    assert_eq!(session_memory::decay_session_memory(&conn, 30).unwrap(), 1);
    assert_eq!(
        session_memory::get_session_memory(&conn, &fresh.id)
            .unwrap()
            .importance,
        2
    );

    assert_eq!(session_memory::decay_session_memory(&conn, 30).unwrap(), 0);
    assert_eq!(
        session_memory::get_session_memory(&conn, &e.id)
            .unwrap()
            .importance,
        1
    );
}

#[test]
fn rollup_session_candidates_surface_linked_fading_episodes() {
    let conn = test_db();
    let a = session_memory::save_session_memory(&conn, &smem("s1", "deploy checklist", "compose"))
        .unwrap();
    let b = session_memory::save_session_memory(
        &conn,
        &smem("s1", "rollback checklist", "compose down"),
    )
    .unwrap();
    session_memory::add_session_edge(&conn, &a.id, &b.id, "relates").unwrap();
    conn.execute(
        "UPDATE session_memory SET importance = 2 WHERE id IN (?1, ?2)",
        [&a.id, &b.id],
    )
    .unwrap();
    let lone =
        session_memory::save_session_memory(&conn, &smem("s1", "solo note", "nothing")).unwrap();
    conn.execute(
        "UPDATE session_memory SET importance = 2 WHERE id = ?1",
        [lone.id.clone()],
    )
    .unwrap();

    let ids: Vec<String> = session_memory::rollup_session_candidates(&conn, 5)
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert!(ids.contains(&a.id) && ids.contains(&b.id), "{ids:?}");
    assert!(!ids.contains(&lone.id), "{ids:?}");
}

#[test]
fn session_graph_reports_degrees() {
    let conn = test_db();
    let a = session_memory::save_session_memory(&conn, &smem("s1", "deploy", "ok")).unwrap();
    let b = session_memory::save_session_memory(&conn, &smem("s1", "verify", "ok")).unwrap();
    session_memory::add_session_edge(&conn, &a.id, &b.id, "led_to").unwrap();

    let g = session_memory::load_session_graph(&conn, 10).unwrap();
    assert_eq!(g.nodes.len(), 2);
    assert_eq!(g.edges.len(), 1);
    assert!(g.nodes.iter().all(|n| n.degree == 1));
}

#[test]
fn session_memory_from_turn_shapes_capture_data() {
    let e = session_memory::session_memory_from_turn(
        "Deploy the backend now\nsecond line ignored",
        &["terminal".into(), "terminal".into(), "fs.write".into()],
        true,
    );
    assert_eq!(e.task, "Deploy the backend now");
    assert_eq!(e.summary, "tools: terminal, fs.write");
    assert_eq!(e.outcome, "ok");
    assert_eq!(e.importance, 2);

    let e = session_memory::session_memory_from_turn("  ", &[], true);
    assert_eq!(e.task, "untitled turn");
    assert_eq!(e.importance, 1);

    let e = session_memory::session_memory_from_turn("fix it", &["terminal".into()], false);
    assert_eq!(e.outcome, "error");
    assert_eq!(e.importance, 1);
}
