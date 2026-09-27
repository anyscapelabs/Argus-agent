use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use argus_lib::gateway::Gateway;
use argus_lib::profiles;
use argus_lib::profiles::schema::Grant;
use argus_lib::sessions::schema::{NewMsg, NewSession};
use argus_lib::sessions::store;
use argus_lib::tools;
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
            attachments: None,
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

#[test]
fn a_listed_session_carries_the_profile_that_owns_it() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Manager").unwrap();
    let mine = new_session(&conn, "mine");
    let theirs = new_session(&conn, "theirs");
    profiles::store::set_for_session(&conn, &mine, &p.id).unwrap();

    let list = store::list_sessions(&conn, None).unwrap();
    let by_id = |id: &str| {
        list.iter()
            .find(|s| s.id == id)
            .map(|s| s.profile_id.clone())
            .unwrap()
    };

    // The subquery for running_agents sits after profile_id. Getting the two
    // out of step would hand every chat a count where the id should be.
    assert_eq!(by_id(&mine).as_deref(), Some(p.id.as_str()));
    assert_eq!(by_id(&theirs).as_deref(), Some("default"));
    assert!(list.iter().all(|s| s.running_agents == 0));
}

#[test]
fn nothing_picked_yet_means_the_default() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    assert_eq!(profiles::store::active(&conn), "default");
}

#[test]
fn the_pick_survives_being_read_back() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Manager").unwrap();
    profiles::store::set_active(&conn, &p.id).unwrap();

    // A separate connection stands in for the next launch.
    assert_eq!(profiles::store::active(&conn), p.id);
}

#[test]
fn a_pick_that_names_a_missing_profile_is_refused() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    profiles::store::create(&conn, "Manager").unwrap();
    profiles::store::set_active(&conn, "ghost").unwrap_err();
}

fn grant(capability: &str, target_id: &str) -> Grant {
    Grant {
        capability: capability.into(),
        target_id: target_id.into(),
    }
}

#[test]
fn a_new_profile_reaches_nothing() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let p = profiles::store::create(&conn, "Manager").unwrap();
    let r = profiles::store::reach(&conn, &p.id).unwrap();

    // Off by default. A profile that could see everything the moment it was
    // created would be a profile the user never agreed to.
    assert!(!r.reach_all);
    assert!(r.grants.is_empty());
}

#[test]
fn grants_survive_being_read_back() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();
    let b = profiles::store::create(&conn, "Accountant").unwrap();

    profiles::store::set_reach(
        &conn,
        &a.id,
        false,
        vec![grant("see_activity", &b.id), grant("read_chats", &b.id)],
    )
    .unwrap();

    let r = profiles::store::reach(&conn, &a.id).unwrap();

    assert!(!r.reach_all);
    assert_eq!(r.grants.len(), 2);
    assert!(r.grants.contains(&grant("see_activity", &b.id)));
    assert!(r.grants.contains(&grant("read_chats", &b.id)));
}

#[test]
fn reach_all_keeps_the_matrix_underneath_it() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();
    let b = profiles::store::create(&conn, "Accountant").unwrap();
    let custom = vec![grant("see_activity", &b.id)];

    profiles::store::set_reach(&conn, &a.id, false, custom.clone()).unwrap();
    profiles::store::set_reach(&conn, &a.id, true, custom).unwrap();

    // "All profiles" is a switch, not a deletion. Turning it off again must
    // give back exactly what was there before.
    let all = profiles::store::reach(&conn, &a.id).unwrap();
    assert!(all.reach_all);
    assert_eq!(all.grants.len(), 1);

    profiles::store::set_reach(&conn, &a.id, false, all.grants).unwrap();
    let back = profiles::store::reach(&conn, &a.id).unwrap();

    assert!(!back.reach_all);
    assert_eq!(back.grants.len(), 1);
}

#[test]
fn setting_the_matrix_replaces_it_rather_than_adding_to_it() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();
    let b = profiles::store::create(&conn, "Accountant").unwrap();
    let c = profiles::store::create(&conn, "Writer").unwrap();

    profiles::store::set_reach(&conn, &a.id, false, vec![grant("read_chats", &b.id)]).unwrap();
    profiles::store::set_reach(&conn, &a.id, false, vec![grant("see_activity", &c.id)]).unwrap();

    // A revoked permission is gone, not shadowed by a stale row.
    let r = profiles::store::reach(&conn, &a.id).unwrap();
    assert_eq!(r.grants, vec![grant("see_activity", &c.id)]);
}

