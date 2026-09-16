use atspi::{
    proxy::{accessible::AccessibleProxy, action::ActionProxy, component::ComponentProxy},
    CoordType, Role, State,
};
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, OnceCell};

const MAX_ELEMS: usize = 200;
const MAX_PER_APP: usize = 60;
const MAX_DEPTH: usize = 10;
const MAX_KIDS: usize = 30;
const ROOT_DEST: &str = "org.a11y.atspi.Registry";
const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";

fn daemon_app(name: &str) -> bool {
    name.starts_with("csd-")
        || name.starts_with("ibus")
        || name.contains("daemon")
        || name.contains("portal")
        || name == "dconf-service"
}

#[derive(Clone)]
pub struct Elem {
    pub role: String,
    pub name: String,
    pub center: Option<(i32, i32)>,
    pub password: bool,
    pub actions: Vec<String>,
    pub dest: String,
    pub path: String,
}

struct ObsSnap {
    gen: u64,
    elems: Vec<Elem>,
}

static SNAPSHOT: AsyncMutex<ObsSnap> = AsyncMutex::const_new(ObsSnap {
    gen: 0,
    elems: Vec::new(),
});
static SHOWN: AsyncMutex<Option<u64>> = AsyncMutex::const_new(None);

pub(crate) async fn note_shown(gen: u64) {
    *SHOWN.lock().await = Some(gen);
}
static CONN: OnceCell<atspi::AccessibilityConnection> = OnceCell::const_new();

async fn conn() -> Result<&'static atspi::AccessibilityConnection, String> {
    CONN.get_or_try_init(|| async {
        atspi::AccessibilityConnection::new()
            .await
            .map_err(|err| format!("no accessibility bus (is at-spi2 running?): {err}"))
    })
    .await
}

async fn accessible(
    conn: &atspi::AccessibilityConnection,
    dest: &str,
    path: &str,
) -> Result<AccessibleProxy<'static>, String> {
    AccessibleProxy::builder(conn.inner().connection())
        .destination(dest.to_string())
        .map_err(|err| err.to_string())?
        .path(path.to_string())
        .map_err(|err| err.to_string())?
        .build()
        .await
        .map_err(|err| err.to_string())
}

fn showable_role(role: &str) -> bool {
    !matches!(
        role,
        "filler" | "root pane" | "layered pane" | "scroll pane" | "separator" | "panel"
    )
}

fn interactive_role(role: &str) -> bool {
    matches!(
        role,
        "push button"
            | "toggle button"
            | "check box"
            | "radio button"
            | "menu item"
            | "menu"
            | "menu bar"
            | "entry"
            | "text"
            | "password text"
            | "combo box"
            | "list item"
            | "table cell"
            | "link"
            | "page tab"
            | "slider"
            | "spin button"
            | "tool bar"
    )
}

pub async fn tree() -> Result<String, String> {
    let c = conn().await?;
    let gen = super::next_obs();

    let mut out = format!("Desktop observation {gen} (element refs):\n");
    let mut elems: Vec<Elem> = Vec::new();

    let root = accessible(c, ROOT_DEST, ROOT_PATH).await?;
    let mut g = 0usize;

    for app in root
        .get_children()
        .await
        .unwrap_or_default()
        .iter()
        .take(30)
    {
        if elems.len() >= MAX_ELEMS {
            break;
        }

        let dest = app.name_as_str().unwrap_or_default().to_string();
        if dest.is_empty() {
            continue;
        }

        let path = app.path_as_str().to_string();
        let Ok(app_proxy) = accessible(c, &dest, &path).await else {
            continue;
        };
        let app_name = app_proxy.name().await.unwrap_or_default();

        if daemon_app(&app_name) {
            continue;
        }

        let mut per_app: Vec<Elem> = Vec::new();
        let mut pn = 0usize;

        walk(
            c,
            &dest,
            &path,
            &format!("app '{app_name}'"),
            0,
            &mut out,
            &mut per_app,
            &mut pn,
            &mut g,
        )
        .await;

        elems.append(&mut per_app);

        if elems.len() >= MAX_ELEMS {
            break;
        }
    }

    elems.truncate(MAX_ELEMS);
    *SNAPSHOT.lock().await = ObsSnap {
        gen,
        elems: elems.clone(),
    };

    if elems.is_empty() {
        out.push_str(
            "(no accessible elements — some apps expose no tree; fall back to \
             computer.screen + coordinates)\n",
        );
    }

    Ok(out)
}

async fn node_actions(c: &atspi::AccessibilityConnection, dest: &str, path: &str) -> Vec<String> {
    let b = match ActionProxy::builder(c.inner().connection())
        .destination(dest)
        .and_then(|b| b.path(path))
    {
        Ok(b) => b,
        Err(_) => return vec![],
    };

    let Ok(ap) = b.build().await else {
        return vec![];
    };

    ap.get_actions()
        .await
        .map(|a| a.into_iter().map(|x| x.name).collect())
        .unwrap_or_default()
}

