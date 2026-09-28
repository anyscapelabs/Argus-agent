// The resume record is only worth anything if the model actually reads it.
// These tests drive `project()` — the real prompt builder — because a unit
// test of `resume.rs` passes happily while the wiring that injects it is
// dead. That is exactly the bug this file exists to prevent.

use argus_lib::prompt::project;
use argus_lib::sessions::schema::ResumeRow;
use argus_lib::sessions::store;

fn dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("argus-resume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup() -> (rusqlite::Connection, String) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    store::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, title, permission) VALUES ('s1', 't', 'ask')",
        [],
    )
    .unwrap();
    (conn, dir().to_string_lossy().into_owned())
}

fn add_user(conn: &rusqlite::Connection, sid: &str, text: &str) {
    store::add_msg(
        conn,
        &argus_lib::sessions::schema::NewMsg {
            session_id: sid.into(),
            role: "user".into(),
            content: text.into(),
            ..Default::default()
        },
    )
    .unwrap();
}

fn row(goal: &str, done: &str, next: &str) -> ResumeRow {
    ResumeRow {
        goal: goal.into(),
        done: done.into(),
        next: next.into(),
    }
}

// THE regression test. A resume row exists, and the prompt the model is
// actually given must contain it. Before the fix, `send()` deleted the row
// before `project()` read it and this assertion failed.
#[test]
fn a_stored_resume_reaches_the_assembled_prompt() {
    let (conn, d) = setup();
    add_user(&conn, "s1", "audit the parser");
    store::save_resume(
        &conn,
        "s1",
        &row("audit the parser", "! grep: no match", "try rg"),
    )
    .unwrap();

    let p = project(&conn, "s1", std::path::Path::new(&d)).unwrap();

    assert!(p.system.contains("<resume>"), "{}", p.system);
    assert!(p.system.contains("audit the parser"), "{}", p.system);
    assert!(p.system.contains("no match"), "{}", p.system);
}

// No row, no block: a session that never stopped unfinished must not be told
// it did.
#[test]
fn a_session_without_a_resume_is_not_given_one() {
    let (conn, d) = setup();
    add_user(&conn, "s1", "just a normal question");

    let p = project(&conn, "s1", std::path::Path::new(&d)).unwrap();

    assert!(!p.system.contains("<resume>"), "{}", p.system);
}

// A sub-agent must never inherit the parent's unfinished work. It would spend
// its whole window on a conversation it was never part of.
#[test]
fn a_child_session_never_inherits_a_resume() {
    let (conn, d) = setup();
    conn.execute(
        "INSERT INTO sessions (id, title, permission, parent_id)
         VALUES ('c1', 'sub', 'ask', 's1')",
        [],
    )
    .unwrap();
    store::save_resume(
        &conn,
        "s1",
        &row("parent work", "parent stuff", "parent next"),
    )
    .unwrap();

    let p = project(&conn, "c1", std::path::Path::new(&d)).unwrap();

    assert!(!p.system.contains("<resume>"), "{}", p.system);
    assert!(!p.system.contains("parent work"), "{}", p.system);
}

// The goal is the task as first asked. On a resumed turn the newest message
// is the word "continue", and a resume saying `task: continue` is useless.
#[test]
fn the_goal_is_the_first_user_message_not_the_continue() {
    let (conn, _d) = setup();
    add_user(&conn, "s1", "migrate the database to postgres");
    add_user(&conn, "s1", "continue");

    let goal = store::first_user_msg(&conn, "s1").expect("a first message exists");

    assert_eq!(goal, "migrate the database to postgres");
    assert_ne!(goal, "continue");
}

// A local slash line is the app answering itself, never a task.
#[test]
fn the_goal_ignores_local_turns() {
    let (conn, _d) = setup();
    store::add_local_msg(&conn, "s1", "/usage").unwrap();
    add_user(&conn, "s1", "real work");

    assert_eq!(
        store::first_user_msg(&conn, "s1").as_deref(),
        Some("real work")
    );
}

// Superseded (retried) turns are not the task either.
#[test]
fn the_goal_ignores_inactive_messages() {
    let (conn, _d) = setup();
    add_user(&conn, "s1", "first ask");
    add_user(&conn, "s1", "superseded ask");
    conn.execute("UPDATE messages SET active = 0 WHERE seq = 2", [])
        .unwrap();

    assert_eq!(
        store::first_user_msg(&conn, "s1").as_deref(),
        Some("first ask")
    );
}

// Clearing is what a finished turn does, and the prompt must stop carrying
// the block the moment it happens.
#[test]
fn clearing_stops_the_injection_immediately() {
    let (conn, d) = setup();
    add_user(&conn, "s1", "task");
    store::save_resume(&conn, "s1", &row("task", "ran stuff", "")).unwrap();
    assert!(project(&conn, "s1", std::path::Path::new(&d))
        .unwrap()
        .system
        .contains("<resume>"));

    store::clear_resume(&conn, "s1").unwrap();

    assert!(!project(&conn, "s1", std::path::Path::new(&d))
        .unwrap()
        .system
        .contains("<resume>"));
}

// An over-long task must be clipped by the same helper the unit test covers,
// so the row can never outgrow the block it becomes.
#[test]
fn a_long_goal_is_clipped_on_save() {
    let (conn, d) = setup();
    add_user(&conn, "s1", &"x".repeat(5000));
    let goal = store::first_user_msg(&conn, "s1").unwrap();

    argus_lib::sessions::resume::save(&conn, "s1", &goal, &[("terminal: ls".into(), false)]);

    let stored = store::get_resume(&conn, "s1").unwrap();
    assert!(
        stored.goal.chars().count() <= 201,
        "{}",
        stored.goal.chars().count()
    );

    let p = project(&conn, "s1", std::path::Path::new(&d)).unwrap();
    assert!(p.system.contains("<resume>"), "{}", p.system);
    assert!(
        p.system.len() < 40_000,
        "a resume must not flood the prompt"
    );
}
