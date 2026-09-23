use crate::tools::sandbox::backends::Probe;
use crate::tools::sandbox::plan::{Plan, SandboxError, SandboxResult, SbplPlan};
use crate::tools::sandbox::policy::{EnvPolicy, FsAccess, FsPolicy, NetPolicy, Policy, Profile};

pub const NAME: &str = "macos";

// Apple deprecated this and shipped no successor. When it stops, isolated
// profiles refuse rather than fall through to the host.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

// Loader paths only. A blanket deny would make any command fail and look
// like a working sandbox, so the probe runs twice.
pub const CANARY_SYS: &[&str] = &[
    "/bin",
    "/usr/bin",
    "/usr/lib",
    "/usr/libexec",
    "/lib",
    "/System",
    "/Library",
    "/private/var/db/dyld",
    "/dev",
];

// Exists everywhere, so a denial is never a missing file.
pub const CANARY_TARGET: &str = "/etc/hosts";

pub fn canary_profile(grant_target: bool) -> String {
    let mut s = String::from(
        "(version 1)\n(deny default)\n(allow process-exec)\n(allow process-fork)\n\
         (allow sysctl-read)\n(allow file-read-metadata)\n(allow mach-lookup)\n",
    );

    for p in CANARY_SYS {
        s.push_str(&format!("(allow file-read* (subpath {}))\n", quote(p)));
    }

    if grant_target {
        s.push_str(&format!(
            "(allow file-read* (literal {}))\n",
            quote(CANARY_TARGET)
        ));
    }

    s
}

