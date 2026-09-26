use argus_lib::memory::schema::NewMemory;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    conn
}

fn remember(conn: &rusqlite::Connection, content: &str) -> String {
    argus_lib::memory::store::save(
        conn,
        &NewMemory {
            content: content.into(),
            kind: Some("fact".into()),
            importance: Some(3),
            session_id: None,
        },
    )
    .unwrap()
    .id
}

#[test]
fn recall_expands_two_hops_in_distance_order() {
    let conn = test_db();
    let a = remember(&conn, "alpha deploy pipeline runs on Fridays");
    let b = remember(&conn, "beta rollback steps for releases");
    let c = remember(&conn, "gamma garden tomatoes need sun");
    argus_lib::memory::store::link(&conn, &a, &b, "related").unwrap();
    argus_lib::memory::store::link(&conn, &b, &c, "related").unwrap();

    let hits = argus_lib::memory::store::recall(&conn, "alpha deploy pipeline", 8).unwrap();
    let pos = |id: &str| hits.iter().position(|h| h.ref_id == id).unwrap();
    assert_eq!(hits[pos(&a)].source, "memory");
    assert!(pos(&a) < pos(&b) && pos(&b) < pos(&c), "{hits:?}");
}

#[test]
fn recall_terminates_on_cycles_without_dupes() {
    let conn = test_db();
    let a = remember(&conn, "alpha deploy pipeline runs on Fridays");
    let b = remember(&conn, "beta rollback steps for releases");
    argus_lib::memory::store::link(&conn, &a, &b, "related").unwrap();
    argus_lib::memory::store::link(&conn, &b, &a, "related").unwrap();

    let hits = argus_lib::memory::store::recall(&conn, "alpha deploy pipeline", 8).unwrap();
    let mut seen = HashSet::new();
    for h in &hits {
        assert!(seen.insert(h.ref_id.clone()), "duplicate hit: {h:?}");
    }
    assert!(hits.iter().any(|h| h.ref_id == a));
    assert!(hits.iter().any(|h| h.ref_id == b));
}

#[test]
fn recall_expansion_respects_cap() {
    let conn = test_db();
    let hub = remember(&conn, "hub deploy topic");
    for i in 0..10 {
        let leaf = remember(&conn, &format!("leaf node number {i} unrelated filler"));
        argus_lib::memory::store::link(&conn, &hub, &leaf, "related").unwrap();
    }

    let hits = argus_lib::memory::store::recall(&conn, "hub deploy topic", 3).unwrap();
    assert!(hits.len() <= 3, "{hits:?}");
}

#[test]
fn rollup_prompt_is_deterministic_and_complete() {
    let conn = test_db();
    let a = remember(&conn, "Brnx prefers dark mode");
    let b = remember(&conn, "Brnx deploys on Fridays");
    let mems = vec![
        argus_lib::memory::store::get(&conn, &a).unwrap(),
        argus_lib::memory::store::get(&conn, &b).unwrap(),
    ];
    let p1 = argus_lib::memory::rollup_prompt(&mems);
    let p2 = argus_lib::memory::rollup_prompt(&mems);
    assert_eq!(p1, p2);
    assert!(p1.contains("dark mode") && p1.contains("Fridays"));
}

#[test]
fn apply_rollup_writes_condensed_memory_and_links() {
    let conn = test_db();
    let a = remember(&conn, "Brnx prefers dark mode");
    let b = remember(&conn, "Brnx deploys on Fridays");

    let condensed = argus_lib::memory::apply_rollup(
        &conn,
        &[a.clone(), b.clone(), "missing".into()],
        "Brnx summary",
    )
    .unwrap();
    assert_eq!(condensed.content, "Brnx summary");
    assert_eq!(condensed.kind, "fact");
    assert!(argus_lib::memory::apply_rollup(&conn, &[a.clone()], "").is_err());
    assert!(argus_lib::memory::apply_rollup(&conn, &[], "x".into()).is_err());
    assert!(argus_lib::memory::apply_rollup(&conn, &["missing".into()], "x".into()).is_err());

    let links: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT from_id, to_id, relation FROM memory_links")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .flatten()
            .collect()
    };
    assert!(links.contains(&(a.clone(), condensed.id.clone(), "condensed_into".into())));
    assert!(links.contains(&(b.clone(), condensed.id.clone(), "condensed_into".into())));
    assert!(argus_lib::memory::store::get(&conn, &a).is_ok());
}

