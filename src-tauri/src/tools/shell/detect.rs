use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShellKind {
    Bash,
    Sh,
    Wsl,
    PowerShell,
    Cmd,
}

#[derive(Clone, Debug)]
pub struct ShellConfig {
    pub binary: PathBuf,
    pub kind: ShellKind,
    pub version: Option<String>,
}

static AUTO: OnceLock<ShellConfig> = OnceLock::new();
static SHELL_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn status() -> ShellConfig {
    if let Ok(g) = SHELL_OVERRIDE.lock() {
        if let Some(p) = g.clone() {
            return ShellConfig {
                binary: p.clone(),
                kind: ShellKind::Bash,
                version: probe(&p),
            };
        }
    }

    AUTO.get_or_init(detect).clone()
}

pub fn override_active() -> bool {
    SHELL_OVERRIDE.lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn set_override(path: &str) -> Result<ShellConfig, String> {
    let p = PathBuf::from(crate::tools::expand(path));

    if !p.is_file() {
        return Err(format!("shell not found: {}", p.display()));
    }

    let cfg = ShellConfig {
        binary: p.clone(),
        kind: ShellKind::Bash,
        version: probe(&p),
    };

    SHELL_OVERRIDE
        .lock()
        .map_err(|err| err.to_string())?
        .replace(p);

    Ok(cfg)
}

pub fn clear_override() {
    if let Ok(mut g) = SHELL_OVERRIDE.lock() {
        *g = None;
    }
}

fn detect() -> ShellConfig {
    #[cfg(windows)]
    return detect_windows();

    #[cfg(not(windows))]
    return detect_unix();
}

#[cfg(not(windows))]
fn detect_unix() -> ShellConfig {
    let mut cands: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    cands.extend(["/opt/homebrew/bin/bash", "/usr/local/bin/bash"].map(PathBuf::from));

    if let Some(p) = path_lookup("bash") {
        cands.push(p);
    }

    cands.extend(["/usr/bin/bash", "/bin/bash"].map(PathBuf::from));

    let mut fallback: Option<ShellConfig> = None;

    for p in cands {
        if !p.is_file() {
            continue;
        }

        let ver = probe(&p);
        let cfg = ShellConfig {
            binary: p,
            kind: ShellKind::Bash,
            version: ver.clone(),
        };

        if major(&ver).unwrap_or(0) >= 4 {
            return cfg;
        }

        if fallback.is_none() {
            fallback = Some(cfg);
        }
    }

    fallback.unwrap_or(ShellConfig {
        binary: PathBuf::from("sh"),
        kind: ShellKind::Sh,
        version: None,
    })
}

#[cfg(windows)]
fn detect_windows() -> ShellConfig {
    let git_bash = PathBuf::from("C:\\Program Files\\Git\\bin\\bash.exe");

    if git_bash.is_file() {
        return ShellConfig {
            binary: git_bash.clone(),
            kind: ShellKind::Bash,
            version: probe(&git_bash),
        };
    }

    if let Some(p) = path_lookup("bash.exe").or_else(|| path_lookup("bash")) {
        return ShellConfig {
            binary: p.clone(),
            kind: ShellKind::Bash,
            version: probe(&p),
        };
    }

    if wsl_ready() {
        return ShellConfig {
            binary: PathBuf::from("wsl.exe"),
            kind: ShellKind::Wsl,
            version: None,
        };
    }

    for exe in ["pwsh.exe", "powershell.exe"] {
        if let Some(p) = path_lookup(exe) {
            return ShellConfig {
                binary: p,
                kind: ShellKind::PowerShell,
                version: None,
            };
        }
    }

    ShellConfig {
        binary: PathBuf::from("cmd.exe"),
        kind: ShellKind::Cmd,
        version: None,
    }
}

#[cfg(windows)]
fn wsl_ready() -> bool {
    let out = std::process::Command::new("wsl.exe")
        .arg("--list")
        .arg("--quiet")
        .output();

    match out {
        Ok(o) => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        Err(_) => false,
    }
}

fn path_lookup(exe: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;

    for dir in std::env::split_paths(&paths) {
        let p = dir.join(exe);

        if p.is_file() {
            return Some(p);
        }
    }

    None
}

fn probe(path: &Path) -> Option<String> {
    let out = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

fn major(ver: &Option<String>) -> Option<u64> {
    let v = ver.as_ref()?;
    let rest = v.split("version ").nth(1)?;
    rest.split('.').next()?.trim().parse().ok()
}
