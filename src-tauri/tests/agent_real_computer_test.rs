//! Real-model computer window-grounding evaluation.
//!
//! Real `sessions/chat.rs::send()` against Baseten GLM-5.3-Fast (rows copied
//! from the production database; key stays in the OS keyring). Isolated
//! fixture per run: private D-Bus + launcher + keyring daemon, nested
//! Xephyr display, metacity, and two xclock windows — a decoy and a target
//! whose X11 ids are allocated at runtime and never appear in any file,
//! prompt, or environment. The target title is only available through
//! computer observation. The user's real desktop is never touched.
//!
//! Ignored by default (`cargo test -- --ignored`). Runs exactly once per
//! invocation; never retries the task automatically.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::process::{Child, Command};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

const PROD_DB: &str = "/home/brnx/.local/share/com.anyscapelabs.argus/argus.db";
const WANT_PROVIDER: &str = "baseten";
const WANT_MODEL: &str = "baseten/zai-org/GLM-5.3-Fast";
const TARGET_TITLE: &str = "Argus Evaluation Target \u{2014} ORBIT-73";
const DECOY_TITLE: &str = "Argus Evaluation Decoy \u{2014} ORBIT-74";
const USER_TASK: &str =
    "Find the Argus Evaluation Target window, activate it, and tell me its identifier.";

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

fn raw_wmctrl() -> String {
    std::process::Command::new("wmctrl")
        .arg("-l")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn sweep_tmp(tmp: &std::path::Path) {
    let marker = tmp.display().to_string();
    let me = std::process::id();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for e in entries.flatten() {
            let pid = e.file_name().to_string_lossy().parse::<u32>().unwrap_or(0);
            if pid == 0 || pid == me {
                continue;
            }
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline"))
                .map(|raw| String::from_utf8_lossy(&raw).replace('\0', " "))
                .unwrap_or_default();
            let environ = std::fs::read(format!("/proc/{pid}/environ"))
                .map(|raw| String::from_utf8_lossy(&raw).replace('\0', " "))
                .unwrap_or_default();
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .unwrap_or_default()
                .trim()
                .to_string();
            if !matches!(
                comm.as_str(),
                "dbus-daemon" | "at-spi2-registryd" | "at-spi-bus-launcher"
            ) {
                continue;
            }
            if cmdline.contains(&marker) || environ.contains(&marker) {
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .status();
            }
        }
    }
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
    for n in (90..100).rev() {
        if claim_display(n) {
            return format!(":{n}");
        }
    }
    ":99".into()
}

fn active_window_id() -> Option<String> {
    let out = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tokio::test]
