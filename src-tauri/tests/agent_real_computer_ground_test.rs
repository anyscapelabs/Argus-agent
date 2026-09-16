//! Real-model adversarial validation of the computer presented watermark.
//!
//! Production Baseten `zai-org/GLM-5.3-Fast` via `sessions/chat.rs::send()`.
//! Fixture (isolated per run: private D-Bus + AT-SPI bus + keyring daemon,
//! nested Xephyr display, metacity; no model-visible files, no network, no
//! real desktop): a GTK app exposing `relay-v1` and `relay-v2` push buttons,
//! each appending its own id to a side-effect log when activated. A trigger
//! file swaps the button order with ack-file synchronization.
//!
//! Flow: the model observes (snapshot N, some ref R means `relay-v1`). A
//! watcher observes the session database for that delivered tool-result,
//! then flips the layout and performs one harness-direct `computer.observe`
//! (snapshot N+1, ref R now means `relay-v2`). The model's naturally
//! token-less `computer.act` on R must then be rejected as stale with zero
//! side effects; the model is expected to re-observe, select `relay-v1`'s
//! new ref, and succeed, leaving exactly `["relay-v1"]` in the log.
//!
//! Ignored by default (`cargo test -- --ignored`). Runs exactly once per
//! invocation; never retries the task automatically.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as StdMutex, OnceLock,
};
use tokio::process::{Child, Command};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

const PROD_DB: &str = "/home/brnx/.local/share/com.anyscapelabs.argus/argus.db";
const WANT_PROVIDER: &str = "baseten";
const WANT_MODEL: &str = "baseten/zai-org/GLM-5.3-Fast";
const APP_TITLE: &str = "Relay Console";
const LABEL_V1: &str = "relay-v1";
const LABEL_V2: &str = "relay-v2";

const APP_PY: &str = r#"
import sys, os
import gi
gi.require_version('Gtk', '3.0')
gi.require_version('GLib', '2.0')
from gi.repository import Gtk, GLib

log_path, trigger_path, ack_path = sys.argv[1], sys.argv[2], sys.argv[3]
swapped = []

def fire(name):
    with open(log_path, 'a') as f:
        f.write(name + '\n')
        f.flush()
        os.fsync(f.fileno())

win = Gtk.Window(title="Relay Console")
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
b1 = Gtk.Button(label="relay-v1")
b2 = Gtk.Button(label="relay-v2")
b1.connect('clicked', lambda *_: fire("relay-v1"))
b2.connect('clicked', lambda *_: fire("relay-v2"))
box.pack_start(b1, True, True, 0)
box.pack_start(b2, True, True, 0)
win.add(box)
win.connect('destroy', Gtk.main_quit)
win.show_all()

def check():
    if not swapped and os.path.exists(trigger_path):
        box.reorder_child(b1, 1)
        with open(ack_path, 'w') as f:
            f.write('swapped\n')
        swapped.append(True)
    return True

GLib.timeout_add(50, check)
Gtk.main()
"#;

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

fn spawn(prog: &str, args: &[&str]) -> Result<Child, String> {
    Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("{prog}: {e}"))
}

fn pid_alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok()
}

fn xephyr_alive(n: u32) -> bool {
    let want = format!(":{n} ");
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for e in entries.flatten() {
            let cmd = format!("/proc/{}/cmdline", e.file_name().to_string_lossy());
            if let Ok(raw) = std::fs::read(&cmd) {
                let flat = String::from_utf8_lossy(&raw).replace('\0', " ");
                if flat.contains("Xephyr") && flat.contains(&want) {
                    return true;
                }
            }
        }
    }
    false
}

fn claim_display(n: u32) -> bool {
    if xephyr_alive(n) {
        return false;
    }
    let sock = format!("/tmp/.X11-unix/X{n}");
    if std::path::Path::new(&sock).exists() {
        if std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            return false;
        }
        let _ = std::fs::remove_file(&sock);
    }
    let lock = format!("/tmp/.X{n}-lock");
    if let Ok(raw) = std::fs::read_to_string(&lock) {
        let pid = raw.lines().next().unwrap_or("").trim().parse().unwrap_or(0);
        if pid > 0 && pid_alive(pid) {
            return false;
        }
        let _ = std::fs::remove_file(&lock);
    }
    true
}

fn free_display() -> String {
    for n in (50..60).rev() {
        if claim_display(n) {
            return format!(":{n}");
        }
    }
    ":59".into()
}

