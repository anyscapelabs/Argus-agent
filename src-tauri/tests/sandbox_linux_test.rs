#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;

use argus_lib::tools::sandbox::backends::{self, Backend};
use argus_lib::tools::sandbox::plan::{Plan, SandboxError};
use argus_lib::tools::sandbox::policy::{
    EnvPolicy, FsAccess, FsPolicy, FsRule, Limits, NetPolicy, Policy, Profile,
};

use tokio::process::Command;

fn sh(script: &str) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd
}

fn scratch(name: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    // Unique per call: tests run in parallel and would otherwise share a dir.
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("argus-sb-{name}-{}-{n}", std::process::id()));

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");

    dir
}

fn base_policy(allowed: &Path, net: NetPolicy, limits: Limits) -> Policy {
    let ro = |p: &str| FsRule {
        path: PathBuf::from(p),
        access: FsAccess::Read,
    };

    Policy {
        profile: Profile::Restricted,
        fs: FsPolicy::Scoped(vec![
            ro("/usr"),
            ro("/bin"),
            ro("/sbin"),
            ro("/lib"),
            ro("/lib64"),
            ro("/etc/ld.so.cache"),
            // /dev/null and friends: a shell cannot even run without them.
            ro("/dev"),
            ro("/proc"),
            ro("/sys"),
            FsRule {
                path: allowed.to_path_buf(),
                access: FsAccess::Write,
            },
        ]),
        net,
        limits,
        env: EnvPolicy::Inherit,
        cwd: allowed.to_path_buf(),
    }
}

fn plan_for(policy: &Policy) -> Plan {
    backends::current().plan(policy).expect("plan")
}

/// One runtime for the whole binary: tokio forbids nesting.
fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt")
}

fn run_sandboxed(plan: &Plan, script: &str) -> Output {
    let plan = plan.clone();

    // spawn() registers the child's pipes with the reactor, so it has to
    // happen inside the runtime too.
    rt().block_on(async move {
        let mut cmd = sh(script);
        let _guard = backends::current().apply(&plan, &mut cmd).expect("apply");

        cmd.output().await.expect("run")
    })
}

fn has(binary: &str) -> bool {
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("command -v {binary}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| has(p))
}

#[test]
fn landlock_denies_writes_outside_the_scope() {
    let allowed = scratch("allow");
    let forbidden = scratch("deny");

    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, Limits::default()));
    let out = run_sandboxed(&plan, &format!("touch {}/x", forbidden.display()));

    assert!(
        !out.status.success(),
        "a write outside the scope must fail: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!forbidden.join("x").exists(), "the file was created anyway");
}

#[test]
fn landlock_denies_reads_outside_the_scope() {
    let allowed = scratch("allow");
    let secret = scratch("secret");
    std::fs::write(secret.join("token"), "s3cret").unwrap();

    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, Limits::default()));
    let out = run_sandboxed(&plan, &format!("cat {}/token", secret.display()));

    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("s3cret"));
}

#[test]
fn landlock_allows_writes_inside_the_scope() {
    let allowed = scratch("allow");

    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, Limits::default()));
    let out = run_sandboxed(&plan, &format!("touch {}/ok", allowed.display()));

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(allowed.join("ok").exists());
}

#[test]
fn network_none_denies_connect() {
    let Some(py) = python() else {
        return;
    };

    let allowed = scratch("allow");
    let plan = plan_for(&base_policy(&allowed, NetPolicy::None, Limits::default()));

    let script = format!(
        "{py} -c \"import socket;socket.setdefaulttimeout(2);\
         socket.create_connection(('1.1.1.1',443))\" 2>&1"
    );

    let out = run_sandboxed(&plan, &script);
    // The script merges stderr into stdout, so the traceback lands there.
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();

    assert!(!out.status.success(), "connect must fail");
    assert!(
        text.contains("permission") || text.contains("denied") || text.contains("not permitted"),
        "expected a policy denial, got: {text}"
    );
}

