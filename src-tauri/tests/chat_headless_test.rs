use argus_lib::sessions::chat::NullSink;

fn setup() -> (
    argus_lib::gateway::Gateway,
    tauri::AppHandle<tauri::test::MockRuntime>,
    String,
) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: std::sync::Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: std::env::temp_dir().join("argus-headless-skills"),
        library_dir: std::env::temp_dir().join("argus-headless-library"),
        logos_dir: std::env::temp_dir().join("argus-headless-logos"),
        approvals: std::sync::Mutex::new(std::collections::HashMap::new()),
        tasks: std::sync::Mutex::new(std::collections::HashMap::new()),
    };

    let sid = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "t".into(),
                model_id: None,
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
            },
        )
        .unwrap()
        .id
    };

    let app = tauri::test::mock_app();
    (gw, app.handle().clone(), sid)
}

#[tokio::test]
async fn headless_send_tags_system_role_without_frontend() {
    let (gw, handle, sid) = setup();
    let sink = NullSink;

    let out =
        argus_lib::sessions::chat::send(&gw, &handle, &sid, "scheduled tick", &sink, "system")
            .await;

    assert!(out.is_err(), "no model is configured, the turn must fail");

    let conn = gw.conn.lock().unwrap();
    let (role, content): (String, String) = conn
        .query_row(
            "SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY seq DESC LIMIT 1",
            [sid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(role, "system");
    assert_eq!(content, "scheduled tick");
}
