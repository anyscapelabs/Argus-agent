use argus_lib::gateway::Gateway;
use argus_lib::tools::sandbox::{build_run, classify, detect_runtime, trust_of, Origin, Trust};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

fn allow_hosts() -> Vec<String> {
    vec!["github.com".into(), "git.mycorp.dev".into()]
}

#[test]
fn user_owned_origins_are_trusted() {
    for o in [Origin::UserFile, Origin::ProjectDir, Origin::Paste] {
        assert_eq!(classify(&o, &[]), Trust::Trusted);
    }
}

#[test]
fn web_and_clones_default_to_untrusted() {
    let o = Origin::WebFetch("https://random-site.example.com/x".into());
    assert_eq!(trust_of(&o, &allow_hosts()), Trust::Untrusted);

    let o = Origin::GitClone("https://gitlab.com/someone/thing.git".into());
    assert_eq!(trust_of(&o, &allow_hosts()), Trust::Untrusted);
}

#[test]
fn allowlisted_hosts_are_trusted() {
    let o = Origin::WebFetch("https://github.com/you/your-repo/raw/main/x".into());
    assert_eq!(trust_of(&o, &allow_hosts()), Trust::Trusted);

    let o = Origin::GitClone("git@git.mycorp.dev:team/repo.git".into());
    assert_eq!(trust_of(&o, &allow_hosts()), Trust::Trusted);
}

#[test]
fn raw_ips_and_shorteners_are_always_untrusted() {
    let hosts = allow_hosts();

    for url in [
        "http://192.168.1.10/payload",
        "https://bit.ly/abc",
        "https://t.co/x",
        "http://localhost:8080/x",
    ] {
        let o = Origin::WebFetch(url.into());
        assert_eq!(trust_of(&o, &hosts), Trust::Untrusted, "{url}");
    }
}

fn test_gw() -> Gateway {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();

    Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: PathBuf::from("/tmp"),
        library_dir: PathBuf::from("/tmp"),
        logos_dir: PathBuf::from("/tmp"),
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
    }
}

#[test]
fn build_run_constructs_hardened_offline_command() {
    let gw = test_gw();

    let allow = vec!["docker.io/library/alpine:3.20".to_string()];
    {
        let conn = gw.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO kv (k, v) VALUES ('sandbox.allow_images', ?1)
             ON CONFLICT(k) DO UPDATE SET v = ?1",
            rusqlite::params![serde_json::to_string(&allow).unwrap()],
        )
        .unwrap();
    }

    match detect_runtime() {
        None => {
            let err = build_run(
                "docker.io/library/alpine:3.20",
                std::path::Path::new("/tmp/repo"),
                "ls",
                false,
                &allow,
            )
            .unwrap_err();
            assert!(err.contains("podman"));
        }
        Some(_) => {
            let (runtime, args) = build_run(
                "docker.io/library/alpine:3.20",
                std::path::Path::new("/tmp/repo"),
                "make test",
                false,
                &allow,
            )
            .unwrap();

            assert!(runtime == "podman" || runtime == "docker");

            for flag in [
                "--rm",
                "--read-only",
                "--cap-drop=all",
                "--security-opt=no-new-privileges",
                "--pids-limit=256",
                "--memory=2g",
                "--cpus=2",
                "--network=none",
            ] {
                assert!(args.iter().any(|a| a.starts_with(flag)), "missing {flag}");
            }

            assert!(args.iter().any(|a| a.contains("/tmp/repo")));
            assert!(args.iter().any(|a| a == "docker.io/library/alpine:3.20"));
            assert!(args
                .last()
                .map(|c| c.contains("make test"))
                .unwrap_or(false));
        }
    }
}

#[test]
fn build_run_refuses_unlisted_images() {
    let gw = test_gw();

    match detect_runtime() {
        None => return,
        Some(_) => {}
    }

    let err = build_run(
        "random/image:tag",
        std::path::Path::new("/tmp"),
        "ls",
        false,
        &[],
    )
    .unwrap_err();

    assert!(err.contains("allowlist"));
}

#[test]
fn origin_of_tool_spots_untrusted_sources() {
    use argus_lib::tools::sandbox::{origin_of_tool, Origin};

    assert!(matches!(
        origin_of_tool("web.read", r#"{"url":"https://evil.example/x"}"#),
        Some(Origin::WebFetch(_))
    ));
    assert!(matches!(
        origin_of_tool("browser.open", r#"{"url":"https://x.test"}"#),
        Some(Origin::WebFetch(_))
    ));
    assert!(matches!(
        origin_of_tool(
            "terminal",
            r#"{"command":"git clone https://github.com/a/b.git"}"#
        ),
        Some(Origin::GitClone(_))
    ));
    assert!(origin_of_tool("terminal", r#"{"command":"cargo test"}"#).is_none());
    assert!(origin_of_tool("memory.search", r#"{"query":"x"}"#).is_none());
    assert!(origin_of_tool("code.run", r#"{"command":"ls"}"#).is_none());
}

#[test]
fn resolve_image_needs_allowlisted_default() {
    use argus_lib::tools::sandbox::resolve_image;

    let gw = test_gw();
    let conn = gw.conn.lock().unwrap();

    assert!(resolve_image(&conn, None).is_err());

    conn.execute(
        "INSERT INTO kv (k, v) VALUES ('sandbox.allow_images', ?1)
         ON CONFLICT(k) DO UPDATE SET v = ?1",
        rusqlite::params![r#"["docker.io/library/rust:1.84@sha256:abc"]"#],
    )
    .unwrap();

    assert!(resolve_image(&conn, None).is_err(), "no default set");

    conn.execute(
        "INSERT INTO kv (k, v) VALUES ('sandbox.default_image', ?1)
         ON CONFLICT(k) DO UPDATE SET v = ?1",
        rusqlite::params!["docker.io/library/rust:1.84@sha256:abc"],
    )
    .unwrap();

    assert_eq!(
        resolve_image(&conn, None).unwrap(),
        "docker.io/library/rust:1.84@sha256:abc"
    );
    assert!(resolve_image(&conn, Some("evil/image:latest")).is_err());
}

#[test]
fn validate_config_rejects_loose_images() {
    use argus_lib::tools::sandbox::validate_config;

    assert!(validate_config(
        &["github.com".to_string()],
        &["docker.io/library/rust:1.84@sha256:abc".to_string()],
        "docker.io/library/rust:1.84@sha256:abc",
    )
    .is_ok());

    assert!(validate_config(&[], &["rust:latest".to_string()], "",).is_err());
    assert!(validate_config(
        &[],
        &["docker.io/library/rust:1.84@sha256:abc".to_string()],
        "other:1@sha256:x",
    )
    .is_err());
}

#[test]
fn record_block_escapes_and_labels() {
    use argus_lib::tools::sandbox::record_block;

    let blk = record_block("echo <hi>", "offline", "web fetch", "ok", "out & done");
    assert!(blk.contains("<sandbox"));
    assert!(blk.contains("command=\"echo &lt;hi>\""));
    assert!(blk.contains("origin=\"web fetch\""));
    assert!(blk.contains("out &amp; done"));
}
