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

static SNAPSHOT: AsyncMutex<Vec<Elem>> = AsyncMutex::const_new(Vec::new());
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

    let mut out = String::new();
    let mut elems: Vec<Elem> = Vec::new();

    let root = accessible(c, ROOT_DEST, ROOT_PATH).await?;
    let count = root.child_count().await.unwrap_or(0);

    for i in 0..count.min(30) {
        if elems.len() >= MAX_ELEMS {
            break;
        }

        let app = match root.get_child_at_index(i).await {
            Ok(app) => app,
            Err(_) => continue,
        };

        let dest = app.name_as_str().unwrap_or_default().to_string();
        if dest.is_empty() {
            continue;
        }

        let path = app.path_as_str().to_string();
        let app_proxy = accessible(c, &dest, &path).await?;
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
        )
        .await;

        elems.append(&mut per_app);
    }

    elems.truncate(MAX_ELEMS);
    *SNAPSHOT.lock().await = elems.clone();

    if out.is_empty() {
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
            let idx = *n;

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
            if kd.is_empty() {
                continue;
            }

            let sub = format!("{indent}  ");
            walk(c, &kd, kid.path_as_str(), &sub, depth + 1, out, elems, n).await;
        }
    })
}

pub async fn act(args: &Value) -> Result<String, String> {
    let e = resolve(args).await?;
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

    tree().await
}

async fn resolve(args: &Value) -> Result<Elem, String> {
    let r = args
        .get("ref")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing ref".to_string())? as usize;

    let snap = SNAPSHOT.lock().await.clone();

    Ok(snap
        .get(r.wrapping_sub(1))
        .cloned()
        .ok_or_else(|| "unknown ref — run computer.observe for a fresh list".to_string())?)
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
