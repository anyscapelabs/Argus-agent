use argus_lib::skills::schema::{NewSkill, MIGRATE};
use argus_lib::skills::store;
use rusqlite::Connection;
use std::path::PathBuf;

fn setup(tag: &str) -> (Connection, PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(MIGRATE).unwrap();

    let dir = std::env::temp_dir().join(format!("argus-skills-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    (conn, dir)
}

fn sample_body() -> String {
    "## When to use\n\nWhenever the task matches this skill's purpose and no other skill covers it better. Trigger on the phrases the description names.\n\n## Steps\n\n1. Do the thing carefully.\n2. Verify the result.\n3. Report what changed.\n\n## Pitfalls\n\nDo not overdo it. Do not skip verification. Do not run the same step twice without checking the outcome first.".into()
}

#[test]
fn create_skill_writes_bundle_layout() {
    let (conn, dir) = setup("create");

    let s = store::create_skill(
        &conn,
        &dir,
        &NewSkill {
            name: "my-skill".into(),
            description: "Use when testing bundles".into(),
            body: sample_body(),
            source: Some("user".into()),
            origin: None,
        },
    )
    .unwrap();

    assert_eq!(s.name, "my-skill");
    assert!(dir.join("my-skill").join("SKILL.md").is_file());
    assert!(!dir.join("my-skill.md").exists());

    let files = store::list_files(&dir, "my-skill").unwrap();
    assert_eq!(files, vec!["SKILL.md".to_string()]);

    let body = store::read_file(&dir, "my-skill", "SKILL.md").unwrap();
    assert!(body.starts_with("---\nname: my-skill"));
}

#[test]
fn read_file_rejects_traversal_and_outside_paths() {
    let (conn, dir) = setup("traversal");

    store::create_skill(
        &conn,
        &dir,
        &NewSkill {
            name: "guard-skill".into(),
            description: "Use when guarding paths".into(),
            body: sample_body(),
            source: None,
            origin: None,
        },
    )
    .unwrap();

    assert!(store::read_file(&dir, "guard-skill", "../other/SKILL.md").is_err());
    assert!(store::read_file(&dir, "guard-skill", "/etc/passwd").is_err());
    assert!(store::read_file(&dir, "guard-skill", "scripts/../../secret.md").is_err());
    assert!(store::read_file(&dir, "guard-skill", "random/notes.md").is_err());
    assert!(store::read_file(&dir, "guard-skill", "missing.md").is_err());
}

#[test]
fn sync_migrates_legacy_flat_files() {
    let (conn, dir) = setup("migrate");

    std::fs::write(
        dir.join("old-skill.md"),
        "---\nname: old-skill\ndescription: Use when migrating\n---\n\n## When to use\n\nLegacy.\n\n## Steps\n\n1. Run.\n\n## Pitfalls\n\nNone.",
    )
    .unwrap();

    let n = store::sync(&conn, &dir).unwrap();
    assert_eq!(n, 1);
    assert!(!dir.join("old-skill.md").exists());
    assert!(dir.join("old-skill").join("SKILL.md").is_file());

    let sk = store::get_skill(&conn, &dir, "old-skill").unwrap();
    assert_eq!(sk.description, "Use when migrating");
    assert!(sk.body.contains("## When to use"));
}

#[test]
fn sync_parses_folded_description() {
    let (conn, dir) = setup("folded");

    let bundle = dir.join("folded-skill");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(
        bundle.join("SKILL.md"),
        "---\nname: folded-skill\ndescription: >\n  Use this whenever the\n  task needs folding.\n---\n\n## When to use\n\nFolded.\n\n## Steps\n\n1. Go.\n\n## Pitfalls\n\nNone.",
    )
    .unwrap();

    store::sync(&conn, &dir).unwrap();

    let sk = store::get_skill(&conn, &dir, "folded-skill").unwrap();
    assert_eq!(sk.description, "Use this whenever the task needs folding.");
}

#[test]
fn reference_files_list_and_read() {
    let (conn, dir) = setup("reference");

    store::create_skill(
        &conn,
        &dir,
        &NewSkill {
            name: "ref-skill".into(),
            description: "Use when referencing".into(),
            body: sample_body(),
            source: None,
            origin: None,
        },
    )
    .unwrap();

    let ref_dir = dir.join("ref-skill").join("reference");
    std::fs::create_dir_all(&ref_dir).unwrap();
    std::fs::write(ref_dir.join("INDEX.md"), "- api.md — details").unwrap();
    std::fs::write(ref_dir.join("api.md"), "# API\n\nDeep detail.").unwrap();

    let files = store::list_files(&dir, "ref-skill").unwrap();
    assert_eq!(
        files,
        vec![
            "SKILL.md".to_string(),
            "reference/INDEX.md".to_string(),
            "reference/api.md".to_string(),
        ]
    );

    let content = store::read_file(&dir, "ref-skill", "reference/api.md").unwrap();
    assert!(content.contains("Deep detail."));
}
