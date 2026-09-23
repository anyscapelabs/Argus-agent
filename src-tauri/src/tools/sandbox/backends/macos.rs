use crate::tools::sandbox::plan::{Plan, SandboxResult, SbplPlan};
use crate::tools::sandbox::policy::{EnvPolicy, FsAccess, FsPolicy, NetPolicy, Policy, Profile};

pub const NAME: &str = "macos";

pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Apple deprecated `sandbox-exec`. It still works; when it stops, isolated
/// profiles must fail rather than fall through to the host.
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
use crate::tools::sandbox::plan::SandboxError;

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

    fn plan(&self, policy: &Policy) -> SandboxResult<Plan> {
        self.available()?;
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

/// `sandbox-exec` is a launcher, so the boundary is the wrapper rather than a
/// pre-exec hook. Kept separate from `apply` because it changes argv, which
/// makes it testable off-platform.
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
