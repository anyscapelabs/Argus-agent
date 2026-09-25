use argus_lib::memory::store;
use rusqlite::params;

fn test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT, seq INTEGER, role TEXT, content TEXT, active INTEGER DEFAULT 1);
         CREATE TABLE summaries (id TEXT PRIMARY KEY, session_id TEXT, covers_to INTEGER, content TEXT);
         CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, updated_at TEXT);",
    )
    .unwrap();
    store::migrate(&conn).unwrap();
    conn
}

fn session(conn: &rusqlite::Connection, id: &str, title: &str, updated: &str) {
    conn.execute(
        "INSERT INTO sessions (id, title, updated_at) VALUES (?1, ?2, ?3)",
        params![id, title, updated],
    )
    .unwrap();
}

fn say(conn: &rusqlite::Connection, sid: &str, seq: i64, who: &str, text: &str) {
    conn.execute(
        "INSERT INTO messages (id, session_id, seq, role, content, active)
         VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        params![format!("{sid}#{seq}"), sid, seq, who, text],
    )
    .unwrap();
}

#[test]
fn finds_a_past_session_by_keyword() {
    let conn = test_db();
    session(&conn, "old", "Netflix investigation", "2026-01-01 10:00:00");
    say(
        &conn,
        "old",
        1,
        "user",
        "research the Netflix crime documentary lead",
    );

    let hits = store::recall_sessions(&conn, "Netflix documentary", None, 5).unwrap();

    assert_eq!(hits.len(), 1, "must reach across sessions");
    assert_eq!(hits[0].session_id, "old");
    assert_eq!(hits[0].title, "Netflix investigation");
    assert!(hits[0].snippets[0].contains("Netflix"));
}

#[test]
fn excludes_the_session_asking() {
    let conn = test_db();
    session(&conn, "old", "Netflix investigation", "2026-01-01 10:00:00");
    session(&conn, "cur", "current", "2026-01-02 10:00:00");
    say(&conn, "old", 1, "user", "netflix documentary research");
    say(
        &conn,
        "cur",
        1,
        "user",
        "netflix documentary research again",
    );

    let hits = store::recall_sessions(&conn, "netflix documentary", Some("cur"), 5).unwrap();

    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].session_id, "old",
        "must not return the asking session"
    );
}

#[test]
fn stopwords_alone_find_nothing() {
    let conn = test_db();
    session(&conn, "old", "Anything", "2026-01-01 10:00:00");
    say(&conn, "old", 1, "user", "the quick brown fox");

    assert!(store::recall_sessions(&conn, "what is the", None, 5)
        .unwrap()
        .is_empty());
}

#[test]
fn long_question_relaxes_to_or() {
    let conn = test_db();
    session(&conn, "old", "Trello cleanup", "2026-01-01 10:00:00");
    say(
        &conn,
        "old",
        1,
        "user",
        "moved every trello card into the archive board",
    );

    // AND over every term finds nothing; OR must still land.
    let hits = store::recall_sessions(
        &conn,
        "can you tell me again about that trello thing",
        None,
        5,
    )
    .unwrap();

    assert_eq!(hits.len(), 1, "loose match beats no match");
    assert_eq!(hits[0].session_id, "old");
}

#[test]
fn ranks_summary_hits_above_single_messages() {
    let conn = test_db();
    session(&conn, "one", "Mentioned once", "2026-01-03 10:00:00");
    session(&conn, "two", "Summarised twice", "2026-01-01 10:00:00");
    say(&conn, "one", 1, "user", "the postgres migration went badly");
    say(&conn, "two", 1, "user", "postgres migration notes");
    say(
        &conn,
        "two",
        2,
        "user",
        "postgres migration went badly again",
    );
    conn.execute(
        "INSERT INTO summaries (id, session_id, covers_to, content)
         VALUES ('s1', 'two', 2, 'postgres migration rollback and retry')",
        [],
    )
    .unwrap();

    let hits = store::recall_sessions(&conn, "postgres migration", None, 5).unwrap();

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].session_id, "two", "the summarised session wins");
}