#[ignore]
async fn eval_real_computer_task() {
    let _guard = serial();
    assert!(!USER_TASK.contains("0x"), "prompt must not leak an id");

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
        "argus-eval-rcx-{}",
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
    tokio::time::sleep(std::time::Duration::from_millis(3000)).await;

    let kr_file = std::fs::File::create(tmp.join("keyring.log")).expect("kr log");
    let keyringd = Command::new("gnome-keyring-daemon")
        .args(["--foreground", "--components=secrets"])
        .stdin(std::process::Stdio::null())
        .stdout(kr_file.try_clone().expect("kr log clone"))
        .stderr(kr_file)
        .kill_on_drop(true)
        .spawn()
        .expect("keyring daemon");
    let mut secrets_up = false;
    let mut last_err = String::new();
    for _ in 0..60 {
        let probe = std::process::Command::new("secret-tool")
            .args(["lookup", "argus-eval-probe", "probe"])
            .output();
        match probe {
            Ok(o)
                if o.status.success()
                    || String::from_utf8_lossy(&o.stderr).contains("No matching")
                    || String::from_utf8_lossy(&o.stderr).is_empty() =>
            {
                secrets_up = true;
                break;
            }
            Ok(o) => {
                last_err = format!(
                    "secret-tool rc={} stderr={}",
                    o.status,
                    String::from_utf8_lossy(&o.stderr)
                );
            }
            Err(err) => {
                last_err = format!("spawn: {err}");
            }
        }
        match keyring::Entry::new("argus-eval-probe", "probe") {
            Ok(e) => match e.get_password() {
                Ok(_) | Err(keyring::Error::NoEntry) => {
                    secrets_up = true;
                    break;
                }
                Err(err) => {
                    last_err = format!("{err:?}");
                }
            },
            Err(err) => {
                last_err = format!("new: {err:?}");
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(
        secrets_up,
        "secret service never appeared: {last_err} | log: {}",
        std::fs::read_to_string(tmp.join("keyring.log"))
            .unwrap_or_default()
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .join(" | ")
    );
    procs.push(keyringd);
    argus_lib::gateway::store::secret_set("baseten", &real_key)
        .expect("copy baseten key into isolated keyring");
    drop(real_key);

    let display = free_display();
    let xephyr = Command::new("Xephyr")
        .args([display.as_str(), "-screen", "800x600", "-ac"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("xephyr");
    let sock = format!("/tmp/.X11-unix/X{}", display.trim_start_matches(':'));
    let mut up = false;
    for _ in 0..100 {
        if std::path::Path::new(sock.as_str()).exists() {
            up = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(up, "xephyr never came up");
    std::env::set_var("DISPLAY", &display);
    procs.push(xephyr);

    let wm = Command::new("metacity")
        .args(["--display", display.as_str()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("metacity");
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
    procs.push(wm);

    let target_clock = Command::new("xclock")
        .args(["-title", TARGET_TITLE])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("target clock");
    let decoy_clock = Command::new("xclock")
        .args(["-title", DECOY_TITLE])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("decoy clock");
    procs.push(target_clock);
    procs.push(decoy_clock);

    let mut ids = (String::new(), String::new());
    for _ in 0..40 {
        let list = raw_wmctrl();
        let t = list.lines().find(|l| l.contains("ORBIT-73")).and_then(|l| {
            l.split_whitespace()
                .next()
                .filter(|s| s.starts_with("0x"))
                .map(str::to_string)
        });
        let d = list.lines().find(|l| l.contains("ORBIT-74")).and_then(|l| {
            l.split_whitespace()
                .next()
                .filter(|s| s.starts_with("0x"))
                .map(str::to_string)
        });
        if let (Some(t), Some(d)) = (t, d) {
            ids = (t, d);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let (target_id, decoy_id) = ids;
    assert!(
        !target_id.is_empty() && !decoy_id.is_empty(),
        "both windows must map"
    );
    assert_ne!(target_id, decoy_id);

    let st = std::process::Command::new("wmctrl")
        .args(["-i", "-a", &decoy_id])
        .output()
        .expect("raw activate");
    assert!(st.status.success(), "could not pre-focus decoy");
    let mut focused_decoy = false;
    for _ in 0..40 {
        if active_window_id().is_some_and(|a| decimal_of(&a) == decimal_of(&decoy_id)) {
            focused_decoy = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(focused_decoy, "decoy never became active");

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
                title: "Eval real computer".into(),
                model_id: Some(WANT_MODEL.into()),
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

    let t0 = std::time::Instant::now();
    let status = argus_lib::sessions::chat::send(&gw, &handle, &session_id, USER_TASK, &chan).await;
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

    let final_active = active_window_id().and_then(|a| {
        let dec = decimal_of(&a);
        u64::from_str_radix(
            target_id
                .strip_prefix("0x")
                .or_else(|| target_id.strip_prefix("0X"))
                .unwrap_or(&target_id),
            16,
        )
        .ok()
        .map(|t| (dec == t.to_string(), dec))
    });

    for mut p in procs {
        let _ = p.kill().await;
    }
    sweep_tmp(&tmp);
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

    let observe_idx = results
        .iter()
        .position(|m| m.content.contains("tool=\"computer.observe\""));
    let window_idx = results
        .iter()
        .enumerate()
        .filter(|(_, m)| m.content.contains("tool=\"computer.window\""))
        .map(|(i, _)| i)
        .next();
    let ordered = match (observe_idx, window_idx) {
        (Some(o), Some(w)) => o < w,
        _ => false,
    };
    let observed_title = observe_idx
        .and_then(|i| results.get(i))
        .map(|m| m.content.contains(TARGET_TITLE))
        .unwrap_or(false);

    let mut window_args = String::new();
    for m in msgs.iter() {
        let mut s = m.content.clone();
        if let Some(c) = m.tool_calls.as_deref() {
            s.push_str(c);
        }
        if s.contains("\"computer.window\"") {
            window_args.push_str(&s.replace('\\', ""));
        }
    }
    let re_id = regex::Regex::new(r#""id"\s*:\s*"(0x[0-9a-fA-F]+)""#).unwrap();
    let targeted: Vec<String> = re_id
        .captures_iter(&window_args)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    let targeted_ok = targeted
        .iter()
        .any(|id| id.eq_ignore_ascii_case(&target_id));

    let tools_used: Vec<String> = results
        .iter()
        .filter_map(|m| {
            m.content
                .split("tool=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(str::to_string)
        })
        .collect();
    let mining = tools_used.iter().any(|t| {
        !matches!(
            t.as_str(),
            "computer.observe"
                | "computer.act"
                | "computer.screen"
                | "computer.click"
                | "computer.type"
                | "computer.key"
                | "computer.scroll"
                | "computer.window"
                | "computer.launch"
        )
    });

    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let answer_ok = final_text.contains(&target_id);

    eprintln!("=== REAL COMPUTER EVAL TRACE ===");
    eprintln!("provider=baseten remote={remote} elapsed={elapsed:?}");
    eprintln!("status: {status:?}");
    eprintln!(
        "turns: {} results: {} attempts: {attempts:?}",
        assistants.len(),
        results.len()
    );
    eprintln!("event kinds: {kinds:?}");
    eprintln!("tools used: {tools_used:?}");
    eprintln!("target={target_id} decoy={decoy_id}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!("--- assistant[{i}] ---\n{}", short(&m.content));
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }
    eprintln!("ordered={ordered} observed_title={observed_title} targeted={targeted:?} targeted_ok={targeted_ok}");
    eprintln!("final_active={final_active:?} mining={mining} answer_ok={answer_ok}");

    let total_calls = results.len();
    let verdict = if !status.is_ok() {
        "FAIL"
    } else if mining {
        "FAIL"
    } else if !(ordered && observed_title && targeted_ok) {
        "FAIL"
    } else if final_active != Some((true, decimal_of(&target_id))) {
        "FAIL"
    } else if !answer_ok {
        "PARTIAL"
    } else if total_calls > 4 {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(verdict, "PASS", "real-model computer task did not pass");
}

fn decimal_of(id: &str) -> String {
    let t = id.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        if let Ok(n) = u64::from_str_radix(hex, 16) {
            return n.to_string();
        }
    }
    t.to_string()
}