pub fn verdict(control_ok: bool, with_grant: bool, sandboxed: bool) -> SandboxResult<Probe> {
    if !control_ok {
        return Err(SandboxError::Unavailable {
            backend: NAME,
            why: format!(
                "cannot read {CANARY_TARGET} outside the sandbox; \
                         the self-test cannot tell enforcement from a broken system"
            ),
        });
    }

    // Too tight to start at all: our bug, not the OS ignoring us.
    if with_grant && !sandboxed {
        return Err(SandboxError::Unavailable {
            backend: NAME,
            why: format!(
                "the self-test profile is too strict to run {CANARY_TARGET}; \
                 refusing to guess whether seatbelt is enforcing"
            ),
        });
    }

    if !with_grant && sandboxed {
        return Err(SandboxError::Unavailable {
            backend: NAME,
            why: "seatbelt is present but a denied read succeeded — this system \
                  is not enforcing the sandbox"
                .into(),
        });
    }

    Ok(Probe {
        backend: NAME,
        enforcing: true,
        detail: format!("seatbelt enforced a path denial on {CANARY_TARGET}"),
    })
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');

    for c in s.chars() {
        match c {
            // Unescaped, a quote closes the string and the rest parses as profile syntax.
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }

    out.push('"');
    out
}

pub fn sbpl(policy: &Policy) -> SandboxResult<String> {
    let mut s = String::from("(version 1)\n(deny default)\n");

    s.push_str("(allow process-exec)\n(allow process-fork)\n(allow sysctl-read)\n");
    s.push_str("(allow file-read-metadata)\n(allow mach-lookup)\n");

    match &policy.fs {
        FsPolicy::Unrestricted => s.push_str("(allow file*)\n"),
        FsPolicy::Scoped(rules) => {
            for rule in rules {
                let op = match rule.access {
                    FsAccess::Read => "file-read*",
                    FsAccess::Write => "file-read* file-write*",
                };

                s.push_str(&format!(
                    "(allow {op} (subpath {}))\n",
                    quote(&rule.path.to_string_lossy())
                ));
            }
        }
    }

    match &policy.net {
        NetPolicy::None => s.push_str("(deny network*)\n"),
        NetPolicy::Full => s.push_str("(allow network*)\n"),
        NetPolicy::Ports(ports) => {
            s.push_str("(deny network*)\n");

            for p in ports {
                s.push_str(&format!(
                    "(allow network-outbound (remote tcp {}))\n",
                    quote(&format!("*:{p}"))
                ));
            }
        }
    }

    Ok(s)
}

pub fn macos_plan(policy: &Policy) -> SandboxResult<Plan> {
    if policy.profile == Profile::Host {
        return Ok(Plan::None);
    }

    Ok(Plan::Macos(SbplPlan {
        profile: sbpl(policy)?,
    }))
}

pub fn env_args(policy: &Policy) -> (bool, Vec<String>) {
    match &policy.env {
        EnvPolicy::Inherit => (false, Vec::new()),
        EnvPolicy::Only(keys) => (true, keys.clone()),
    }
}

#[cfg(target_os = "macos")]
pub struct MacBackend;

#[cfg(target_os = "macos")]
use tokio::process::Command;

#[cfg(target_os = "macos")]
use super::{Backend, Guard};

#[cfg(target_os = "macos")]
fn read_target() -> Option<bool> {
    use std::process::{Command, Stdio};

    Command::new("/bin/cat")
        .arg(CANARY_TARGET)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .map(|s| s.success())
}

#[cfg(target_os = "macos")]
fn run_canary(grant_target: bool) -> Option<bool> {
    use std::process::{Command, Stdio};

    Command::new(SANDBOX_EXEC)
        .arg("-p")
        .arg(canary_profile(grant_target))
        .arg("/bin/cat")
        .arg(CANARY_TARGET)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .map(|s| s.success())
}

#[cfg(target_os = "macos")]
fn probe() -> SandboxResult<Probe> {
    let Some(control) = read_target() else {
        return Err(SandboxError::Unavailable {
            backend: NAME,
            why: "the self-test could not run /bin/cat at all".into(),
        });
    };

    // Grant first, or a later denial proves nothing.
    let granted = run_canary(true).ok_or_else(|| SandboxError::Unavailable {
        backend: NAME,
        why: "sandbox-exec could not run the self-test profile".into(),
    })?;

    if !granted {
        return verdict(control, true, false);
    }

    let denied = run_canary(false).ok_or_else(|| SandboxError::Unavailable {
        backend: NAME,
        why: "sandbox-exec could not run the self-test profile".into(),
    })?;

    verdict(control, false, denied)
}

#[cfg(target_os = "macos")]
impl Backend for MacBackend {
    fn name(&self) -> &'static str {
        NAME
    }

    fn available(&self) -> SandboxResult<()> {
        if !std::path::Path::new(SANDBOX_EXEC).is_file() {
            return Err(SandboxError::Unavailable {
                backend: NAME,
                why: format!("has no {SANDBOX_EXEC} on this system"),
            });
        }

        Ok(())
    }

    fn selftest(&self) -> SandboxResult<Probe> {
        use std::sync::OnceLock;

        static CACHE: OnceLock<Result<Probe, String>> = OnceLock::new();

        let cached = CACHE.get_or_init(|| match probe() {
            Ok(p) => Ok(p),
            Err(e) => Err(e.to_string()),
        });

        match cached {
            Ok(p) => Ok(p.clone()),
            Err(why) => Err(SandboxError::Unavailable {
                backend: NAME,
                why: why.clone(),
            }),
        }
    }

    fn plan(&self, policy: &Policy) -> SandboxResult<Plan> {
        self.available()?;

        // On disk is not enforcing. Refuse rather than pretend.
        self.selftest()?;

        macos_plan(policy)
    }

    fn apply(&self, plan: &Plan, _cmd: &mut Command) -> SandboxResult<Guard> {
        if !matches!(plan, Plan::Macos(_)) {
            return Err(SandboxError::Apply {
                backend: NAME,
                why: "plan does not belong to this backend".into(),
            });
        }

        Ok(Guard::none())
    }
}

pub fn wrap(plan: &Plan, shell: &str, command: &str) -> SandboxResult<Vec<String>> {
    let Plan::Macos(p) = plan else {
        return Ok(vec![shell.to_string(), "-c".into(), command.to_string()]);
    };

    Ok(vec![
        SANDBOX_EXEC.into(),
        "-p".into(),
        p.profile.clone(),
        shell.into(),
        "-c".into(),
        command.into(),
    ])
}
