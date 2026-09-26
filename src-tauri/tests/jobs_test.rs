use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use argus_lib::gateway::Gateway;
use argus_lib::jobs;
use argus_lib::tools::sandbox;
use tauri::Manager;

fn app() -> tauri::AppHandle<tauri::test::MockRuntime> {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::jobs::schema::migrate(&conn).unwrap();

    let gw = Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: std::env::temp_dir().join("argus-jobs-skills"),
        library_dir: std::env::temp_dir().join("argus-jobs-library"),
        logos_dir: std::env::temp_dir().join("argus-jobs-logos"),
        jobs_dir: std::env::temp_dir().join("argus-jobs-logs"),
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
        jobs: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        turns: Mutex::new(HashSet::new()),
        watching: Mutex::new(HashSet::new()),
    };

    std::fs::create_dir_all(&gw.jobs_dir).unwrap();

    let app = tauri::test::mock_app();
    app.manage(gw);

    app.handle().clone()
}

fn spec(command: &str) -> jobs::Spec {
    jobs::Spec {
        session_id: None,
        command: command.into(),
        cwd: None,
        profile: sandbox::Profile::Host,
        privileged: false,
        permission: "never".into(),
        label: "t".into(),
        wake: false,
        timeout_secs: Some(30),
    }
}

fn wait_done(gw: &Gateway, id: &str) -> jobs::Job {
    for _ in 0..300 {
        {
            let conn = gw.conn.lock().unwrap();
            let j = jobs::get(&conn, id).unwrap();

            if j.state != "running" {
                return j;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    panic!("job {id} never finished");
}

#[test]
fn a_job_starts_in_the_background_and_records_its_exit() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(&app, gw.inner(), spec("echo hello-from-the-job && exit 0")).unwrap();

    assert_eq!(job.state, "running");
    assert!(!job.id.is_empty());

    let done = wait_done(gw.inner(), &job.id);
    assert_eq!(done.state, "done", "{done:?}");
    assert_eq!(done.exit, Some(0));
}

#[test]
fn a_nonzero_exit_is_recorded_as_a_failure_not_a_success() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(&app, gw.inner(), spec("echo working; exit 3")).unwrap();
    let done = wait_done(gw.inner(), &job.id);

    assert_eq!(done.state, "failed", "{done:?}");
    assert_eq!(done.exit, Some(3));
}

#[test]
fn job_output_lands_on_disk_and_reads_back() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(&app, gw.inner(), spec("printf 'line-one\\nline-two\\n'")).unwrap();
    let done = wait_done(gw.inner(), &job.id);

    assert_eq!(done.state, "done");
    assert!(done.out_bytes > 0);

    let text = jobs::tail(&jobs::log_path(gw.inner(), &job.id), 8000).unwrap();
    assert!(text.contains("line-one"), "{text}");
    assert!(text.contains("line-two"), "{text}");
}

#[test]
fn a_long_job_outlives_the_call_that_started_it() {
    let app = app();
    let gw = app.state::<Gateway>();

    let t0 = std::time::Instant::now();
    let job = jobs::spawn(&app, gw.inner(), spec("sleep 3; echo finished")).unwrap();
    let spent = t0.elapsed();

    assert!(
        spent < std::time::Duration::from_secs(2),
        "spawn blocked for {spent:?} — it should return at once"
    );

    let done = wait_done(gw.inner(), &job.id);
    assert_eq!(done.state, "done");
    assert!(done.duration_ms.unwrap() >= 2_500, "{done:?}");
}

#[test]
fn a_job_can_be_killed_before_it_finishes() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(&app, gw.inner(), spec("sleep 60")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));

    assert!(jobs::kill(gw.inner(), &job.id).unwrap());

    let done = wait_done(gw.inner(), &job.id);
    assert_eq!(done.state, "killed", "{done:?}");
}

#[test]
fn killing_a_finished_job_is_not_an_error() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(&app, gw.inner(), spec("echo quick")).unwrap();
    wait_done(gw.inner(), &job.id);

    assert!(!jobs::kill(gw.inner(), &job.id).unwrap());
}

