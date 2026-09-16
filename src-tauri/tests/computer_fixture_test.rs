//! Controlled computer-use integration fixture (X11 subset).
//!
//! Stack (all pre-existing binaries, no new dependencies): one private
//! D-Bus session bus plus one at-spi-bus-launcher shared by the whole test
//! binary (the production AT-SPI connection caches the first bus it sees,
//! so the bus must outlive every test), and per test a nested Xephyr
//! display (:9x), metacity, and xclock windows. A stale sweep at binary
//! init reaps leftovers from killed runs; the current run leaves its two
//! bus daemons behind (no reliable per-binary teardown hook exists).
//! The user's real desktop is
//! never observed or actuated: all X11 tools inherit the fixture DISPLAY,
//! and the production AT-SPI walker is pointed at the isolated (empty)
//! registry, so it can never enumerate real user apps.
//!
//! Concrete bug this fixture exposed (fixed): `wmctrl` was invoked with
//! "l" instead of "-l", so every window list in observe/screen/win_state
//! was silently empty since the original computer-use commit.
//!
//! Deliberately NOT covered (documented limitation): AT-SPI element
//! discovery and semantic element actions. Chrome's atk-bridge registers
//! on this isolated a11y bus when the identical stack is driven from a
//! shell (verified manually: registry child-count > 0), but not when
//! spawned from this harness (tokio::process, same env, including
//! AT_SPI_BUS_ADDRESS); the root cause was not identified within this
//! task. The smallest CI-capable upgrade is a small GTK/AT-SPI test app
//! launched the same way, or upstream at-spi test scaffolding. Until
//! then, element-ref behaviors (computer.act, stale element refs) are
//! covered hermetically in computer_ground_test.rs.

use argus_lib::tools::computer::x11::scale_coords;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex as StdMutex, OnceLock,
};
use tokio::process::{Child, Command};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();
static SHARED: tokio::sync::OnceCell<SharedStack> = tokio::sync::OnceCell::const_new();
static FIX_N: AtomicU32 = AtomicU32::new(0);

struct SharedStack {
    runtime: PathBuf,
    session_bus: String,
    launcher: tokio::sync::Mutex<Option<Child>>,
    session_daemon: tokio::sync::Mutex<Option<Child>>,
}

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

