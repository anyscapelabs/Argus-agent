use argus_lib::tools::notepad;
use std::sync::{Mutex as StdMutex, OnceLock};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

fn isolated_home(tag: &str) -> (PathBufGuard, Option<String>) {
    let prev = std::env::var("XDG_DATA_HOME").ok();
    let tmp: std::path::PathBuf = std::env::temp_dir().join(format!(
        "argus-notepad-{tag}-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    std::env::set_var("XDG_DATA_HOME", &tmp);
    (PathBufGuard(tmp), prev)
}

struct PathBufGuard(std::path::PathBuf);

impl Drop for PathBufGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn restore_home(prev: Option<String>) {
    match prev {
        Some(v) => std::env::set_var("XDG_DATA_HOME", v),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
}

fn args(text: Option<&str>, scope: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("scope".into(), serde_json::Value::String(scope.into()));
    if let Some(t) = text {
        m.insert("text".into(), serde_json::Value::String(t.into()));
    }
    serde_json::Value::Object(m)
}

async fn scoped<T>(id: &str, fut: impl std::future::Future<Output = T>) -> T {
    notepad::SESSION_ID.scope(Some(id.to_string()), fut).await
}

#[tokio::test]
async fn session_round_trip() {
    let _guard = serial();
    let (_tmp, prev) = isolated_home("roundtrip");

    scoped("sess-a", async {
        assert_eq!(
            notepad::read(&args(None, "session")).unwrap(),
            "(notepad empty)"
        );
        assert_eq!(
            notepad::append(&args(Some("first lead"), "session")).unwrap(),
            "appended"
        );
        assert_eq!(
            notepad::append(&args(Some("second lead"), "session")).unwrap(),
            "appended"
        );
        let body = notepad::read(&args(None, "session")).unwrap();
        assert!(body.contains("first lead") && body.contains("second lead"));
        assert_eq!(
            notepad::replace(&args(Some("condensed"), "session")).unwrap(),
            "replaced"
        );
        assert_eq!(notepad::read(&args(None, "session")).unwrap(), "condensed");
        assert_eq!(
            notepad::clear(&args(None, "session")).unwrap(),
            "notepad cleared"
        );
        assert_eq!(
            notepad::read(&args(None, "session")).unwrap(),
            "(notepad empty)"
        );
    })
    .await;

    restore_home(prev);
}

#[tokio::test]
async fn sessions_are_isolated_and_global_is_shared() {
    let _guard = serial();
    let (_tmp, prev) = isolated_home("isolation");

    scoped("sess-a", async {
        notepad::append(&args(Some("aaa"), "session")).unwrap();
    })
    .await;
    scoped("sess-b", async {
        assert_eq!(
            notepad::read(&args(None, "session")).unwrap(),
            "(notepad empty)"
        );
        notepad::append(&args(Some("shared"), "global")).unwrap();
    })
    .await;
    scoped("sess-a", async {
        assert!(notepad::read(&args(None, "session"))
            .unwrap()
            .contains("aaa"));
        assert!(notepad::read(&args(None, "global"))
            .unwrap()
            .contains("shared"));
    })
    .await;

    restore_home(prev);
}

#[tokio::test]
async fn session_scope_requires_context_and_valid_scope() {
    let _guard = serial();
    let (_tmp, prev) = isolated_home("ctx");

    assert!(notepad::read(&args(None, "session")).is_err());
    assert!(notepad::append(&args(Some("x"), "session")).is_err());
    assert!(notepad::read(&args(None, "bogus")).is_err());
    scoped("sess-a", async {
        assert!(notepad::read(&args(None, "bogus")).is_err());
    })
    .await;
    assert!(notepad::read(&args(None, "global")).is_ok());

    restore_home(prev);
}

#[tokio::test]
async fn hostile_session_id_stays_jailed() {
    let _guard = serial();
    let (_tmp, prev) = isolated_home("jail");
    let root = std::env::temp_dir().join(format!(
        "argus-notepad-jail-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    let _ = std::fs::remove_dir_all(&root);

    scoped("../../evil", async {
        notepad::append(&args(Some("x"), "session")).unwrap();
    })
    .await;

    let escaped = root.join("evil.md");
    assert!(!escaped.exists(), "path escaped the notepad dir");
    let _ = std::fs::remove_dir_all(&root);

    restore_home(prev);
}

#[tokio::test]
async fn cap_is_enforced() {
    let _guard = serial();
    let (_tmp, prev) = isolated_home("cap");

    scoped("sess-a", async {
        let big = "y".repeat(9000);
        assert!(notepad::append(&args(Some(&big), "session")).is_err());
        assert!(notepad::replace(&args(Some(&big), "session")).is_err());
        let edge = "z".repeat(8191);
        notepad::append(&args(Some(&edge), "session")).unwrap();
        assert!(notepad::append(&args(Some("one more"), "session")).is_err());
        assert_eq!(
            notepad::replace(&args(Some("small"), "session")).unwrap(),
            "replaced"
        );
    })
    .await;

    restore_home(prev);
}

#[test]
fn prompt_include_caps_and_empties() {
    let _guard = serial();
    let (_tmp, prev) = isolated_home("include");

    assert!(notepad::prompt_include("nosuch").is_none());

    let dir = std::env::var("XDG_DATA_HOME").unwrap();
    let pad = std::path::Path::new(&dir).join("com.anyscapelabs.argus/notepad");
    std::fs::create_dir_all(&pad).unwrap();
    std::fs::write(pad.join("s1.md"), "short note").unwrap();
    let shown = notepad::prompt_include("s1").unwrap();
    assert!(shown.contains("short note") && !shown.contains("truncated"));

    let long = "L".repeat(3000);
    std::fs::write(pad.join("s2.md"), &long).unwrap();
    let shown = notepad::prompt_include("s2").unwrap();
    assert!(shown.contains("truncated") && shown.contains("notepad.read"));
    assert!(shown.len() < long.len());

    std::fs::write(pad.join("s3.md"), "   \n ").unwrap();
    assert!(notepad::prompt_include("s3").is_none());

    restore_home(prev);
}

#[test]
fn meta_lists_four_non_mutating_tools() {
    assert_eq!(argus_lib::tools::notepad::META.len(), 4);
    for t in argus_lib::tools::notepad::META {
        assert!(t.name.starts_with("notepad."));
        assert!(!t.mutating, "{} must not require approval", t.name);
    }

    let section = argus_lib::tools::section(false);
    assert!(section.contains("notepad.read"));
    assert!(section.contains("untrusted data"));
}

#[test]
fn prompt_include_is_delimited_and_bounded() {
    let _guard = serial();
    let (_tmp, prev) = isolated_home("delimited");

    let dir = std::env::var("XDG_DATA_HOME").unwrap();
    let pad = std::path::Path::new(&dir).join("com.anyscapelabs.argus/notepad");
    std::fs::create_dir_all(&pad).unwrap();
    std::fs::write(pad.join("s9.md"), "remember the milk").unwrap();

    let shown = notepad::prompt_include("s9").unwrap();
    assert!(shown.starts_with("<working-notes>"));
    assert!(shown.ends_with("</working-notes>"));
    assert!(shown.contains("remember the milk"));

    let big = "B".repeat(5000);
    std::fs::write(pad.join("s8.md"), &big).unwrap();
    let shown = notepad::prompt_include("s8").unwrap();
    assert!(shown.starts_with("<working-notes>"));
    assert!(shown.len() < 1400, "notepad must stay bounded");

    restore_home(prev);
}
