use std::os::windows::io::FromRawHandle;
use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, GetExitCodeProcess, ResumeThread, TerminateProcess, WaitForSingleObject,
    CREATE_SUSPENDED, EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
};

use crate::tools::sandbox::plan::{SandboxError, SandboxResult};

use super::acl;
use super::appcontainer::{AttrList, Capabilities};
use super::job::Job;
use super::quote::command_line;

const INFINITE: u32 = 0xFFFF_FFFF;

pub struct RawChild {
    pub pid: u32,
    process: HANDLE,
    thread: HANDLE,
    job: Job,
    pub out: Option<std::fs::File>,
    pub err: Option<std::fs::File>,
}

impl RawChild {
    pub fn id(&self) -> Option<u32> {
        Some(self.pid)
    }

    pub fn kill_tree(&self) {
        self.job.terminate();
    }

    pub fn take_stdout(&mut self) -> Option<tokio::fs::File> {
        self.out.take().map(tokio::fs::File::from_std)
    }

    pub fn take_stderr(&mut self) -> Option<tokio::fs::File> {
        self.err.take().map(tokio::fs::File::from_std)
    }

    // A Windows pipe is a synchronous handle, so the wait has to happen off the
    // runtime. HANDLE is Copy but not Send, so it travels as the integer the
    // kernel already treats it as.
    pub async fn wait(&self) -> i64 {
        let handle = self.process.0 as usize;

        match tokio::task::spawn_blocking(move || wait_handle(handle)).await {
            Ok(code) => code,
            Err(_) => -1,
        }
    }

    // Blocking. The caller owns the thread.
    pub fn wait_blocking(&self) -> i64 {
        wait_handle(self.process.0 as usize)
    }
}

fn wait_handle(h: usize) -> i64 {
    let h = HANDLE(h as *mut std::ffi::c_void);

    // SAFETY: `h` is a live process handle, and every caller keeps the owning
    // RawChild alive for the whole wait.
    unsafe {
        let _ = WaitForSingleObject(h, INFINITE);

        let mut code = 0u32;

        match GetExitCodeProcess(h, &mut code) {
            Err(_) => -1,
            Ok(_) => code as i64,
        }
    }
}

impl Drop for RawChild {
    fn drop(&mut self) {
        // SAFETY: each handle is closed exactly once. The job kills the tree on
        // close because it carries JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.
        unsafe {
            let _ = CloseHandle(self.thread);
            let _ = CloseHandle(self.process);
        }
    }
}

fn inherit() -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: BOOL(1),
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Pipe {
    read: HANDLE,
    write: HANDLE,
}

fn make_pipe(sa: *const SECURITY_ATTRIBUTES) -> SandboxResult<Pipe> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();

    // SAFETY: both out-params are valid slots; `sa` outlives the call.
    unsafe { CreatePipe(&mut read, &mut write, Some(sa), 0) }
        .map_err(|err| SandboxError::Spawn(format!("CreatePipe: {err}")))?;

    Ok(Pipe { read, write })
}

fn into_file(h: HANDLE) -> std::fs::File {
    // SAFETY: `h` is a real pipe handle we own; the File takes it over.
    unsafe { std::fs::File::from_raw_handle(h.0 as *mut std::ffi::c_void) }
}

/// Grant, spawn suspended into the job, then revoke — including on every
/// failure path, so a failed spawn never leaves an ACE behind.
pub fn spawn_appcontainer(
    exe: &Path,
    args: &[String],
    cwd: &str,
    caps: &Capabilities,
    job: Job,
    grant_paths: &[(PathBuf, u32)],
    env: Option<&[String]>,
) -> SandboxResult<RawChild> {
    let sid = caps.sid();

    for (p, mask) in grant_paths {
        acl::grant(p, sid, *mask)?;
    }

    let outcome = spawn_inner(exe, args, cwd, caps, &job, env);

    for (p, _) in grant_paths {
        let _ = acl::revoke(p, sid);
    }

    let (pi, out, err) = outcome?;

    Ok(RawChild {
        pid: pi.dwProcessId,
        process: pi.hProcess,
        thread: pi.hThread,
        job,
        out: Some(out),
        err: Some(err),
    })
}

type Spawned = (PROCESS_INFORMATION, std::fs::File, std::fs::File);

// CREATE_UNICODE_ENVIRONMENT is mandatory once lpEnvironment is supplied: a
// plain byte block here corrupts every value with a non-ASCII character.
const CREATE_UNICODE_ENVIRONMENT: windows::Win32::System::Threading::PROCESS_CREATION_FLAGS =
    windows::Win32::System::Threading::PROCESS_CREATION_FLAGS(0x0000_0400);

