pub mod quote;
use crate::tools::sandbox::plan::{Capability, JobPlan, Plan, SandboxError, SandboxResult};
use crate::tools::sandbox::policy::{FsPolicy, NetPolicy, Policy, Profile};

pub const NAME: &str = "windows";

// Job Objects carry limits and containment but cannot scope the filesystem.
// Until the AppContainer spawn path is wired in, a filesystem policy is a
// hard stop, not a downgrade.

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

pub fn require_container(plan: &JobPlan) -> SandboxResult<()> {
    if plan.fs_grants.is_empty() {
        return Ok(());
    }

    Err(SandboxError::Unsupported {
        backend: NAME,
        what: format!(
            "a filesystem scope of {} path(s) — AppContainer needs a raw CreateProcessW spawn path",
            plan.fs_grants.len()
        ),
    })
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

    fn plan(&self, policy: &Policy) -> SandboxResult<Plan> {
        job_plan(policy)
    }

    fn apply(&self, plan: &Plan, _cmd: &mut Command) -> SandboxResult<Guard> {
        let Plan::Windows(jp) = plan else {
            return Err(SandboxError::Apply {
                backend: NAME,
                why: "plan does not belong to this backend".into(),
            });
        };

        let limits = jp.clone();
        let job = create_job(&limits)?;

        Ok(Guard::post(move |pid| assign_to_job(job, pid)))
    }
}

#[cfg(target_os = "windows")]
mod ffi {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};

    pub struct Job(pub HANDLE);

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn create_job(plan: &JobPlan) -> SandboxResult<ffi::Job> {
    use std::mem::size_of;

    use windows::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };

    let job = unsafe { CreateJobObjectW(None, None) }.map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("CreateJobObject: {err}"),
    })?;

    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();

    // KILL_ON_JOB_CLOSE: an Argus crash must not orphan the tree.
    let mut flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    if let Some(bytes) = plan.memory_bytes {
        flags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        info.ProcessMemoryLimit = bytes as usize;
    }

    if let Some(n) = plan.max_procs {
        flags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.BasicLimitInformation.ActiveProcessLimit = n;
    }

    info.BasicLimitInformation.LimitFlags = flags;

    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("SetInformationJobObject: {err}"),
    })?;

    Ok(ffi::Job(job.0))
}

#[cfg(target_os = "windows")]
fn assign_to_job(job: ffi::Job, pid: u32) -> SandboxResult<()> {
    use windows::Win32::System::JobObjects::AssignProcessToJobObject;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    let proc = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) }.map_err(
        |err| SandboxError::Apply {
            backend: NAME,
            why: format!("OpenProcess({pid}): {err}"),
        },
    )?;

    let res = unsafe { AssignProcessToJobObject(job.0, proc) };

    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(proc);
    }

    res.map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("AssignProcessToJobObject: {err}"),
    })
}