#[test]
fn the_same_cell_twice_is_one_permission() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();
    let b = profiles::store::create(&conn, "Accountant").unwrap();

    profiles::store::set_reach(
        &conn,
        &a.id,
        false,
        vec![grant("read_chats", &b.id), grant("read_chats", &b.id)],
    )
    .unwrap();

    assert_eq!(
        profiles::store::reach(&conn, &a.id).unwrap().grants.len(),
        1
    );
}

#[test]
fn a_capability_that_is_not_on_the_ladder_is_refused() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();
    let b = profiles::store::create(&conn, "Accountant").unwrap();

    // A capability the code has never heard of would sit in the table forever
    // and enforce nothing.
    let err =
        profiles::store::set_reach(&conn, &a.id, false, vec![grant("delete_everything", &b.id)])
            .unwrap_err();

    assert!(err.contains("unknown capability"), "got: {err}");
    assert!(profiles::store::reach(&conn, &a.id)
        .unwrap()
        .grants
        .is_empty());
}

#[test]
fn a_profile_is_never_a_target_of_itself() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();

    let err = profiles::store::set_reach(&conn, &a.id, false, vec![grant("change_access", &a.id)])
        .unwrap_err();

    assert!(err.contains("itself"), "got: {err}");
}

#[test]
fn granting_to_a_profile_that_is_not_there_is_refused() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();

    let err = profiles::store::set_reach(&conn, &a.id, false, vec![grant("read_chats", "ghost")])
        .unwrap_err();

    assert!(err.contains("not found"), "got: {err}");
}

#[test]
fn reach_is_one_way() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();
    let b = profiles::store::create(&conn, "Accountant").unwrap();

    profiles::store::set_reach(&conn, &a.id, false, vec![grant("read_chats", &b.id)]).unwrap();

    // The permission is the grantor's, not a mutual one. The other direction is
    // a separate cell and it is still empty.
    assert_eq!(
        profiles::store::reach(&conn, &a.id).unwrap().grants.len(),
        1
    );
    assert!(profiles::store::reach(&conn, &b.id)
        .unwrap()
        .grants
        .is_empty());
}

#[test]
fn deleting_a_profile_takes_its_grants_with_it() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let a = profiles::store::create(&conn, "Manager").unwrap();
    let b = profiles::store::create(&conn, "Accountant").unwrap();

    profiles::store::set_reach(&conn, &a.id, false, vec![grant("read_chats", &b.id)]).unwrap();
    profiles::store::delete(&conn, &a.id).unwrap();

    let left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM profile_grants WHERE profile_id = ?1",
            rusqlite::params![a.id],
            |r| r.get(0),
        )
        .unwrap();

    // b outlived a. Nothing may point at a profile that is gone.
    assert_eq!(left, 0);
    assert!(profiles::store::reach(&conn, &b.id).is_ok());
}

// --- profile.list / profile.read -------------------------------------------------
// The two read-only tools. What matters here is that a refusal is loud: a tool
// that returns nothing when the grant is missing reads as "they are idle".

