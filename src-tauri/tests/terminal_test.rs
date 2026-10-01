use argus_lib::tools::shell::{
    auth_agent_hint, default_timeout_for, detect, kill_process_group, timeout_from,
};
use std::time::Duration;

#[cfg(unix)]
#[tokio::test]
async fn detect_finds_runnable_shell() {
    let cfg = detect::status();
    assert!(cfg.binary.is_file(), "no shell binary: {cfg:?}");

    let out = std::process::Command::new(&cfg.binary)
        .arg("-c")
        .arg("echo ok")
        .output()
        .expect("spawn detected shell");

    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
}

#[test]
fn the_default_timeout_is_one_number_and_the_agent_owns_the_rest() {
    // No substring table. It read `git status` and `git clone` the same way,
    // so a real clone got cut off at the two-minute mark while `pip install`
    // got ten. The agent sets what it needs; this is only where it starts.
    assert_eq!(default_timeout_for(), 120);
}

#[test]
fn a_timeout_the_agent_sets_is_the_one_that_runs() {
    let clone = serde_json::json!({"command": "git clone x", "timeout": 900});
    assert_eq!(timeout_from(&clone), Duration::from_secs(900));

    let none = serde_json::json!({"command": "git clone x"});
    assert_eq!(timeout_from(&none), Duration::from_secs(120));

    // Clamped at both ends, so neither a stray zero nor an optimistic number
    // turns into a one-second command or an hour-long turn.
    let over = serde_json::json!({"command": "sleep 1", "timeout": 999_999});
    assert_eq!(timeout_from(&over), Duration::from_secs(1800));

    let under = serde_json::json!({"command": "sleep 1", "timeout": 1});
    assert_eq!(timeout_from(&under), Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn group_kill_reaps_backgrounded_sleep() {
    use std::os::unix::process::CommandExt;

    let mut child = std::process::Command::new("bash");
    child.arg("-c").arg("sleep 30 & wait");

    unsafe {
        child.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let mut child = child.spawn().expect("spawn sleep tree");
    let pid = child.id();

    std::thread::sleep(std::time::Duration::from_millis(300));
    kill_process_group(pid).expect("killpg");

    let start = std::time::Instant::now();

    loop {
        if child.try_wait().expect("try_wait").is_some() {
            break; // Sorted
        }

        assert!(
            start.elapsed().as_secs() < 5,
            "group kill did not reap the tree"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_kills_the_whole_tree() {
    let start = std::time::Instant::now();
    let args = serde_json::json!({"command": "sleep 30 & wait", "timeout": 10});

    let (out, code) = argus_lib::tools::shell::run_stream(&args, 0, None)
        .await
        .expect("run_stream");

    assert_eq!(code, -1);
    assert!(out.contains("timed out after 10s"), "got: {out}");
    // The message has to name the way out. A bare "timed out" reads as a
    // failure, and the turn ends there instead of the agent running it again
    // with a bigger number.
    assert!(out.contains("larger timeout"), "no way out in: {out}");
    assert!(out.contains("1800"), "no ceiling in: {out}");
    assert!(
        start.elapsed().as_secs() < 20,
        "timeout path hung past its cap"
    );
}

#[test]
fn trash_delete_clears_dir_off_the_hot_path() {
    let root = std::env::temp_dir().join(format!("argus-trash-{}", std::process::id()));
    let dir = root.join("big");

    std::fs::create_dir_all(dir.join("nested")).expect("seed dirs");
    std::fs::write(dir.join("nested").join("f.txt"), "x".repeat(1024)).expect("seed file");

    argus_lib::tools::fs::remove_dir_fast(&dir).expect("trash");

    assert!(!dir.exists(), "dir still on the hot path");

    let start = std::time::Instant::now();

    loop {
        let left = std::fs::read_dir(&root)
            .expect("read root")
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".trash-"));

        if !left {
            break; // Sorted
        }

        assert!(start.elapsed().as_secs() < 10, "background delete stalled");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn shell_override_roundtrips() {
    let auto = detect::status().binary;

    detect::set_override(auto.to_str().expect("utf8 path")).expect("set override");
    assert!(detect::override_active());

    detect::clear_override();
    assert!(!detect::override_active());

    assert!(detect::set_override("/no/such/shell-xyz").is_err());
}

#[tokio::test]
async fn sudo_commands_fail_fast_without_hanging() {
    use argus_lib::tools::shell::{needs_elevation, run_stream};

    assert!(needs_elevation("sudo apt install x"));
    assert!(needs_elevation("  su -c whoami"));
    assert!(needs_elevation("doas reboot"));
    assert!(!needs_elevation("echo hello"));
    assert!(!needs_elevation("echo sudo apt"));

    let err = run_stream(&serde_json::json!({"command": "sudo whoami"}), 0, None)
        .await
        .unwrap_err();
    assert!(err.contains("elevated privileges"));
}

#[tokio::test]
async fn admin_privilege_bypasses_sudo_refusal() {
    use argus_lib::tools::shell::run_stream;

    // Headless machines have no polkit agent, so pkexec hangs until killed
    // instead of failing fast. Bound the wait, then read the outcome: "hi"
    // means the dialog worked, a timeout means there was nobody to show it
    // to — environmental, not a code failure.
    let out = run_stream(
        &serde_json::json!({"command": "echo hi", "privilege": "admin", "timeout": 20}),
        0,
        None,
    )
    .await;

    match out {
        Ok((text, _)) if text.contains("hi") => {}
        Ok((text, _)) if text.contains("timed out") => return,
        Ok((text, _)) => panic!("unexpected admin output: {text}"),
        Err(err) => assert!(!err.contains("do not write sudo"), "{err}"),
    }
}

// pkexec says "Request dismissed" both when the user said no and when there
// was nobody to show the prompt to. The second is a broken machine, not a
// refusal, and the model that reads it as a refusal retries forever.
#[test]
fn a_pkexec_dismissal_names_the_missing_agent() {
    let hint = auth_agent_hint("Error executing command as another user: Request dismissed")
        .expect("the pkexec shape must be recognised");

    assert!(hint.contains("policykit-1-gnome"), "{hint}");
    assert!(
        hint.contains("retrying will not help"),
        "the point is to stop the retry loop: {hint}"
    );
}

#[test]
fn a_command_that_merely_mentions_dismissal_is_left_alone() {
    // The hint is only for a failed elevated run, and only for pkexec's own
    // wording. A normal command that mentions authorization is not a broken
    // polkit and must not be rewritten into one.
    assert!(auth_agent_hint("apt: not authorized to install, check your sources.list").is_none());
    assert!(auth_agent_hint("Everything installed, done.").is_none());
    assert!(auth_agent_hint("").is_none());
    assert!(auth_agent_hint("command not found").is_none());
}
