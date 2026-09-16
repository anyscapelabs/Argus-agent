//! Presented-watermark (Design A) integration tests for computer `act`.
//!
//! Mock LLM drives the real `sessions/chat.rs::send()` loop; the desktop is
//! the deterministic GTK relay fixture (two buttons, trigger-file reorder
//! with ack synchronization, per-button side-effect log). The mock always
//! emits legacy text `<action>` blocks with `ref` and no `observation`,
//! emulating the observed GLM behavior. Tool ordering is asserted through
//! structured `tool="..."` result attributes, never substring matching.
//!
//! Each test runs its own Xephyr display and relay app under a binary-shared
//! private bus (the production AT-SPI connection caches the first bus it
//! sees). Tests are serialized; every flow re-observes before acting, so no
//! test depends on another's watermark state.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();
static SHARED: tokio::sync::OnceCell<SharedStack> = tokio::sync::OnceCell::const_new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

const APP_TITLE: &str = "Relay Console";
const LABEL_V1: &str = "relay-v1";
const LABEL_V2: &str = "relay-v2";

const APP_PY: &str = r#"
import sys, os, time
import gi
gi.require_version('Gtk', '3.0')
gi.require_version('GLib', '2.0')
from gi.repository import Gtk, GLib

log_path, trigger_path, ack_path = sys.argv[1], sys.argv[2], sys.argv[3]
swapped = []
seq = [0]

def fire(name):
    seq[0] += 1
    with open(log_path, 'a') as f:
        f.write("%d %s %d\n" % (seq[0], name, int(time.time() * 1000)))
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

struct SharedStack {
    runtime: PathBuf,
    session_bus: String,
    _daemon: tokio::sync::Mutex<Option<Child>>,
    _launcher: tokio::sync::Mutex<Option<Child>>,
    _keyring: tokio::sync::Mutex<Option<Child>>,
}

