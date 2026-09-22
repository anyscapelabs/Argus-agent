use argus_lib::prompt::{budget, project, tools_budget, BASE};
use argus_lib::tools::{section, tool_specs};

fn setup() -> (rusqlite::Connection, String) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("argus-prompt-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    conn.execute(
        "INSERT INTO sessions (id, title, permission, web_search) VALUES ('s1', 't', 'ask', 1)",
        [],
    )
    .unwrap();

    for (i, (name, desc)) in [
        (
            "alpha-skill",
            "Trigger on alpha workflows and alpha deployments",
        ),
        ("beta-skill", "Trigger on beta reviews and beta checklists"),
    ]
    .iter()
    .enumerate()
    {
        argus_lib::skills::store::create_skill(
            &conn,
            &dir,
            &argus_lib::skills::schema::NewSkill {
                name: name.to_string(),
                description: desc.to_string(),
                body: format!(
                    "## When to use\n\nSkill number {i} with plenty of body text to pass the validation checks that require real content here and now.\n\n## Steps\n\n1. Do the thing carefully and verify the outcome twice.\n2. Report what changed in one line.\n\n## Pitfalls\n\nDo not skip verification ever. Do not run the same step twice without checking."
                ),
                source: None,
                origin: None,
            },
        )
        .unwrap();
    }

    for (i, content) in [
        "The user prefers concise answers with no fluff ever",
        "The project deploys with a blue-green strategy always",
    ]
    .iter()
    .enumerate()
    {
        argus_lib::memory::store::save(
            &conn,
            &argus_lib::memory::schema::NewMemory {
                content: format!("{content} marker{i}"),
                kind: Some("fact".into()),
                importance: None,
                session_id: None,
            },
        )
        .unwrap();
    }

    (conn, "s1".into())
}

#[test]
fn projection_has_no_skill_index() {
    let (conn, sid) = setup();
    let p = project(&conn, &sid).unwrap();

    assert!(!p.system.contains("alpha-skill"), "skill index leaked");
    assert!(!p.system.contains("beta-skill"), "skill index leaked");
    assert!(p.system.contains("skill.search"), "search tool missing");
}

#[test]
fn projection_has_no_automatic_memories() {
    let (conn, sid) = setup();
    let p = project(&conn, &sid).unwrap();

    assert!(!p.system.contains("marker0"), "memory dump leaked");
    assert!(!p.system.contains("marker1"), "memory dump leaked");
    assert!(
        !p.system.contains("Relevant memories"),
        "recall block leaked"
    );
    assert!(p.system.contains("memory.search"), "search tool missing");
}

#[test]
fn model_catalog_hides_bash_run_and_keeps_terminal() {
    assert!(!section(false).contains("bash.run"));
    assert!(!section(true).contains("bash.run"));

    for specs in [tool_specs(false), tool_specs(true)] {
        assert!(specs.iter().all(|t| t.name != "bash.run"));
        assert!(specs.iter().any(|t| t.name == "terminal"));
    }
}

#[test]
fn admin_and_password_rules_survive() {
    let (conn, sid) = setup();
    let p = project(&conn, &sid).unwrap();

    assert!(
        p.system.contains("privilege \"admin\"") || section(false).contains("privilege \"admin\"")
    );
    assert!(BASE.contains("Never request, collect, store, or expose the user's sudo password"));
}

#[test]
fn summary_is_optional_and_delimited() {
    let (conn, sid) = setup();

    let plain = project(&conn, &sid).unwrap();
    assert!(!plain.system.contains("<session-summary>"));

    conn.execute(
        "INSERT INTO summaries (session_id, covers_to, content) VALUES ('s1', 0, 'compact recap here')",
        [],
    )
    .unwrap();

    let with = project(&conn, &sid).unwrap();
    assert!(with.system.contains("<session-summary>"));
    assert!(with.system.contains("compact recap here"));
    assert!(with.system.contains("</session-summary>"));
}

#[test]
fn preferences_are_delimited_when_present() {
    let (conn, sid) = setup();

    let plain = project(&conn, &sid).unwrap();
    assert!(!plain.system.contains("<user-preferences>"));

    conn.execute(
        "INSERT INTO kv (k, v) VALUES ('preference_rules', 'Always answer in French')",
        [],
    )
    .unwrap();

    let with = project(&conn, &sid).unwrap();
    assert!(with.system.contains("<user-preferences>"));
    assert!(with.system.contains("Always answer in French"));
}

#[test]
fn budget_sections_sum_to_total() {
    let (conn, sid) = setup();
    let p = project(&conn, &sid).unwrap();

    let parts = budget(&p.system);
    assert!(!parts.is_empty());

    let sum: i64 = parts.iter().map(|(_, n)| n).sum();
    let total = argus_lib::prompt::config::est_tokens(&p.system);
    assert!((sum - total).abs() <= parts.len() as i64 + 1);
    assert!(parts.iter().any(|(n, _)| *n == "stable"));

    let tb = tools_budget(true);
    assert!(tb > 1000, "tools budget implausibly small: {tb}");
}

#[test]
fn full_budget_splits_named_sections_and_zeroes_unused_tiers() {
    use argus_lib::prompt::full_budget;

    let (conn, sid) = setup();
    let p = project(&conn, &sid).unwrap();
    let b = full_budget(&p, true);

    assert!(b.stable > 0);
    assert!(b.tools > 1000);
    assert_eq!(b.skill, 0, "no skill tier is ever injected");
    assert_eq!(b.memory, 0, "no memory tier is ever injected");
    assert_eq!(
        b.total,
        b.stable
            + b.tools
            + b.preferences
            + b.summary
            + b.notepad
            + b.skill
            + b.memory
            + b.conversation
    );
}

#[test]
fn runtime_architecture_stays_out_of_the_prompt() {
    let (conn, sid) = setup();
    let p = project(&conn, &sid).unwrap();
    let tools = section(true);

    for needle in [
        "StreamEvent",
        "StreamDone",
        "ToolExecution",
        "SQLite",
        "prefix_hash",
        "compact_seq",
        "Tauri",
        "ApprovalBlock",
        "SessionStore",
        "Gateway",
    ] {
        assert!(!p.system.contains(needle), "leaked: {needle}");
        assert!(!tools.contains(needle), "leaked: {needle}");
    }
}

#[test]
fn terminal_contract_is_short_and_complete() {
    let specs = tool_specs(false);
    let term = specs.iter().find(|t| t.name == "terminal").unwrap();

    assert!(term.description.contains("Use admin privilege only"));
    assert!(term.description.contains("Never ask for or handle"));
    assert!(!term.description.contains("120s"));
    assert!(!term.description.contains("600s"));
}
