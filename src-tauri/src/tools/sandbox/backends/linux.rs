use std::process::Command as StdCommand;

use tokio::process::Command;

use super::{Backend, Guard};
use crate::tools::sandbox::plan::{
    LinuxPlan, Plan, SandboxError, SandboxResult, SeccompProfile, SECCOMP_DENY,
};
use crate::tools::sandbox::policy::{FsAccess, FsPolicy, Limits, NetPolicy, Policy, Profile};

pub const NAME: &str = "linux";

pub fn linux_plan(policy: &Policy) -> SandboxResult<Plan> {
    if policy.profile == Profile::Host {
        return Ok(Plan::None);
    }

    let fs = match &policy.fs {
        FsPolicy::Unrestricted => Vec::new(),
        FsPolicy::Scoped(rules) => rules.clone(),
    };

    let seccomp = match policy.profile {
        Profile::Restricted => SeccompProfile::Deny(SECCOMP_DENY),
        _ => SeccompProfile::Off,
    };

    Ok(Plan::Linux(LinuxPlan {
        fs,
        net: policy.net.clone(),
        // RLIMIT_NPROC counts every process of the real uid, so a per-sandbox
        // budget is not expressible here; counting per tree needs cgroups.
        limits: Limits {
            procs: None,
            ..policy.limits
        },
        seccomp,
    }))
}

/// Unreadable means unknown, and unknown means try it: `restrict_self` reports
/// `NotEnforced` and the caller fails closed.
#[cfg(target_os = "linux")]
fn lsm_has_landlock() -> bool {
    match std::fs::read_to_string("/sys/kernel/security/lsm") {
        Ok(s) => s.split(',').any(|l| l.trim() == "landlock"),
        Err(_) => true,
    }
}

#[cfg(target_os = "linux")]
pub fn abi_version() -> Option<i32> {
    const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;

    let ret = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<libc::c_void>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };

    if ret < 0 {
        return None;
    }

    Some(ret as i32)
}

#[cfg(target_os = "linux")]
fn syscall_number(name: &str) -> Option<i64> {
    let n = match name {
        "ptrace" => libc::SYS_ptrace,
        "process_vm_readv" => libc::SYS_process_vm_readv,
        "process_vm_writev" => libc::SYS_process_vm_writev,
        "mount" => libc::SYS_mount,
        "umount2" => libc::SYS_umount2,
        "pivot_root" => libc::SYS_pivot_root,
        "chroot" => libc::SYS_chroot,
        "setns" => libc::SYS_setns,
        "unshare" => libc::SYS_unshare,
        "init_module" => libc::SYS_init_module,
        "finit_module" => libc::SYS_finit_module,
        "delete_module" => libc::SYS_delete_module,
        "kexec_load" => libc::SYS_kexec_load,
        "reboot" => libc::SYS_reboot,
        "swapon" => libc::SYS_swapon,
        "swapoff" => libc::SYS_swapoff,
        "acct" => libc::SYS_acct,
        "bpf" => libc::SYS_bpf,
        "userfaultfd" => libc::SYS_userfaultfd,
        "perf_event_open" => libc::SYS_perf_event_open,
        "io_uring_setup" => libc::SYS_io_uring_setup,
        "add_key" => libc::SYS_add_key,
        "keyctl" => libc::SYS_keyctl,
        "request_key" => libc::SYS_request_key,
        _ => return None,
    };

    Some(n as i64)
}

#[cfg(target_os = "linux")]
fn target_arch() -> Option<seccompiler::TargetArch> {
    #[cfg(target_arch = "x86_64")]
    {
        return Some(seccompiler::TargetArch::x86_64);
    }

    #[cfg(target_arch = "aarch64")]
    {
        return Some(seccompiler::TargetArch::aarch64);
    }

    #[allow(unreachable_code)]
    None
}

/// Built before the fork so the pre-exec closure only moves a finished buffer.
#[cfg(target_os = "linux")]
fn build_seccomp(profile: &SeccompProfile) -> SandboxResult<Option<seccompiler::BpfProgram>> {
    let SeccompProfile::Deny(names) = profile else {
        return Ok(None);
    };

    let Some(arch) = target_arch() else {
        return Err(SandboxError::Unsupported {
            backend: NAME,
            what: format!("seccomp on {} (no syscall table)", std::env::consts::ARCH),
        });
    };

    let mut rules: std::collections::BTreeMap<i64, Vec<seccompiler::SeccompRule>> =
        std::collections::BTreeMap::new();

    for name in names.iter() {
        let Some(nr) = syscall_number(name) else {
            continue;
        };

        // Empty rule list: match unconditionally.
        rules.insert(nr, vec![]);
    }

    let filter = seccompiler::SeccompFilter::new(
        rules,
        seccompiler::SeccompAction::Allow,
        seccompiler::SeccompAction::Errno(libc::EPERM as u32),
        arch,
    )
    .map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("seccomp filter: {err}"),
    })?;

    let prog: seccompiler::BpfProgram = filter.try_into().map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("seccomp program: {err}"),
    })?;

    Ok(Some(prog))
}

#[cfg(target_os = "linux")]
fn set_rlimit(resource: libc::c_uint, value: Option<u64>) -> SandboxResult<()> {
    let Some(v) = value else {
        return Ok(());
    };

    let lim = libc::rlimit {
        rlim_cur: v as libc::rlim_t,
        rlim_max: v as libc::rlim_t,
    };

    if unsafe { libc::setrlimit(resource, &lim) } != 0 {
        return Err(SandboxError::Apply {
            backend: NAME,
            why: format!("setrlimit({resource}) failed"),
        });
    }

    Ok(())
}