fn env_block(keep: Option<&[String]>) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();

    let vars: Vec<(String, String)> = match keep {
        Some(keys) => keys
            .iter()
            .filter_map(|k| std::env::var(k).ok().map(|v| (k.clone(), v)))
            .collect(),
        None => std::env::vars().collect(),
    };

    // A sorted, case-insensitive block: Windows expects the variables in
    // alphabetical order, and duplicates with different case are undefined.
    let mut vars = vars;
    vars.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    vars.dedup_by(|a, b| a.0.to_lowercase() == b.0.to_lowercase());

    for (k, v) in vars {
        if k.contains('=') || k.contains('\0') || v.contains('\0') {
            continue;
        }

        out.extend(k.encode_utf16());
        out.push(b'=' as u16);
        out.extend(v.encode_utf16());
        out.push(0);
    }

    // Double NUL terminates the block.
    out.push(0);
    out
}

fn spawn_inner(
    exe: &Path,
    args: &[String],
    cwd: &str,
    caps: &Capabilities,
    job: &Job,
    env: Option<&[String]>,
) -> SandboxResult<Spawned> {
    let sa = inherit();

    let in_pipe = make_pipe(&sa)?;
    let out_pipe = make_pipe(&sa)?;
    let err_pipe = make_pipe(&sa)?;

    let mut info = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: in_pipe.read,
            hStdOutput: out_pipe.write,
            hStdError: err_pipe.write,
            ..Default::default()
        },
        lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST(std::ptr::null_mut()),
    };

    // The attribute list must outlive CreateProcessW, and it names the only
    // three handles the child is allowed to inherit.
    let std_handles = [in_pipe.read, out_pipe.write, err_pipe.write];
    let attrs = AttrList::new(caps, &std_handles)?;
    info.lpAttributeList = attrs.as_ptr();

    let mut argv = vec![exe.to_string_lossy().into_owned()];
    argv.extend(args.iter().cloned());

    let mut cmdline = wide(&command_line(&argv));
    let cwd_w = wide(cwd);
    let exe_w = wide(&exe.to_string_lossy());
    let env = env_block(env);

    let flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT;

    let mut pi = PROCESS_INFORMATION::default();

    // SAFETY: every pointer below refers to a local that outlives the call,
    // the attribute list is live, and pi is initialised.
    let created = unsafe {
        CreateProcessW(
            PCWSTR(exe_w.as_ptr()),
            windows::core::PWSTR(cmdline.as_mut_ptr()),
            None,
            None,
            BOOL(1),
            flags,
            Some(env.as_ptr().cast()),
            PCWSTR(cwd_w.as_ptr()),
            std::ptr::from_ref(&info.StartupInfo),
            &mut pi,
        )
    };

    // stdin write end goes now so the child sees EOF instead of hanging.
    // SAFETY: each handle is closed exactly once here.
    unsafe {
        let _ = CloseHandle(in_pipe.write);
        let _ = CloseHandle(in_pipe.read);
        let _ = CloseHandle(out_pipe.write);
        let _ = CloseHandle(err_pipe.write);
    }

    if let Err(err) = created {
        // SAFETY: the read ends are ours and unused.
        unsafe {
            let _ = CloseHandle(out_pipe.read);
            let _ = CloseHandle(err_pipe.read);
        }

        return Err(SandboxError::Spawn(format!("CreateProcessW: {err}")));
    }

    // Still suspended: not one instruction has run, so nothing can escape yet.
    if let Err(err) = job.assign(pi.hProcess) {
        // SAFETY: the process never resumed, so it is safe to kill and close.
        unsafe {
            let _ = TerminateProcess(pi.hProcess, 1);
            let _ = CloseHandle(pi.hProcess);
            let _ = CloseHandle(pi.hThread);
            let _ = CloseHandle(out_pipe.read);
            let _ = CloseHandle(err_pipe.read);
        }

        return Err(err);
    }

    // SAFETY: assigned to the job first, so the tree is contained.
    if unsafe { ResumeThread(pi.hThread) } == u32::MAX {
        unsafe {
            let _ = TerminateProcess(pi.hProcess, 1);
            let _ = CloseHandle(pi.hProcess);
            let _ = CloseHandle(pi.hThread);
            let _ = CloseHandle(out_pipe.read);
            let _ = CloseHandle(err_pipe.read);
        }

        return Err(SandboxError::Spawn("ResumeThread failed".into()));
    }

    Ok((pi, into_file(out_pipe.read), into_file(err_pipe.read)))
}
