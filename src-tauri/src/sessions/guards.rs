// Loop guards: when to refuse a call because the agent is going in circles,
// and how much of a turn's budget is left.
//
// These are pure functions over the turn's own history. Nothing here touches
// the database, the channel, or the model, which is why the whole class is
// testable without a harness and why a change to it cannot alter a tool's
// side effects.

pub fn repeated(recent: &[(String, String)], key: &(String, String)) -> bool {
    if recent.iter().rev().take_while(|p| *p == key).count() >= 2 {
        return true;
    }

    if !key.0.starts_with("web.") {
        return false;
    }

    let host = |args: &str| -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(args).ok()?;
        let u = v.get("url")?.as_str()?;
        url::Url::parse(u).ok()?.host_str().map(|h| h.to_string())
    };

    let Some(h) = host(&key.1) else {
        return false;
    };

    let same_site = |p: &&(String, String)| -> bool {
        p.0 == key.0
            && host(&p.1)
                .map(|o| {
                    let base = |x: &str| -> String {
                        let mut it = x.rsplit('.');
                        let t = it.next().unwrap_or("");
                        let m = it.next().unwrap_or("");
                        format!("{m}.{t}")
                    };

                    base(&o) == base(&h)
                })
                .unwrap_or(false)
    };

    recent.iter().rev().take_while(same_site).count() >= 2
}

// Args that vary without changing what the call does. Retries that differ
// only here are the same attempt wearing a different timeout.
//
// `background` and `privilege` are deliberately NOT here. They are the two
// moves the tool description tells the agent to make when a foreground call
// keeps failing: background a long job, escalate a denied one. Erasing them
// makes the guard refuse the exact recovery it exists to encourage.
const THRASH_VOLATILE: &[&str] = &["timeout", "label", "wake"];

fn stable_args(args: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(args) {
        Ok(serde_json::Value::Object(mut m)) => {
            for k in THRASH_VOLATILE {
                m.remove(*k);
            }
            serde_json::Value::Object(m).to_string()
        }
        _ => args.trim().to_string(),
    }
}

// Keys carry the identity of a call; values carry its bulk. `fs.write` and
// `doc.create` pair a long body with a one-token path, so comparing values
// alone says every file is the same file. Each value contributes under its
// own key, so a shared body cannot outvote the path that tells them apart.
fn arg_tokens(stable: &str) -> std::collections::HashSet<String> {
    let mut toks = std::collections::HashSet::new();

    fn walk(prefix: &str, v: &serde_json::Value, toks: &mut std::collections::HashSet<String>) {
        match v {
            serde_json::Value::String(s) => {
                for t in s.split_whitespace() {
                    toks.insert(format!("{prefix}={t}"));
                }
            }
            serde_json::Value::Number(n) => {
                toks.insert(format!("{prefix}={n}"));
            }
            serde_json::Value::Bool(b) => {
                toks.insert(format!("{prefix}={b}"));
            }
            serde_json::Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    walk(&format!("{prefix}[{i}]"), x, toks);
                }
            }
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    walk(&format!("{prefix}.{k}"), x, toks);
                }
            }
            serde_json::Value::Null => {}
        }
    }

    match serde_json::from_str::<serde_json::Value>(stable) {
        Ok(serde_json::Value::Object(m)) => {
            for (k, v) in &m {
                walk(k, v, &mut toks);
            }
        }
        Ok(v) => walk("", &v, &mut toks),
        Err(_) => {
            toks.extend(stable.split_whitespace().map(str::to_string));
        }
    }
    toks
}

