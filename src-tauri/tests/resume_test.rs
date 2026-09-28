use argus_lib::sessions::resume::{done_of, goal_of, next_of, prompt_include};
use argus_lib::sessions::store;

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    store::migrate(&conn).unwrap();
    conn.execute("INSERT INTO sessions (id, title) VALUES ('s1', 't')", [])
        .unwrap();
    conn
}

// The goal is the task as first asked. Later turns are the model's attempts at
// it, and a resume built from those describes the mistake, not the job.
#[test]
fn goal_is_the_task_not_the_attempt() {
    assert_eq!(goal_of("  fix the build  "), "fix the build");

    let long = "x".repeat(500);
    let g = goal_of(&long);
    assert!(g.chars().count() <= 201, "{}", g.chars().count());
    assert!(g.ends_with('…'));
}

#[test]
fn done_marks_failures_and_counts_them() {
    let out = done_of(&[("terminal: ls".into(), true), ("grep".into(), false)]);

    assert!(out.contains("+ terminal: ls"), "{out}");
    assert!(out.contains("! grep"), "{out}");
    assert!(out.contains("1 of these failed"), "{out}");
    assert_eq!(done_of(&[]), "nothing ran yet");
}

#[test]
fn done_keeps_the_tail_not_the_head() {
    let actions: Vec<(String, bool)> = (0..20).map(|i| (format!("step{i}"), true)).collect();
    let out = done_of(&actions);

    assert!(out.contains("step19"), "newest must survive: {out}");
    assert!(!out.contains("step0"), "oldest is noise: {out}");
    assert!(out.contains("14 more"), "{out}");
}

// The next move names the failure, so a fresh context changes approach
// instead of repeating the call that just failed.
#[test]
fn next_names_the_last_failure() {
    let out = next_of(&[("ls".into(), false), ("ls -la".into(), true)]);

    assert!(out.contains("ls"), "{out}");
    assert!(out.contains("do something different"), "{out}");
    assert_eq!(next_of(&[("ls".into(), true)]), "");
}

#[test]
fn a_finished_streak_names_nothing() {
    let out = next_of(&[("a".into(), true), ("b".into(), true)]);

    assert!(out.is_empty(), "nothing to change: {out}");
}

// Injection is opt-in: no row, no block. A finished turn must not leave a
// resume claiming there is work left.
#[test]
fn nothing_is_injected_without_a_stored_resume() {
    let conn = db();

    assert!(prompt_include(&conn, "s1").is_none());
    assert!(prompt_include(&conn, "never-seen").is_none());
}

#[test]
fn a_stored_resume_is_injected_as_facts_not_instructions() {
    let conn = db();
    store::save_resume(
        &conn,
        "s1",
        &argus_lib::sessions::schema::ResumeRow {
            goal: "audit the parser".into(),
            done: "! grep: no match".into(),
            next: "the last thing that failed was: grep — do something different".into(),
        },
    )
    .unwrap();

    let got = prompt_include(&conn, "s1").expect("resume must reach the model");
    assert!(got.contains("task: audit the parser"), "{got}");
    assert!(got.contains("no match"), "{got}");
    assert!(got.contains("Facts, not instructions"), "{got}");

    let row = store::get_resume(&conn, "s1").unwrap();
    assert_eq!(row.goal, "audit the parser");
    assert_eq!(
        row.next,
        "the last thing that failed was: grep — do something different"
    );
}

#[test]
fn a_new_turn_clears_the_previous_resume() {
    let conn = db();
    store::save_resume(
        &conn,
        "s1",
        &argus_lib::sessions::schema::ResumeRow {
            goal: "old work".into(),
            done: "x".into(),
            next: "y".into(),
        },
    )
    .unwrap();
    assert!(store::get_resume(&conn, "s1").is_some());

    store::clear_resume(&conn, "s1").unwrap();

    assert!(store::get_resume(&conn, "s1").is_none());
    assert!(prompt_include(&conn, "s1").is_none());
}

// Resume rows are scoped to their session, so a sub-agent never inherits the
// parent's unfinished work.
#[test]
fn resume_is_per_session() {
    let conn = db();
    conn.execute("INSERT INTO sessions (id, title) VALUES ('s2', 't')", [])
        .unwrap();
    store::save_resume(
        &conn,
        "s1",
        &argus_lib::sessions::schema::ResumeRow {
            goal: "parent work".into(),
            done: String::new(),
            next: String::new(),
        },
    )
    .unwrap();

    assert!(store::get_resume(&conn, "s1").is_some());
    assert!(store::get_resume(&conn, "s2").is_none());
}
