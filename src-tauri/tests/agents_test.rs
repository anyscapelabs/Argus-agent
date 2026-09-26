use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use argus_lib::agents;
use argus_lib::gateway::Gateway;
use argus_lib::sessions::schema::NewSession;
use argus_lib::sessions::store;
use tauri::Manager;

fn app() -> tauri::AppHandle<tauri::test::MockRuntime> {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    store::migrate(&conn).unwrap();

    let gw = Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: std::env::temp_dir().join("argus-agents-skills"),
        library_dir: std::env::temp_dir().join("argus-agents-library"),
        logos_dir: std::env::temp_dir().join("argus-agents-logos"),
        jobs_dir: std::env::temp_dir().join("argus-agents-logs"),
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
        jobs: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        turns: Mutex::new(HashSet::new()),
        watching: Mutex::new(HashSet::new()),
    };

    let app = tauri::test::mock_app();
    app.manage(gw);
    app.handle().clone()
}

fn parent(app: &tauri::AppHandle<tauri::test::MockRuntime>) -> String {
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    store::create_session(
        &conn,
        &NewSession {
            title: "parent".into(),
            model_id: None,
            permission: Some("never".into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap()
    .id
}

fn spec(parent_id: &str, name: &str) -> agents::Spec {
    agents::Spec {
        parent_id: parent_id.into(),
        name: name.into(),
        title: format!("{name} work"),
        prompt: "do the thing".into(),
        model_id: None,
        permission: "never".into(),
    }
}

#[test]
fn a_sub_agent_gets_its_own_chat_and_its_own_card() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    let run = agents::spawn(&app, gw.inner(), spec(&pid, "scout")).unwrap();

    assert!(!run.id.is_empty());
    assert_eq!(run.name, "scout");

    let conn = gw.conn.lock().unwrap();
    let child = store::get_session(&conn, &run.id).unwrap();
    assert_eq!(child.parent_id.as_deref(), Some(pid.as_str()));
    assert_eq!(child.agent_name.as_deref(), Some("scout"));
}

#[test]
fn a_sub_agent_is_not_a_chat_in_the_users_list() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    let run = agents::spawn(&app, gw.inner(), spec(&pid, "scout")).unwrap();

    let conn = gw.conn.lock().unwrap();
    let listed = store::list_sessions(&conn, None).unwrap();

    assert!(listed.iter().any(|s| s.id == pid), "the parent is a chat");
    assert!(
        !listed.iter().any(|s| s.id == run.id),
        "a sub-agent is not a chat in the sidebar"
    );

    let children = store::children_of(&conn, &pid).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, run.id);
}

#[test]
fn a_sub_agent_cannot_start_another_sub_agent() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    let first = agents::spawn(&app, gw.inner(), spec(&pid, "scout")).unwrap();
    let mut nested = spec(&first.id, "nested");
    nested.prompt = "go deeper".into();

    let err = agents::spawn(&app, gw.inner(), nested).unwrap_err();
    assert!(err.contains("cannot start another"), "got: {err}");
}

#[test]
fn fan_out_is_capped() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    // Seated rather than spawned: a spawned child fails fast on a test db with
    // no model, so it leaves 'running' on its own and the cap stops meaning
    // anything.
    {
        let conn = gw.conn.lock().unwrap();

        for i in 0..agents::MAX_CHILDREN {
            store::create_child(&conn, &pid, &format!("a{i}"), "work", None, "never").unwrap();
        }
    }

    let err = agents::spawn(&app, gw.inner(), spec(&pid, "one-too-many")).unwrap_err();
    assert!(err.contains("already running"), "got: {err}");
}

#[test]
fn a_sub_agent_needs_a_name_and_a_prompt() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    let mut blank = spec(&pid, "x");
    blank.name = "   ".into();
    assert!(agents::spawn(&app, gw.inner(), blank).is_err());

    let mut empty = spec(&pid, "x");
    empty.prompt = "  ".into();
    assert!(agents::spawn(&app, gw.inner(), empty).is_err());
}

#[test]
fn listing_is_scoped_to_the_parent_that_started_them() {
    let app = app();
    let a = parent(&app);
    let b = parent(&app);
    let gw = app.state::<Gateway>();

    agents::spawn(&app, gw.inner(), spec(&a, "one")).unwrap();
    agents::spawn(&app, gw.inner(), spec(&a, "two")).unwrap();
    agents::spawn(&app, gw.inner(), spec(&b, "other")).unwrap();

    let conn = gw.conn.lock().unwrap();
    let mine = agents::list(&conn, &a).unwrap();
    let theirs = agents::list(&conn, &b).unwrap();

    assert_eq!(mine.len(), 2);
    assert_eq!(theirs.len(), 1);
    assert!(theirs.iter().all(|r| r.name == "other"));
}