#[test]
fn seccomp_denies_a_listed_syscall() {
    let Some(py) = python() else {
        return;
    };

    let allowed = scratch("allow");
    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, Limits::default()));

    let script = format!(
        "{py} -c \"import ctypes;l=ctypes.CDLL(None,use_errno=True);\
         r=l.unshare(0);print(r, ctypes.get_errno())\""
    );

    let out = run_sandboxed(&plan, &script);

    // Denied means the syscall returns -1/EPERM, not that python dies: it
    // reports the errno itself and exits 0.
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("-1 1"),
        "expected EPERM, got: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn rlimit_as_bites() {
    let Some(py) = python() else {
        return;
    };

    let allowed = scratch("allow");
    let limits = Limits {
        memory_bytes: Some(128 * 1024 * 1024),
        ..Limits::default()
    };

    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, limits));

    let script = format!("{py} -c \"b=bytearray(600*1024*1024);print('allocated')\" 2>&1");

    let out = run_sandboxed(&plan, &script);

    assert!(!out.status.success(), "a 600 MiB allocation must fail");
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("allocated"),
        "the allocation succeeded"
    );
}

#[test]
fn linux_plan_does_not_claim_a_process_budget() {
    let allowed = scratch("allow");
    let limits = Limits {
        procs: Some(4),
        ..Limits::default()
    };

    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, limits));

    let Plan::Linux(lp) = plan else {
        panic!("expected a linux plan");
    };

    assert_eq!(
        lp.limits.procs, None,
        "RLIMIT_NPROC is per-uid; claiming it here would be a lie"
    );
}

#[test]
fn forking_still_works_under_the_default_limits() {
    let allowed = scratch("allow");
    let limits = Limits {
        procs: Some(64),
        ..Limits::default()
    };

    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, limits));
    let out = run_sandboxed(&plan, "sh -c true");

    assert!(
        out.status.success(),
        "a low procs budget must not starve the session: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn no_new_privs_is_irreversible() {
    let allowed = scratch("allow");
    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, Limits::default()));

    let out = run_sandboxed(&plan, "grep -c NoNewPrivs /proc/self/status");

    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}

#[test]
fn a_foreign_plan_is_refused_not_ignored() {
    let mut cmd = sh("true");

    let err = backends::current()
        .apply(&Plan::None, &mut cmd)
        .unwrap_err();

    assert!(matches!(err, SandboxError::Apply { .. }), "{err}");
}

#[test]
fn a_refused_plan_never_runs_the_command() {
    let marker = std::env::temp_dir().join(format!("argus-sb-ran-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);

    let mut cmd = sh(&format!("touch {}", marker.display()));

    let refused = backends::current().apply(&Plan::None, &mut cmd).is_err();

    assert!(refused, "the backend must refuse the mismatched plan");
    assert!(
        !marker.exists(),
        "the command ran despite the refusal — this is a fail-open"
    );
}

#[test]
fn unsupported_backend_refuses_everything() {
    let be = backends::Unsupported;

    assert!(be.available().is_err());
    assert!(be
        .plan(&base_policy(
            Path::new("/tmp"),
            NetPolicy::None,
            Limits::default()
        ))
        .is_err());

    let mut cmd = sh("true");
    assert!(be.apply(&Plan::None, &mut cmd).is_err());
}

#[test]
fn killpg_tears_down_the_whole_tree() {
    let allowed = scratch("allow");
    let plan = plan_for(&base_policy(&allowed, NetPolicy::Full, Limits::default()));

    let (mut child, pid) = rt().block_on(async {
        let mut cmd = sh("sleep 30 & sleep 30");
        let _guard = backends::current().apply(&plan, &mut cmd).unwrap();
        let child = cmd.spawn().expect("spawn");
        let pid = child.id().expect("pid");

        (child, pid)
    });

    std::thread::sleep(Duration::from_millis(300));

    // The child called setpgid(0,0), so its pgid equals its pid.
    let killed = unsafe { libc::killpg(pid as i32, libc::SIGKILL) };
    assert_eq!(killed, 0, "killpg on the sandboxed group");

    let status = rt().block_on(async {
        let wait = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;

        assert!(wait.is_ok(), "the group did not die within 5s");
        wait.unwrap().expect("wait")
    });

    assert!(!status.success(), "a killed group reports failure");
}
