use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use argus_lib::gateway::Gateway;
use argus_lib::profiles;
use argus_lib::sessions::schema::{NewMsg, NewSession};
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
        skills_dir: std::env::temp_dir().join("argus-profiles-skills"),
        library_dir: std::env::temp_dir().join("argus-profiles-library"),
        logos_dir: std::env::temp_dir().join("argus-profiles-logos"),
        jobs_dir: std::env::temp_dir().join("argus-profiles-logs"),
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

fn new_session(conn: &rusqlite::Connection, title: &str) -> String {
    store::create_session(
        conn,
        &NewSession {
            title: title.into(),
            model_id: None,
            permission: Some("never".into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap()
    .id
}

fn prompt_of(conn: &rusqlite::Connection, session_id: &str) -> String {
    argus_lib::prompt::project(conn, session_id).unwrap().system
}

#[test]
fn a_fresh_database_has_one_nameless_profile() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let list = profiles::store::list(&conn).unwrap();

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "default");
    assert!(list[0].name.is_empty());
}

#[test]
fn a_new_chat_belongs_to_the_default_without_being_told_to() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let id = new_session(&conn, "plain");

    assert_eq!(
        profiles::store::of_session(&conn, &id).unwrap().as_deref(),
        Some("default")
    );
}

#[test]
fn created_profiles_come_back_with_the_name_they_were_given() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "  Senior Developer  ").unwrap();

    assert_eq!(p.name, "Senior Developer");
    assert_eq!(
        profiles::store::get(&conn, &p.id).unwrap().name,
        "Senior Developer"
    );
}

#[test]
fn the_default_stays_at_the_top_of_the_list() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    profiles::store::create(&conn, "Manager").unwrap();
    profiles::store::create(&conn, "Accountant").unwrap();

    let list = profiles::store::list(&conn).unwrap();

    assert_eq!(list[0].id, "default");
    assert_eq!(list.len(), 3);
}

#[test]
fn the_eleventh_profile_is_refused() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    // The default counts toward the cap — it occupies a slot in the list.
    for i in 0..9 {
        profiles::store::create(&conn, &format!("p{i}")).unwrap();
    }

    let err = profiles::store::create(&conn, "one too many").unwrap_err();

    assert!(err.contains("cap"), "got: {err}");
    assert_eq!(profiles::store::list(&conn).unwrap().len(), 10);
}

#[test]
fn the_default_profile_cannot_be_deleted() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let err = profiles::store::delete(&conn, "default").unwrap_err();

    assert!(err.contains("default"), "got: {err}");
    assert!(profiles::store::get(&conn, "default").is_ok());
}

#[test]
fn a_profile_still_owning_chats_refuses_to_be_deleted() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Manager").unwrap();
    let sid = new_session(&conn, "work");
    profiles::store::set_for_session(&conn, &sid, &p.id).unwrap();

    let err = profiles::store::delete(&conn, &p.id).unwrap_err();

    assert!(err.contains("chats"), "got: {err}");
}

#[test]
fn an_unused_profile_goes_away_cleanly() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Manager").unwrap();
    profiles::store::delete(&conn, &p.id).unwrap();

    assert!(profiles::store::get(&conn, &p.id).is_err());
}

#[test]
fn editing_a_name_leaves_the_instructions_alone() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Dev").unwrap();
    let p = profiles::store::edit(&conn, &p.id, Some("Reviewer"), None).unwrap();

    assert_eq!(p.name, "Reviewer");
    assert_eq!(p.instructions, "");
}

#[test]
fn a_chat_can_move_to_another_profile_at_any_time() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Reviewer").unwrap();
    let sid = new_session(&conn, "already talking");

    store::add_msg(
        &conn,
        &NewMsg {
            session_id: sid.clone(),
            role: "user".into(),
            content: "hello".into(),
            model_id: None,
            provider_id: None,
            tok_in: None,
            tok_out: None,
            tool_calls: None,
            tool_call_id: None,
        },
    )
    .unwrap();

    profiles::store::set_for_session(&conn, &sid, &p.id).unwrap();

    assert_eq!(
        profiles::store::of_session(&conn, &sid).unwrap().as_deref(),
        Some(p.id.as_str())
    );
}