#[test]
fn an_unknown_sub_agent_id_is_an_error_not_a_silent_empty() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let err = agents::get(&conn, "nope").unwrap_err();
    assert!(err.contains("no sub-agent"), "got: {err}");
}

#[test]
fn a_parent_is_never_its_own_child() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    let conn = gw.conn.lock().unwrap();
    assert!(!store::is_child(&conn, &pid).unwrap());
}

#[test]
fn reconcile_marks_sub_agents_the_app_never_came_back_for() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    // Seated, not spawned: a real child settles on its own and would race the
    // state this test is setting by hand.
    let run = {
        let conn = gw.conn.lock().unwrap();
        store::create_child(&conn, &pid, "scout", "work", None, "never").unwrap()
    };

    let conn = gw.conn.lock().unwrap();
    store::set_agent_state(&conn, &run.id, "done", None).unwrap();
    assert_eq!(agents::reconcile(&conn).unwrap(), 0);

    store::set_agent_state(&conn, &run.id, "running", None).unwrap();
    assert_eq!(agents::reconcile(&conn).unwrap(), 1);

    let after = agents::get(&conn, &run.id).unwrap();
    assert_eq!(after.state, "interrupted");
}

#[test]
fn a_sub_agent_prompt_is_capped_so_a_wish_cannot_become_a_wall() {
    let long = "x".repeat(500);
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    let mut s = spec(&pid, &long);
    s.title = long.clone();
    let run = agents::spawn(&app, gw.inner(), s).unwrap();

    assert!(run.name.chars().count() <= 40);
    assert!(run.title.chars().count() <= 90);
}

#[test]
fn a_sub_agent_cannot_ask_for_approval_when_nobody_is_watching() {
    use argus_lib::gateway::schema::StreamEvent;
    use argus_lib::sessions::chat::ChatSink;

    struct Fan<'a>(&'a Gateway, String);

    impl ChatSink for Fan<'_> {
        fn emit(&self, _ev: StreamEvent) {}

        fn detached(&self) -> bool {
            !self.0.attached(&self.1)
        }
    }

    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    let sink = Fan(gw.inner(), pid.clone());
    assert!(
        sink.detached(),
        "no window on the parent, nobody can answer"
    );

    let _watcher = gw.subscribe(&pid);
    assert!(
        !sink.detached(),
        "the parent's chat is open, a human can answer"
    );

    gw.subscribe(&format!("{pid}-elsewhere"));
    assert!(
        !sink.detached(),
        "a window on another chat is not a window here"
    );
}

#[test]
fn the_prune_keeps_the_newest_and_never_touches_a_live_one() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    for i in 0..5 {
        let c = store::create_child(&conn, &pid, &format!("a{i}"), "work", None, "never").unwrap();
        // Stamped by hand: five rows created in the same second are ordered by
        // id, and the ids are uuids, so the test would be a coin toss.
        conn.execute(
            "UPDATE sessions SET created_at = ?2 WHERE id = ?1",
            rusqlite::params![c.id, format!("2026-01-01 00:00:0{i}")],
        )
        .unwrap();
        store::set_agent_state(&conn, &c.id, "done", None).unwrap();
    }

    assert_eq!(agents::cleanup(&conn, 3).unwrap(), 2);

    let left = agents::list(&conn, &pid).unwrap();
    let names: Vec<&str> = left.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["a2", "a3", "a4"], "the two oldest go");

    let live = store::create_child(&conn, &pid, "live", "work", None, "never").unwrap();
    assert_eq!(
        agents::cleanup(&conn, 3).unwrap(),
        0,
        "a live one is not history yet, the cap does not count it"
    );
    assert_eq!(store::children_of(&conn, &pid).unwrap().len(), 4);
    assert_eq!(agents::get(&conn, &live.id).unwrap().state, "running");
}

#[test]
fn the_prune_is_counted_per_parent_not_globally() {
    let app = app();
    let a = parent(&app);
    let b = parent(&app);
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    for i in 0..4 {
        let c = store::create_child(&conn, &a, &format!("a{i}"), "work", None, "never").unwrap();
        store::set_agent_state(&conn, &c.id, "done", None).unwrap();
    }

    let c = store::create_child(&conn, &b, "b0", "work", None, "never").unwrap();
    store::set_agent_state(&conn, &c.id, "done", None).unwrap();

    assert_eq!(agents::cleanup(&conn, 3).unwrap(), 1);
    assert_eq!(store::children_of(&conn, &a).unwrap().len(), 3);
    assert_eq!(
        store::children_of(&conn, &b).unwrap().len(),
        1,
        "a busy chat does not spend a quiet one's history"
    );
}