async fn node_center(
    c: &atspi::AccessibilityConnection,
    dest: &str,
    path: &str,
) -> Option<(i32, i32)> {
    let b = ComponentProxy::builder(c.inner().connection())
        .destination(dest)
        .and_then(|b| b.path(path))
        .ok()?;

    let cp = b.build().await.ok()?;
    let (x, y, w, h) = cp.get_extents(CoordType::Screen).await.ok()?;

    Some((x + w / 2, y + h / 2))
}

#[allow(clippy::too_many_arguments)]
fn walk<'a>(
    c: &'a atspi::AccessibilityConnection,
    dest: &'a str,
    path: &'a str,
    indent: &'a str,
    depth: usize,
    out: &'a mut String,
    elems: &'a mut Vec<Elem>,
    n: &'a mut usize,
    g: &'a mut usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if depth > MAX_DEPTH || *n >= MAX_PER_APP {
            return;
        }

        let Ok(node) = accessible(c, dest, path).await else {
            return;
        };

        let role = node.get_role_name().await.unwrap_or_default();
        let name = node.name().await.unwrap_or_default();
        let state = node.get_state().await.unwrap_or_default();

        if !state.contains(State::Showing) && depth > 0 {
            return;
        }

        let password = role == "password text" || role == Role::PasswordText.name();

        if (interactive_role(&role) || !name.is_empty()) && showable_role(&role) {
            let center = node_center(c, dest, path).await;
            let actions = node_actions(c, dest, path).await;
            *n += 1;
            *g += 1;
            let idx = *g;

            let mut line = format!(
                "{indent}[{idx}] {role} '{}'",
                crate::tools::clip(name.clone())
            );

            if let Some((x, y)) = center {
                line.push_str(&format!(" center={x},{y}"));
            }

            if !actions.is_empty() {
                line.push_str(&format!(" actions={}", actions.join("|")));
            }

            if password {
                line.push_str(" [PASSWORD]");
            }

            line.push('\n');
            out.push_str(&line);

            elems.push(Elem {
                role,
                name,
                center,
                password,
                actions,
                dest: dest.to_string(),
                path: path.to_string(),
            });
        }

        let count = node.child_count().await.unwrap_or(0);

        for i in 0..count.min(MAX_KIDS as i32) {
            if *n >= MAX_PER_APP {
                break;
            }

            let kid = match node.get_child_at_index(i).await {
                Ok(kid) => kid,
                Err(_) => continue,
            };

            let kd = kid.name_as_str().unwrap_or_default().to_string();
            let kd = if kd.is_empty() { dest.to_string() } else { kd };

            let sub = format!("{indent}  ");
            walk(c, &kd, kid.path_as_str(), &sub, depth + 1, out, elems, n, g).await;
        }
    })
}

pub async fn act(args: &Value) -> Result<String, String> {
    let e = resolve(args).await?;
    let before = super::x11::win_state().await;
    let c = conn().await?;

    let want = args
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("press")
        .to_string();

    let mut done = false;

    if let Some(ix) = e.actions.iter().position(|a| a.eq_ignore_ascii_case(&want)) {
        let ap = match ActionProxy::builder(c.inner().connection())
            .destination(e.dest.as_str())
            .and_then(|b| b.path(e.path.as_str()))
        {
            Ok(b) => b.build().await.ok(),
            Err(_) => None,
        };

        if let Some(ap) = ap {
            done = ap.do_action(ix as i32).await.unwrap_or(false);
        }
    }

    if !done {
        if let Some((x, y)) = e.center {
            super::x11::click_xy(x, y).await?;
            done = true;
        }
    }

    if !done {
        return Err(format!(
            "element {} exposes no '{}' action and has no position — re-observe",
            args.get("ref").and_then(|v| v.as_i64()).unwrap_or(0),
            want
        ));
    }

    super::x11::finish_verify(before, super::x11::refresh().await).await
}

pub fn ref_index(r: i64, len: usize) -> Option<usize> {
    if r < 1 {
        return None;
    }

    let i = r.wrapping_sub(1) as usize;

    if i < len {
        Some(i)
    } else {
        None
    }
}

async fn resolve(args: &Value) -> Result<Elem, String> {
    let r = args
        .get("ref")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing ref".to_string())?;

    let tok = super::obs_token(args);
    let snap = SNAPSHOT.lock().await;

    if let Some(g) = tok {
        if g != snap.gen {
            return Err(format!(
                "stale ref {r} from observation {g} — observation {} is current; run computer.observe for a fresh list",
                snap.gen
            ));
        }
    } else if let Some(shown) = *SHOWN.lock().await {
        if shown != snap.gen && ref_index(r, snap.elems.len()).is_some() {
            return Err(format!(
                "stale ref {r} from observation {shown} — observation {} is current; run computer.observe for a fresh list",
                snap.gen
            ));
        }
    }

    match ref_index(r, snap.elems.len()) {
        Some(i) => Ok(snap.elems[i].clone()),
        None => Err("unknown ref — run computer.observe for a fresh list".to_string()),
    }
}

