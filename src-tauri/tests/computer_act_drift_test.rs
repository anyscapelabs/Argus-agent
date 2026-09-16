//! Deterministic diagnostic: token-less `computer.act` cannot reach any
//! target while production app enumeration yields zero rows.
//!
//! Suspected defect under test (computer-use equivalent of the browser
//! pre-Design-A hole): `observe` (generation N, ref R means target A) → UI
//! drift (generation N+1, ref R means target B) → token-less
//! `computer.act({"ref": R})` resolves R against the CURRENT table and fires
//! against B.
//!
//! Fixture (fully isolated: private D-Bus + AT-SPI bus, nested Xephyr
//! display, metacity; no model, no network, no real desktop): a small GTK
//! app exposing two push buttons, `relay-v1` and `relay-v2`, each appending
//! its own id to a side-effect log when activated. A trigger file makes the
//! app swap the buttons' order with ack-file synchronization.
//!
//! Finding locked in by this test: both apps ARE registered on the fixture
//! AT-SPI bus (proven below via bus-level enumeration, independent of the
//! production walker), yet production `computer.observe` reports zero
//! elements — the `atspi` client's root `ChildCount` read is incompatible
//! with the system registryd, and production maps that failure to an empty
//! table. Consequently every ref, token-less or not, fails closed with
//! `unknown ref` and NOTHING executes (side-effect log stays empty).
//!
//! The suspected wrong-target execution is therefore NOT reproducible
//! end-to-end here: the defect logic (resolve-against-current on omitted
//! token) still exists in code, but it is unreachable while enumeration
//! yields no rows. If the walker is ever fixed, this test's `unknown ref`
//! expectation will fail — that failure is the tripwire showing the latent
//! token-less drift hole is now live, at which point a computer-use
//! presented-watermark (browser Design A equivalent) becomes required.

use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use tokio::process::{Child, Command};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

const APP_TITLE: &str = "Relay Console";
const APP_BUS_NAME: &str = "relay_app.py";

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
    for n in (70..80).rev() {
        if claim_display(n) {
            return format!(":{n}");
        }
    }
    ":79".into()
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

fn obs_gen(out: &str) -> Option<u64> {
    let re = regex::Regex::new(r"Desktop observation (\d+)").ok()?;
    re.captures(out)?.get(1)?.as_str().parse().ok()
}

fn bus_text(args: &[&str]) -> String {
    std::process::Command::new("busctl")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn a11y_address() -> String {
    let out = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.a11y.Bus",
            "/org/a11y/bus",
            "org.a11y.Bus.GetAddress",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.lines()
        .filter_map(|l| {
            let s = l.trim().strip_prefix("string \"")?.strip_suffix('"')?;
            Some(s.split(',').next().unwrap_or("").to_string())
        })
        .next()
        .unwrap_or_default()
}

fn a11y_child_count(addr: &str) -> Option<u64> {
    bus_text(&[
        "--address",
        addr,
        "--",
        "get-property",
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
        "org.a11y.atspi.Accessible",
        "ChildCount",
    ])
    .lines()
    .find_map(|l| l.trim().strip_prefix('i')?.trim().parse().ok())
}

fn a11y_app_names(addr: &str, count: u64) -> Vec<String> {
    let mut names = vec![];
    for i in 0..count.min(16) {
        let child = bus_text(&[
            "--address",
            addr,
            "--",
            "call",
            "org.a11y.atspi.Registry",
            "/org/a11y/atspi/accessible/root",
            "org.a11y.atspi.Accessible",
            "GetChildAtIndex",
            "i",
            &i.to_string(),
        ]);
        let dest = child.split('"').nth(1).unwrap_or("").to_string();
        if dest.is_empty() {
            continue;
        }
        let name = bus_text(&[
            "--address",
            addr,
            "--",
            "get-property",
            &dest,
            "/org/a11y/atspi/accessible/root",
            "org.a11y.atspi.Accessible",
            "Name",
        ]);
        if let Some(n) = name
            .lines()
            .filter_map(|l| {
                let s = l.trim().strip_prefix("s \"")?.strip_suffix('"')?;
                Some(s.to_string())
            })
            .next()
        {
            names.push(n);
        }
    }
    names
}

fn test_gw() -> (argus_lib::gateway::Gateway, std::path::PathBuf) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let base = std::env::temp_dir().join(format!("argus-cactdrift-{}", std::process::id()));
    for d in ["skills", "library", "logos"] {
        std::fs::create_dir_all(base.join(d)).unwrap();
    }
    let gw = argus_lib::gateway::Gateway {
        conn: StdMutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: base.join("skills"),
        library_dir: base.join("library"),
        logos_dir: base.join("logos"),
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
    };
    (gw, base)
}