#[test]
fn killing_a_sub_agent_that_never_ran_is_not_an_error() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();

    // A handle put in by hand: a spawned child drops its own the moment its
    // turn ends, which on a test db is instantly.
    let run = {
        let conn = gw.conn.lock().unwrap();
        store::create_child(&conn, &pid, "scout", "work", None, "never").unwrap()
    };

    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    gw.tasks
        .lock()
        .unwrap()
        .insert(run.id.clone(), stop.clone());

    assert!(agents::kill(gw.inner(), &run.id).unwrap());

    gw.tasks.lock().unwrap().remove(&run.id);
    assert!(!agents::kill(gw.inner(), &run.id).unwrap(), "already gone");
    assert!(agents::kill(gw.inner(), "nope").is_err());
}

#[test]
fn reading_a_sub_agent_hands_back_the_end_of_its_answer_not_the_start() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let run = store::create_child(&conn, &pid, "scout", "work", None, "never").unwrap();

    assert!(
        agents::tail(&agents::get(&conn, &run.id).unwrap(), 8_000)
            .unwrap_err()
            .contains("has not produced an answer"),
        "a running one has nothing to read yet"
    );

    let answer = "A".repeat(3_000) + "TAIL";
    conn.execute(
        "INSERT INTO messages (id, session_id, seq, role, content, active, created_at)
         VALUES ('m1', ?1, 1, 'assistant', ?2, 1, datetime('now'))",
        rusqlite::params![run.id, answer],
    )
    .unwrap();

    let run = agents::get(&conn, &run.id).unwrap();
    let full = agents::tail(&run, 8_000).unwrap();
    assert_eq!(full.chars().count(), 3_004);

    let clipped = agents::tail(&run, 500).unwrap();
    assert_eq!(clipped.chars().count(), 500);
    assert!(clipped.ends_with("TAIL"), "the summary cut off the head");
}

#[test]
fn keeping_everything_prunes_nothing_but_still_sweeps_orphans() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    for i in 0..5 {
        let c = store::create_child(&conn, &pid, &format!("a{i}"), "work", None, "never").unwrap();
        store::set_agent_state(&conn, &c.id, "done", None).unwrap();
    }

    assert_eq!(agents::cleanup(&conn, agents::KEEP_ALL).unwrap(), 0);
    assert_eq!(store::children_of(&conn, &pid).unwrap().len(), 5);
}

#[test]
fn a_child_whose_parent_is_gone_is_swept_however_old_or_new_it_is() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    // Written with the key off, which is how one gets there at all: deleting a
    // parent cascades its children, so an orphan can only predate the pragma
    // or come from a db that was written while it was off.
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, title, permission, parent_id, agent_name, agent_state, created_at)
         VALUES ('orphan', 'lost', 'never', 'no-such-parent', 'lost', 'running', '2026-01-01 00:00:00')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();

    // Nothing reaches an orphan: not the sidebar, not a card. Left alone it
    // would sit in the db forever, and even a generous cap would not touch it.
    assert_eq!(agents::cleanup(&conn, 200).unwrap(), 1);
    assert!(agents::get(&conn, "orphan").is_err());
}

#[test]
fn retention_is_the_users_choice_and_survives_a_restart() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    assert_eq!(
        agents::keep(&conn),
        agents::KEEP_DEFAULT,
        "a db that predates the setting still prunes"
    );

    agents::set_keep(&conn, agents::KEEP_ALL).unwrap();
    assert_eq!(agents::keep(&conn), 0);

    agents::set_keep(&conn, 50).unwrap();
    assert_eq!(agents::keep(&conn), 50);

    agents::set_keep(&conn, 999_999_999).unwrap();
    assert_eq!(agents::keep(&conn), 100_000, "clamped, not trusted");
    agents::set_keep(&conn, agents::KEEP_DEFAULT).unwrap();
}

#[test]
fn a_child_with_no_state_recorded_is_history_not_a_live_turn() {
    let app = app();
    let pid = parent(&app);
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    for i in 0..4 {
        let c = store::create_child(&conn, &pid, &format!("a{i}"), "work", None, "never").unwrap();
        conn.execute(
            "UPDATE sessions SET agent_state = NULL, created_at = ?2 WHERE id = ?1",
            rusqlite::params![c.id, format!("2026-01-01 00:00:0{i}")],
        )
        .unwrap();
    }

    assert_eq!(agents::cleanup(&conn, 3).unwrap(), 1);
    assert_eq!(store::children_of(&conn, &pid).unwrap().len(), 3);
}
