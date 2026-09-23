use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use argus_lib::gateway::Gateway;
use argus_lib::tools::sandbox::backends::{linux, macos, windows};
use argus_lib::tools::sandbox::plan::{JobPlan, Plan, SandboxError, SeccompProfile};
use argus_lib::tools::sandbox::policy::{
    self, EnvPolicy, FsAccess, FsPolicy, FsRule, NetPolicy, PolicyCtx, Profile,
};
use argus_lib::tools::sandbox::{
    classify, origin_of_tool, parse_profile, record_block, trust_of, Origin, Trust,
};

use rusqlite::Connection;

fn allow_hosts() -> Vec<String> {
    vec!["github.com".into(), "git.mycorp.dev".into()]
}

fn ctx() -> (PathBuf, PathBuf, PathBuf) {
    (
        PathBuf::from("/home/dev/proj"),
        PathBuf::from("/home/dev/.argus/sandbox"),
        PathBuf::from("/home/dev"),
    )
}

fn resolved(p: Profile) -> argus_lib::tools::sandbox::Policy {
    let (project, tmp, home) = ctx();

    policy::resolve(
        p,
        &PolicyCtx {
            project: &project,
            tmp: &tmp,
            home: &home,
        },
    )
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

#[test]
fn origin_of_tool_spots_untrusted_sources() {
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
fn record_block_escapes_and_labels() {
    let blk = record_block("echo <hi>", "restricted", "web fetch", "ok", "out & done");
    assert!(blk.contains("<sandbox"));
    assert!(blk.contains("command=\"echo &lt;hi>\""));
    assert!(blk.contains("profile=\"restricted\""));
    assert!(blk.contains("origin=\"web fetch\""));
    assert!(blk.contains("out &amp; done"));
}

#[test]
fn profile_parses_and_rejects() {
    assert_eq!(Profile::parse("restricted"), Some(Profile::Restricted));
    assert_eq!(Profile::parse(" Project "), Some(Profile::Project));
    assert_eq!(Profile::parse("HOST"), Some(Profile::Host));
    assert_eq!(Profile::parse("none"), None);

    assert!(Profile::Restricted.is_isolated());
    assert!(Profile::Project.is_isolated());
    assert!(!Profile::Host.is_isolated());
}

#[test]
fn parse_profile_defaults_and_refuses_unknown() {
    let missing = serde_json::json!({});
    assert_eq!(
        parse_profile(&missing, Profile::Host).unwrap(),
        Profile::Host
    );

    let blank = serde_json::json!({ "profile": "  " });
    assert_eq!(parse_profile(&blank, Profile::Host).unwrap(), Profile::Host);

    let good = serde_json::json!({ "profile": "project" });
    assert_eq!(
        parse_profile(&good, Profile::Host).unwrap(),
        Profile::Project
    );

    let bad = serde_json::json!({ "profile": "chroot" });
    assert!(matches!(
        parse_profile(&bad, Profile::Host),
        Err(SandboxError::Profile(_))
    ));
}

#[test]
fn host_profile_is_unrestricted() {
    let p = resolved(Profile::Host);

    assert_eq!(p.fs, FsPolicy::Unrestricted);
    assert_eq!(p.net, NetPolicy::Full);
    assert_eq!(p.env, EnvPolicy::Inherit);
    assert_eq!(p.limits.memory_bytes, None);
    assert_eq!(p.limits.wall_secs, None);
    assert!(p.limits.output_bytes.is_some(), "output stays bounded");
}

#[test]
fn restricted_profile_denies_network_and_scopes_writes() {
    let p = resolved(Profile::Restricted);
    let FsPolicy::Scoped(rules) = &p.fs else {
        panic!("restricted must be scoped");
    };

    assert_eq!(p.net, NetPolicy::None);
    assert!(p.limits.wall_secs.is_some());
    assert!(p.limits.memory_bytes.is_some());
    assert!(p.limits.procs.is_some());

    let writes: Vec<&Path> = rules
        .iter()
        .filter(|r| r.access == FsAccess::Write)
        .map(|r| r.path.as_path())
        .collect();

    assert_eq!(writes, vec![Path::new("/home/dev/.argus/sandbox")]);
    assert!(
        rules.iter().all(|r| r.path != Path::new("/home/dev/proj")),
        "untrusted code must not reach the project"
    );
    assert!(rules.iter().any(|r| r.path == Path::new("/usr")));
}

#[test]
fn project_profile_grants_the_project_and_readonly_caches() {
    let p = resolved(Profile::Project);
    let FsPolicy::Scoped(rules) = &p.fs else {
        panic!("project must be scoped");
    };

    assert_eq!(p.net, NetPolicy::Ports(vec![80, 443]));
    assert_eq!(p.env, EnvPolicy::Inherit);

    let access = |path: &str| -> Option<FsAccess> {
        rules
            .iter()
            .find(|r| r.path == Path::new(path))
            .map(|r| r.access)
    };

    assert_eq!(access("/home/dev/proj"), Some(FsAccess::Write));
    assert_eq!(access("/home/dev/.cache"), Some(FsAccess::Write));
    assert_eq!(access("/home/dev/.cargo"), Some(FsAccess::Read));
    assert_eq!(access("/etc/passwd"), None, "not explicitly allowed");
}

#[test]
fn macos_sbpl_denies_by_default() {
    let s = macos::sbpl(&resolved(Profile::Restricted)).unwrap();

    assert!(s.contains("(deny default)"));
    assert!(s.contains("(deny network*)"));
    assert!(s.contains(r#"(subpath "/usr")"#));
    assert!(s.contains(r#"(subpath "/home/dev/.argus/sandbox")"#));
    assert!(!s.contains("/home/dev/proj"));
}

#[test]
fn macos_sbpl_allows_allowlisted_ports_only() {
    let s = macos::sbpl(&resolved(Profile::Project)).unwrap();

    assert!(s.contains("(deny network*)"));
    assert!(s.contains(r#"(allow network-outbound (remote tcp "*:80"))"#));
    assert!(s.contains(r#"(allow network-outbound (remote tcp "*:443"))"#));
    assert!(!s.contains("(allow network*)"));
}

#[test]
fn macos_sbpl_host_is_unrestricted() {
    let host = resolved(Profile::Host);
    assert!(matches!(macos::macos_plan(&host), Ok(Plan::None)));
}

#[test]
fn macos_quoting_cannot_escape_the_profile() {
    let project = PathBuf::from("/tmp/we\"ird)\\path");
    let tmp = PathBuf::from("/tmp/t");

    let p = policy::resolve(
        Profile::Project,
        &PolicyCtx {
            project: &project,
            tmp: &tmp,
            home: Path::new("/tmp/h"),
        },
    );

    let s = macos::sbpl(&p).unwrap();

    assert!(
        s.contains(r#"(subpath "/tmp/we\"ird)\\path")"#),
        "quote must be escaped: {s}"
    );
    assert!(
        !s.contains("/tmp/we\"ird"),
        "an unescaped quote closes the string"
    );
}

#[test]
fn macos_newlines_do_not_break_the_profile() {
    let project = PathBuf::from("/tmp/a\n(allow file-write* (subpath \"/\"))");
    let tmp = PathBuf::from("/tmp/t");

    let p = policy::resolve(
        Profile::Project,
        &PolicyCtx {
            project: &project,
            tmp: &tmp,
            home: Path::new("/tmp/h"),
        },
    );

    let s = macos::sbpl(&p).unwrap();

    for line in s.lines() {
        assert_ne!(
            line.trim(),
            r#"(allow file-write* (subpath "/"))"#,
            "a newline in the path injected a rule"
        );
    }
}

#[test]
fn macos_wrap_puts_sandbox_exec_first() {
    let plan = macos::macos_plan(&resolved(Profile::Restricted)).unwrap();
    let argv = macos::wrap(&plan, "/bin/bash", "echo hi").unwrap();

    assert_eq!(argv[0], "/usr/bin/sandbox-exec");
    assert_eq!(argv[1], "-p");
    assert!(argv[2].contains("(deny default)"));
    assert_eq!(argv[3], "/bin/bash");
    assert_eq!(argv[4], "-c");
    assert_eq!(argv[5], "echo hi");
}

#[test]
fn windows_job_plan_matches_the_profile() {
    let restricted = windows::job_plan(&resolved(Profile::Restricted)).unwrap();

    let Plan::Windows(r) = &restricted else {
        panic!("expected a job plan");
    };

    assert!(r.app_container);
    assert!(r.capabilities.is_empty(), "no network capability");
    assert!(!r.net_coarse);
    assert!(r.memory_bytes.is_some());
    assert!(r.max_procs.is_some());
    assert!(!r.fs_grants.is_empty());

    let project = windows::job_plan(&resolved(Profile::Project)).unwrap();

    let Plan::Windows(p) = &project else {
        panic!("expected a job plan");
    };

    assert!(p.net_coarse, "a port list degrades to a coarse capability");
    assert_eq!(p.capabilities.len(), 1);
    assert!(matches!(
        windows::job_plan(&resolved(Profile::Host)).unwrap(),
        Plan::None
    ));
}

#[test]
fn windows_read_and_write_grants_cover_both_bits() {
    let read = windows::grant_mask(FsAccess::Read);
    let write = windows::grant_mask(FsAccess::Write);

    assert_eq!(read, windows::READ_MASK);
    assert_eq!(write, windows::READ_MASK | windows::WRITE_MASK);
    assert_ne!(
        read, write,
        "a write scope that omits read breaks every build that reads its own sources"
    );
}

#[test]
fn windows_merges_two_rules_on_one_path_into_one_grant() {
    let path = PathBuf::from("/p");

    let jp = JobPlan {
        memory_bytes: None,
        max_procs: None,
        cpu_secs: None,
        app_container: true,
        capabilities: Vec::new(),
        fs_grants: vec![
            FsRule {
                path: path.clone(),
                access: FsAccess::Read,
            },
            FsRule {
                path: path.clone(),
                access: FsAccess::Write,
            },
        ],
        net_coarse: false,
    };

    // A second SetEntriesInAclW grant for the same trustee replaces the first,
    // so two ACL round-trips would leave the path read-only.
    let grants = windows::grant_list(&jp);

    assert_eq!(grants.len(), 1, "{grants:?}");
    assert_eq!(grants[0].0, path);
    assert_eq!(grants[0].1, windows::READ_MASK | windows::WRITE_MASK);
}

#[test]
fn windows_canary_rejects_a_boundary_that_ignores_us() {
    // The tightening changed nothing: AppContainer is not enforcing.
    let err = windows::canary_verdict(true, false, true).unwrap_err();

    assert!(matches!(err, SandboxError::Unavailable { .. }));
    assert!(err.to_string().contains("no ACE granting it"), "{err}");
}

#[test]
fn windows_canary_reports_its_own_bugs_as_such() {
    // Too tight to read what it was given: our bug, not the OS.
    let err = windows::canary_verdict(true, true, false).unwrap_err();
    assert!(err.to_string().contains("explicitly granted"), "{err}");

    // A broken control run makes the whole verdict meaningless.
    let err = windows::canary_verdict(false, true, false).unwrap_err();
    assert!(err.to_string().contains("control run failed"), "{err}");

    let err = windows::canary_verdict(false, false, true).unwrap_err();
    assert!(err.to_string().contains("control run failed"), "{err}");
}

#[test]
fn windows_canary_accepts_only_a_path_denial_under_a_working_grant() {
    let probe = windows::canary_verdict(true, false, false).unwrap();
    assert!(probe.enforcing);
    assert_eq!(probe.backend, "windows");
    assert!(probe.detail.contains(windows::CANARY_TARGET), "{probe:?}");
}

#[test]
fn linux_plan_denies_syscalls_only_for_restricted() {
    let Plan::Linux(r) = linux::linux_plan(&resolved(Profile::Restricted)).unwrap() else {
        panic!("expected a linux plan");
    };

    assert!(matches!(r.seccomp, SeccompProfile::Deny(_)));
    assert_eq!(r.net, NetPolicy::None);
    assert!(!r.fs.is_empty());

    let Plan::Linux(p) = linux::linux_plan(&resolved(Profile::Project)).unwrap() else {
        panic!("expected a linux plan");
    };

    assert_eq!(p.seccomp, SeccompProfile::Off);
    assert_eq!(p.net, NetPolicy::Ports(vec![80, 443]));
}

#[test]
fn sandbox_errors_are_fail_closed_and_not_retryable() {
    let unsupported = SandboxError::Unsupported {
        backend: "linux",
        what: "no network access".into(),
    };

    assert_eq!(unsupported.backend(), "linux");
    assert!(!unsupported.retryable());
    assert!(unsupported.to_string().contains("refusing to run"));

    let unavailable = SandboxError::Unavailable {
        backend: "macos",
        why: "has no /usr/bin/sandbox-exec".into(),
    };

    assert!(!unavailable.retryable());
    assert!(unavailable.to_string().contains("refusing to run"));

    assert!(!SandboxError::Apply {
        backend: "linux",
        why: "setrlimit failed".into()
    }
    .retryable());

    assert!(SandboxError::Timeout(5).retryable());
    assert!(!SandboxError::Cancelled.retryable());
}

#[test]
fn sandbox_refusals_are_not_retried_by_recovery() {
    use argus_lib::tools::recover::{classify as recover_classify, RecoveryKind};

    let refusal = SandboxError::Unsupported {
        backend: "linux",
        what: "no network access".into(),
    };

    assert_eq!(
        recover_classify(&refusal.to_string()),
        RecoveryKind::PermissionDenied
    );
    assert_eq!(
        recover_classify(&SandboxError::Profile("chroot".into()).to_string()),
        RecoveryKind::PermissionDenied
    );
    assert!(!RecoveryKind::PermissionDenied.may_retry());
}

fn test_gw() -> Gateway {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::tools::sandbox::schema::migrate(&conn).unwrap();

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
fn config_round_trips_and_defaults_fail_closed() {
    use argus_lib::tools::sandbox::{config, set_config, SandboxConfig};

    let gw = test_gw();

    let def = config(&gw).unwrap();
    assert_eq!(def.default_profile, Profile::Restricted);
    assert!(def.hosts.is_empty());
    assert_eq!(def.net_allow, vec![80, 443]);

    let cfg = SandboxConfig {
        hosts: vec!["github.com".into()],
        default_profile: Profile::Project,
        net_allow: vec![443],
    };

    set_config(&gw, &cfg).unwrap();

    let back = config(&gw).unwrap();
    assert_eq!(back.hosts, vec!["github.com".to_string()]);
    assert_eq!(back.default_profile, Profile::Project);
    assert_eq!(back.net_allow, vec![443]);
}

#[test]
fn canary_profile_grants_the_target_only_when_asked() {
    let tight = macos::canary_profile(false);
    let loose = macos::canary_profile(true);

    assert!(tight.starts_with("(version 1)"));
    assert!(tight.contains("(deny default)"));
    assert!(!tight.contains(macos::CANARY_TARGET));
    assert!(loose.contains(macos::CANARY_TARGET));

    // The system paths must survive in both, or a denial proves nothing.
    for p in macos::CANARY_SYS {
        assert!(tight.contains(p), "{p} missing from the canary profile");
    }
}

#[test]
fn a_canary_that_denies_by_blanket_is_not_accepted_as_enforcement() {
    // The permissive profile failed, so the later denial would be meaningless.
    let err = macos::verdict(true, true, false).unwrap_err();
    assert!(err.to_string().contains("too strict"), "{err}");

    // The tightening changed nothing: seatbelt is not enforcing.
    let err = macos::verdict(true, false, true).unwrap_err();
    assert!(err.to_string().contains("not enforcing"), "{err}");
}

#[test]
fn a_canary_denies_the_target_while_still_running_it() {
    let probe = macos::verdict(true, false, false).unwrap();
    assert!(probe.enforcing);
    assert_eq!(probe.backend, "macos");
    assert!(probe.detail.contains(macos::CANARY_TARGET));
}

#[test]
fn a_broken_control_makes_the_canary_inconclusive() {
    let err = macos::verdict(false, false, false).unwrap_err();
    assert!(err.to_string().contains("cannot tell enforcement"), "{err}");

    // Even a denial is meaningless when the control never worked.
    let err = macos::verdict(false, false, true).unwrap_err();
    assert!(err.to_string().contains("cannot tell enforcement"), "{err}");
}

#[test]
fn a_plain_argument_is_not_quoted() {
    for a in ["ls", "-la", "/usr/bin/git", "a|b", "x&y"] {
        assert_eq!(windows::quote::quote_arg(a), a, "{a} was quoted needlessly");
    }
}

#[test]
fn an_argument_with_spaces_survives_the_round_trip() {
    let q = windows::quote::quote_arg("C:\\Program Files\\thing.exe");

    assert!(q.starts_with('"') && q.ends_with('"'), "{q}");
    assert!(q.contains("Program Files\\thing.exe"));
}

#[test]
fn a_trailing_backslash_does_not_swallow_the_closing_quote() {
    // Only matters once quoting starts; a bare run needs no quotes.
    assert_eq!(windows::quote::quote_arg("C:\\dir\\"), "C:\\dir\\");

    let q = windows::quote::quote_arg("C:\\Program Files\\dir\\");
    assert_eq!(q, r#""C:\Program Files\dir\\""#);
}

#[test]
fn an_embedded_quote_is_escaped() {
    assert_eq!(windows::quote::quote_arg(r#"a"b"#), r#""a\"b""#);

    assert_eq!(windows::quote::quote_arg(r#"C:\a\"b"#), r#""C:\a\\\"b""#);
}

#[test]
fn an_empty_argument_survives() {
    assert_eq!(windows::quote::quote_arg(""), r#""""#);
}

#[test]
fn the_command_line_joins_in_order() {
    let line =
        windows::quote::command_line(&["cmd".into(), "/c".into(), "echo hello world".into()]);

    assert_eq!(line, r#"cmd /c "echo hello world""#);
}