async fn shared_stack() -> Result<&'static SharedStack, String> {
    SHARED
        .get_or_try_init(|| async {
            let runtime =
                std::env::temp_dir().join(format!("argus-cshown-shared-{}", std::process::id()));
            std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            std::fs::set_permissions(
                &runtime,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            )
            .map_err(|e| e.to_string())?;

            let mut busd = Command::new("dbus-daemon")
                .args(["--session", "--print-address=1"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(false)
                .spawn()
                .map_err(|e| format!("dbus-daemon: {e}"))?;
            let mut line = String::new();
            {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut r = BufReader::new(busd.stdout.as_mut().unwrap());
                r.read_line(&mut line).await.map_err(|e| e.to_string())?;
            }
            let bus = line.trim().split(',').next().unwrap_or("").to_string();
            if bus.is_empty() {
                return Err("no bus address".to_string());
            }
            let launcher = Command::new("/usr/libexec/at-spi-bus-launcher")
                .args(["--launch-immediately", "--a11y=1"])
                .env("DBUS_SESSION_BUS_ADDRESS", &bus)
                .env("XDG_RUNTIME_DIR", &runtime)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(false)
                .spawn()
                .map_err(|e| format!("at-spi-bus-launcher: {e}"))?;
            let keyringd = Command::new("gnome-keyring-daemon")
                .args(["--foreground", "--components=secrets"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(false)
                .spawn()
                .map_err(|e| format!("keyring daemon: {e}"))?;
            let mut secrets_up = false;
            for _ in 0..60 {
                if let Ok(e) = keyring::Entry::new("argus-cshown-probe", "probe") {
                    match e.get_password() {
                        Ok(_) | Err(keyring::Error::NoEntry) => {
                            secrets_up = true;
                            break;
                        }
                        Err(_) => {}
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            if !secrets_up {
                return Err("secret service never appeared".to_string());
            }
            for _ in 0..60 {
                if let Ok(entries) = std::fs::read_dir(runtime.join("at-spi")) {
                    if entries.count() > 0 {
                        return Ok(SharedStack {
                            runtime,
                            session_bus: bus,
                            _daemon: tokio::sync::Mutex::new(Some(busd)),
                            _launcher: tokio::sync::Mutex::new(Some(launcher)),
                            _keyring: tokio::sync::Mutex::new(Some(keyringd)),
                        });
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            Err("a11y bus never appeared".to_string())
        })
        .await
}

struct LlmTurn {
    text: String,
}

fn turn(text: &str) -> LlmTurn {
    LlmTurn { text: text.into() }
}

async fn read_http_request(sock: &mut tokio::net::TcpStream) -> Option<(String, Vec<u8>)> {
    let mut buf = vec![0u8; 65536];
    let mut data = vec![];
    loop {
        let n = sock.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
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
                let n = sock.read(&mut buf).await.ok()?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
            }
            body.truncate(len);
            let line = head.lines().next().unwrap_or("").to_string();
            return Some((line, body));
        }
        if data.len() > 1_000_000 {
            return None;
        }
    }
}

fn sse_body(t: &LlmTurn) -> String {
    let mut out = String::new();
    if !t.text.is_empty() {
        out.push_str(&format!(
            "data: {}\n\n",
            serde_json::to_string(&serde_json::json!({
                "choices": [{"index": 0, "delta": {"role": "assistant", "content": t.text}}]
            }))
            .unwrap()
        ));
    }
    out.push_str(
        "data: {\"choices\": [{\"index\": 0, \"delta\": {}, \"finish_reason\": \"stop\"}]}\n\n",
    );
    out.push_str("data: [DONE]\n\n");
    out
}

async fn start_mock_llm(
    respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync>,
) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let respond = respond.clone();
            tokio::spawn(async move {
                let Some((line, body)) = read_http_request(&mut sock).await else {
                    return;
                };
                let req: serde_json::Value =
                    serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
                let turn_idx = req
                    .get("messages")
                    .and_then(|m| m.as_array())
                    .map(|msgs| {
                        msgs.iter()
                            .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
                            .count()
                    })
                    .unwrap_or(0);
                let stream = line.contains("/chat/completions")
                    && req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
                let t = respond(turn_idx, &req);
                let payload = if stream {
                    let body = sse_body(&t);
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
                    )
                } else {
                    let body = serde_json::json!({
                        "choices": [{"message": {"role": "assistant", "content": t.text}}],
                        "usage": {"prompt_tokens": 1, "completion_tokens": 1},
                    })
                    .to_string();
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let _ = sock.write_all(payload.as_bytes()).await;
            });
        }
    });
    base
}

struct Harness {
    gw: argus_lib::gateway::Gateway,
    tmp: PathBuf,
    session_id: String,
    handle: tauri::AppHandle<tauri::test::MockRuntime>,
    chan: tauri::ipc::Channel<argus_lib::gateway::schema::StreamEvent>,
    _app: tauri::App<tauri::test::MockRuntime>,
}

async fn setup(
    title: &str,
    model: &str,
    respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync>,
) -> Harness {
    let tmp = std::env::temp_dir().join(format!(
        "argus-cshown-gw-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().as_simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    for d in ["skills", "library", "logos"] {
        std::fs::create_dir_all(tmp.join(d)).unwrap();
    }
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let base = start_mock_llm(respond).await;
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
            id: model.into(),
            display_name: "Mock Test".into(),
            family: None,
            capabilities: None,
            suggested_tier: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::link_model(
        &conn,
        &Avail {
            model_id: model.into(),
            provider_id: "mock".into(),
            remote_model_id: "test".into(),
            cost_in: 0.0,
            cost_out: 0.0,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_model_enabled(&conn, model, true).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: StdMutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: tmp.join("skills"),
        library_dir: tmp.join("library"),
        logos_dir: tmp.join("logos"),
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
    };
    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: title.into(),
                model_id: Some(model.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
            },
        )
        .unwrap()
        .id
    };
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let chan = tauri::ipc::Channel::new(move |_| Ok(()));
    Harness {
        gw,
        tmp,
        session_id,
        handle,
        chan,
        _app: app,
    }
}

impl Harness {
    async fn send(&self, user_text: &str) -> Result<(), String> {
        argus_lib::sessions::chat::send(
            &self.gw,
            &self.handle,
            &self.session_id,
            user_text,
            &self.chan,
        )
        .await
    }

    fn results(&self) -> Vec<argus_lib::sessions::schema::Msg> {
        let conn = self.gw.conn.lock().unwrap();
        argus_lib::sessions::store::list_msgs(&conn, &self.session_id)
            .unwrap()
            .into_iter()
            .filter(|m| m.content.contains("<tool-result") || m.tool_call_id.is_some())
            .collect()
    }
}

fn result_tool_name(content: &str) -> Option<String> {
    let re = regex::Regex::new(r#"tool="([^"]+)""#).unwrap();
    re.captures(content)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

fn snapshot_gens(text: &str) -> Vec<u64> {
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
    for n in (60..70).rev() {
        if claim_display(n) {
            return format!(":{n}");
        }
    }
    ":69".into()
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

fn read_log(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn parse_effects(lines: &[String]) -> Vec<(u64, String)> {
    lines
        .iter()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.to_string()))
        })
        .collect()
}

struct Fixture {
    prev: Vec<(String, Option<String>)>,
    tmp: PathBuf,
    procs: Vec<Child>,
    log_path: PathBuf,
    trigger_path: PathBuf,
    ack_path: PathBuf,
}

impl Fixture {
    async fn start(tag: &str) -> Self {
        let stack = shared_stack().await.expect("shared bus stack");
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
            "argus-cshown-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().as_simple()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &stack.session_bus);
        std::env::set_var("XDG_RUNTIME_DIR", &stack.runtime);
        std::env::set_var("XDG_DATA_HOME", &tmp);

        let mut procs: Vec<Child> = vec![];
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

        Self {
            prev,
            tmp,
            procs,
            log_path,
            trigger_path,
            ack_path,
        }
    }

    async fn stop(mut self) {
        for p in self.procs.iter_mut() {
            let _ = p.kill().await;
        }
        let mut drained = false;
        for _ in 0..60 {
            if find_window_id(APP_TITLE).is_none() {
                drained = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        assert!(
            drained,
            "relay window must vanish before releasing the fixture"
        );
        for (k, v) in self.prev {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

fn ref_of_label(refs: &[(u64, String)], label: &str) -> u64 {
    refs.iter()
        .find(|(_, l)| l == label)
        .map(|(i, _)| *i)
        .unwrap_or_else(|| panic!("label {label} missing in {refs:?}"))
}

#[tokio::test]
async fn tokenless_drift_rejected_before_side_effect() {
    let _guard = serial();
    let fx = Fixture::start("drift").await;
    let rcell: Arc<StdMutex<Option<u64>>> = Arc::new(StdMutex::new(None));
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> = {
        let rcell = rcell.clone();
        Arc::new(move |t, _| match t {
            0 => turn(r#"<action tool="computer.observe">{}</action>"#),
            1 => turn("observing"),
            2 => match *rcell.lock().unwrap() {
                Some(r) => turn(&format!(
                    r#"<action tool="computer.act">{{"ref":{r}}}</action>"#
                )),
                None => turn("waiting"),
            },
            _ => turn("done"),
        })
    };
    let h = setup("drift", "mock/cshown-drift", respond).await;

    h.send("observe the relay console").await.expect("send#1");
    let r1 = h.results();
    assert_eq!(
        r1.iter()
            .filter_map(|m| result_tool_name(&m.content))
            .collect::<Vec<_>>(),
        vec!["computer.observe"],
        "send#1 tools"
    );
    let gen_n = snapshot_gens(&r1[0].content).into_iter().next().unwrap();
    let r_old = ref_of_label(&button_refs(&r1[0].content), LABEL_V1);
    *rcell.lock().unwrap() = Some(r_old);
    assert!(read_log(&fx.log_path).is_empty());

    std::fs::write(&fx.trigger_path, "swap\n").unwrap();
    let mut acked = false;
    for _ in 0..80 {
        if fx.ack_path.exists() {
            acked = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(acked, "fixture never acknowledged the reorder");

    let out2 = argus_lib::tools::exec(&h.gw, "computer.observe", "{}", "never", false, true, None)
        .await
        .expect("observe#2");
    let gen_n1 = snapshot_gens(&out2).into_iter().next().unwrap();
    assert_ne!(gen_n, gen_n1, "reorder must advance the generation");
    let occupant = button_refs(&out2)
        .into_iter()
        .find(|(i, _)| *i == r_old)
        .map(|(_, l)| l);
    assert_eq!(
        occupant.as_deref(),
        Some(LABEL_V2),
        "ref must remap after drift"
    );
    assert!(read_log(&fx.log_path).is_empty());

    h.send("press the relay").await.expect("send#2");
    let r2 = h.results();
    let tools2: Vec<String> = r2
        .iter()
        .filter_map(|m| result_tool_name(&m.content))
        .collect();
    assert_eq!(
        tools2,
        vec!["computer.observe", "computer.act"],
        "send#2 tools"
    );
    let stale = &r2[1].content;
    assert!(
        stale.contains(&format!("stale ref {r_old}")),
        "got: {stale}"
    );
    assert!(
        stale.contains(&format!("from observation {gen_n}")),
        "got: {stale}"
    );
    assert!(
        stale.contains(&format!("observation {gen_n1} is current")),
        "got: {stale}"
    );
    assert_eq!(
        read_log(&fx.log_path),
        Vec::<String>::new(),
        "zero side effects"
    );

    fx.stop().await;
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn tokenless_steady_succeeds_after_fresh_observe() {
    let _guard = serial();
    let fx = Fixture::start("steady").await;
    let rcell: Arc<StdMutex<Option<u64>>> = Arc::new(StdMutex::new(None));
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> = {
        let rcell = rcell.clone();
        Arc::new(move |t, _| match t {
            0 => turn(r#"<action tool="computer.observe">{}</action>"#),
            1 => turn("observing"),
            2 => match *rcell.lock().unwrap() {
                Some(r) => turn(&format!(
                    r#"<action tool="computer.act">{{"ref":{r}}}</action>"#
                )),
                None => turn("waiting"),
            },
            _ => turn("done"),
        })
    };
    let h = setup("steady", "mock/cshown-steady", respond).await;

    h.send("observe the relay console").await.expect("send#1");
    let r1 = h.results();
    let r_old = ref_of_label(&button_refs(&r1[0].content), LABEL_V1);
    *rcell.lock().unwrap() = Some(r_old);

    h.send("press the relay").await.expect("send#2");
    let r2 = h.results();
    let tools2: Vec<String> = r2
        .iter()
        .filter_map(|m| result_tool_name(&m.content))
        .collect();
    assert_eq!(
        tools2,
        vec!["computer.observe", "computer.act"],
        "send#2 tools"
    );
    assert!(
        !r2[1].content.contains("stale ref"),
        "steady action must succeed: {}",
        r2[1].content
    );
    assert_eq!(
        parse_effects(&read_log(&fx.log_path)),
        vec![(1, LABEL_V1.to_string())],
        "steady action must fire exactly the intended relay first"
    );

    fx.stop().await;
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn explicit_token_current_succeeds_stale_rejected() {
    let _guard = serial();
    let fx = Fixture::start("explicit").await;
    let h = setup(
        "explicit",
        "mock/cshown-explicit",
        Arc::new(|_, _| turn("done")),
    )
    .await;

    let out1 = argus_lib::tools::exec(&h.gw, "computer.observe", "{}", "never", false, true, None)
        .await
        .expect("observe#1");
    let gen_n = snapshot_gens(&out1).into_iter().next().unwrap();
    let out2 = argus_lib::tools::exec(&h.gw, "computer.observe", "{}", "never", false, true, None)
        .await
        .expect("observe#2");
    let gen_n1 = snapshot_gens(&out2).into_iter().next().unwrap();
    assert_ne!(gen_n, gen_n1);
    let r_new = ref_of_label(&button_refs(&out2), LABEL_V1);

    let err = argus_lib::tools::exec(
        &h.gw,
        "computer.act",
        &format!(r#"{{"ref":{r_new},"observation":{gen_n}}}"#),
        "never",
        false,
        true,
        None,
    )
    .await
    .expect_err("old explicit token must be stale");
    assert!(err.contains("stale ref"), "got: {err}");
    assert!(read_log(&fx.log_path).is_empty());

    argus_lib::tools::exec(
        &h.gw,
        "computer.act",
        &format!(r#"{{"ref":{r_new},"observation":{gen_n1}}}"#),
        "never",
        false,
        true,
        None,
    )
    .await
    .expect("current explicit token must succeed");
    assert_eq!(
        parse_effects(&read_log(&fx.log_path)),
        vec![(1, LABEL_V1.to_string())],
        "explicit action must fire exactly the intended relay first"
    );

    fx.stop().await;
    let _ = std::fs::remove_dir_all(&h.tmp);
}
