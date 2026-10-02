use argus_lib::library::doc;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

fn test_gw(tag: &str) -> (argus_lib::gateway::Gateway, std::path::PathBuf) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let base = std::env::temp_dir().join(format!("argus-doc-edit-{tag}-{}", std::process::id()));
    let skills_dir = base.join("skills");
    let library_dir = base.join("library");
    let logos_dir = base.join("logos");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::create_dir_all(&library_dir).unwrap();
    std::fs::create_dir_all(&logos_dir).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir,
        library_dir,
        logos_dir,
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
        jobs: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        turns: Mutex::new(HashSet::new()),
        watching: Mutex::new(HashSet::new()),
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
    };

    (gw, base)
}

fn make_md(gw: &argus_lib::gateway::Gateway, body: &str) -> argus_lib::library::schema::LibItem {
    let args = serde_json::json!({"name": "notes", "kind": "md", "title": "t", "content": body});
    let (item, _) = doc::create(gw, &args, None).unwrap();
    item
}

#[test]
fn patch_applies_search_replace_blocks() {
    let out = doc::patch::apply_edits(
        "hello brave world",
        &[("brave".to_string(), "small".to_string())],
    )
    .unwrap();
    assert_eq!(out, "hello small world");
}

#[test]
fn patch_applies_sequentially() {
    let out = doc::patch::apply_edits(
        "a b c",
        &[
            ("a".to_string(), "x".to_string()),
            ("x b".to_string(), "y".to_string()),
        ],
    )
    .unwrap();
    assert_eq!(out, "y c");
}

#[test]
fn patch_rejects_bad_edits() {
    assert!(doc::patch::apply_edits("hi", &[]).is_err());
    assert!(doc::patch::apply_edits("hi", &[("".into(), "x".into())]).is_err());
    assert!(doc::patch::apply_edits("hi", &[("nope".into(), "x".into())]).is_err());
    assert!(doc::patch::apply_edits("a a", &[("a".into(), "b".into())]).is_err());
    assert!(doc::patch::apply_edits("hi", &[("hi".into(), "  ".into())]).is_err());
}