// No comparable tokens means no evidence, so two un-informative calls are
// treated as different. Scoring emptiness as maximal similarity inverts the
// safe direction: it blocked `{"background":true}` against `false`.
fn jaccard(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

// The short argument that says *which* thing this call is about. A long body
// cannot outvote these: `fs.write` on three different files with the same
// content is three different files.
const IDENTITY_KEYS: &[&str] = &["path", "id", "name", "url", "pattern", "query", "file"];

fn identity(stable: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(stable).ok()?;
    let m = v.as_object()?;

    for k in IDENTITY_KEYS {
        if let Some(s) = m.get(*k).and_then(|x| x.as_str()) {
            let s = s.trim();

            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// One deliberate move that must never read as a retry of itself: the two
/// things the tool description tells the agent to do when a call keeps
/// failing. Structurally these only *add* a flag, so no similarity metric can
/// tell them from churn — they are named instead.
fn escalates(win: &[&str], proposed: &str) -> bool {
    let asked_for = |a: &str| {
        let v: serde_json::Value = match serde_json::from_str(a) {
            Ok(v) => v,
            Err(_) => return false,
        };
        v.get("background").and_then(|x| x.as_bool()) == Some(true)
            || v.get("privilege")
                .and_then(|x| x.as_str())
                .is_some_and(|p| p != "user")
    };

    asked_for(proposed) && !win.iter().any(|a| asked_for(a))
}

/// Are these two attempts the same action? Growth counts: `ls` then `ls -la`
/// is one call being refined, and refusing that is refusing the whole point of
/// the guard. Divergence does not: four attempts that each barely resemble the
/// last are four different ideas, and stopping the fourth protects the agent.
fn same_action(a: &str, b: &str) -> bool {
    if let (Some(ia), Some(ib)) = (identity(a), identity(b)) {
        return ia == ib;
    }

    let ta = arg_tokens(a);
    let tb = arg_tokens(b);

    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    if ta.is_subset(&tb) || tb.is_subset(&ta) {
        return true;
    }
    jaccard(&ta, &tb) >= 0.5
}

// The semantic twin of `repeated`: same tool, failing streak, arguments that
// are equal modulo volatile keys or a chain of near-identical variants —
// `ls`, `ls -la`, `ls -la /tmp` dying the same death. Successes never trip
// it: sequential similar calls that work are real multi-step work, not churn.
// `hist`/`failed` hold completed attempts only; `key` is the one proposed.
pub fn thrashing(hist: &[(String, String)], failed: &[bool], key: &(String, String)) -> bool {
    debug_assert_eq!(
        hist.len(),
        failed.len(),
        "guard history and its outcomes must stay the same length"
    );
    if hist.len() < 3 || failed.len() < 3 {
        return false;
    }
    let win = &hist[hist.len() - 3..];
    let fout = &failed[failed.len() - 3..];
    if !fout.iter().all(|f| *f) {
        return false;
    }
    if win.iter().any(|(t, _)| t != &key.0) {
        return false;
    }
    let mut stable: Vec<String> = win.iter().map(|(_, a)| stable_args(a)).collect();
    stable.push(stable_args(&key.1));
    if stable.iter().all(|s| s == &stable[0]) {
        return true;
    }

    let refs: Vec<&str> = stable.iter().map(String::as_str).collect();
    if escalates(&refs[..3], refs[3]) {
        return false;
    }
    // Every step alike, and the last still recognisably the first: drift
    // compounds, so neighbours alone let four unrelated attempts through.
    refs.windows(2).all(|w| same_action(w[0], w[1])) && same_action(refs[0], refs[refs.len() - 1])
}

// A turn spends a bounded number of context tokens, the way a training run
// spends a fixed wall clock: comparable runs, and a spiral becomes a verdict
// instead of a cost. Sized off the model's own window so a 1M model is not
// throttled like a 32k one.
const BUDGET_WINDOW_FRACTION: f64 = 0.25;
pub const BUDGET_WARN_FRACTION: f64 = 0.7;
const BUDGET_MIN: i64 = 32_000;
const BUDGET_MAX: i64 = 200_000;

pub fn turn_budget(ctx_tokens: i64) -> i64 {
    let base = if ctx_tokens > 0 {
        ctx_tokens
    } else {
        crate::prompt::config::DEFAULT_CONTEXT
    };
    let scaled = (base as f64 * BUDGET_WINDOW_FRACTION) as i64;
    scaled.clamp(BUDGET_MIN, BUDGET_MAX)
}

// Nudge at 70% (the model can still wind up), stop at 100% (nothing left to
// spend). Enforced, never asked for politely: a warning a model ignores is
// not a budget.
pub fn budget_state(spent: i64, budget: i64) -> Budget {
    if budget <= 0 || spent < (BUDGET_WARN_FRACTION * budget as f64) as i64 {
        return Budget::Ok;
    }
    if spent < budget {
        return Budget::Warn;
    }
    Budget::Stop
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    Ok,
    Warn,
    Stop,
}
