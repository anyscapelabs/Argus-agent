use argus_lib::sessions::sink::NullSink;
use std::collections::HashSet;
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
        watching: Mutex::new(HashSet::new()),
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

    let out = argus_lib::sessions::chat::send(
        &gw,
        &handle,
        &sid,
        "scheduled tick",
        None,
        &sink,
        "system",
    )
    .await;

    assert!(out.is_err(), "no model is configured, the turn must fail");

    let conn = gw.conn.lock().unwrap();
    let rows: Vec<(String, String)> = conn
        .prepare(
            "SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY seq DESC LIMIT 2",
        )
        .unwrap()
        .query_map([sid], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, rusqlite::Error>>()
        .unwrap();

    assert_eq!(rows.len(), 2, "user tick plus a visible assistant error");
    assert_eq!(rows[1].0, "system");
    assert_eq!(rows[1].1, "scheduled tick");
    assert_eq!(rows[0].0, "assistant");
    assert!(rows[0].1.contains("<error"), "got: {}", rows[0].1);
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
    use argus_lib::sessions::sink::ChatSink;

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
    assert!(!gw.watched(&sid));

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
async fn a_sub_agents_page_and_the_chat_that_started_it_are_both_tailed() {
    let (gw, _app, sid) = setup();
    let child = format!("{sid}-child");
    let other = format!("{sid}-other");

    gw.start_watching(&sid);
    assert!(gw.watching(&sid));

    // Opening the card does not take the chat down with it.
    gw.start_watching(&child);
    assert!(gw.watching(&sid));
    assert!(gw.watching(&child));

    // Closing one leaves the other alone.
    gw.stop_watching(&child);
    assert!(!gw.watching(&child));
    assert!(gw.watching(&sid));

    gw.start_watching(&other);
    gw.stop_watching_all();
    assert!(!gw.watching(&sid));
    assert!(!gw.watching(&other));
}

// A sub-agent's report comes back as a turn nobody asked for. It has no
// forwarder of its own — it publishes onto the bus and hopes. If a claimed
// turn tore the window's own tail down, every delta of the parent's answer
// went nowhere: the chat sat on "waiting for sub-agents" with the finished
// answer sitting in the database, visible only after a restart.
#[tokio::test]
async fn a_claimed_turn_does_not_cut_the_open_windows_tail() {
    use argus_lib::gateway::Gateway;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::ipc::Channel;
    use tauri::{Manager, State};

    let (gw, _app, sid) = setup();
    let app = Box::leak(Box::new(
        tauri::test::mock_builder()
            .manage(gw)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap(),
    ));
    let state: State<'static, Gateway> = app.state::<Gateway>();

    let got = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&got);
    let chan = Channel::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });

    let sid2 = sid.clone();
    tokio::spawn(
        async move { argus_lib::sessions::chat::sess_watch_events(state, sid2, chan).await },
    );

    let gw = &app.state::<Gateway>();
    let settle = std::time::Duration::from_millis(400);

    for _ in 0..50 {
        if gw.attached(&sid) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(gw.attached(&sid), "the window never subscribed");

    // The wake turn takes its claim, long enough for the tail to notice.
    assert!(gw.claim_turn(&sid));
    tokio::time::sleep(settle).await;

    for want in 1..=2usize {
        gw.publish(
            &sid,
            argus_lib::gateway::schema::StreamEvent::Notice {
                msg: format!("delta {want}"),
            },
        );

        for _ in 0..50 {
            if got.load(Ordering::SeqCst) >= want {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        assert_eq!(
            got.load(Ordering::SeqCst),
            want,
            "delta {want} never reached the window while a turn held the claim"
        );
        tokio::time::sleep(settle).await;
    }

    gw.release_turn(&sid);
    gw.stop_watching(&sid);
}