#[test]
fn update_keeps_id_and_path() {
    let (gw, base) = test_gw("upd");
    let item = make_md(&gw, "first version here");
    let old_path = item.path.clone();

    let conn = gw.conn.lock().unwrap();
    let next = argus_lib::library::store::update_bytes(
        &conn,
        &gw.library_dir,
        &item.id,
        b"second version here",
    )
    .unwrap();
    drop(conn);

    assert_eq!(next.id, item.id);
    assert_eq!(next.path, old_path);
    assert_eq!(next.sz, "second version here".len() as i64);

    let conn = gw.conn.lock().unwrap();
    let pv = argus_lib::library::store::preview(&conn, &gw.library_dir, &item.id, 12000).unwrap();
    drop(conn);
    assert_eq!(pv.text.as_deref(), Some("second version here"));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn update_refuses_empty_and_noops_identical() {
    let (gw, base) = test_gw("upd2");
    let item = make_md(&gw, "same text here");

    let conn = gw.conn.lock().unwrap();
    assert!(
        argus_lib::library::store::update_bytes(&conn, &gw.library_dir, &item.id, b"").is_err()
    );
    let same = argus_lib::library::store::update_bytes(
        &conn,
        &gw.library_dir,
        &item.id,
        b"same text here",
    )
    .unwrap();
    drop(conn);
    assert_eq!(same.id, item.id);

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn edit_rewrites_full_content_same_id() {
    let (gw, base) = test_gw("edit");
    let item = make_md(&gw, "line one\nline two");

    let args = serde_json::json!({"id": item.id, "content": "brand new body text here"});
    let (next, _) = doc::edit(&gw, &args, None).unwrap();
    assert_eq!(next.id, item.id);
    assert_eq!(next.path, item.path);

    let conn = gw.conn.lock().unwrap();
    let pv = argus_lib::library::store::preview(&conn, &gw.library_dir, &item.id, 12000).unwrap();
    drop(conn);
    assert_eq!(pv.text.as_deref(), Some("brand new body text here"));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn patch_fixes_typo_in_place() {
    let (gw, base) = test_gw("patch");
    let item = make_md(&gw, "the quick brown fox jumps");

    let args = serde_json::json!({"id": item.id, "edits": [{"old": "brown", "new": "red"}]});
    let (next, _) = doc::patch(&gw, &args).unwrap();
    assert_eq!(next.id, item.id);
    assert_eq!(next.path, item.path);

    let conn = gw.conn.lock().unwrap();
    let pv = argus_lib::library::store::preview(&conn, &gw.library_dir, &item.id, 12000).unwrap();
    drop(conn);
    assert_eq!(pv.text.as_deref(), Some("the quick red fox jumps"));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn patch_failure_writes_nothing() {
    let (gw, base) = test_gw("patchfail");
    let item = make_md(&gw, "untouched content here");

    let before: Vec<u8> = std::fs::read(gw.library_dir.join(&item.path)).unwrap();
    let args = serde_json::json!({"id": item.id, "edits": [{"old": "missing", "new": "x"}]});
    assert!(doc::patch(&gw, &args).is_err());

    let after: Vec<u8> = std::fs::read(gw.library_dir.join(&item.path)).unwrap();
    assert_eq!(before, after);

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn patch_docx_roundtrip() {
    let (gw, base) = test_gw("docx");
    let args = serde_json::json!({"name": "rep", "kind": "docx", "title": "Report", "content": "revenue was low this quarter"});
    let (item, _) = doc::create(&gw, &args, None).unwrap();

    let pargs = serde_json::json!({"id": item.id, "edits": [{"old": "low", "new": "high"}]});
    let (next, _) = doc::patch(&gw, &pargs).unwrap();
    assert_eq!(next.id, item.id);

    let conn = gw.conn.lock().unwrap();
    let pv = argus_lib::library::store::preview(&conn, &gw.library_dir, &item.id, 12000).unwrap();
    drop(conn);
    let text = pv.text.expect("docx text");
    assert!(text.contains("high"));
    assert!(!text.contains("low"));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn patch_xlsx_cell_and_pptx_slide() {
    let (gw, base) = test_gw("sheet");
    let args =
        serde_json::json!({"name": "data", "kind": "xlsx", "rows": [["name", "n"], ["a", "1"]]});
    let (item, _) = doc::create(&gw, &args, None).unwrap();
    let pargs = serde_json::json!({"id": item.id, "edits": [{"old": "a | 1", "new": "a | 2"}]});
    let (next, _) = doc::patch(&gw, &pargs).unwrap();
    assert_eq!(next.id, item.id);

    let conn = gw.conn.lock().unwrap();
    let pv = argus_lib::library::store::preview(&conn, &gw.library_dir, &item.id, 12000).unwrap();
    drop(conn);
    assert!(pv.text.expect("sheet text").contains("a | 2"));

    let sargs = serde_json::json!({"name": "deck", "kind": "pptx", "title": "Deck", "slides": [{"title": "Intro", "bullets": ["one here"]}]});
    let (deck, _) = doc::create(&gw, &sargs, None).unwrap();
    let dpargs =
        serde_json::json!({"id": deck.id, "edits": [{"old": "one here", "new": "two here"}]});
    let (dnext, pages) = doc::patch(&gw, &dpargs).unwrap();
    assert_eq!(dnext.id, deck.id);
    assert_eq!(pages, 1);

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn edit_and_patch_are_mutating_gated_tools() {
    assert!(argus_lib::tools::is_mutating("doc.edit"));
    assert!(argus_lib::tools::is_mutating("doc.patch"));
    assert!(argus_lib::tools::is_mutating("doc.create"));
    assert!(!argus_lib::tools::is_mutating("library.read"));
}
