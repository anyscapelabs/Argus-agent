use super::policy::{FsRule, Limits, NetPolicy};

pub const SECCOMP_DENY: &[&str] = &[
    "ptrace",
    "process_vm_readv",
    "process_vm_writev",
    "mount",
    "umount2",
    "pivot_root",
    "chroot",
    "setns",
    "unshare",
    "init_module",
    "finit_module",
    "delete_module",
    "kexec_load",
    "reboot",
    "swapon",
    "swapoff",
    "acct",
    "bpf",
    "userfaultfd",
    "perf_event_open",
    "io_uring_setup",
    "add_key",
    "keyctl",
    "request_key",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeccompProfile {
    Off,
    Deny(&'static [&'static str]),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxPlan {
    pub fs: Vec<FsRule>,
    pub net: NetPolicy,
    pub limits: Limits,
    pub seccomp: SeccompProfile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SbplPlan {
    pub profile: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    InternetClient,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobPlan {
    pub memory_bytes: Option<u64>,
    pub max_procs: Option<u32>,
    pub cpu_secs: Option<u64>,
    pub app_container: bool,
    pub capabilities: Vec<Capability>,
    pub fs_grants: Vec<FsRule>,
    pub net_coarse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    None,
    Linux(LinuxPlan),
    Macos(SbplPlan),
    Windows(JobPlan),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SandboxError {
    Unsupported { backend: &'static str, what: String },
    Unavailable { backend: &'static str, why: String },
    Apply { backend: &'static str, why: String },
    Spawn(String),
    Profile(String),
    OutputLimit(usize),
    Timeout(u64),
    Cancelled,
}

impl SandboxError {
    pub fn backend(&self) -> &'static str {
        match self {
            SandboxError::Unsupported { backend, .. }
            | SandboxError::Unavailable { backend, .. }
            | SandboxError::Apply { backend, .. } => backend,
            _ => "sandbox",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, SandboxError::Timeout(_))
    }
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::Unsupported { backend, what } => write!(
                f,
                "sandbox unavailable: the {backend} backend cannot enforce {what}; \
                 refusing to run this outside a sandbox"
            ),
            SandboxError::Unavailable { backend, why } => write!(
                f,
                "sandbox unavailable: {backend} {why}; refusing to run this outside a sandbox"
            ),
            SandboxError::Apply { backend, why } => {
                write!(f, "sandbox failed to apply ({backend}): {why}")
            }
            SandboxError::Spawn(e) => write!(f, "could not start command: {e}"),
            SandboxError::Profile(p) => write!(
                f,
                "unknown sandbox profile '{p}' — use restricted, project or host"
            ),
            SandboxError::OutputLimit(n) => {
                write!(f, "output exceeded {n} bytes and was truncated")
            }
            SandboxError::Timeout(s) => write!(f, "command timed out after {s}s"),
            SandboxError::Cancelled => write!(f, "stopped"),
        }
    }
}

impl std::error::Error for SandboxError {}

pub type SandboxResult<T> = std::result::Result<T, SandboxError>;
