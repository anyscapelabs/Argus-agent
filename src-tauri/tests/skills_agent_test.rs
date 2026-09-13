use argus_lib::sessions::chat::SKILL_NUDGE;
use argus_lib::skills::schema::NewSkill;
use argus_lib::skills::store::{
    create_skill, delete_skill, get_skill, migrate, search_skills, touch_skill,
};
use argus_lib::tools::section;
use rusqlite::Connection;

const BODY: &str = "## When to use\nWhenever x happens in the course of normal work and you recognize the shape of the problem from prior experience.\n\n## Steps\nDo x first, then y, then z, carefully and completely, verifying each stage before moving on to the next one.\n\n## Pitfalls\nWatch out for w, which bites when you least expect it, so double-check everything twice before calling the job done.\n";

fn mem(tag: &str) -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let dir = std::env::temp_dir().join(format!("argus-skill-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    (conn, dir)
}

fn new_skill(name: &str) -> NewSkill {
    NewSkill {
        name: name.into(),
        description: "does x well".into(),
        body: BODY.into(),
        source: Some("agent".into()),
        origin: None,
    }
}

#[test]
fn agent_skill_roundtrip() {
    let (conn, dir) = mem("roundtrip");

    let sk = create_skill(&conn, &dir, &new_skill("test-x")).unwrap();
    assert_eq!(sk.name, "test-x");

    let got = get_skill(&conn, &dir, "test-x").unwrap();
    assert!(got.body.contains("Pitfalls"));

    let found = search_skills(&conn, "test-x", 5).unwrap();
    assert!(found.iter().any(|s| s.name == "test-x"));

    touch_skill(&conn, "test-x").unwrap();
    delete_skill(&conn, &dir, "test-x").unwrap();
    assert!(get_skill(&conn, &dir, "test-x").is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn agent_skill_validation_rejects_junk() {
    let (conn, dir) = mem("validation");
    let mut bad = new_skill("Bad_Name");

    assert!(create_skill(&conn, &dir, &bad).is_err());

    bad = new_skill("short-body");
    bad.body = "too short".into();
    assert!(create_skill(&conn, &dir, &bad).is_err());

    bad = new_skill("no-sections");
    bad.body = "x".repeat(300);
    assert!(create_skill(&conn, &dir, &bad).is_err());

    bad = new_skill("dup-skill");
    bad.body = BODY.into();
    create_skill(&conn, &dir, &bad).unwrap();
    assert!(create_skill(&conn, &dir, &bad).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn skill_tools_are_listed_for_the_agent() {
    let s = section(false);

    assert!(s.contains("skill.create"), "{s}");
    assert!(s.contains("skill.search"), "{s}");
    assert!(s.contains("skill.read"), "{s}");
}

#[test]
fn hard_task_nudge_points_at_skill_create() {
    assert!(SKILL_NUDGE.contains("skill.create"), "{SKILL_NUDGE}");
    assert!(SKILL_NUDGE.contains("skill.search"), "{SKILL_NUDGE}");
}
