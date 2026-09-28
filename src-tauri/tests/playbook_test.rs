use argus_lib::playbook::store::{self, Kind, Scope};

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    store::migrate(&conn).unwrap();
    conn
}

fn count(conn: &rusqlite::Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

// Scope is a claim about who, and getting it wrong is how a sandbox refusal
// becomes "this model is bad at paths".
#[test]
fn each_signal_says_who_it_is_about() {
    let model = [Kind::EmptyArgs, Kind::Thrashing, Kind::StyleXml];
    let host = [
        Kind::SandboxDenied,
        Kind::SandboxNoCwd,
        Kind::BudgetStop,
        Kind::Degraded,
    ];

    for k in model {
        assert_eq!(k.scope(), Scope::Model, "{k:?} is about the model");
    }

    for k in host {
        assert_eq!(k.scope(), Scope::Host, "{k:?} is about this machine");
    }
}

// Round-trip: an unknown kind must not decode into one we do understand. A
// downgrade would let a future signal be laundered into a stale lesson.
#[test]
fn unknown_kinds_and_scopes_do_not_decode() {
    assert!(Kind::parse("empty_args").is_some());
    assert!(Kind::parse("something_new").is_none());
    assert!(Kind::parse("").is_none());
    assert!(Scope::parse("model").is_some());
    assert!(Scope::parse("everything").is_none());
}

// Repeats bump one row. A model that fails the same way a hundred times is
// one signal, and the table must not grow with it.
#[test]
fn a_repeated_signal_stays_one_row() {
    let conn = db();

    for _ in 0..25 {
        store::record(&conn, Kind::EmptyArgs, "prov/model", Some("s1"), "terminal").unwrap();
    }

    assert_eq!(count(&conn, "protocol_events"), 1);

    let got = store::signals_for(&conn, Kind::EmptyArgs, "prov/model").unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].seen, 25);
}

#[test]
fn different_scopes_do_not_merge() {
    let conn = db();
    store::record(&conn, Kind::EmptyArgs, "prov/a", None, "x").unwrap();
    store::record(&conn, Kind::EmptyArgs, "prov/b", None, "x").unwrap();

    assert_eq!(count(&conn, "protocol_events"), 2);
    assert_eq!(
        store::signals_for(&conn, Kind::EmptyArgs, "prov/a")
            .unwrap()
            .len(),
        1
    );
}

// Evidence accumulates on the lesson, not on a fresh copy of the sentence.
#[test]
fn lessons_accumulate_under_one_key() {
    let conn = db();

    let first = store::remember(&conn, Scope::Model, "prov/m", "empty_args", "send args").unwrap();
    let second = store::remember(&conn, Scope::Model, "prov/m", "empty_args", "send args").unwrap();

    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(count(&conn, "playbook_items"), 1);
}

// A multi-byte character must not panic the detail clip. Slicing at a byte
// offset is how this class of bug gets in.
#[test]
fn detail_clip_survives_multibyte_text() {
    let detail = "héllo wörld — ünïcode ✅ 🎉 and more to push past the cap";

    let clipped = store::clip_detail(detail);

    assert!(
        clipped.chars().count() <= 201,
        "{}",
        clipped.chars().count()
    );
    assert!(clipped.ends_with('…') || clipped.chars().count() <= 200);
}

#[test]
fn detail_is_one_line() {
    let clipped = store::clip_detail("line one\nline two\r\nline three");

    assert!(!clipped.contains('\n'), "{clipped}");
    assert!(!clipped.contains('\r'), "{clipped}");
    assert!(clipped.contains("line one"));
}

// An empty scope would attach a lesson to every model at once.
#[test]
fn an_empty_scope_is_refused() {
    let conn = db();

    assert!(store::record(&conn, Kind::EmptyArgs, "  ", None, "x").is_err());
    assert!(store::remember(&conn, Scope::Model, "", "k", "t").is_err());
    assert!(store::remember(&conn, Scope::Model, "s", "  ", "t").is_err());
    assert_eq!(count(&conn, "protocol_events"), 0);
    assert_eq!(count(&conn, "playbook_items"), 0);
}

// A lesson below the evidence bar is stored but never taught: the row exists,
// the prompt does not. This is the gate that keeps one unlucky turn from
// becoming a standing instruction.
#[test]
fn a_lesson_under_the_evidence_bar_is_not_taught() {
    let conn = db();
    store::remember(&conn, Scope::Model, "prov/m", "k", "t").unwrap();

    assert_eq!(count(&conn, "playbook_items"), 1, "stored for the record");
    assert_eq!(
        store::lessons(&conn, Scope::Model, "prov/m").unwrap().len(),
        0,
        "but not taught"
    );

    store::remember(&conn, Scope::Model, "prov/m", "k", "t").unwrap();
    assert_eq!(
        store::lessons(&conn, Scope::Model, "prov/m").unwrap().len(),
        1,
        "taught once it clears the bar"
    );
}

// Forgetting is a real delete, scoped, not a soft flag.
#[test]
fn forget_removes_only_the_named_scope() {
    let conn = db();
    for _ in 0..2 {
        store::remember(&conn, Scope::Model, "prov/a", "k", "t").unwrap();
        store::remember(&conn, Scope::Model, "prov/b", "k", "t").unwrap();
    }

    assert_eq!(store::forget(&conn, Scope::Model, "prov/a").unwrap(), 1);
    assert_eq!(
        store::lessons(&conn, Scope::Model, "prov/a").unwrap().len(),
        0
    );
    assert_eq!(
        store::lessons(&conn, Scope::Model, "prov/b").unwrap().len(),
        1,
        "the other scope is untouched"
    );

    assert_eq!(store::forget_all(&conn).unwrap(), 1);
    assert_eq!(count(&conn, "playbook_items"), 0);
}

// A database written before these tables existed gains them on migrate.
#[test]
fn an_old_database_gains_the_playbook_tables() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT NOT NULL);")
        .unwrap();

    store::migrate(&conn).unwrap();

    for table in ["protocol_events", "playbook_items"] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{table} must exist after migrate");
    }
}
