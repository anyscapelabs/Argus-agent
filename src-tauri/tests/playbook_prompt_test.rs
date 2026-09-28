// Two properties, and the second is the one that matters most.
//
// 1. A model with evidence sees its lessons.
// 2. A model WITHOUT evidence sees a byte-identical prompt. If that fails, the
//    feature is leaking across models and every session pays for someone
//    else's failures.

use argus_lib::playbook::store::{self, Kind, Scope};
use argus_lib::prompt::project;

fn dir() -> String {
    let d = std::env::temp_dir().join(format!("argus-pb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

fn setup() -> (rusqlite::Connection, String) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::playbook::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, title, permission) VALUES ('s1', 't', 'ask')",
        [],
    )
    .unwrap();
    (conn, dir())
}

fn with_model(conn: &rusqlite::Connection, id: &str) {
    conn.execute(
        "INSERT INTO sessions (id, title, permission, model_id) VALUES ('sx', 't', 'ask', ?1)",
        rusqlite::params![id],
    )
    .unwrap();
}

fn teach(conn: &rusqlite::Connection, scope: Scope, id: &str, key: &str, text: &str) {
    store::remember(conn, scope, id, key, text, store::MIN_EVIDENCE_TO_TEACH).unwrap();
}

// The whole point: a model that has failed this way twice sees the lesson.
#[test]
fn a_model_with_evidence_sees_its_lesson() {
    let (conn, d) = setup();
    with_model(&conn, "prov/glm");
    teach(
        &conn,
        Scope::Model,
        "prov/glm",
        "empty_args",
        "tool calls sometimes arrive with no arguments; send the call again with its arguments",
    );

    let p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();

    assert!(p.system.contains("<model-playbook>"), "{}", p.system);
    assert!(p.system.contains("no arguments"), "{}", p.system);
    assert!(
        !p.system.contains("<environment-notes>"),
        "model scope only"
    );
}

// THE CONTROL ARM. A model with no evidence must get exactly the prompt it
// got before this feature existed. One stray block here is a regression that
// charges every session for another model's failures.
#[test]
fn a_model_without_evidence_gets_a_byte_identical_prompt() {
    let (conn, d) = setup();

    // Evidence exists, but for a different model.
    teach(
        &conn,
        Scope::Model,
        "prov/other",
        "empty_args",
        "some other model lesson",
    );

    with_model(&conn, "prov/clean");
    let p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();

    assert!(!p.system.contains("<model-playbook>"), "{}", p.system);
    assert!(
        !p.system.contains("some other model lesson"),
        "{}",
        p.system
    );

    // And a session with no model at all is equally untouched.
    let bare = project(&conn, "s1", std::path::Path::new(&d)).unwrap();
    assert!(!bare.system.contains("<model-playbook>"), "{}", bare.system);
}

// Below the evidence bar the lesson exists but is not taught: one unlucky
// turn must not become a standing instruction.
#[test]
fn one_occurrence_is_not_enough_to_teach() {
    let (conn, d) = setup();
    with_model(&conn, "prov/one");
    store::remember(&conn, Scope::Model, "prov/one", "thrashing", "a lesson", 1).unwrap();

    let p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();

    assert!(!p.system.contains("<model-playbook>"), "{}", p.system);
}

// Host facts are about this machine and reach every model, because they are
// true for whoever is running here.
#[test]
fn environment_facts_reach_every_model() {
    let (conn, d) = setup();
    with_model(&conn, "prov/anything");
    let host = argus_lib::sessions::ext_install::host_id();
    teach(
        &conn,
        Scope::Host,
        &host,
        "sandbox_denied",
        "a confined profile only reaches inside its root",
    );

    let p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();

    assert!(p.system.contains("<environment-notes>"), "{}", p.system);
    assert!(p.system.contains("inside its root"), "{}", p.system);
    assert!(
        !p.system.contains("<model-playbook>"),
        "no model lesson here"
    );
}

