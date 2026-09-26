use argus_lib::gateway::{approval_reply, ApprovalReply, Gateway};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;

fn test_gw() -> Gateway {
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: std::env::temp_dir().join("argus-appr-skills"),
        library_dir: std::env::temp_dir().join("argus-appr-library"),
        logos_dir: std::env::temp_dir().join("argus-appr-logos"),
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
        jobs: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        turns: Mutex::new(HashSet::new()),
        watching: Mutex::new(HashSet::new()),
    }
}

#[test]
fn reply_validates_edited_args() {
    let err = approval_reply(true, Some("not json".into())).unwrap_err();
    assert_eq!(err, "edited args are not valid JSON");

    let err = approval_reply(true, Some("[1,2]".into())).unwrap_err();
    assert_eq!(err, "edited args must be a JSON object");

    let r = approval_reply(false, None).unwrap();
    assert!(!r.allow);
    assert!(r.args.is_none());

    let r = approval_reply(true, Some(r#"{"to":"a@b.com","body":"hi"}"#.into())).unwrap();
    assert!(r.allow);
    assert_eq!(r.args.as_deref(), Some(r#"{"to":"a@b.com","body":"hi"}"#));
}

#[tokio::test]
async fn approval_channel_round_trips_edited_args() {
    let gw = test_gw();
    let (tx, rx) = tokio::sync::oneshot::channel::<ApprovalReply>();

    gw.approvals.lock().unwrap().insert("appr-1".into(), tx);

    let reply = approval_reply(
        true,
        Some(r#"{"to":"a@b.com","subject":"s","body":"edited body"}"#.into()),
    )
    .unwrap();

    gw.approvals
        .lock()
        .unwrap()
        .remove("appr-1")
        .unwrap()
        .send(reply)
        .unwrap();

    let got = rx.await.unwrap();
    assert!(got.allow);
    assert!(got.args.unwrap().contains("edited body"));
}