fn chat_of(conn: &rusqlite::Connection, title: &str, profile_id: &str, body: &str) -> String {
    let id = new_session(conn, title);

    profiles::store::set_for_session(conn, &id, profile_id).unwrap();
    store::add_msg(
        conn,
        &NewMsg {
            session_id: id.clone(),
            role: "user".into(),
            content: body.into(),
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

    id
}

fn as_args(json: &str) -> serde_json::Value {
    serde_json::from_str(json).unwrap()
}

/// Run a body as if it were called from inside `session_id`'s chat. The tool
/// learns who is asking from the chat, never from the model.
async fn as_caller<T>(session_id: &str, f: impl FnOnce() -> T) -> T {
    tools::notepad::SESSION_ID
        .scope(Some(session_id.to_string()), async { f() })
        .await
}

#[tokio::test]
async fn a_profile_with_no_grants_sees_itself_and_is_told_the_rest_are_withheld() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let ops = profiles::store::create(&conn, "Ops").unwrap();

    let mine = chat_of(&conn, "Fix login", &dev.id, "on it");
    let _theirs = chat_of(&conn, "Rotate keys", &ops.id, "later");

    let out = as_caller(&mine, || tools::profile::list(&conn, &as_args("{}")))
        .await
        .unwrap();

    assert!(out.contains("Fix login"), "{out}");
    assert!(out.contains("Dev (you)"), "{out}");
    // Not absent — named as withheld, so silence is never mistaken for idleness.
    assert!(out.contains("Not shown, no grant"), "{out}");
    assert!(out.contains("Ops"), "{out}");
    assert!(!out.contains("Rotate keys"), "{out}");
}

#[tokio::test]
async fn seeing_activity_does_not_also_buy_the_transcript() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let ops = profiles::store::create(&conn, "Ops").unwrap();

    profiles::store::set_reach(&conn, &dev.id, false, vec![grant("see_activity", &ops.id)])
        .unwrap();

    let mine = chat_of(&conn, "Mine", &dev.id, "mine");
    let theirs = chat_of(&conn, "Theirs", &ops.id, "theirs");

    let listed = as_caller(&mine, || tools::profile::list(&conn, &as_args("{}")))
        .await
        .unwrap();

    assert!(listed.contains("Theirs"), "{listed}");

    // The ladder's whole point: one rung down and it is refused.
    let read = as_caller(&mine, || {
        tools::profile::read(&conn, &as_args(&format!(r#"{{"session_id":"{theirs}"}}"#)))
    })
    .await;

    let err = read.unwrap_err();
    assert!(err.contains("no grant"), "{err}");
    assert!(!err.contains("theirs"), "the transcript leaked: {err}");
}

#[tokio::test]
async fn a_refused_read_never_comes_back_empty() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let ops = profiles::store::create(&conn, "Ops").unwrap();

    let mine = chat_of(&conn, "Mine", &dev.id, "mine");
    let theirs = chat_of(&conn, "Theirs", &ops.id, "quiet content");

    let out = as_caller(&mine, || {
        tools::profile::read(&conn, &as_args(&format!(r#"{{"session_id":"{theirs}"}}"#)))
    })
    .await;

    match out {
        Ok(text) => panic!("expected a refusal, got: {text}"),
        Err(err) => {
            assert!(err.contains("no grant"), "{err}");
            assert!(!err.contains("quiet content"), "{err}");
        }
    }
}

#[tokio::test]
async fn read_chats_buys_the_transcript_and_nothing_more() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let ops = profiles::store::create(&conn, "Ops").unwrap();
    let cfo = profiles::store::create(&conn, "Cfo").unwrap();

    profiles::store::set_reach(&conn, &dev.id, false, vec![grant("read_chats", &ops.id)]).unwrap();

    let mine = chat_of(&conn, "Mine", &dev.id, "mine");
    let theirs = chat_of(&conn, "Theirs", &ops.id, "quarterly numbers");
    let other = chat_of(&conn, "Theirs too", &cfo.id, "salaries");

    let read = as_caller(&mine, || {
        tools::profile::read(&conn, &as_args(&format!(r#"{{"session_id":"{theirs}"}}"#)))
    })
    .await
    .unwrap();

    assert!(read.contains("quarterly numbers"), "{read}");

    // Sparse: one target, not the whole column.
    let nope = as_caller(&mine, || {
        tools::profile::read(&conn, &as_args(&format!(r#"{{"session_id":"{other}"}}"#)))
    })
    .await;

    assert!(nope.unwrap_err().contains("no grant"));
}

#[tokio::test]
async fn a_grant_is_one_way() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let boss = profiles::store::create(&conn, "Boss").unwrap();
    let help = profiles::store::create(&conn, "Help").unwrap();

    profiles::store::set_reach(&conn, &boss.id, false, vec![grant("read_chats", &help.id)])
        .unwrap();

    let boss_chat = chat_of(&conn, "Directing", &boss.id, "go");
    let help_chat = chat_of(&conn, "Helping", &help.id, "on it");

    let up = as_caller(&help_chat, || tools::profile::list(&conn, &as_args("{}")))
        .await
        .unwrap();

    assert!(up.contains("no grant"), "{up}");
    assert!(!up.contains("Directing"), "{up}");

    // The boss's grant is real, so the two directions are not symmetric by
    // accident of setup. Note it buys the transcript and not the listing:
    // read_chats is the rung below see_activity and does not imply it.
    let down = as_caller(&boss_chat, || {
        tools::profile::read(
            &conn,
            &as_args(&format!(r#"{{"session_id":"{help_chat}"}}"#)),
        )
    })
    .await
    .unwrap();

    assert!(down.contains("on it"), "{down}");

    let not_listed = as_caller(&boss_chat, || {
        tools::profile::list(
            &conn,
            &as_args(&format!(r#"{{"profile_id":"{}"}}"#, help.id)),
        )
    })
    .await;

    assert!(not_listed.unwrap_err().contains("no grant"));
}

#[tokio::test]
async fn reaching_every_profile_needs_no_per_target_grant() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let boss = profiles::store::create(&conn, "Boss").unwrap();
    let a = profiles::store::create(&conn, "A").unwrap();
    let b = profiles::store::create(&conn, "B").unwrap();

    profiles::store::set_reach(&conn, &boss.id, true, vec![]).unwrap();

    let mine = chat_of(&conn, "Mine", &boss.id, "mine");
    let one = chat_of(&conn, "One", &a.id, "one");
    let two = chat_of(&conn, "Two", &b.id, "two");

    let listed = as_caller(&mine, || tools::profile::list(&conn, &as_args("{}")))
        .await
        .unwrap();

    assert!(listed.contains("One") && listed.contains("Two"), "{listed}");
    assert!(!listed.contains("Not shown"), "{listed}");

    let read = as_caller(&mine, || {
        tools::profile::read(&conn, &as_args(&format!(r#"{{"session_id":"{one}"}}"#)))
    })
    .await
    .unwrap();

    assert!(read.contains("one"), "{read}");

    let _ = two;
}

#[tokio::test]
async fn your_own_chat_needs_no_grant() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let mine = chat_of(&conn, "Mine", &dev.id, "my own business");

    let read = as_caller(&mine, || {
        tools::profile::read(&conn, &as_args(&format!(r#"{{"session_id":"{mine}"}}"#)))
    })
    .await
    .unwrap();

    assert!(read.contains("my own business"), "{read}");
    assert!(read.contains("your own chat"), "{read}");
}

#[tokio::test]
async fn the_caller_is_the_chat_that_asked_not_the_name_it_gave() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let ops = profiles::store::create(&conn, "Ops").unwrap();

    let dev_chat = chat_of(&conn, "Dev work", &dev.id, "dev");
    let ops_secret = chat_of(&conn, "Ops work", &ops.id, "ops only");

    // Asks for the whole world by naming somebody else as the target.
    let out = as_caller(&dev_chat, || {
        tools::profile::read(
            &conn,
            &as_args(&format!(r#"{{"session_id":"{ops_secret}"}}"#)),
        )
    })
    .await;

    assert!(out.unwrap_err().contains("no grant"));
}

#[tokio::test]
async fn a_chat_with_no_profile_is_the_default_one() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    // The backfill in migration normally prevents this, but the tools must not
    // depend on it holding forever.
    let id = new_session(&conn, "Unowned");
    conn.execute(
        "UPDATE sessions SET profile_id = NULL WHERE id = ?1",
        rusqlite::params![id],
    )
    .unwrap();

    let read = as_caller(&id, || {
        tools::profile::read(&conn, &as_args(&format!(r#"{{"session_id":"{id}"}}"#)))
    })
    .await
    .unwrap();

    assert!(read.contains("your own chat"), "{read}");
}

#[tokio::test]
async fn a_sub_agent_is_not_a_chat_of_its_own() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let parent = chat_of(&conn, "Parent", &dev.id, "fan out");

    store::create_child(&conn, &parent, "worker", "Worker", None, "do the piece").unwrap();

    let listed = as_caller(&parent, || tools::profile::list(&conn, &as_args("{}")))
        .await
        .unwrap();

    assert!(listed.contains("Parent"), "{listed}");
    assert!(
        !listed.contains("Worker"),
        "a sub-agent is a session, not a chat: {listed}"
    );
}

#[tokio::test]
async fn an_empty_chat_says_it_is_empty_rather_than_looking_untouched() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let dev = profiles::store::create(&conn, "Dev").unwrap();
    let fresh = new_session(&conn, "Fresh");
    profiles::store::set_for_session(&conn, &fresh, &dev.id).unwrap();

    let listed = as_caller(&fresh, || tools::profile::list(&conn, &as_args("{}")))
        .await
        .unwrap();

    assert!(listed.contains("Fresh | 0 messages"), "{listed}");
}

#[tokio::test]
async fn outside_a_conversation_the_tools_refuse_rather_than_guessing() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    let out = tools::profile::list(&conn, &as_args("{}"));

    assert!(out.unwrap_err().contains("inside a conversation"));
}