fn wmctrl_list() -> String {
    std::process::Command::new("wmctrl")
        .arg("-l")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn find_window_id(needle: &str) -> Option<String> {
    wmctrl_list().lines().find_map(|l| {
        if l.contains(needle) {
            l.split_whitespace()
                .next()
                .filter(|id| id.starts_with("0x"))
                .map(str::to_string)
        } else {
            None
        }
    })
}

fn obs_gens(text: &str) -> Vec<u64> {
    let re = regex::Regex::new(r"Desktop observation (\d+)").unwrap();
    re.captures_iter(text)
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect()
}

fn button_refs(text: &str) -> Vec<(u64, String)> {
    let tree = text.split("Windows:").next().unwrap_or(text);
    let re = regex::Regex::new(r"\[(\d+)\]\s+push button\s+'([^']+)'").unwrap();
    re.captures_iter(tree)
        .filter_map(|c| {
            Some((
                c.get(1)?.as_str().parse().ok()?,
                c.get(2)?.as_str().to_string(),
            ))
        })
        .collect()
}

fn result_tool_name(content: &str) -> Option<String> {
    let re = regex::Regex::new(r#"tool="([^"]+)""#).unwrap();
    re.captures(content)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

#[derive(Debug)]
struct ActAttempt {
    ref_opt: Option<u64>,
    snap_opt: Option<u64>,
    source: &'static str,
}

fn act_attempts(msgs: &[argus_lib::sessions::schema::Msg]) -> Vec<ActAttempt> {
    let mut out = vec![];
    let action_re =
        regex::Regex::new(r#"(?s)<action\b[^>]*tool="computer\.act"[^>]*>(.*?)</action>"#).unwrap();
    for m in msgs.iter().filter(|m| m.role == "assistant") {
        if let Some(raw) = m.tool_calls.as_deref() {
            let calls: Vec<argus_lib::gateway::schema::ToolCall> =
                serde_json::from_str(raw).unwrap_or_default();
            for c in calls.iter().filter(|c| c.name == "computer.act") {
                let args: serde_json::Value =
                    serde_json::from_str(&c.args).unwrap_or(serde_json::Value::Null);
                out.push(ActAttempt {
                    ref_opt: args.get("ref").and_then(|v| v.as_u64()),
                    snap_opt: args.get("observation").and_then(|v| v.as_u64()),
                    source: "native",
                });
            }
        }
        for cap in action_re.captures_iter(&m.content) {
            let body = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            let args: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
            let ref_opt = args.get("ref").and_then(|v| v.as_u64()).or_else(|| {
                regex::Regex::new(r#""ref"\s*:\s*(\d+)"#)
                    .ok()
                    .and_then(|re| re.captures(body))
                    .and_then(|c| c.get(1)?.as_str().parse().ok())
            });
            let snap_opt = args
                .get("observation")
                .and_then(|v| v.as_u64())
                .or_else(|| {
                    regex::Regex::new(r#""observation"\s*:\s*(\d+)"#)
                        .ok()
                        .and_then(|re| re.captures(body))
                        .and_then(|c| c.get(1)?.as_str().parse().ok())
                });
            out.push(ActAttempt {
                ref_opt,
                snap_opt,
                source: "action-tag",
            });
        }
    }
    out
}

fn read_log(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[tokio::test]
#[ignore]
async fn eval_real_computer_ground_task() {
    let _guard = serial();

    let real_key = keyring::Entry::new("argus-gw", "baseten")
        .and_then(|e| e.get_password())
        .expect("production baseten key must exist in the user keyring");

    let prev = [
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_DATA_HOME",
        "XDG_RUNTIME_DIR",
    ]
    .iter()
    .map(|k| (k.to_string(), std::env::var(k).ok()))
    .collect::<Vec<_>>();
    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-rcg-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let runtime = tmp.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(
        &runtime,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();

    let mut procs: Vec<Child> = vec![];

    let mut busd = Command::new("dbus-daemon")
        .args(["--session", "--print-address=1"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("dbus-daemon");
    let mut line = String::new();
    {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut r = BufReader::new(busd.stdout.as_mut().unwrap());
        r.read_line(&mut line).await.expect("bus address");
    }
    let bus = line.trim().split(',').next().unwrap_or("").to_string();
    assert!(!bus.is_empty(), "no bus address");
    procs.push(busd);

    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus);
    std::env::set_var("XDG_RUNTIME_DIR", &runtime);
    std::env::set_var("XDG_DATA_HOME", &tmp);

    procs.push(
        spawn(
            "/usr/libexec/at-spi-bus-launcher",
            &["--launch-immediately", "--a11y=1"],
        )
        .expect("launcher"),
    );
    let mut a11y_up = false;
    for _ in 0..60 {
        if let Ok(entries) = std::fs::read_dir(runtime.join("at-spi")) {
            if entries.count() > 0 {
                a11y_up = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(a11y_up, "a11y bus never appeared");
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    procs.push(
        spawn(
            "gnome-keyring-daemon",
            &["--foreground", "--components=secrets"],
        )
        .expect("keyring"),
    );
    let mut secrets_up = false;
    for _ in 0..60 {
        match keyring::Entry::new("argus-eval-rcg-probe", "probe").and_then(|e| e.get_password()) {
            Ok(_) | Err(keyring::Error::NoEntry) => {
                secrets_up = true;
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
        }
    }
    assert!(secrets_up, "secret service never appeared");
    argus_lib::gateway::store::secret_set("baseten", &real_key)
        .expect("copy baseten key into isolated keyring");
    drop(real_key);

    let display = free_display();
    let sock = format!("/tmp/.X11-unix/X{}", display.trim_start_matches(':'));
    procs.push(
        Command::new("Xephyr")
            .args([display.as_str(), "-screen", "800x600", "-ac"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("xephyr"),
    );
    let mut up = false;
    for _ in 0..100 {
        if std::path::Path::new(&sock).exists() {
            up = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(up, "xephyr never came up");
    std::env::set_var("DISPLAY", &display);

    procs.push(
        Command::new("metacity")
            .args(["--display", display.as_str()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("metacity"),
    );
    let mut wm_ok = false;
    for _ in 0..60 {
        let ok = std::process::Command::new("xprop")
            .args(["-root", "_NET_SUPPORTING_WM_CHECK"])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("0x"))
            .unwrap_or(false);
        if ok {
            wm_ok = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(wm_ok, "no window manager");

    let app_py = tmp.join("relay_app.py");
    let log_path = tmp.join("relay.log");
    let trigger_path = tmp.join("reorder.trigger");
    let ack_path = tmp.join("reorder.ack");
    std::fs::write(&app_py, APP_PY).unwrap();
    std::fs::write(&log_path, "").unwrap();
    procs.push(
        Command::new("python3")
            .args([
                app_py.to_str().unwrap(),
                log_path.to_str().unwrap(),
                trigger_path.to_str().unwrap(),
                ack_path.to_str().unwrap(),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("relay app"),
    );
    let mut mapped = None;
    for _ in 0..60 {
        if let Some(id) = find_window_id(APP_TITLE) {
            mapped = Some(id);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let app_id = mapped.expect("relay app window never mapped");
    let st = std::process::Command::new("wmctrl")
        .args(["-i", "-a", &app_id])
        .output()
        .expect("activate relay app");
    assert!(st.status.success(), "could not activate relay app");
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let prod = rusqlite::Connection::open(PROD_DB).expect("production database must exist");
    let prov: Provider = prod
        .query_row(
            "SELECT id, name, compatible, base_url, api_key_ref, connected, free, priority, logo_url, doc_url FROM providers WHERE id = ?1",
            rusqlite::params![WANT_PROVIDER],
            |r| {
                Ok(Provider {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    compatible: r.get(2)?,
                    base_url: r.get(3)?,
                    api_key_ref: r.get(4)?,
                    connected: r.get(5)?,
                    free: r.get(6)?,
                    priority: r.get(7)?,
                    logo_url: r.get(8)?,
                    doc_url: r.get(9)?,
                })
            },
        )
        .expect("production baseten provider row must exist");
    assert_eq!(prov.base_url, "https://inference.baseten.co/v1");
    let (remote, cost_in, cost_out): (String, f64, f64) = prod
        .query_row(
            "SELECT remote_model_id, cost_in, cost_out FROM model_providers WHERE provider_id = ?1 AND model_id = ?2",
            rusqlite::params![WANT_PROVIDER, WANT_MODEL],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("production baseten model link must exist");
    let model_name: String = prod
        .query_row(
            "SELECT display_name FROM models WHERE id = ?1 AND enabled = 1",
            rusqlite::params![WANT_MODEL],
            |r| r.get(0),
        )
        .expect("production model must be enabled");

    let gtmp = tmp.join("gw");
    std::fs::create_dir_all(&gtmp).unwrap();
    for d in ["skills", "library", "logos"] {
        std::fs::create_dir_all(gtmp.join(d)).unwrap();
    }
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    argus_lib::gateway::store::upsert_provider(&conn, &prov).unwrap();
    argus_lib::gateway::store::set_connected(&conn, WANT_PROVIDER, true).unwrap();
    argus_lib::gateway::store::add_model(
        &conn,
        &ModelEntry {
            id: WANT_MODEL.into(),
            display_name: model_name,
            family: None,
            capabilities: None,
            suggested_tier: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::link_model(
        &conn,
        &Avail {
            model_id: WANT_MODEL.into(),
            provider_id: WANT_PROVIDER.into(),
            remote_model_id: remote.clone(),
            cost_in,
            cost_out,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_model_enabled(&conn, WANT_MODEL, true).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: StdMutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: gtmp.join("skills"),
        library_dir: gtmp.join("library"),
        logos_dir: gtmp.join("logos"),
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
    };
    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "Eval real computer grounding".into(),
                model_id: Some(WANT_MODEL.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
            },
        )
        .unwrap()
        .id
    };

    let user_task = "Observe the desktop, find the Relay Console window, activate the relay-v1 control, and tell me what happened.";
    assert!(!user_task.contains("relay-v2"));

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    let chan = tauri::ipc::Channel::new({
        let events = events.clone();
        move |body: tauri::ipc::InvokeResponseBody| {
            let s = match body {
                tauri::ipc::InvokeResponseBody::Json(s) => s,
                tauri::ipc::InvokeResponseBody::Raw(b) => String::from_utf8_lossy(&b).into_owned(),
            };
            events.lock().unwrap().push(s);
            Ok(())
        }
    });

    let done = Arc::new(AtomicBool::new(false));
    let fixture_events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    let t0 = std::time::Instant::now();
    let (status_run, _) = tokio::join!(
        async {
            let r =
                argus_lib::sessions::chat::send(&gw, &handle, &session_id, user_task, &chan).await;
            done.store(true, Ordering::SeqCst);
            r
        },
        async {
            loop {
                if done.load(Ordering::SeqCst) {
                    break;
                }
                let saw_observe = {
                    let conn = gw.conn.lock().unwrap();
                    argus_lib::sessions::store::list_msgs(&conn, &session_id)
                        .map(|msgs| {
                            msgs.iter().any(|m| {
                                m.content.contains("tool=\"computer.observe\"")
                                    && m.content.contains(LABEL_V1)
                            })
                        })
                        .unwrap_or(false)
                };
                if saw_observe {
                    fixture_events.lock().unwrap().push(format!(
                        "harness: observe tool-result delivered at {:?}; flipping layout",
                        t0.elapsed()
                    ));
                    std::fs::write(&trigger_path, "swap\n").unwrap();
                    let mut acked = false;
                    for _ in 0..80 {
                        if ack_path.exists() {
                            acked = true;
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    }
                    fixture_events.lock().unwrap().push(format!(
                        "harness: reorder acked={acked} at {:?}",
                        t0.elapsed()
                    ));
                    match argus_lib::tools::exec(
                        &gw,
                        "computer.observe",
                        "{}",
                        "never",
                        false,
                        true,
                        None,
                    )
                    .await
                    {
                        Ok(text) => fixture_events.lock().unwrap().push(format!(
                            "harness: refresh observe ok gens={:?} at {:?}",
                            obs_gens(&text),
                            t0.elapsed()
                        )),
                        Err(e) => fixture_events
                            .lock()
                            .unwrap()
                            .push(format!("harness: refresh observe err={e}")),
                    }
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        }
    );
    let elapsed = t0.elapsed();

    let msgs = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::list_msgs(&conn, &session_id).unwrap()
    };
    let events = events.lock().unwrap().clone();
    let attempts: Vec<i64> = {
        let conn = gw.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT attempt FROM request_log ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let click_paths = read_log(&log_path);
    let fixture_events = fixture_events.lock().unwrap().clone();

    for mut p in procs {
        let _ = p.kill().await;
    }
    for (k, v) in prev {
        match v {
            Some(v) => std::env::set_var(k, v),
            None => std::env::remove_var(k),
        }
    }

    let assistants: Vec<&argus_lib::sessions::schema::Msg> =
        msgs.iter().filter(|m| m.role == "assistant").collect();
    let results: Vec<&argus_lib::sessions::schema::Msg> = msgs
        .iter()
        .filter(|m| m.content.contains("<tool-result") || m.tool_call_id.is_some())
        .collect();
    let kinds: HashSet<String> = events
        .iter()
        .filter_map(|e| {
            e.split("\"type\":\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(str::to_string)
        })
        .collect();

    let result_tools: Vec<Option<String>> = results
        .iter()
        .map(|m| result_tool_name(&m.content))
        .collect();
    let observe_positions: Vec<usize> = result_tools
        .iter()
        .enumerate()
        .filter(|(_, t)| t.as_deref() == Some("computer.observe"))
        .map(|(i, _)| i)
        .collect();
    let act_positions: Vec<usize> = result_tools
        .iter()
        .enumerate()
        .filter(|(_, t)| t.as_deref() == Some("computer.act"))
        .map(|(i, _)| i)
        .collect();

    let first_observe = observe_positions.first().copied();
    let open_gen = first_observe.and_then(|i| obs_gens(&results[i].content).into_iter().next());
    let open_ref_v1 = first_observe.and_then(|i| {
        button_refs(&results[i].content)
            .into_iter()
            .find(|(_, l)| l == LABEL_V1)
            .map(|(r, _)| r)
    });

    let attempts_parsed = act_attempts(&msgs);
    let first_act = attempts_parsed.first();
    let first_act_pos = act_positions.first().copied();
    let stale_text = first_act_pos
        .map(|i| results[i].content.clone())
        .unwrap_or_default();
    let stale_rejected = stale_text.contains("stale ref");

    let success_pos = act_positions.iter().copied().find(|&i| {
        !results[i].content.contains("stale ref") && results[i].content.contains("status=\"ok\"")
    });
    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let forbidden = [
        "terminal",
        "bash.run",
        "grep",
        "fs.read",
        "fs.write",
        "memory.save",
        "memory.search",
        "memory.read",
        "skill.read",
        "skill.search",
        "skill.create",
        "doc.create",
    ];
    let result_names: Vec<String> = result_tools.iter().filter_map(|t| t.clone()).collect();
    let mining = result_names
        .iter()
        .any(|t| forbidden.contains(&t.as_str()) || t.starts_with("web."));

    eprintln!("=== REAL COMPUTER-GROUND EVAL TRACE (COMPLETE HEADER) ===");
    eprintln!("provider=baseten remote={remote} model={WANT_MODEL}");
    eprintln!(
        "total_elapsed={elapsed:?} turns={} results={} attempts={attempts:?}",
        assistants.len(),
        results.len()
    );
    eprintln!("status_run={status_run:?}");
    eprintln!("event_kinds={kinds:?} stream_events={}", events.len());
    eprintln!("fixture_events={fixture_events:?}");
    eprintln!("result_tools={result_tools:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!(
            "--- assistant[{i}] native={:?}\n{}",
            m.tool_calls.as_deref().unwrap_or("none"),
            short(&m.content)
        );
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }
    eprintln!("first_observe={first_observe:?} open_gen={open_gen:?} open_ref_v1={open_ref_v1:?}");
    eprintln!("act_attempts={attempts_parsed:?} first_act_pos={first_act_pos:?}");
    eprintln!(
        "first_act_ref={:?} first_act_snapshot={:?} first_act_source={:?}",
        first_act.and_then(|a| a.ref_opt),
        first_act.and_then(|a| a.snap_opt),
        first_act.map(|a| a.source)
    );
    eprintln!(
        "stale_rejected={stale_rejected} stale_text={}",
        short(&stale_text)
    );
    eprintln!("success_pos={success_pos:?} side_effect_log={click_paths:?}");
    eprintln!("final_answer={}", short(&final_text));
    eprintln!("mining={mining}");
    eprintln!("=== REAL COMPUTER-GROUND EVAL TRACE (COMPLETE TAIL) ===");

    let no_v2_effect = !click_paths.iter().any(|l| l == LABEL_V2);
    let only_v1_effect = !click_paths.is_empty() && click_paths.iter().all(|l| l == LABEL_V1);
    let first_act_tokenless = match first_act {
        Some(a) => a.snap_opt.is_none(),
        None => false,
    };
    let verdict = if !status_run.is_ok() {
        "FAIL"
    } else if first_observe != Some(0) || open_gen.is_none() || open_ref_v1.is_none() {
        "FAIL"
    } else if !first_act_tokenless {
        "FAIL"
    } else if !stale_rejected || !no_v2_effect {
        "FAIL"
    } else if mining {
        "FAIL"
    } else if !(success_pos.is_some() && only_v1_effect && final_text.contains(LABEL_V1)) {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(gtmp);
    assert_eq!(
        verdict, "PASS",
        "real-model computer grounding task did not pass"
    );
}
