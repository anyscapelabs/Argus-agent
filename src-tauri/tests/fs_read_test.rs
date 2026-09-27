// fs.read opens the files that were handed to the conversation and nothing
// else. The whole loop this replaces was: read the file, get the head back,
// read it again, get the same head. So two of these tests exist purely to
// prove the second call lands somewhere new.

use argus_lib::library::schema::MIGRATE as LIB_MIGRATE;

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    conn.execute_batch(LIB_MIGRATE).unwrap();
    conn
}

fn session(conn: &rusqlite::Connection) -> String {
    use argus_lib::sessions::schema::NewSession;
    argus_lib::sessions::store::create_session(
        conn,
        &NewSession {
            title: "files".into(),
            model_id: Some("mock/test".into()),
            permission: Some("never".into()),
            folder_id: None,
            web_search: false,
        },
    )
    .unwrap()
    .id
}

fn say(conn: &rusqlite::Connection, sid: &str, content: &str, attachments: Option<&str>) {
    use argus_lib::sessions::schema::NewMsg;
    argus_lib::sessions::store::add_msg(
        conn,
        &NewMsg {
            session_id: sid.into(),
            role: "user".into(),
            content: content.into(),
            model_id: None,
            provider_id: None,
            tok_in: None,
            tok_out: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: attachments.map(str::to_string),
        },
    )
    .unwrap();
}

fn dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("argus-fsr-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(d: &std::path::Path, name: &str, body: &str) -> String {
    let p = d.join(name);
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().into_owned()
}

fn attach(conn: &rusqlite::Connection, sid: &str, path: &str, name: &str) {
    say(
        conn,
        sid,
        "have a look",
        Some(
            &serde_json::json!([{ "path": path, "name": name, "kind": "doc", "sz": 1 }])
                .to_string(),
        ),
    );
}

fn call(
    conn: &rusqlite::Connection,
    library_dir: &std::path::Path,
    args: serde_json::Value,
) -> Result<String, String> {
    argus_lib::tools::fs::read::read(conn, library_dir, &args)
}

/// The task local is what a running turn sets; the tests set it by hand.
async fn scoped<F, T>(sid: &str, body: F) -> T
where
    F: std::future::Future<Output = T>,
{
    argus_lib::tools::notepad::SESSION_ID
        .scope(Some(sid.to_string()), body)
        .await
}

#[test]
fn an_attached_file_reads_back_whole() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let d = dir();
        let conn = db();
        let sid = session(&conn);
        let body = "one\ntwo\nthree";
        let path = write(&d, "notes.md", body);

        attach(&conn, &sid, &path, "notes.md");

        let out = scoped(&sid, async {
            call(&conn, &d, serde_json::json!({ "path": path }))
        })
        .await
        .unwrap();

        assert!(out.contains(body), "{out}");
    });
}

#[test]
fn the_second_call_lands_somewhere_new() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let d = dir();
        let conn = db();
        let sid = session(&conn);

        // Longer than one reply's 60 000 characters, so the first call splits.
        let line = "abcdefghij";
        let body = line.repeat(20_000);
        let path = write(&d, "long.txt", &body);

        attach(&conn, &sid, &path, "long.txt");

        let (first, second) = scoped(&sid, async {
            let first = call(&conn, &d, serde_json::json!({ "path": path })).unwrap();
            // The reply names the offset to continue from, so take the number
            // it gave rather than one the model worked out.
            let marker = "Continue with fs.read offset=";
            let at = first
                .rfind(marker)
                .expect("the reply must say how to continue");
            let tail = &first[at + marker.len()..];
            let next: usize = tail
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let second = call(
                &conn,
                &d,
                serde_json::json!({ "path": path, "offset": next }),
            )
            .unwrap();
            (first, second)
        })
        .await;

        // The bug this file exists for: two calls, two different pages.
        assert_ne!(first, second, "the second call repeated the first");
        assert!(
            second.contains("Continue with fs.read offset="),
            "still more"
        );
    });
}

#[test]
fn a_file_that_was_never_attached_is_refused() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let d = dir();
        let conn = db();
        let sid = session(&conn);

        let mine = write(&d, "mine.md", "the one I sent");
        let theirs = write(&d, "theirs.md", "the one you may not have");

        attach(&conn, &sid, &mine, "mine.md");

        let err = scoped(&sid, async {
            call(&conn, &d, serde_json::json!({ "path": theirs }))
        })
        .await
        .expect_err("a file nobody sent is not readable");

        assert!(err.contains("permission denied"), "{err}");
        // Final, not retryable: a refusal that a retry could turn into a
        // success is not a refusal.
        assert_eq!(
            argus_lib::tools::recover::classify(&err),
            argus_lib::tools::recover::RecoveryKind::PermissionDenied,
            "{err}"
        );
    });
}

#[test]
fn a_file_from_another_conversation_is_refused() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let d = dir();
        let conn = db();
        let mine = session(&conn);
        let theirs = session(&conn);

        let path = write(&d, "notes.md", "sent in the other chat");

        attach(&conn, &theirs, &path, "notes.md");

        let err = scoped(&mine, async {
            call(&conn, &d, serde_json::json!({ "path": path }))
        })
        .await
        .expect_err("another conversation's file is not this one's");

        assert!(err.contains("permission denied"), "{err}");
    });
}

#[test]
fn outside_a_conversation_there_is_nothing_to_read() {
    let d = dir();
    let conn = db();
    let path = write(&d, "notes.md", "body");
    let err = call(&conn, &d, serde_json::json!({ "path": path }))
        .expect_err("no conversation, no permission");

    assert!(err.contains("permission denied"), "{err}");
}

#[test]
fn a_docx_comes_back_as_text_not_as_zip_bytes() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let d = dir();
        let conn = db();
        let sid = session(&conn);

        let docx = argus_lib::library::doc::build(
            "docx",
            "Report",
            &serde_json::json!({ "content": "Revenue grew eleven percent." }),
        )
        .unwrap();
        let p = d.join("report.docx");
        std::fs::write(&p, &docx.bytes).unwrap();
        let path = p.to_string_lossy().into_owned();

        attach(&conn, &sid, &path, "report.docx");

        let out = scoped(&sid, async {
            call(&conn, &d, serde_json::json!({ "path": path }))
        })
        .await
        .unwrap();

        assert!(out.contains("eleven percent"), "{out}");
        assert!(!out.contains("PK"), "not the raw zip: {out}");
    });
}
