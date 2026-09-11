use argus_lib::sessions::browser_import::SKIP_DIRS;
use argus_lib::sessions::ext_install::unpacked_id;
use argus_lib::sessions::schema::Session;

use std::path::Path;

#[test]
fn session_deserializes_from_ui_row() {
    let js = serde_json::json!({
        "id": "628a",
        "title": "t",
        "status": "live",
        "model_id": null,
        "permission": "never",
        "folder_id": null,
        "created_at": "2026-09-09 06:21:23",
        "updated_at": "2026-09-09 06:21:23",
        "ctx_tokens": 0,
        "compact_seq": 0,
        "compactions": 0
    });

    let s: Result<Session, _> = serde_json::from_value(js);
    assert!(s.is_ok(), "{s:?}");
}

#[test]
fn skips_cache_dirs_wherever_they_live() {
    assert!(SKIP_DIRS.contains(&"Cache"));
    assert!(SKIP_DIRS.contains(&"Service Worker"));
    assert!(SKIP_DIRS.contains(&"component_crx_cache"));
}

#[test]
fn id_is_32_ap_letters() {
    let dir = Path::new("/tmp/argus-extension");
    let id = unpacked_id(dir);

    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| ('a'..='p').contains(&c)));
}