#[test]
fn respects_the_limit() {
    let conn = test_db();
    for i in 0..8 {
        let sid = format!("s{i}");
        session(&conn, &sid, &format!("Session {i}"), "2026-01-01 10:00:00");
        say(&conn, &sid, 1, "user", "shared topic kubernetes rollout");
    }

    assert_eq!(
        store::recall_sessions(&conn, "kubernetes rollout", None, 3)
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn empty_index_is_not_an_error() {
    let conn = test_db();
    assert!(store::recall_sessions(&conn, "anything", None, 5)
        .unwrap()
        .is_empty());
}

#[test]
fn read_returns_prose_not_blocks() {
    let conn = test_db();
    session(&conn, "s", "Build failure", "2026-01-01 10:00:00");
    say(&conn, "s", 1, "user", "the build is red on main");
    say(
        &conn,
        "s",
        2,
        "assistant",
        "yes, the linker is missing\n<terminal id=\"a1\" command=\"cargo build\">error[E0432] unresolved</terminal>",
    );

    let t = store::read_session(&conn, "s", 0, 20).unwrap();

    assert_eq!(t.turns.len(), 2, "both turns survive");
    assert!(t.turns[1].text.contains("the linker is missing"));
    assert!(
        !t.turns[1].text.contains("unresolved"),
        "terminal body dropped"
    );
    assert!(!t.turns[1].text.contains("<terminal"), "block tag dropped");
}

#[test]
fn read_drops_tool_result_rows_entirely() {
    let conn = test_db();
    session(&conn, "s", "Chat", "2026-01-01 10:00:00");
    say(&conn, "s", 1, "user", "run the tests");
    say(
        &conn,
        "s",
        2,
        "user",
        "<tool-result tool=\"terminal\" status=\"ok\">test result: ok. 42 passed</tool-result>",
    );

    let t = store::read_session(&conn, "s", 0, 20).unwrap();

    assert_eq!(t.turns.len(), 1, "the tool row is not a turn");
    assert!(!t.turns[0].text.contains("42 passed"));
}

#[test]
fn read_pages_through_a_long_chat() {
    let conn = test_db();
    session(&conn, "s", "Long", "2026-01-01 10:00:00");
    for i in 1..=10 {
        say(&conn, "s", i, "user", &format!("message number {i}"));
    }

    let page1 = store::read_session(&conn, "s", 0, 4).unwrap();
    assert_eq!(page1.turns.len(), 4);
    assert!(page1.more, "must signal there is more");
    assert_eq!(page1.turns[0].text, "message number 1");

    let page2 = store::read_session(&conn, "s", page1.next_seq, 4).unwrap();
    assert_eq!(
        page2.turns[0].text, "message number 5",
        "resumes after the last turn"
    );
    assert!(page2.more, "two messages still follow");

    let page3 = store::read_session(&conn, "s", page2.next_seq, 4).unwrap();
    assert_eq!(page3.turns.len(), 2, "the tail holds the remainder");
    assert_eq!(
        page3.turns[1].text, "message number 10",
        "nothing is skipped"
    );
    assert!(!page3.more, "the tail is the last page");
}

#[test]
fn reading_an_unknown_session_is_an_error() {
    let conn = test_db();
    assert!(store::read_session(&conn, "nope", 0, 20).is_err());
}

#[test]
fn read_costs_a_fraction_of_the_raw_transcript() {
    let conn = test_db();
    session(&conn, "s", "Busy", "2026-01-01 10:00:00");
    let noise = "output ".repeat(300);
    say(&conn, "s", 1, "user", "deploy the service");
    for i in 2..=20 {
        say(
            &conn,
            "s",
            i,
            "user",
            &format!("<tool-result tool=\"terminal\">{noise}</tool-result>"),
        );
    }
    say(&conn, "s", 21, "assistant", "deployed, all green");

    let raw: i64 = conn
        .query_row("SELECT SUM(length(content)) FROM messages", [], |r| {
            r.get(0)
        })
        .unwrap();
    let t = store::read_session(&conn, "s", 0, 40).unwrap();
    let read: usize = t.turns.iter().map(|x| x.text.len()).sum();

    assert!(t.turns.iter().any(|x| x.text.contains("deployed")));
    assert!(
        (read as i64) * 10 < raw,
        "read {read} must be well under raw {raw}"
    );
}
