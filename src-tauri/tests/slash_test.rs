use argus_lib::slash::usage;

// A database with the two tables /usage reads, and nothing else.
fn db() -> (rusqlite::Connection, String) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    use argus_lib::sessions::schema::NewSession;
    let sid = argus_lib::sessions::store::create_session(
        &conn,
        &NewSession {
            title: "usage".into(),
            model_id: Some("mock/test".into()),
            permission: Some("never".into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap()
    .id;

    (conn, sid)
}

/// A message at a fixed offset from now, because the worked-for arithmetic is
/// only interesting when the two ends of a turn differ.
fn say(conn: &rusqlite::Connection, sid: &str, role: &str, content: &str, mins_ago: i64) {
    conn.execute(
        "INSERT INTO messages (id, session_id, seq, role, content, active, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, datetime('now', ?6))",
        rusqlite::params![
            format!("m{role}{mins_ago}{}", content.len()),
            sid,
            mins_ago,
            role,
            content,
            format!("-{mins_ago} minutes"),
        ],
    )
    .unwrap();
}

fn log(
    conn: &rusqlite::Connection,
    model: &str,
    status: &str,
    tok_in: i64,
    tok_out: i64,
    cost: f64,
) {
    conn.execute(
        "INSERT INTO request_log (ts, model_id, provider_id, status, tok_in, tok_out, cost)
         VALUES (datetime('now', '-1 hour'), ?1, 'p', ?2, ?3, ?4, ?5)",
        rusqlite::params![model, status, tok_in, tok_out, cost],
    )
    .unwrap();
}

#[test]
fn an_empty_history_reports_zeroes_rather_than_an_error() {
    let (conn, _sid) = db();
    let out = usage::report(&conn, "week").unwrap();

    assert!(out.contains("nothing yet"), "{out}");
    assert!(!out.contains("NaN"), "{out}");
}

#[test]
fn a_bad_period_is_refused_by_name() {
    let (conn, _sid) = db();
    let err = usage::report(&conn, "fortnight").unwrap_err();

    assert!(err.contains("fortnight"), "{err}");
    for word in ["today", "week", "month"] {
        assert!(err.contains(word), "{word} must be offered: {err}");
    }
}

#[test]
fn spend_and_failures_are_totalled_per_model() {
    let (conn, _sid) = db();
    log(&conn, "opus", "ok", 1000, 500, 3.0);
    log(&conn, "opus", "error", 10, 0, 0.0);
    log(&conn, "mini", "ok", 200, 100, 0.25);

    let out = usage::report(&conn, "week").unwrap();

    assert!(
        out.contains("3 requests"),
        "a failed attempt is a request: {out}"
    );
    assert!(out.contains("1 failed"), "{out}");
    assert!(out.contains("| opus | 2 |"), "{out}");
    assert!(out.contains("| mini | 1 |"), "{out}");
    assert!(out.contains("$3.25 total"), "{out}");
}

// The check that matters: the SQL and the transcript must be counting the
// same thing, or the weekly total silently disagrees with the labels above
// each turn.
#[test]
fn worked_time_is_the_span_the_transcript_labels_each_turn() {
    let (conn, sid) = db();

    // Turn one: user, then a tool-only assistant row, then real text 6 minutes
    // later. The transcript starts its clock at the first row with real text,
    // not at the tool row, so this turn is 6 minutes -- not 8.
    say(&conn, &sid, "user", "do the thing", 10);
    say(&conn, &sid, "assistant", "<thinking>hm</thinking>", 8);
    say(&conn, &sid, "assistant", "done", 4);

    // Turn two: a tool-result row must not start a turn, and an unanswered
    // turn has no end, so it scores zero.
    say(&conn, &sid, "user", "<tool-result>ok</tool-result>", 3);
    say(&conn, &sid, "user", "and this one", 2);

    let worked = usage::report(&conn, "week").unwrap();
    assert!(worked.contains("Worked for 4 min"), "{worked}");
    assert!(
        !worked.contains("4 min 2 sec"),
        "an unanswered turn is not time spent: {worked}"
    );
}

#[test]
fn a_turn_with_no_assistant_reply_scores_zero_not_a_negative() {
    let (conn, sid) = db();
    say(&conn, &sid, "user", "hello", 5);

    let out = usage::report(&conn, "week").unwrap();
    assert!(out.contains("nothing yet"), "{out}");
}

#[test]
fn spend_of_a_cent_under_reads_as_such_rather_than_as_zero() {
    let (conn, _sid) = db();
    log(&conn, "mini", "ok", 100, 50, 0.004);

    let out = usage::report(&conn, "week").unwrap();
    assert!(out.contains("under a cent"), "{out}");
}

#[test]
fn the_registry_describes_only_commands_that_exist() {
    // /help renders the same rows the dispatcher matches on, so this holds by
    // construction -- the test is here to catch someone adding a second list.
    let help = argus_lib::slash::help();
    for c in argus_lib::slash::CMDS {
        assert!(help.contains(c.name), "{} missing from help", c.name);
        assert!(
            help.contains(c.desc),
            "{} has no description in help",
            c.name
        );
    }
}

#[test]
fn a_prompt_macro_substitutes_its_argument() {
    let tpl = argus_lib::slash::CMDS
        .iter()
        .find(|c| c.name == "review")
        .and_then(|c| c.tpl)
        .expect("review is a prompt macro");

    let out = tpl.replace("{{arg}}", "src/lib/ipc.ts");
    assert!(out.contains("src/lib/ipc.ts"), "{out}");
    assert!(
        !out.contains("{{arg}}"),
        "every placeholder must fill: {out}"
    );
}

#[test]
fn a_client_command_is_refused_rather_than_answered_by_the_backend() {
    let c = argus_lib::slash::CMDS
        .iter()
        .find(|c| c.name == "clear")
        .expect("clear is in the registry");

    // The files it drops are the window's, not the library's, so a backend
    // that answered it would be claiming an effect it cannot have.
    assert_eq!(c.kind, argus_lib::slash::Kind::Client);
    assert!(
        argus_lib::slash::help().contains("handled by the app"),
        "help must say who runs it"
    );
}
