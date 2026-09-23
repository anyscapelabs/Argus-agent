use std::ffi::c_void;

use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL};
use windows::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows::Win32::Security::{
    CreateWellKnownSid, WinNetworkSid, PSID, SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES,
    WELL_KNOWN_SID_TYPE,
};

use crate::tools::sandbox::plan::{Capability, SandboxError, SandboxResult};

use super::NAME;

const HRESULT_ALREADY_EXISTS: i32 = 0x8007_00B7u32 as i32;

pub const PROFILE_NAME: &str = "com.argus.sandbox";

// Owns the buffer a PSID points into, and remembers who has to free it.
//
// The two allocators are not interchangeable: `PSID::free` is FreeSid, which
// only understands AllocateSid memory. A well-known SID lives in our own
// buffer, and both AppContainer profile APIs hand back LocalAlloc memory, so
// calling FreeSid on either is undefined behaviour.
pub struct SidBuf {
    sid: PSID,
    local_alloc: bool,
    _bytes: Option<Box<[u8]>>,
}

impl SidBuf {
    pub fn get(&self) -> PSID {
        self.sid
    }
}

impl Drop for SidBuf {
    fn drop(&mut self) {
        if !self.local_alloc || self.sid.is_invalid() {
            return;
        }

        // SAFETY: only set for a SID one of the two allocating APIs returned,
        // and released exactly once.
        unsafe {
            let _ = LocalFree(HLOCAL(self.sid.0 as *mut c_void));
        }
    }
}

fn ours(bytes: Box<[u8]>, sid: PSID) -> SidBuf {
    SidBuf {
        sid,
        local_alloc: false,
        _bytes: Some(bytes),
    }
}

fn win32(sid: PSID) -> SidBuf {
    SidBuf {
        sid,
        local_alloc: true,
        _bytes: None,
    }
}

