use argus_lib::connectors::store::{enabled, get, migrate, put, set_enabled};
use rusqlite::Connection;

fn mem() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn
}

#[test]
fn put_enable_and_read_back() {
    let c = mem();

    put(
        &c,
        "gmail",
        "npx",
        &["-y".to_string(), "@x/gmail-mcp".to_string()],
        &[("CLIENT_ID".into(), "abc".into())],
    )
    .unwrap();
    set_enabled(&c, "gmail", true).unwrap();

    let list = enabled(&c).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "gmail");
    assert_eq!(list[0].args, vec!["-y", "@x/gmail-mcp"]);
    assert_eq!(list[0].env, vec![("CLIENT_ID".into(), "abc".into())]);

    let (on, status, _) = get(&c, "gmail").unwrap().unwrap();
    assert!(on);
    assert_eq!(status, "off");
}

#[test]
fn disabled_connectors_are_not_listed() {
    let c = mem();

    put(
        &c,
        "cal",
        "npx",
        &["-y".to_string(), "@x/cal".to_string()],
        &[],
    )
    .unwrap();

    assert!(enabled(&c).unwrap().is_empty());

    set_enabled(&c, "cal", true).unwrap();
    set_enabled(&c, "cal", false).unwrap();

    assert!(enabled(&c).unwrap().is_empty());
}

#[test]
fn put_upserts_launch_config() {
    let c = mem();

    put(
        &c,
        "gmail",
        "npx",
        &["-y".to_string(), "old".to_string()],
        &[],
    )
    .unwrap();
    put(
        &c,
        "gmail",
        "npx",
        &["-y".to_string(), "new".to_string()],
        &[],
    )
    .unwrap();
    set_enabled(&c, "gmail", true).unwrap();

    assert_eq!(enabled(&c).unwrap()[0].args, vec!["-y", "new"]);
}
