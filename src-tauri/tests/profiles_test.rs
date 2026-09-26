use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use argus_lib::gateway::Gateway;
use argus_lib::profiles;
use argus_lib::profiles::schema::Grant;
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