#[test]
fn a_profile_that_does_not_exist_is_refused_loudly() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let sid = new_session(&conn, "chat");

    let err = profiles::store::set_for_session(&conn, &sid, "nope").unwrap_err();

    assert!(err.contains("not found"), "got: {err}");
}

#[test]
fn a_sub_agent_inherits_its_parents_profile() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Senior Developer").unwrap();
    let pid = new_session(&conn, "parent");
    profiles::store::set_for_session(&conn, &pid, &p.id).unwrap();

    let child = store::create_child(&conn, &pid, "helper", "helper work", None, "never").unwrap();

    assert_eq!(
        profiles::store::of_session(&conn, &child.id)
            .unwrap()
            .as_deref(),
        Some(p.id.as_str())
    );
}

#[test]
fn a_sub_agent_of_the_default_still_gets_a_profile() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let pid = new_session(&conn, "parent");
    let child = store::create_child(&conn, &pid, "helper", "helper work", None, "never").unwrap();

    assert_eq!(
        profiles::store::of_session(&conn, &child.id)
            .unwrap()
            .as_deref(),
        Some("default")
    );
}

#[test]
fn instructions_reach_the_prompt_without_displacing_the_base() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Code Reviewer").unwrap();
    profiles::store::edit(
        &conn,
        &p.id,
        None,
        Some("Quote file:line for every claim. Report, do not fix."),
    )
    .unwrap();

    let sid = new_session(&conn, "review");
    profiles::store::set_for_session(&conn, &sid, &p.id).unwrap();

    let system = prompt_of(&conn, &sid);

    assert!(system.contains("<profile>"), "no profile layer");
    assert!(system.contains("Code Reviewer"), "name missing");
    assert!(
        system.contains("Quote file:line for every claim."),
        "instructions missing"
    );
    // Added to, not substituted for. These are the rules a profile cannot lose.
    assert!(
        system.contains("PERMISSIONS"),
        "base lost its approval rules"
    );
    assert!(
        system.contains("RECOVERY"),
        "base lost its recovery section"
    );
}

#[test]
fn a_nameless_profile_with_no_instructions_adds_nothing() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let sid = new_session(&conn, "plain");
    let bare = prompt_of(&conn, &sid);

    let p = profiles::store::create(&conn, "Ghost").unwrap();
    profiles::store::set_for_session(&conn, &sid, &p.id).unwrap();
    let named = prompt_of(&conn, &sid);

    // A name is a label, not a mechanism.
    assert!(!bare.contains("<profile>"));
    assert!(!named.contains("<profile>"));
    assert_eq!(bare, named);
}

#[test]
fn editing_the_prompt_changes_the_next_turn_but_not_the_old_one() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Reviewer").unwrap();
    profiles::store::edit(&conn, &p.id, None, Some("first rule")).unwrap();

    let sid = new_session(&conn, "chat");
    profiles::store::set_for_session(&conn, &sid, &p.id).unwrap();

    let before = prompt_of(&conn, &sid);
    assert!(before.contains("first rule"));

    profiles::store::edit(&conn, &p.id, None, Some("second rule")).unwrap();
    let after = prompt_of(&conn, &sid);

    // The personality changes exactly when the user changes the prompt.
    assert!(!after.contains("first rule"), "stale instructions survived");
    assert!(after.contains("second rule"));
}

#[test]
fn a_child_agent_answers_as_the_profile_too() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Accountant").unwrap();
    profiles::store::edit(&conn, &p.id, None, Some("Reconcile before reporting.")).unwrap();

    let pid = new_session(&conn, "parent");
    profiles::store::set_for_session(&conn, &pid, &p.id).unwrap();
    let child = store::create_child(&conn, &pid, "helper", "helper work", None, "never").unwrap();

    let system = prompt_of(&conn, &child.id);

    assert!(system.contains("Reconcile before reporting."));
    // And a sub-agent still gets the sub-agent rules, not the parent's.
    assert!(system.contains("Accountant"));
}