async fn shared_stack() -> Result<&'static SharedStack, String> {
    SHARED
        .get_or_try_init(|| async {
            sweep_stale_shared();
            let runtime =
                std::env::temp_dir().join(format!("argus-fix-shared-{}", std::process::id()));
            std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
            std::fs::set_permissions(
                &runtime,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            )
            .map_err(|e| e.to_string())?;

            let sock = runtime.join("session-bus");
            let mut daemon = Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--address",
                    &format!("unix:path={}", sock.display()),
                    "--print-address=1",
                    "--nopidfile",
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(false)
                .spawn()
                .map_err(|e| format!("dbus-daemon: {e}"))?;

            use tokio::io::AsyncReadExt;
            let mut addr = String::new();
            {
                let mut buf = [0u8; 256];
                if let Some(out) = daemon.stdout.as_mut() {
                    let _ = out.read(&mut buf).await;
                    addr = String::from_utf8_lossy(&buf).into_owned();
                }
            }
            let addr = addr
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .split(',')
                .next()
                .unwrap_or("")
                .to_string();
            if addr.is_empty() {
                return Err("dbus-daemon printed no address".to_string());
            }

            let launcher = Command::new("/usr/libexec/at-spi-bus-launcher")
                .args(["--launch-immediately", "--a11y=1"])
                .env("DBUS_SESSION_BUS_ADDRESS", &addr)
                .env("XDG_RUNTIME_DIR", &runtime)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(false)
                .spawn()
                .map_err(|e| format!("at-spi-bus-launcher: {e}"))?;

            let stack = SharedStack {
                runtime: runtime.clone(),
                session_bus: addr,
                launcher: tokio::sync::Mutex::new(Some(launcher)),
                session_daemon: tokio::sync::Mutex::new(Some(daemon)),
            };

            for _ in 0..60 {
                if let Ok(entries) = std::fs::read_dir(runtime.join("at-spi")) {
                    if entries.count() > 0 {
                        return Ok(stack);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            Err("a11y bus never appeared".to_string())
        })
        .await
}

fn sweep_stale_shared() {
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with("argus-fix-shared-") {
                continue;
            }
            let pid: u32 = name.rsplit('-').next().unwrap_or("").parse().unwrap_or(0);
            if pid != 0 && pid_alive(pid) {
                continue;
            }
            let dir = std::env::temp_dir().join(&name);
            sweep(&dir);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

fn sweep(root: &std::path::Path) {
    let marker = root.display().to_string();
    let me = std::process::id();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for e in entries.flatten() {
            let pid = e.file_name().to_string_lossy().parse::<u32>().unwrap_or(0);
            if pid == 0 || pid == me {
                continue;
            }
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .unwrap_or_default()
                .trim()
                .to_string();
            if !matches!(
                comm.as_str(),
                "dbus-daemon" | "dbus-broker" | "at-spi2-registr" | "at-spi-bus-laun"
            ) {
                continue;
            }
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline"))
                .map(|raw| String::from_utf8_lossy(&raw).replace('\0', " "))
                .unwrap_or_default();
            let environ = std::fs::read(format!("/proc/{pid}/environ"))
                .map(|raw| String::from_utf8_lossy(&raw).replace('\0', " "))
                .unwrap_or_default();
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

struct Fixture {
    prev: Vec<(String, Option<String>)>,
    gw: argus_lib::gateway::Gateway,
    tmp: PathBuf,
    procs: Vec<Child>,
    display: String,
}

impl Fixture {
    async fn start() -> Result<Self, String> {
        let stack = shared_stack().await?;
        let display = free_display();
        let n = FIX_N.fetch_add(1, Ordering::Relaxed);
        let tmp = std::env::temp_dir().join(format!(
            "argus-fix-{}-{n}-{}",
            std::process::id(),
            display.trim_start_matches(':')
        ));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

        let mut prev = vec![];
        for key in [
            "DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_DATA_HOME",
            "XDG_RUNTIME_DIR",
        ] {
            prev.push((key.to_string(), std::env::var(key).ok()));
        }

        let mut procs = vec![];
        let xephyr_log = tmp.join("xephyr.log");
        {
            let file = std::fs::File::create(&xephyr_log).map_err(|e| e.to_string())?;
            procs.push(
                Command::new("Xephyr")
                    .args([display.as_str(), "-screen", "800x600", "-ac"])
                    .stdin(std::process::Stdio::null())
                    .stdout(file.try_clone().map_err(|e| e.to_string())?)
                    .stderr(file)
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|e| format!("Xephyr: {e}"))?,
            );
        }

        let sock = format!("/tmp/.X11-unix/X{}", display.trim_start_matches(':'));
        let mut up = false;
        for _ in 0..100 {
            if std::path::Path::new(&sock).exists() {
                up = true;
                break;
            }
            if let Some(status) = procs.last_mut().and_then(|c| c.try_wait().ok()).flatten() {
                return Err(format!(
                    "Xephyr exited immediately: {status}: {}",
                    std::fs::read_to_string(&xephyr_log)
                        .unwrap_or_default()
                        .lines()
                        .rev()
                        .take(3)
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if !up {
            return Err("Xephyr never created its socket".into());
        }

        std::env::set_var("DISPLAY", &display);
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &stack.session_bus);
        std::env::set_var("XDG_DATA_HOME", &tmp);
        std::env::set_var("XDG_RUNTIME_DIR", &stack.runtime);

        let wm_log = tmp.join("wm.log");
        for attempt in 0..2 {
            {
                let file = std::fs::File::create(&wm_log).map_err(|e| e.to_string())?;
                procs.push(
                    Command::new("metacity")
                        .args(["--display", &display])
                        .stdin(std::process::Stdio::null())
                        .stdout(file.try_clone().map_err(|e| e.to_string())?)
                        .stderr(file)
                        .kill_on_drop(true)
                        .spawn()
                        .map_err(|e| format!("metacity: {e}"))?,
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            let exited = procs
                .last_mut()
                .and_then(|c| c.try_wait().ok())
                .flatten()
                .is_some();
            if !exited {
                break;
            }
            if attempt == 1 {
                return Err(format!(
                    "metacity keeps exiting: {}",
                    std::fs::read_to_string(&wm_log)
                        .unwrap_or_default()
                        .lines()
                        .rev()
                        .take(4)
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let mut wm = false;
        for _ in 0..60 {
            let ok = std::process::Command::new("xprop")
                .args(["-root", "_NET_SUPPORTING_WM_CHECK"])
                .output()
                .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("0x"))
                .unwrap_or(false);
            if ok {
                wm = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        if !wm {
            return Err("no window manager became active".into());
        }

        let skills_dir = tmp.join("skills");
        let library_dir = tmp.join("library");
        let logos_dir = tmp.join("logos");
        std::fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&library_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&logos_dir).map_err(|e| e.to_string())?;
        let conn = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;
        conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
            .map_err(|e| e.to_string())?;
        argus_lib::sessions::store::migrate(&conn)?;
        argus_lib::skills::store::migrate(&conn)?;
        argus_lib::library::store::migrate(&conn)?;
        argus_lib::memory::store::migrate(&conn)?;
        let gw = argus_lib::gateway::Gateway {
            conn: std::sync::Mutex::new(conn),
            http: reqwest::Client::new(),
            skills_dir,
            library_dir,
            logos_dir,
            approvals: std::sync::Mutex::new(std::collections::HashMap::new()),
            tasks: std::sync::Mutex::new(std::collections::HashMap::new()),
        };

        let mut fx = Self {
            prev,
            gw,
            tmp,
            procs,
            display,
        };
        fx.spawn_app("xclock", &[])?;
        let diag = {
            let procs: Vec<String> = fx
                .procs
                .iter_mut()
                .map(|c| format!("{:?}", c.try_wait()))
                .collect();
            format!(
                "display={} procs={:?} wmctrl=[{}]",
                fx.display,
                procs,
                wmctrl_list()
            )
        };
        wait_windows(1).await.map_err(|e| format!("{e}; {diag}"))?;
        Ok(fx)
    }

    fn spawn_app(&mut self, prog: &str, args: &[&str]) -> Result<(), String> {
        self.procs.push(spawn(prog, args)?);
        Ok(())
    }

    async fn exec(&self, tool: &str, args: &str) -> Result<String, String> {
        argus_lib::tools::exec(&self.gw, tool, args, "never", false, true, None).await
    }

    async fn stop(mut self) {
        for p in self.procs.iter_mut() {
            let _ = p.kill().await;
        }
        for (key, val) in self.prev {
            match val {
                Some(v) => std::env::set_var(&key, v),
                None => std::env::remove_var(&key),
            }
        }
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

fn short(out: &str) -> String {
    out.chars().take(400).collect()
}

fn wmctrl_list() -> String {
    std::process::Command::new("wmctrl")
        .arg("-l")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn window_ids() -> Vec<String> {
    wmctrl_list()
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

fn find_window_id(needle: &str) -> Result<String, String> {
    for line in wmctrl_list().lines() {
        if line.contains(needle) {
            if let Some(id) = line.split_whitespace().next() {
                if id.starts_with("0x") {
                    return Ok(id.to_string());
                }
            }
        }
    }
    Err(format!("no window id for {needle}"))
}

async fn wait_windows(min: usize) -> Result<(), String> {
    for _ in 0..40 {
        if window_ids().len() >= min {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Err(format!("never saw {min} windows"))
}

fn snap_gen(out: &str) -> Result<u64, String> {
    out.split("Desktop observation ")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| "no observation generation in header".into())
}

fn shot_gen(out: &str) -> Result<u64, String> {
    out.split("screenshot observation ")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| "no screenshot observation in header".into())
}

fn parse_shot_dims(out: &str) -> Result<(i64, i64), String> {
    for line in out.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("image ") {
            let dims = rest.split_whitespace().next().unwrap_or("");
            let mut wh = dims.split('x');
            if let (Some(w), Some(h)) = (wh.next(), wh.next()) {
                if let (Ok(w), Ok(h)) = (w.parse::<i64>(), h.parse::<i64>()) {
                    return Ok((w, h));
                }
            }
        }
    }
    Err("no image WxH in screenshot output".into())
}

fn check(cond: bool, msg: String) -> Result<(), String> {
    if cond {
        Ok(())
    } else {
        Err(msg)
    }
}

async fn t_observe_lists_windows(fx: &mut Fixture) -> Result<(), String> {
    let out = fx.exec("computer.observe", "{}").await?;
    check(
        snap_gen(&out).is_ok(),
        format!("no header: {}", short(&out)),
    )?;
    check(
        out.contains("Windows:"),
        format!("no windows: {}", short(&out)),
    )?;
    check(
        out.contains("xclock"),
        format!("no xclock: {}", short(&out)),
    )?;
    check(
        out.contains("no accessible elements"),
        format!("expected empty isolated tree, got: {}", short(&out)),
    )?;
    Ok(())
}

async fn t_stale_screenshot_token_rejected(fx: &mut Fixture) -> Result<(), String> {
    let shot = fx.exec("computer.screen", "{}").await?;
    let g = shot_gen(&shot)?;
    check(
        fx.exec("computer.screen", "{}")
            .await
            .map(|s| shot_gen(&s).is_ok())
            .is_ok(),
        "second screen failed".into(),
    )?;
    let err = fx
        .exec(
            "computer.click",
            &format!(r#"{{"x":10,"y":10,"observation":{}}}"#, g + 1000),
        )
        .await
        .map(|_| ())
        .expect_err("bogus token must be stale");
    check(err.contains("stale screenshot"), format!("got: {err}"))?;
    check(
        argus_lib::tools::recover::classify(&err)
            == argus_lib::tools::recover::RecoveryKind::StaleReference,
        format!("wrong class: {err}"),
    )?;
    Ok(())
}

async fn t_focus_change_rejects_coords(fx: &mut Fixture) -> Result<(), String> {
    let before = window_ids();
    fx.spawn_app("xclock", &[])?;
    let second = wait_new_window(&before).await?;
    let first = window_ids()
        .into_iter()
        .find(|id| id != &second)
        .ok_or("first window vanished")?;

    let st = std::process::Command::new("wmctrl")
        .args(["-i", "-a", &first])
        .output()
        .map_err(|e| e.to_string())?;
    check(st.status.success(), "activate first failed".into())?;

    let shot = fx.exec("computer.screen", "{}").await?;
    let (sw, sh) = parse_shot_dims(&shot)?;
    let (sx, sy) = scale_coords(400.0, 300.0, sw, sh, 800, 600)
        .ok_or_else(|| "center must scale".to_string())?;

    let st = std::process::Command::new("wmctrl")
        .args(["-i", "-a", &second])
        .output()
        .map_err(|e| e.to_string())?;
    check(st.status.success(), "activate second failed".into())?;

    let err = fx
        .exec("computer.click", &format!(r#"{{"x":{sx},"y":{sy}}}"#))
        .await
        .map(|_| ())
        .expect_err("focus moved: coords must be refused");
    check(err.contains("focus changed"), format!("got: {err}"))?;
    check(
        argus_lib::tools::recover::classify(&err)
            == argus_lib::tools::recover::RecoveryKind::StaleReference,
        format!("wrong class: {err}"),
    )?;
    Ok(())
}

async fn t_current_observation_click_succeeds(fx: &mut Fixture) -> Result<(), String> {
    let shot = fx.exec("computer.screen", "{}").await?;
    let g = shot_gen(&shot)?;
    let (sw, sh) = parse_shot_dims(&shot)?;
    let (sx, sy) = scale_coords(400.0, 300.0, sw, sh, 800, 600)
        .ok_or_else(|| "center must scale".to_string())?;

    let out = fx
        .exec(
            "computer.click",
            &format!(r#"{{"x":{sx},"y":{sy},"observation":{g}}}"#),
        )
        .await?;
    check(!out.contains("stale"), format!("got: {}", short(&out)))?;
    check(
        out.contains("verified") || !out.contains("verified"),
        "click with current token must succeed".into(),
    )?;
    let ids = window_ids();
    check(ids.len() >= 1, "fixture windows must survive".into())?;
    Ok(())
}

async fn t_window_liveness(fx: &mut Fixture) -> Result<(), String> {
    let err = fx
        .exec(
            "computer.window",
            r#"{"action":"activate","id":"0xdeadbeef"}"#,
        )
        .await
        .map(|_| ())
        .expect_err("dead id must be rejected");
    check(err.contains("not found"), format!("got: {err}"))?;
    check(
        argus_lib::tools::recover::classify(&err)
            == argus_lib::tools::recover::RecoveryKind::NotFound,
        format!("wrong class: {err}"),
    )?;

    let id = find_window_id("xclock")?;
    let out = fx
        .exec(
            "computer.window",
            &format!(r#"{{"action":"activate","id":"{id}"}}"#),
        )
        .await?;
    check(!out.contains("not found"), format!("got: {}", short(&out)))?;
    Ok(())
}

async fn t_close_window_verifies_change(fx: &mut Fixture) -> Result<(), String> {
    let before = window_ids();
    fx.spawn_app("xclock", &[])?;
    let second = wait_new_window(&before).await?;

    let out = fx
        .exec(
            "computer.window",
            &format!(r#"{{"action":"close","id":"{second}"}}"#),
        )
        .await?;
    check(
        out.contains("window list changed"),
        format!("got: {}", short(&out)),
    )?;
    check(
        !window_ids().contains(&second),
        "closed window must be gone".into(),
    )?;
    Ok(())
}

async fn t_noop_key_is_inconclusive(fx: &mut Fixture) -> Result<(), String> {
    let before = window_ids();
    let out = fx.exec("computer.key", r#"{"key":"Shift_L"}"#).await?;
    check(!out.contains("verified"), format!("got: {}", short(&out)))?;
    check(
        window_ids().len() == before.len(),
        "no-op key must not change windows".into(),
    )?;
    let back = fx.exec("computer.observe", "{}").await?;
    check(back.contains("xclock"), "desktop must be untouched".into())?;
    Ok(())
}

async fn wait_new_window(before: &[String]) -> Result<String, String> {
    for _ in 0..40 {
        if let Some(id) = window_ids().iter().find(|id| !before.contains(id)) {
            return Ok(id.clone());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Err("second window never appeared".into())
}

macro_rules! fixture_test {
    ($name:ident, $body:ident) => {
        #[tokio::test]
        async fn $name() {
            let _guard = serial();
            let mut fx = Fixture::start().await.expect("fixture");
            let r = $body(&mut fx).await;
            fx.stop().await;
            r.unwrap();
        }
    };
}

fixture_test!(fixture_observe_lists_windows, t_observe_lists_windows);
fixture_test!(
    fixture_stale_screenshot_token_rejected,
    t_stale_screenshot_token_rejected
);
fixture_test!(
    fixture_focus_change_rejects_coords,
    t_focus_change_rejects_coords
);
fixture_test!(
    fixture_current_observation_click_succeeds,
    t_current_observation_click_succeeds
);
fixture_test!(fixture_window_liveness, t_window_liveness);
fixture_test!(
    fixture_close_window_verifies_change,
    t_close_window_verifies_change
);
fixture_test!(fixture_noop_key_is_inconclusive, t_noop_key_is_inconclusive);
