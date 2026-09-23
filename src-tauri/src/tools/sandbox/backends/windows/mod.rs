pub mod quote;

#[cfg(target_os = "windows")]
pub mod acl;
#[cfg(target_os = "windows")]
pub mod appcontainer;
#[cfg(target_os = "windows")]
pub mod job;
#[cfg(target_os = "windows")]
pub mod spawn;

use std::path::PathBuf;

use super::Probe;
use crate::tools::sandbox::plan::{Capability, JobPlan, Plan, SandboxError, SandboxResult};
use crate::tools::sandbox::policy::{FsAccess, FsPolicy, NetPolicy, Policy, Profile};

pub const NAME: &str = "windows";

// FILE_GENERIC_READ / FILE_GENERIC_WRITE.
pub const READ_MASK: u32 = 0x0012_0089;
pub const WRITE_MASK: u32 = 0x0012_0116;

pub fn job_plan(policy: &Policy) -> SandboxResult<Plan> {
    if policy.profile == Profile::Host {
        return Ok(Plan::None);
    }

    let (capabilities, net_coarse) = match &policy.net {
        NetPolicy::None => (Vec::new(), false),
        // Capability grants are all-or-nothing here: coarser than a port list.
        NetPolicy::Ports(_) | NetPolicy::Full => (vec![Capability::InternetClient], true),
    };

    let fs_grants = match &policy.fs {
        FsPolicy::Unrestricted => Vec::new(),
        FsPolicy::Scoped(rules) => rules.clone(),
    };

    Ok(Plan::Windows(JobPlan {
        memory_bytes: policy.limits.memory_bytes,
        max_procs: policy.limits.procs,
        cpu_secs: policy.limits.cpu_secs,
        app_container: policy.profile.is_isolated(),
        capabilities,
        fs_grants,
        net_coarse,
    }))
}

/// Windows has no per-path deny, only per-path grant, so a write scope needs
/// the write bit and a read scope does not.
pub fn grant_mask(access: FsAccess) -> u32 {
    match access {
        FsAccess::Read => READ_MASK,
        FsAccess::Write => READ_MASK | WRITE_MASK,
    }
}

pub fn grant_list(plan: &JobPlan) -> Vec<(PathBuf, u32)> {
    let mut out: Vec<(PathBuf, u32)> = Vec::new();

    for rule in &plan.fs_grants {
        let mask = grant_mask(rule.access);

        match out.iter_mut().find(|(p, _)| *p == rule.path) {
            // Two rules on one path must merge, or the second grant replaces
            // the first and a write scope ends up read-only.
            Some((_, m)) => *m |= mask,
            None => out.push((rule.path.clone(), mask)),
        }
    }

    out
}

#[cfg(target_os = "windows")]
use crate::tools::sandbox::policy::EnvPolicy;

#[cfg(target_os = "windows")]
pub fn env_keys(policy: &Policy) -> Option<&[String]> {
    match &policy.env {
        EnvPolicy::Inherit => None,
        EnvPolicy::Only(keys) => Some(keys),
    }
}

#[cfg(target_os = "windows")]
pub struct WinBackend;

#[cfg(target_os = "windows")]
use tokio::process::Command;

#[cfg(target_os = "windows")]
use super::{Backend, Guard};