async fn read_http_body(sock: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut buf = vec![0u8; 65536];
    let mut data = vec![];
    loop {
        let n = sock.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..pos]).into_owned();
            let len: usize = head
                .lines()
                .skip(1)
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.trim().to_lowercase() == "content-length")
                .and_then(|(_, v)| v.trim().parse().ok())
                .unwrap_or(0);
            let mut body = data[pos + 4..].to_vec();
            while body.len() < len {
                let n = sock.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
            }
            body.truncate(len);
            return body;
        }
        if data.len() > 1_000_000 {
            break;
        }
    }
    vec![]
}

#[tokio::test]
async fn rollup_with_model_end_to_end() {
    use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};

    let conn = test_db();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Arc<StdMutex<Vec<serde_json::Value>>> = Arc::new(StdMutex::new(vec![]));
    tokio::spawn({
        let seen = seen.clone();
        async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let seen = seen.clone();
                tokio::spawn(async move {
                    let body = read_http_body(&mut sock).await;
                    if let Ok(req) = serde_json::from_slice::<serde_json::Value>(&body) {
                        seen.lock().unwrap().push(req);
                    }
                    let resp = "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Brnx condensed fact\"}}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}";
                    let out = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp}",
                        resp.len()
                    );
                    let _ = sock.write_all(out.as_bytes()).await;
                });
            }
        }
    });

    argus_lib::gateway::store::upsert_provider(
        &conn,
        &Provider {
            id: "mock".into(),
            name: "Mock".into(),
            compatible: "openAI".into(),
            base_url: base,
            api_key_ref: None,
            connected: true,
            free: true,
            priority: 0,
            logo_url: None,
            doc_url: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_connected(&conn, "mock", true).unwrap();
    argus_lib::gateway::store::add_model(
        &conn,
        &ModelEntry {
            id: "mock/m".into(),
            display_name: "Mock".into(),
            family: None,
            capabilities: None,
            suggested_tier: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::link_model(
        &conn,
        &Avail {
            model_id: "mock/m".into(),
            provider_id: "mock".into(),
            remote_model_id: "test".into(),
            cost_in: 0.0,
            cost_out: 0.0,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_model_enabled(&conn, "mock/m", true).unwrap();

    let tmp =
        std::env::temp_dir().join(format!("argus-rollup-{}", uuid::Uuid::new_v4().as_simple()));
    for d in ["skills", "library", "logos"] {
        std::fs::create_dir_all(tmp.join(d)).unwrap();
    }
    let gw = argus_lib::gateway::Gateway {
        conn: StdMutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: tmp.join("skills"),
        library_dir: tmp.join("library"),
        logos_dir: tmp.join("logos"),
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
        jobs: StdMutex::new(HashMap::new()),
        events: StdMutex::new(HashMap::new()),
        turns: StdMutex::new(HashSet::new()),
        watching: StdMutex::new(HashSet::new()),
    };

    let a = {
        let conn = gw.conn.lock().unwrap();
        remember(&conn, "Brnx prefers dark mode")
    };
    let b = {
        let conn = gw.conn.lock().unwrap();
        remember(&conn, "Brnx deploys on Fridays")
    };

    let condensed = argus_lib::memory::rollup_with_model(&gw, vec![a.clone(), b.clone()])
        .await
        .expect("rollup must succeed against the mock");
    assert_eq!(condensed.content, "Brnx condensed fact");

    let reqs = seen.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].get("tools").is_none());
    drop(reqs);

    {
        let conn = gw.conn.lock().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_links WHERE to_id = ?1 AND relation = 'condensed_into'",
                [condensed.id.clone()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    let _ = std::fs::remove_dir_all(&tmp);
}
