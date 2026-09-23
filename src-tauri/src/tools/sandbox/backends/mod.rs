pub mod linux;
pub mod macos;
pub mod windows;

use tokio::process::Command;

use super::plan::{Plan, SandboxError, SandboxResult};
use super::policy::Policy;

/// What a self-test can say about the local boundary. Settings shows it so
/// someone who suspects a silent downgrade can see the real state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub backend: &'static str,
    pub enforcing: bool,
    pub detail: String,
}

/// Work that must happen after the process exists. Unix installs the boundary
/// between fork and exec, so its guard is empty; Windows has no pre-exec hook.
pub struct Guard {
    action: Option<Box<dyn FnOnce(u32) -> SandboxResult<()> + Send>>,
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard")
            .field("has_action", &self.action.is_some())
            .finish()
    }
}

impl Guard {
    pub fn none() -> Self {
        Self { action: None }
    }

    pub fn post<F>(f: F) -> Self
    where
        F: FnOnce(u32) -> SandboxResult<()> + Send + 'static,
    {
        Self {
            action: Some(Box::new(f)),
        }
    }

    pub fn run(self, pid: u32) -> SandboxResult<()> {
        match self.action {
            Some(f) => f(pid),
            None => Ok(()),
        }
    }
}

pub trait Backend: Send + Sync {
    fn name(&self) -> &'static str;

    fn available(&self) -> SandboxResult<()>;

    /// Run the boundary once and report whether it actually enforces. An
    /// `Err` here is a refusal, never a downgrade.
    fn selftest(&self) -> SandboxResult<Probe>;

    /// Pure translation. No syscalls, so it is tested on every platform.
    fn plan(&self, policy: &Policy) -> SandboxResult<Plan>;

    fn apply(&self, plan: &Plan, cmd: &mut Command) -> SandboxResult<Guard>;
}

/// Refuses everything rather than pretending the policy was enforced.
pub struct Unsupported;

impl Backend for Unsupported {
    fn name(&self) -> &'static str {
        "unsupported"
    }

    fn available(&self) -> SandboxResult<()> {
        Err(SandboxError::Unavailable {
            backend: "unsupported",
            why: format!("has no sandbox backend for {}", std::env::consts::OS),
        })
    }

    fn plan(&self, _policy: &Policy) -> SandboxResult<Plan> {
        self.available().map(|()| Plan::None)
    }

    fn selftest(&self) -> SandboxResult<Probe> {
        self.available()?;
        Err(SandboxError::Unavailable {
            backend: "unsupported",
            why: "has no boundary to test".into(),
        })
    }
    fn apply(&self, _plan: &Plan, _cmd: &mut Command) -> SandboxResult<Guard> {
        self.available().map(|()| Guard::none())
    }
}

pub fn current() -> Box<dyn Backend> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxBackend)
    }

    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacBackend)
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WinBackend)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Box::new(Unsupported)
    }
}

pub fn current_name() -> &'static str {
    current().name()
}