// Omitting this capability is the entire network deny: no SID, no route.
fn net_capability() -> SandboxResult<SidBuf> {
    // CreateWellKnownSid's own minimum for WinNetworkSid, rounded up.
    let mut bytes = vec![0u8; 68];
    let mut len = bytes.len() as u32;

    // SAFETY: bytes is at least the documented size for this well-known SID.
    unsafe {
        CreateWellKnownSid(
            WELL_KNOWN_SID_TYPE(WinNetworkSid.0),
            None,
            PSID(bytes.as_mut_ptr() as *mut c_void),
            &mut len,
        )
    }
    .map_err(|err| fail("CreateWellKnownSid(internetClient)", err))?;

    // into_boxed_slice does not reallocate, so the pointer stays valid.
    let sid = PSID(bytes.as_mut_ptr() as *mut c_void);

    Ok(ours(bytes.into_boxed_slice(), sid))
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn p(s: &[u16]) -> windows::core::PCWSTR {
    windows::core::PCWSTR(s.as_ptr())
}

pub fn profile_sid() -> SandboxResult<SidBuf> {
    let name = w(PROFILE_NAME);
    let display = w("Argus Sandbox");

    // SAFETY: all four pointers live until the call returns; the capability
    // slice is empty so the pointer is ignored.
    let created = unsafe { CreateAppContainerProfile(p(&name), p(&display), p(&[]), None) };

    if let Ok(sid) = created {
        return Ok(win32(sid));
    }

    // Any other failure is real: no profile means no lowbox token, so there is
    // no boundary to enforce.
    let code = match created {
        Err(err) => err.code().0,
        Ok(_) => return Err(fail("CreateAppContainerProfile", "reported an error twice")),
    };

    if code != HRESULT_ALREADY_EXISTS {
        return Err(fail("CreateAppContainerProfile", format_args!("{code:#x}")));
    }

    // SAFETY: name outlives the call. The derived SID comes back in LocalAlloc
    // memory, which LocalFree releases.
    let derived = unsafe { DeriveAppContainerSidFromAppContainerName(p(&name)) }
        .map_err(|err| fail("DeriveAppContainerSidFromAppContainerName", err))?;

    Ok(win32(derived))
}

fn fail(call: &str, why: impl std::fmt::Display) -> SandboxError {
    SandboxError::Apply {
        backend: NAME,
        why: format!("{call}: {why}"),
    }
}

pub struct Capabilities {
    profile: SidBuf,
    net: Option<SidBuf>,
    list: Vec<SID_AND_ATTRIBUTES>,
    sec: SECURITY_CAPABILITIES,
}

impl Capabilities {
    pub fn new(caps: &[Capability]) -> SandboxResult<Self> {
        let profile = profile_sid()?;

        let net = match caps.contains(&Capability::InternetClient) {
            true => Some(net_capability()?),
            false => None,
        };

        let mut list: Vec<SID_AND_ATTRIBUTES> = net
            .iter()
            .map(|n| SID_AND_ATTRIBUTES {
                Sid: n.get(),
                Attributes: 0,
            })
            .collect();

        let sec = SECURITY_CAPABILITIES {
            AppContainerSid: profile.get(),
            Capabilities: list.as_mut_ptr(),
            CapabilityCount: list.len() as u32,
            Reserved: 0,
        };

        Ok(Self {
            profile,
            net,
            list,
            sec,
        })
    }

    pub fn sid(&self) -> PSID {
        self.profile.get()
    }

    pub fn net(&self) -> bool {
        self.net.is_some()
    }

    pub fn as_ptr(&self) -> *const SECURITY_CAPABILITIES {
        &self.sec as *const _
    }

    pub fn size(&self) -> usize {
        std::mem::size_of::<SECURITY_CAPABILITIES>()
    }
}

pub struct AttrList {
    // Kept alive because the kernel reads the handle array at CreateProcessW
    // time, not when the attribute is set.
    handles: Vec<HANDLE>,
    buf: Vec<u8>,
    ptr: windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttrList {
    pub fn new(caps: &Capabilities, handles: &[HANDLE]) -> SandboxResult<Self> {
        use windows::Win32::System::Threading::{
            InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
        };

        let count = 2;
        let mut size = 0usize;

        // Pass 1 asks how much room the list needs. A null pointer with a zero
        // count is the documented way to ask.
        // SAFETY: the null pointer is never dereferenced on this call.
        unsafe {
            InitializeProcThreadAttributeList(
                LPPROC_THREAD_ATTRIBUTE_LIST(std::ptr::null_mut()),
                count,
                0,
                &mut size,
            )
        }
        .map_err(|err| fail("InitializeProcThreadAttributeList(size)", err))?;

        if size == 0 {
            return Err(fail(
                "InitializeProcThreadAttributeList",
                "the kernel reported a zero-sized list",
            ));
        }

        // The list is opaque and may need pointer alignment, so over-align the
        // buffer rather than trusting Vec's layout.
        let align = std::mem::align_of::<usize>();
        let mut buf = vec![0u8; size + align];
        let base = buf.as_mut_ptr();
        let offset = base.align_offset(align);

        // SAFETY: offset is < align, and buf has size+align bytes.
        let ptr = unsafe { LPPROC_THREAD_ATTRIBUTE_LIST(base.add(offset) as *mut _) };

        // SAFETY: ptr points at least `size` writable bytes.
        unsafe {
            InitializeProcThreadAttributeList(ptr, count, 0, &mut size)
                .map_err(|err| fail("InitializeProcThreadAttributeList", err))?;
            UpdateProcThreadAttribute(
                ptr,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                Some(caps.as_ptr() as *const c_void),
                caps.size(),
                None,
                None,
            )
        }
        .map_err(|err| fail("UpdateProcThreadAttribute(SECURITY_CAPABILITIES)", err))?;

        // bInheritHandle copies every inheritable handle in the process, not just
        // the std ones. Argus holds its database and sockets on such handles, so
        // a child that inherits them writes argus.db directly and the ACL
        // boundary means nothing. The handle list narrows inheritance to exactly
        // the three pipes.
        let handles = handles.to_vec();

        // SAFETY: handles outlives the CreateProcessW call, and the size is
        // the byte length of that array.
        unsafe {
            UpdateProcThreadAttribute(
                ptr,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(handles.as_ptr() as *const c_void),
                handles.len() * std::mem::size_of::<HANDLE>(),
                None,
                None,
            )
        }
        .map_err(|err| fail("UpdateProcThreadAttribute(HANDLE_LIST)", err))?;

        Ok(Self { handles, buf, ptr })
    }

    pub fn as_ptr(&self) -> windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST {
        self.ptr
    }
}

// Dropping this before CreateProcessW returns is a use-after-free.
impl Drop for AttrList {
    fn drop(&mut self) {
        use windows::Win32::System::Threading::DeleteProcThreadAttributeList;

        // SAFETY: the list was initialized and is dropped exactly once.
        unsafe { DeleteProcThreadAttributeList(self.ptr) };
    }
}
