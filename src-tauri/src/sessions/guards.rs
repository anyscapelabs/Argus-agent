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

// Args that vary without changing what the call does. `background` and
// `privilege` are deliberately NOT here — they are the recovery the tool
// description tells the agent to make, and erasing them refuses it.
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

// Each value counts under its own key: a shared body must not outvote the path.
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

// No comparable tokens is no evidence: un-informative calls read as different.
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

// The short argument saying *which* thing this call is about.
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

/// The one deliberate move that must never read as a retry of itself.
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

/// Growth counts: `ls` then `ls -la` is one call refined. Divergence does not.
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

// Same tool, failing streak, arguments equal modulo volatile keys or a chain of
// near-identical variants. Successes never trip it. `hist`/`failed` hold
// completed attempts only; `key` is the one proposed.
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
    // Every step alike, and the last still the first: drift compounds.
    refs.windows(2).all(|w| same_action(w[0], w[1])) && same_action(refs[0], refs[refs.len() - 1])
}

// Bounded spend per turn, sized off the model's own window — and off the way
// the meter works. `tok_in_sum` adds up every step's full prompt (history is
// re-sent each step), so one long turn legitimately spends many multiples of
// a single prompt. The budget must cover a whole MAX_STEPS turn; the step
// limit and compaction are the real bounds, this is only the backstop.
const BUDGET_WINDOW_FRACTION: f64 = 8.0;
pub const BUDGET_WARN_FRACTION: f64 = 0.7;
const BUDGET_MIN: i64 = 200_000;
const BUDGET_MAX: i64 = 8_000_000;

pub fn turn_budget(ctx_tokens: i64) -> i64 {
    let base = if ctx_tokens > 0 {
        ctx_tokens
    } else {
        crate::prompt::config::DEFAULT_CONTEXT
    };
    let scaled = (base as f64 * BUDGET_WINDOW_FRACTION) as i64;
    scaled.clamp(BUDGET_MIN, BUDGET_MAX)
}

// Nudge at 70%, stop at 100%. Enforced, not asked for politely.
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