#[test]
fn a_job_that_cannot_start_is_recorded_as_failed() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(&app, gw.inner(), spec("definitely-not-a-real-binary-xyz")).unwrap();
    let done = wait_done(gw.inner(), &job.id);

    assert_eq!(done.state, "failed", "{done:?}");
    assert_eq!(done.exit, Some(127));

    let log = jobs::tail(&jobs::log_path(gw.inner(), &job.id), 4000).unwrap();
    assert!(log.contains("not found"), "the shell says why: {log}");
}

#[test]
fn the_log_tail_is_trimmed_from_the_front_and_stays_readable() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(
        &app,
        gw.inner(),
        spec("for i in $(seq 1 500); do echo \"line-$i-aaaaaaaaaaaaaaaa\"; done"),
    )
    .unwrap();
    wait_done(gw.inner(), &job.id);

    let tail = jobs::tail(&jobs::log_path(gw.inner(), &job.id), 400).unwrap();

    assert!(tail.len() <= 500, "tail was {} bytes", tail.len());
    assert!(
        tail.contains("line-500"),
        "the newest line must survive: {tail}"
    );
    assert!(!tail.contains("line-1 "), "the oldest line must be gone");
}

#[test]
fn reconcile_marks_jobs_the_app_never_came_back_for() {
    let app = app();
    let gw = app.state::<Gateway>();

    let job = jobs::spawn(&app, gw.inner(), spec("sleep 60")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));

    let n = {
        let conn = gw.conn.lock().unwrap();
        jobs::reconcile(&conn).unwrap()
    };

    assert_eq!(n, 1);

    let conn = gw.conn.lock().unwrap();
    let j = jobs::get(&conn, &job.id).unwrap();
    assert_eq!(j.state, "interrupted");
    assert!(j.note.unwrap().contains("Argus exited"));
}

#[test]
fn listing_is_newest_first_and_scoped_to_a_session() {
    let app = app();
    let gw = app.state::<Gateway>();

    let a = jobs::spawn(
        &app,
        gw.inner(),
        jobs::Spec {
            session_id: Some("s-one".into()),
            ..spec("echo a")
        },
    )
    .unwrap();

    let b = jobs::spawn(
        &app,
        gw.inner(),
        jobs::Spec {
            session_id: Some("s-two".into()),
            ..spec("echo b")
        },
    )
    .unwrap();

    wait_done(gw.inner(), &a.id);
    wait_done(gw.inner(), &b.id);

    let conn = gw.conn.lock().unwrap();

    let all = jobs::list(&conn, None, 20).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, b.id, "newest first");

    let one = jobs::list(&conn, Some("s-one"), 20).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].id, a.id);
}

#[test]
fn an_unknown_job_id_is_an_error_not_a_silent_empty() {
    let app = app();
    let gw = app.state::<Gateway>();
    let conn = gw.conn.lock().unwrap();

    assert!(jobs::get(&conn, "nope").is_err());
}

#[test]
fn a_background_job_may_run_past_the_interactive_ceiling() {
    assert!(jobs::DEADMAN_MAX > argus_lib::tools::shell::TERM_TIMEOUT_MAX);
    assert_eq!(jobs::DEADMAN_MAX, 12 * 60 * 60);
}

#[test]
fn output_is_on_disk_while_the_job_is_still_running() {
    let app = app();
    let gw = app.state::<Gateway>();
    let job = jobs::spawn(&app, gw.inner(), spec("echo EARLY; sleep 20; echo LATE")).unwrap();

    let path = jobs::log_path(gw.inner(), &job.id);
    let mut seen = String::new();

    for _ in 0..300 {
        seen = std::fs::read_to_string(&path).unwrap_or_default();
        if seen.contains("EARLY") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    assert!(
        seen.contains("EARLY"),
        "output is written as it runs, not at exit"
    );
    assert!(!seen.contains("LATE"), "the job has not finished yet");

    jobs::kill(gw.inner(), &job.id).unwrap();
    let done = wait_done(gw.inner(), &job.id);
    assert_eq!(done.state, "killed", "{done:?}");

    // The partial log survives the kill.
    let after = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        after.contains("EARLY"),
        "a killed job keeps what it printed"
    );
}
