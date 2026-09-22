use argus_lib::tools::shell::{default_timeout_for, detect, kill_process_group};

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
fn timeout_categories_split_quick_and_long() {
    assert_eq!(default_timeout_for("git status"), 120);
    assert_eq!(default_timeout_for("ls ~/Documents"), 120);
    assert_eq!(default_timeout_for("cargo build --release"), 600);
    assert_eq!(default_timeout_for("bun install"), 600);
    assert_eq!(default_timeout_for("rm -rf /tmp/cache"), 600);
    assert_eq!(default_timeout_for("find / -name '*.log'"), 600);
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

    let out = run_stream(
        &serde_json::json!({"command": "echo hi", "privilege": "admin"}),
        0,
        None,
    )
    .await;

    match out {
        Ok((text, _)) => assert!(text.contains("hi")),
        Err(err) => assert!(!err.contains("do not write sudo")),
    }
}