#[cfg(target_os = "windows")]
impl Backend for WinBackend {
    fn name(&self) -> &'static str {
        NAME
    }

    fn available(&self) -> SandboxResult<()> {
        Ok(())
    }

    fn selftest(&self) -> SandboxResult<Probe> {
        use std::sync::OnceLock;

        static CACHE: OnceLock<Result<Probe, String>> = OnceLock::new();

        let cached = CACHE.get_or_init(|| match probe() {
            Ok(p) => Ok(p),
            Err(err) => Err(err.to_string()),
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

        // An AppContainer profile that exists on disk is not a boundary that
        // enforces. Refuse rather than pretend.
        self.selftest()?;

        job_plan(policy)
    }

    fn apply(&self, plan: &Plan, _cmd: &mut Command) -> SandboxResult<Guard> {
        let Plan::Windows(jp) = plan else {
            return Err(SandboxError::Apply {
                backend: NAME,
                why: "plan does not belong to this backend".into(),
            });
        };

        // Containment is applied by the raw spawn path, not post-spawn: the
        // token is minted at CreateProcessW, and there is no later hook that
        // can add it. Reaching here means something asked for a Command we
        // cannot confine.
        Err(SandboxError::Apply {
            backend: NAME,
            why: format!(
                "AppContainer spawns through CreateProcessW, not a Command ({} grant(s) pending)",
                jp.fs_grants.len()
            ),
        })
    }
}

#[cfg(target_os = "windows")]
pub fn spawn(
    exe: &std::path::Path,
    args: &[String],
    cwd: &str,
    jp: &JobPlan,
    env: Option<&[String]>,
) -> SandboxResult<spawn::RawChild> {
    let job = job::create(jp)?;
    let caps = appcontainer::Capabilities::new(&jp.capabilities)?;
    let grants = grant_list(jp);

    spawn::spawn_appcontainer(exe, args, cwd, &caps, job, &grants, env)
}

// Reads a path that exists on every Windows install, so a refusal is a
// boundary decision and never a missing file.
pub const CANARY_TARGET: &str = r"C:\Windows\System32\drivers\etc\hosts";
pub const CANARY_SHELL: &str = r"C:\Windows\System32\cmd.exe";

pub fn canary_verdict(control_ok: bool, with_grant: bool, sandboxed: bool) -> SandboxResult<Probe> {
    if !control_ok {
        return Err(SandboxError::Unavailable {
            backend: NAME,
            why: "the control run failed, so enforcement cannot be told from a broken system"
                .into(),
        });
    }

    // Too tight to start at all: our bug, not the OS ignoring us.
    if with_grant && !sandboxed {
        return Err(SandboxError::Unavailable {
            backend: NAME,
            why: "AppContainer could not read a path it was explicitly granted".into(),
        });
    }

    if !with_grant && sandboxed {
        return Err(SandboxError::Unavailable {
            backend: NAME,
            why: format!("AppContainer read {CANARY_TARGET} with no ACE granting it"),
        });
    }

    Ok(Probe {
        backend: NAME,
        enforcing: true,
        detail: format!("AppContainer enforced a path denial on {CANARY_TARGET}"),
    })
}

#[cfg(target_os = "windows")]
// True when the AppContainer process managed to read the canary target.
fn read_target(grants: &[(PathBuf, u32)]) -> bool {
    let jp = JobPlan {
        memory_bytes: None,
        max_procs: None,
        cpu_secs: None,
        app_container: true,
        capabilities: Vec::new(),
        fs_grants: Vec::new(),
        net_coarse: false,
    };

    let Ok(job) = job::create(&jp) else {
        return false;
    };

    let Ok(caps) = appcontainer::Capabilities::new(&[]) else {
        return false;
    };

    let args = vec![
        "/c".to_string(),
        "type".to_string(),
        CANARY_TARGET.to_string(),
    ];

    let Ok(child) = spawn::spawn_appcontainer(
        std::path::Path::new(CANARY_SHELL),
        &args,
        r"C:\Windows\System32",
        &caps,
        job,
        grants,
        None,
    ) else {
        return false;
    };

    let code = child.wait_blocking();

    if code != 0 {
        return false;
    }

    // Exit code alone cannot tell a read from an empty file, so the bytes have
    // to be read too.
    let mut child = child;
    let mut text = String::new();
    let Some(mut out) = child.out.take() else {
        return false;
    };

    let read = std::io::Read::read_to_string(&mut out, &mut text);

    matches!(read, Ok(n) if n > 0)
}

#[cfg(target_os = "windows")]
// Grant first: a boundary that cannot even read a path it was given is our
// bug, and it must be reported as such rather than as "not enforcing".
fn probe() -> SandboxResult<Probe> {
    let control_ok = std::process::Command::new(CANARY_SHELL)
        .args(["/c", "type", CANARY_TARGET])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let granted = vec![(PathBuf::from(CANARY_TARGET), READ_MASK)];
    let with_grant = read_target(&granted);
    let without = read_target(&[]);

    canary_verdict(control_ok, with_grant, without)
}