pub async fn focus_ref(args: &Value) -> Result<(), String> {
    if args.get("ref").is_none() {
        return Ok(());
    }

    let e = resolve(args).await?;

    if e.password {
        return Err(
            "that field is a password field — the user types it themselves; \
             tell them to enter it"
                .into(),
        );
    }

    let c = conn().await?;

    let focused = match ComponentProxy::builder(c.inner().connection())
        .destination(e.dest.as_str())
        .and_then(|b| b.path(e.path.as_str()))
    {
        Ok(b) => match b.build().await.ok() {
            Some(cp) => cp.grab_focus().await.unwrap_or(false),
            None => false,
        },
        Err(_) => false,
    };

    if !focused {
        if let Some((x, y)) = e.center {
            super::x11::click_xy(x, y).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod shown_tests {
    use super::{resolve, Elem, ObsSnap, SHOWN, SNAPSHOT};
    use std::sync::{Mutex as StdMutex, OnceLock};

    static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

    fn elem(name: &str) -> Elem {
        Elem {
            role: "push button".into(),
            name: name.into(),
            center: Some((10, 10)),
            password: false,
            actions: vec!["click".into()],
            dest: "d".into(),
            path: "p".into(),
        }
    }

    async fn set_table(gen: u64, names: &[&str], shown: Option<u64>) {
        let mut s = SNAPSHOT.lock().await;
        *s = ObsSnap {
            gen,
            elems: names.iter().map(|n| elem(n)).collect(),
        };
        *SHOWN.lock().await = shown;
    }

    #[tokio::test]
    async fn tokenless_steady_state_resolves() {
        let _guard = SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap();
        set_table(5, &["a", "b"], Some(5)).await;
        match resolve(&serde_json::json!({"ref": 1})).await {
            Ok(e) => assert_eq!(e.name, "a"),
            Err(e) => panic!("unexpected: {e}"),
        }
        match resolve(&serde_json::json!({"ref": 2})).await {
            Ok(e) => assert_eq!(e.name, "b"),
            Err(e) => panic!("unexpected: {e}"),
        }
    }

    #[tokio::test]
    async fn tokenless_drift_is_rejected_without_touching_state() {
        let _guard = SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap();
        set_table(5, &["x", "y"], Some(4)).await;
        match resolve(&serde_json::json!({"ref": 1})).await {
            Ok(_) => panic!("drifted token-less ref must be stale"),
            Err(err) => {
                assert!(err.contains("stale ref 1 from observation 4"), "got: {err}");
                assert!(err.contains("observation 5 is current"), "got: {err}");
                assert!(err.contains("computer.observe"), "got: {err}");
            }
        }
        assert_eq!(*SHOWN.lock().await, Some(4));
        assert_eq!(SNAPSHOT.lock().await.gen, 5);
    }

    #[tokio::test]
    async fn tokenless_without_presentation_proceeds() {
        let _guard = SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap();
        set_table(5, &["a"], None).await;
        match resolve(&serde_json::json!({"ref": 1})).await {
            Ok(e) => assert_eq!(e.name, "a"),
            Err(e) => panic!("unexpected: {e}"),
        }
    }

    #[tokio::test]
    async fn tokenless_unknown_ref_under_drift_falls_through() {
        let _guard = SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap();
        set_table(5, &["a"], Some(4)).await;
        match resolve(&serde_json::json!({"ref": 9})).await {
            Ok(_) => panic!("out of range ref must fail"),
            Err(err) => assert!(err.contains("unknown ref"), "got: {err}"),
        }
    }

    #[tokio::test]
    async fn explicit_token_paths_unchanged() {
        let _guard = SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap();
        set_table(5, &["a"], Some(5)).await;
        match resolve(&serde_json::json!({"ref": 1, "observation": 4})).await {
            Ok(_) => panic!("old explicit token must be stale"),
            Err(err) => assert!(err.contains("stale ref 1 from observation 4"), "got: {err}"),
        }
        match resolve(&serde_json::json!({"ref": 1, "observation": 5})).await {
            Ok(e) => assert_eq!(e.name, "a"),
            Err(e) => panic!("unexpected: {e}"),
        }
    }

    #[tokio::test]
    async fn presented_marker_extraction() {
        assert_eq!(
            crate::tools::computer::shown_gen_in(
                "Desktop observation 12 (element refs):\n[0] button 'Go'\n"
            ),
            Some(12)
        );
        assert_eq!(
            crate::tools::computer::shown_gen_in(
                "screenshot: p.png (screenshot observation 4)\nimage 1x1"
            ),
            None
        );
        assert_eq!(crate::tools::computer::shown_gen_in(""), None);
    }
}