/// Runs between fork and exec: async-signal-safe calls only, no allocation.
#[cfg(target_os = "linux")]
fn enter_sandbox(plan: &LinuxPlan, prog: Option<seccompiler::BpfProgram>) -> SandboxResult<()> {
    use landlock::{
        Access, AccessFs, AccessNet, NetPort, Ruleset, RulesetAttr, RulesetCreatedAttr,
        RulesetStatus, ABI,
    };

    if unsafe { libc::setpgid(0, 0) } != 0 {
        return Err(SandboxError::Apply {
            backend: NAME,
            why: "setpgid failed".into(),
        });
    }

    set_rlimit(libc::RLIMIT_AS, plan.limits.memory_bytes)?;
    set_rlimit(libc::RLIMIT_CPU, plan.limits.cpu_secs)?;
    set_rlimit(libc::RLIMIT_NOFILE, plan.limits.nofile.map(u64::from))?;
    set_rlimit(libc::RLIMIT_FSIZE, plan.limits.fsize_bytes)?;

    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(SandboxError::Apply {
            backend: NAME,
            why: "PR_SET_NO_NEW_PRIVS failed".into(),
        });
    }

    // ABI::V1 is the request; landlock narrows it to what the running kernel
    // supports. Hand-picking a version from a probe is how rules silently
    // differ between runs.
    let abi = ABI::V1;

    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|err| SandboxError::Apply {
            backend: NAME,
            why: format!("landlock fs access: {err}"),
        })?;

    // Handling a class with no rules attached denies all of it, so an
    // unrestricted policy must not handle the class at all.
    let net_capped = !matches!(plan.net, NetPolicy::Full);

    if net_capped {
        // Network rules landed in ABI 4. Refuse rather than leak.
        let net_abi = ABI::V4;
        let rights = AccessNet::from_all(net_abi);

        if rights.is_empty() {
            return Err(SandboxError::Unsupported {
                backend: NAME,
                what: "network restrictions (kernel 6.7+/Landlock ABI 4 needed)".into(),
            });
        }

        ruleset = ruleset
            .handle_access(rights)
            .map_err(|err| SandboxError::Apply {
                backend: NAME,
                why: format!("landlock net access: {err}"),
            })?;
    }

    let mut created = ruleset.create().map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("landlock ruleset: {err}"),
    })?;

    let read_rights = AccessFs::from_read(abi);
    let write_rights = AccessFs::from_all(abi);

    for access in [FsAccess::Read, FsAccess::Write] {
        let paths: Vec<std::path::PathBuf> = plan
            .fs
            .iter()
            .filter(|r| r.access == access)
            .map(|r| r.path.clone())
            .collect();

        if paths.is_empty() {
            continue;
        }

        let rights = match access {
            FsAccess::Read => read_rights,
            FsAccess::Write => write_rights,
        };

        // A missing path is filtered out by landlock; dropping it only tightens
        // the policy.
        for rule in landlock::path_beneath_rules(&paths, rights).flatten() {
            created = created.add_rule(rule).map_err(|err| SandboxError::Apply {
                backend: NAME,
                why: format!("landlock rule: {err}"),
            })?;
        }
    }

    if let NetPolicy::Ports(ports) = &plan.net {
        for p in ports {
            let rule = NetPort::new(*p, AccessNet::ConnectTcp);

            created = created.add_rule(rule).map_err(|err| SandboxError::Apply {
                backend: NAME,
                why: format!("landlock net rule: {err}"),
            })?;
        }
    }

    let status = created.restrict_self().map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("landlock restrict: {err}"),
    })?;

    // Partial enforcement means requested rights were dropped.
    if status.ruleset != RulesetStatus::FullyEnforced {
        return Err(SandboxError::Apply {
            backend: NAME,
            why: format!("landlock not fully enforced: {:?}", status.ruleset),
        });
    }

    // Last: seccomp can only remove capability.
    if let Some(prog) = prog {
        seccompiler::apply_filter(&prog).map_err(|err| SandboxError::Apply {
            backend: NAME,
            why: format!("seccomp apply: {err}"),
        })?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub struct LinuxBackend;

#[cfg(target_os = "linux")]
impl Backend for LinuxBackend {
    fn name(&self) -> &'static str {
        NAME
    }

    fn available(&self) -> SandboxResult<()> {
        if abi_version().is_none() && !lsm_has_landlock() {
            return Err(SandboxError::Unavailable {
                backend: NAME,
                why: "is not enabled on this kernel".into(),
            });
        }

        Ok(())
    }

    fn plan(&self, policy: &Policy) -> SandboxResult<Plan> {
        self.available()?;
        linux_plan(policy)
    }

    fn apply(&self, plan: &Plan, cmd: &mut Command) -> SandboxResult<Guard> {
        use std::os::unix::process::CommandExt;

        let Plan::Linux(lp) = plan else {
            return Err(SandboxError::Apply {
                backend: NAME,
                why: "plan does not belong to this backend".into(),
            });
        };

        let prog = build_seccomp(&lp.seccomp)?;
        let owned = lp.clone();

        // tokio's Command delegates to std's, which is what owns pre_exec.
        let std_cmd: &mut StdCommand = cmd.as_std_mut();

        unsafe {
            std_cmd.pre_exec(move || {
                enter_sandbox(&owned, prog.clone()).map_err(|err| {
                    std::io::Error::new(std::io::ErrorKind::PermissionDenied, err.to_string())
                })
            });
        }

        Ok(Guard::none())
    }
}
