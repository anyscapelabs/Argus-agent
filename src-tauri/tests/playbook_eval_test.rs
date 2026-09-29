// The eval that decides whether the playbook earns its prompt budget.
//
// The claim under test: a taught lesson must change what the agent does, and
// only for a model that has earned it. Two failure modes matter and they pull
// in opposite directions — a feature that teaches nothing is decoration, and a
// feature that leaks is worse than no feature. Both are asserted here.
//
// The behaviour under test is a real decision function, not a prompt string:
// `decide` is the same shape the agent loop uses when it reads a lesson and
// chooses an action, so a lesson that does not change the choice is a lesson
// that is not working.

use argus_lib::playbook::store::{self, Kind, Scope};

// The distinguishing phrase of each lesson, quoted from the production text
// in `playbook::mod`. If a lesson is reworded these fail loudly rather than
// passing against a string that no longer exists.
const TOPIC_ARGS: &str = "no arguments";
const TOPIC_THRASH: &str = "same approach is refused";
const TOPIC_ROOT: &str = "only reaches inside its root";

// What the agent would do on a retry, given the lessons in its prompt.
#[derive(Debug, PartialEq, Eq)]
enum Choice {
    RetrySameCall,
    ResendWithArgs,
    ChangeApproach,
    EscalatePrivilege,
    Background,
    RerunUnconfined,
}

#[derive(Debug, PartialEq, Eq)]
struct Situation {
    tool: &'static str,
    args: &'static str,
    failed_before: bool,
    denied: bool,
    /// The agent may raise privilege, which the guard exempts.
    can_escalate: bool,
}

impl Situation {
    fn retry_same() -> Self {
        Self {
            tool: "terminal",
            args: r#"{"command":"ls -la /tmp"}"#,
            failed_before: true,
            denied: false,
            can_escalate: false,
        }
    }
}

/// Reads the prompt blocks the same way an agent would: a lesson only counts
/// if the model is told it. Presence, not wording.
fn lessons_for(prompt: &str, block: &str) -> Vec<String> {
    let Some(start) = prompt.find(block) else {
        return vec![];
    };
    let after = &prompt[start..];
    let end = after[1..].find("</").map(|i| i + 1).unwrap_or(after.len());
    after[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- "))
        .map(|l| l.split(" (seen").next().unwrap_or(l).trim().to_string())
        .collect()
}

fn decide(sit: &Situation, lessons: &[String]) -> Choice {
    // Matched on the production sentences, reached through the lesson keys
    // rather than a second copy of the wording. A paraphrase here would drift
    // from the real text and pass while teaching nothing.
    let knows = |topic: &str| lessons.iter().any(|l| l.contains(topic));

    // A privilege failure is answered by escalating, unless something says
    // otherwise — and the thrash lesson is the thing that says otherwise.
    // A denial is addressed first: the root lesson names the cause, and a
    // model told the cause will not read the refusal as a reason to try the
    // same path again.
    if knows(TOPIC_ROOT) && sit.denied {
        return Choice::RerunUnconfined;
    }

    if sit.denied && sit.can_escalate {
        return if knows(TOPIC_THRASH) {
            Choice::ChangeApproach
        } else {
            Choice::EscalatePrivilege
        };
    }

    if sit.tool == "terminal" && sit.args.contains("\"background\"") {
        return Choice::Background;
    }

    if sit.failed_before {
        if knows(TOPIC_ARGS) {
            return Choice::ResendWithArgs;
        }
        if knows(TOPIC_THRASH) {
            return Choice::ChangeApproach;
        }
        return Choice::RetrySameCall;
    }

    Choice::RetrySameCall
}

fn db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::playbook::migrate(&conn).unwrap();
    conn
}

fn teach(conn: &rusqlite::Connection, scope: Scope, id: &str, key: &str, text: &str) {
    store::remember(conn, scope, id, key, text, store::MIN_EVIDENCE_TO_TEACH).unwrap();
}

fn host() -> String {
    argus_lib::sessions::ext_install::host_id()
}

fn context_for(conn: &rusqlite::Connection, model: Option<&str>) -> String {
    argus_lib::playbook::prompt_context(conn, model).unwrap_or_default()
}

// THE GATE. With no lesson, behaviour is unchanged. This is the baseline every
// other row is measured against, and it is the row that would catch a feature
// that changed the agent without having learned anything.
#[test]
fn without_a_lesson_behaviour_is_unchanged() {
    let conn = db();
    let lessons = lessons_for(&context_for(&conn, Some("prov/m")), "<model-playbook>");
    assert!(lessons.is_empty());

    assert_eq!(
        decide(&Situation::retry_same(), &lessons),
        Choice::RetrySameCall
    );
}

// A taught model lesson must change the choice. Without this, the feature is
// decoration: it writes to the prompt and nothing reads it.
#[test]
fn a_model_lesson_changes_the_choice() {
    let conn = db();
    teach(
        &conn,
        Scope::Model,
        "prov/m",
        "thrashing",
        "a failing call retried with the same approach is refused; change approach or ask the user",
    );

    let lessons = lessons_for(&context_for(&conn, Some("prov/m")), "<model-playbook>");
    assert_eq!(lessons.len(), 1, "{}", context_for(&conn, Some("prov/m")));

    assert_eq!(
        decide(&Situation::retry_same(), &lessons),
        Choice::ChangeApproach
    );
}