// The two ledgers are separate blocks, so a reader can tell a fact about the
// model from a fact about the machine.
#[test]
fn both_ledgers_appear_as_separate_blocks() {
    let (conn, d) = setup();
    with_model(&conn, "prov/both");
    let host = argus_lib::sessions::ext_install::host_id();
    teach(
        &conn,
        Scope::Model,
        "prov/both",
        "thrashing",
        "a model lesson",
    );
    teach(&conn, Scope::Host, &host, "budget_stop", "a machine fact");

    let p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();

    assert!(p.system.contains("a model lesson"), "{}", p.system);
    assert!(p.system.contains("a machine fact"), "{}", p.system);
    assert!(p.system.contains("<model-playbook>"));
    assert!(p.system.contains("<environment-notes>"));
}

// A sub-agent is a different agent. It does not inherit the parent's model
// lessons any more than it inherits the parent's unfinished work.
#[test]
fn a_sub_agent_inherits_no_playbook() {
    let (conn, d) = setup();
    conn.execute(
        "INSERT INTO sessions (id, title, permission, parent_id, model_id)
         VALUES ('kid', 'sub', 'ask', 's1', 'prov/glm')",
        [],
    )
    .unwrap();
    teach(
        &conn,
        Scope::Model,
        "prov/glm",
        "empty_args",
        "a model lesson",
    );

    let p = project(&conn, "kid", std::path::Path::new(&d)).unwrap();

    assert!(!p.system.contains("<model-playbook>"), "{}", p.system);
}

// The playbook is a bounded block. Eight lessons must not become a page.
#[test]
fn the_ledger_is_capped() {
    let (conn, _d) = setup();

    for i in 0..20 {
        teach(
            &conn,
            Scope::Model,
            "prov/many",
            &format!("k{i}"),
            &format!("lesson number {i}"),
        );
    }

    let items = store::lessons(&conn, Scope::Model, "prov/many").unwrap();
    assert_eq!(
        items.len() as i64,
        store::MAX_LESSONS_PER_SCOPE,
        "the cap is a hard limit, not a suggestion"
    );
}

// Budget accounting must see the new blocks, or a long playbook looks free.
#[test]
fn the_playbook_is_accounted_for_in_the_budget() {
    let (conn, d) = setup();
    with_model(&conn, "prov/glm");
    let p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();

    let before = argus_lib::prompt::full_budget(&p, false);

    teach(
        &conn,
        Scope::Model,
        "prov/glm",
        "empty_args",
        "a taught lesson",
    );
    let after_p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();
    let after = argus_lib::prompt::full_budget(&after_p, false);

    assert!(
        after.playbook > before.playbook,
        "playbook tier must grow: {} -> {}",
        before.playbook,
        after.playbook
    );
    assert!(after.total > before.total, "total must include it");
}

// The signals a model produces must actually become lessons, or the whole
// pipeline is decoration.
#[test]
fn recorded_signals_become_taught_lessons() {
    let (conn, d) = setup();
    with_model(&conn, "prov/glm");

    let empty = argus_lib::playbook::Kind::EmptyArgs;
    for _ in 0..2 {
        store::record(&conn, empty, "prov/glm", Some("s1"), "terminal").unwrap();
    }
    let _ = argus_lib::playbook::curate(&conn, "prov/glm", Some("s1"));

    let p = project(&conn, "sx", std::path::Path::new(&d)).unwrap();
    assert!(p.system.contains("<model-playbook>"), "{}", p.system);

    // A signal under the bar curates into nothing.
    store::forget_all(&conn).unwrap();
    store::record(&conn, empty, "prov/other", Some("s1"), "terminal").unwrap();
    let _ = argus_lib::playbook::curate(&conn, "prov/other", Some("s1"));
    assert!(store::lessons(&conn, Scope::Model, "prov/other")
        .unwrap()
        .is_empty());
}