async fn exec(gw: &argus_lib::gateway::Gateway, tool: &str, args: &str) -> Result<String, String> {
    argus_lib::tools::exec(gw, tool, args, "never", false, true, None).await
}

fn read_log(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn tokenless_act_with_no_enumerated_refs_fails_closed() {
    let _guard = serial();

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
        "argus-cactdrift-{}-{}",
        std::process::id(),
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

    let addr = a11y_address();
    assert!(!addr.is_empty(), "a11y bus address must be discoverable");
    let registered = a11y_child_count(&addr).unwrap_or(0);
    let app_names = a11y_app_names(&addr, registered);
    assert!(
        app_names.iter().any(|n| n == APP_BUS_NAME),
        "relay app must be registered on the fixture a11y bus: {app_names:?}"
    );

    let (gw, gw_base) = test_gw();
    let out1 = exec(&gw, "computer.observe", "{}")
        .await
        .expect("observe#1");
    let gen_n = obs_gen(&out1).expect("observe#1 must carry a generation");
    assert!(
        out1.contains("no accessible elements"),
        "production walker currently enumerates zero apps: {out1}"
    );

    std::fs::write(&trigger_path, "swap\n").unwrap();
    let mut acked = false;
    for _ in 0..80 {
        if ack_path.exists() {
            acked = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(acked, "fixture never acknowledged the reorder");

    let out2 = exec(&gw, "computer.observe", "{}")
        .await
        .expect("observe#2");
    let gen_n1 = obs_gen(&out2).expect("observe#2 must carry a generation");
    assert_ne!(
        gen_n, gen_n1,
        "generations must advance across observations"
    );
    assert!(
        read_log(&log_path).is_empty(),
        "mapping must not activate any relay"
    );

    let args = r#"{"ref":0}"#.to_string();
    assert!(
        !args.contains("observation"),
        "this test is token-less by construction"
    );
    let act_out = exec(&gw, "computer.act", &args).await;
    let effects = read_log(&log_path);

    for mut p in procs {
        let _ = p.kill().await;
    }
    for (k, v) in prev {
        match v {
            Some(v) => std::env::set_var(k, v),
            None => std::env::remove_var(k),
        }
    }

    eprintln!("=== TOKEN-LESS ACT PRE-ENUMERATION TRACE ===");
    eprintln!("registered_apps={registered} app_names={app_names:?}");
    eprintln!("initial_generation={gen_n} production_refs=none");
    eprintln!("post_drift_generation={gen_n1} production_refs=none");
    eprintln!("production_action=computer.act args={args} token_omitted=true");
    eprintln!("act_result={act_out:?}");
    eprintln!("side_effect_log={effects:?}");

    let verdict = match &act_out {
        Err(e) if e.contains("unknown ref") && effects.is_empty() => "MASKED-FAIL-CLOSED",
        _ => "UNEXPECTED",
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&gw_base);
    assert_eq!(
        verdict, "MASKED-FAIL-CLOSED",
        "token-less act must fail closed with zero side effects; see trace"
    );
}