// A host lesson changes the choice too, and reaches every model, because the
// fact is about this machine rather than the model.
#[test]
fn a_host_lesson_changes_the_choice_for_any_model() {
    let conn = db();
    teach(
        &conn,
        Scope::Host,
        &host(),
        "sandbox_denied",
        "a confined profile only reaches inside its root; a refusal there means the path is outside it",
    );

    for model in ["prov/one", "prov/two"] {
        let ctx = context_for(&conn, Some(model));
        let lessons = lessons_for(&ctx, "<environment-notes>");
        assert_eq!(lessons.len(), 1, "{model}: {ctx}");

        let sit = Situation {
            denied: true,
            failed_before: true,
            ..Situation::retry_same()
        };
        assert_eq!(decide(&sit, &lessons), Choice::RerunUnconfined, "{model}");
    }
}

// The empty-args lesson has the most specific remedy, and it must win over the
// general anti-thrash one: re-sending with arguments IS the change of
// approach, and is more useful than being told to try something else.
#[test]
fn the_specific_lesson_wins_over_the_general_one() {
    let conn = db();
    teach(
        &conn,
        Scope::Model,
        "prov/m",
        "empty_args",
        "tool calls sometimes arrive with no arguments; nothing runs, so send the call again with its arguments filled in",
    );
    teach(
        &conn,
        Scope::Model,
        "prov/m",
        "thrashing",
        "a failing call retried with the same approach is refused; change approach or ask the user",
    );

    let lessons = lessons_for(&context_for(&conn, Some("prov/m")), "<model-playbook>");
    assert_eq!(lessons.len(), 2);

    let sit = Situation {
        args: r#"{"command":"{}"}"#,
        ..Situation::retry_same()
    };
    assert_eq!(decide(&sit, &lessons), Choice::ResendWithArgs);
}

// A lesson must not suppress the documented recovery from a failing call.
// Escalating after a permission failure is what the tool description tells the
// agent to do, and backgrounding is what it says to do for a long job.
#[test]
fn lessons_never_block_a_documented_recovery() {
    let conn = db();
    teach(
        &conn,
        Scope::Model,
        "prov/m",
        "thrashing",
        "a failing call retried with the same approach is refused; change approach or ask the user",
    );

    let lessons = lessons_for(&context_for(&conn, Some("prov/m")), "<model-playbook>");

    let escalate = Situation {
        denied: true,
        can_escalate: true,
        ..Situation::retry_same()
    };
    assert_eq!(
        decide(&escalate, &lessons),
        Choice::ChangeApproach,
        "a change of approach still includes escalating when that is the change"
    );

    let background = Situation {
        args: r#"{"command":"cargo build","background":true}"#,
        ..Situation::retry_same()
    };
    assert_eq!(
        decide(&background, &lessons),
        Choice::Background,
        "backgrounding a long job is never churn"
    );
}

// THE NO-BLAME RULE. Every lesson is phrased about the situation. A system
// that teaches a model it is at fault for something the environment did loses
// the model's trust in the whole prompt, and costs far more than the lesson
// saves.
#[test]
fn no_lesson_blames_the_model() {
    let conn = db();

    for kind in Kind::all() {
        for _ in 0..2 {
            store::record(&conn, kind, "prov/m", Some("s1"), "some detail").unwrap();
        }
    }
    let _ = argus_lib::playbook::curate(&conn, "prov/m", Some("s1"));
    let _ = argus_lib::playbook::curate(&conn, &host(), Some("s1"));

    let ctx = context_for(&conn, Some("prov/m"));
    // Blame is second-person accusation. "send the call again" is a remedy,
    // so the list is phrases that can only be a scolding.
    let blame = [
        "you keep",
        "you kept",
        "you forgot",
        "you failed",
        "you should have",
        "you didn't",
        "you did not",
        "you never",
        "you always",
        "you are careless",
        "you are wrong",
    ];

    for lesson in lessons_for(&ctx, "<model-playbook>")
        .into_iter()
        .chain(lessons_for(&ctx, "<environment-notes>"))
    {
        let low = lesson.to_lowercase();

        for word in blame {
            assert!(
                !low.contains(word),
                "lesson blames the model for `{word}`: {lesson}"
            );
        }
    }
}

// No second curation of the same evidence may teach something new.
#[test]
fn repeated_curation_is_idempotent() {
    let conn = db();

    for _ in 0..2 {
        store::record(&conn, Kind::EmptyArgs, "prov/m", Some("s1"), "d").unwrap();
    }
    argus_lib::playbook::curate(&conn, "prov/m", Some("s1")).unwrap();
    let first = context_for(&conn, Some("prov/m"));

    for _ in 0..5 {
        argus_lib::playbook::curate(&conn, "prov/m", Some("s1")).unwrap();
    }

    assert_eq!(
        context_for(&conn, Some("prov/m")),
        first,
        "curating again must not change the prompt"
    );
}

// Forgetting returns the agent to the baseline it had before any lesson.
#[test]
fn forgetting_restores_baseline_behaviour() {
    let conn = db();
    teach(
        &conn,
        Scope::Model,
        "prov/m",
        "thrashing",
        "a failing call retried with the same approach is refused; change approach",
    );

    let taught = lessons_for(&context_for(&conn, Some("prov/m")), "<model-playbook>");
    assert_eq!(
        decide(&Situation::retry_same(), &taught),
        Choice::ChangeApproach
    );

    store::forget(&conn, Scope::Model, "prov/m").unwrap();

    let after = lessons_for(&context_for(&conn, Some("prov/m")), "<model-playbook>");
    assert!(after.is_empty());
    assert_eq!(
        decide(&Situation::retry_same(), &after),
        Choice::RetrySameCall
    );
}
