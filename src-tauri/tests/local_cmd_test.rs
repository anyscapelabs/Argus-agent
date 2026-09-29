use argus_lib::library::schema::MIGRATE as LIB_MIGRATE;

// A line the app answered itself. It is a turn in the transcript and nothing
// else: the model must never be told the user said it, because they did not.

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    conn.execute_batch(LIB_MIGRATE).unwrap();
    conn
}

fn session(conn: &rusqlite::Connection) -> String {
    use argus_lib::sessions::schema::NewSession;
    argus_lib::sessions::store::create_session(
        conn,
        &NewSession {
            title: "usage".into(),
            model_id: Some("mock/test".into()),
            permission: Some("never".into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap()
    .id
}

fn say(conn: &rusqlite::Connection, sid: &str, role: &str, content: &str) {
    use argus_lib::sessions::schema::NewMsg;
    argus_lib::sessions::store::add_msg(
        conn,
        &NewMsg {
            session_id: sid.into(),
            role: role.into(),
            content: content.into(),
            model_id: None,
            provider_id: None,
            tok_in: None,
            tok_out: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: None,
        },
    )
    .unwrap();
}

// The whole point of the flag. A stored /usage is a real user row, and if the
// model ever sees it, it is reading a command nobody sent it — on the turn
// after, in the middle of whatever else the user was saying.
#[test]
fn the_model_is_never_told_a_line_the_app_answered() {
    let conn = db();
    let sid = session(&conn);

    argus_lib::sessions::store::add_local_msg(&conn, &sid, "/usage month").unwrap();
    say(&conn, &sid, "user", "now write the parser");
    say(&conn, &sid, "assistant", "on it");

    let p = argus_lib::prompt::project(&conn, &sid, std::path::Path::new("/tmp")).unwrap();
    let said: Vec<&str> = p.msgs.iter().map(|m| m.content.as_str()).collect();

    assert!(
        !said.iter().any(|c| c.contains("/usage")),
        "the wire carries a command the model was never sent: {said:?}"
    );
    assert!(
        said.contains(&"now write the parser"),
        "the real message is still there: {said:?}"
    );
}

// The other half: excluded from the wire, and still a turn. A row that only
// half exists is a card the transcript cannot draw.
#[test]
fn a_local_command_comes_back_as_a_turn_the_transcript_can_draw() {
    let conn = db();
    let sid = session(&conn);

    let msg = argus_lib::sessions::store::add_local_msg(&conn, &sid, "/usage month").unwrap();
    assert!(msg.local, "the transcript keys the card off this flag");
    assert_eq!(msg.role, "user");
    assert_eq!(msg.content, "/usage month");

    let rows = argus_lib::sessions::store::list_msgs(&conn, &sid).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].local);
}

// Written because this is the failure that would look like the feature
// randomly not working: `clean_dangling` runs on every open, and a local row
// has no assistant reply by design, so without the filter the card greys out
// the next time the chat is looked at.
#[test]
fn opening_the_chat_does_not_erase_the_turn() {
    let conn = db();
    let sid = session(&conn);

    argus_lib::sessions::store::add_local_msg(&conn, &sid, "/usage").unwrap();

    let swept = argus_lib::sessions::store::clean_dangling(&conn, &sid).unwrap();
    assert_eq!(swept, 0, "an app-answered line is not a dangling message");

    let rows = argus_lib::sessions::store::list_msgs(&conn, &sid).unwrap();
    assert_eq!(rows.len(), 1, "still there after the sweep: {rows:?}");
    assert!(rows[0].active);
}

// The same sweep, next to the case it exists for: a real unanswered line is
// still swept, so the fix did not blunt the query.
#[test]
fn a_genuinely_unanswered_line_is_still_swept() {
    let conn = db();
    let sid = session(&conn);

    say(&conn, &sid, "user", "are you there");

    let swept = argus_lib::sessions::store::clean_dangling(&conn, &sid).unwrap();
    assert_eq!(swept, 1);
    assert!(argus_lib::sessions::store::list_msgs(&conn, &sid)
        .unwrap()
        .is_empty());
}

// Nothing was sent, so nothing was worked on. Counting a /usage as a turn
// would report time the user did not spend with the agent.
#[test]
fn a_local_turn_costs_no_worked_time() {
    let conn = db();
    let sid = session(&conn);

    say(&conn, &sid, "user", "do the thing");
    argus_lib::sessions::store::add_local_msg(&conn, &sid, "/usage").unwrap();

    let worked = argus_lib::slash::usage::report(&conn, "week")
        .unwrap()
        .worked_ms;
    assert_eq!(worked, 0.0);
}

// A bad window is refused when the row is written, not left behind as a turn
// the transcript cannot draw. And the row is only the record: the card reads
// live numbers, so `/usage` alone is the right text.
#[test]
fn the_stored_line_is_exactly_what_the_user_typed() {
    assert_eq!(
        argus_lib::slash::local_turn_text("usage", "month").unwrap(),
        "/usage month"
    );
    assert_eq!(
        argus_lib::slash::local_turn_text("usage", "  ").unwrap(),
        "/usage",
        "an empty argument is a command with no argument, not a trailing space"
    );

    let err = argus_lib::slash::local_turn_text("usage", "fortnight").unwrap_err();
    assert!(err.contains("fortnight"), "{err}");
}

// Local in the registry is not the same as answered without a model.
// /compact runs here but costs a model call, so it cannot be a stored turn;
// /review is a macro the user is asking the model to run. Neither is a line the
// app answers.
#[test]
fn a_command_that_needs_a_model_is_refused_here() {
    for name in ["compact", "review", "help", "nope", ""] {
        let err = argus_lib::slash::local_turn_text(name, "").unwrap_err();
        assert!(
            err.contains("not answered by the app itself"),
            "/{name} should be refused, got: {err}"
        );
    }

    assert!(argus_lib::slash::answered_locally("usage"));
    assert!(!argus_lib::slash::answered_locally("compact"));
}
