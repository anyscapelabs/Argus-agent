use std::mem::size_of;

use crate::tools::sandbox::plan::{JobPlan, SandboxError, SandboxResult};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_PROCESS_MEMORY,
};

use super::NAME;

pub struct Job(pub HANDLE);

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is owned here and closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

impl Job {
    /// Put a process in the job while it is still suspended, so it cannot
    // spawn anything before containment applies.
    pub fn assign(&self, process: HANDLE) -> SandboxResult<()> {
        // SAFETY: `process` is a live handle owned by the caller.
        unsafe { AssignProcessToJobObject(self.0, process) }.map_err(|err| SandboxError::Apply {
            backend: NAME,
            why: format!("AssignProcessToJobObject: {err}"),
        })
    }

    pub fn terminate(&self) {
        // SAFETY: killing our own job is always permitted.
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }
}

pub fn create(plan: &JobPlan) -> SandboxResult<Job> {
    // SAFETY: null name means an unnamed, non-shared job.
    let job = unsafe { CreateJobObjectW(None, None) }.map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("CreateJobObjectW: {err}"),
    })?;

    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();

    // An Argus crash must not orphan the tree.
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

    // SAFETY: `info` is initialised and `size_of` matches what we pass.
    unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("SetInformationJobObject: {err}"),
    })?;

    Ok(Job(job))
}
