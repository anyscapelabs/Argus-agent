use argus_lib::sessions::chat::NullSink;
use std::sync::Mutex;

fn setup() -> (
    argus_lib::gateway::Gateway,
    tauri::AppHandle<tauri::test::MockRuntime>,
    String,
) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: std::sync::Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: std::env::temp_dir().join("argus-headless-skills"),
        library_dir: std::env::temp_dir().join("argus-headless-library"),
        logos_dir: std::env::temp_dir().join("argus-headless-logos"),
        approvals: std::sync::Mutex::new(std::collections::HashMap::new()),
        tasks: std::sync::Mutex::new(std::collections::HashMap::new()),
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
        jobs: std::sync::Mutex::new(std::collections::HashMap::new()),
        events: std::sync::Mutex::new(std::collections::HashMap::new()),
        turns: std::sync::Mutex::new(std::collections::HashSet::new()),
        watching: Mutex::new(None),
    };

    let sid = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "t".into(),
                model_id: None,
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
            },
        )
        .unwrap()
        .id
    };

    let app = tauri::test::mock_app();
    (gw, app.handle().clone(), sid)
}

#[tokio::test]
async fn headless_send_tags_system_role_without_frontend() {
    let (gw, handle, sid) = setup();
    let sink = NullSink;

    let out =
        argus_lib::sessions::chat::send(&gw, &handle, &sid, "scheduled tick", &sink, "system")
            .await;

    assert!(out.is_err(), "no model is configured, the turn must fail");

    let conn = gw.conn.lock().unwrap();
    let (role, content): (String, String) = conn
        .query_row(
            "SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY seq DESC LIMIT 1",
            [sid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(role, "system");
    assert_eq!(content, "scheduled tick");
}

#[tokio::test]
async fn an_open_window_does_not_make_a_detached_turn_trusted() {
    let (gw, _app, sid) = setup();

    // A window is open on the session and receiving its events.
    let _watcher = gw.subscribe(&sid);
    assert!(!gw.watched(&sid), "watching is not being driven");

    // A turn the user is sitting in front of.
    gw.go_live(&sid);
    assert!(gw.watched(&sid), "a live turn can ask for approval");

    gw.go_quiet(&sid);
    assert!(!gw.watched(&sid), "the turn ended, nobody is there");
}

#[tokio::test]
async fn a_detached_turn_still_refuses_to_ask_with_a_watcher_attached() {
    use argus_lib::gateway::schema::StreamEvent;
    use argus_lib::sessions::chat::ChatSink;

    struct Watcher<'a>(&'a argus_lib::gateway::Gateway, String);

    impl ChatSink for Watcher<'_> {
        fn emit(&self, _ev: StreamEvent) {}

        // The bus has a subscriber, but nobody is driving. If this ever reads
        // false, a background wake turn starts sending approval prompts into an
        // empty room and stalls until the timeout.
        fn detached(&self) -> bool {
            !self.0.watched(&self.1)
        }
    }

    let (gw, _app, sid) = setup();
    let _rx = gw.subscribe(&sid);
    let sink = Watcher(&gw, sid.clone());

    assert!(sink.detached());
}

#[tokio::test]
async fn a_watch_bus_is_dropped_only_once_nobody_is_left() {
    let (gw, _app, sid) = setup();

    let mut rx = gw.subscribe(&sid);
    gw.drop_bus(&sid);
    assert!(gw.watched(&sid) == false);

    gw.publish(
        &sid,
        argus_lib::gateway::schema::StreamEvent::Notice {
            msg: "still here".into(),
        },
    );
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("event arrives")
        .expect("not closed");
    assert!(matches!(
        ev,
        argus_lib::gateway::schema::StreamEvent::Notice { .. }
    ));

    drop(rx);
    gw.drop_bus(&sid);
}

#[tokio::test]
async fn a_window_only_tails_one_chat_at_a_time() {
    let (gw, _app, sid) = setup();
    let other = format!("{sid}-other");

    gw.set_watching(Some(&sid));
    assert!(gw.watching(&sid));

    // Opening another chat retires the last, rather than leaving a task
    // subscribed to a conversation nobody is looking at.
    gw.set_watching(Some(&other));
    assert!(!gw.watching(&sid));
    assert!(gw.watching(&other));

    gw.set_watching(None);
    assert!(!gw.watching(&other));
}
