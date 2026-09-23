use std::ffi::c_void;

use windows::core::Free;
use windows::Win32::Security::{
    CreateWellKnownSid, PSID, SID_AND_ATTRIBUTES, SECURITY_CAPABILITIES, WELL_KNOWN_SID_TYPE,
    WinNetworkSid,
};
use windows::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};

use crate::tools::sandbox::plan::{Capability, SandboxError, SandboxResult};

use super::NAME;

const HRESULT_ALREADY_EXISTS: i32 = 0x8007_00B7u32 as i32;

pub const PROFILE_NAME: &str = "com.argus.sandbox";

// Omitting this is the whole network deny. No capability, no route.
fn net_capability_sid() -> SandboxResult<PSID> {
    const CAP_SID_SIZE: usize = 68;
    let mut buf = vec![0u8; CAP_SID_SIZE];

    let mut len = CAP_SID_SIZE as u32;

    // SAFETY: buf is CAP_SID_SIZE bytes, which is the documented size for this
    // well-known SID.
    unsafe {
        CreateWellKnownSid(
            WELL_KNOWN_SID_TYPE(WinNetworkSid.0),
            None,
            PSID(buf.as_mut_ptr() as *mut c_void),
            &mut len,
        )
    }
    .map_err(|err| SandboxError::Apply {
        backend: NAME,
        why: format!("CreateWellKnownSid(internetClient): {err}"),
    })?;

    Ok(PSID(buf.into_boxed_slice().as_mut_ptr() as *mut c_void))
}

pub struct Sid(pub PSID);

// Both profile APIs allocate.
impl Drop for Sid {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the SID came from an allocating API and is freed once.
            unsafe {
                let _ = self.0.free();
            }
        }
    }
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn p(s: &[u16]) -> windows::core::PCWSTR {
    windows::core::PCWSTR(s.as_ptr())
}

pub fn profile_sid() -> SandboxResult<Sid> {
    let name = w(PROFILE_NAME);
    let display = w("Argus Sandbox");

    // SAFETY: all four pointers live until the call returns; the capability
    // slice is empty so the pointer is ignored.
    let created = unsafe {
        CreateAppContainerProfile(
            p(&name),
            p(&display),
            windows::core::PCWSTR::null(),
            None,
        )
    };

    if let Ok(sid) = created {
        return Ok(Sid(sid));
    }

    // Any other failure is real: a profile that cannot be created means no
    // lowbox token, so there is no boundary to enforce.
    let code = match created {
        Err(err) => err.code().0,
        Ok(_) => unreachable!(),
    };

    if code != HRESULT_ALREADY_EXISTS {
        return Err(SandboxError::Apply {
            backend: NAME,
            why: format!("CreateAppContainerProfile: {code:#x}"),
        });
    }

    // SAFETY: name outlives the call.
    let derived = unsafe { DeriveAppContainerSidFromAppContainerName(p(&name)) }.map_err(|err| {
        SandboxError::Apply {
            backend: NAME,
            why: format!("DeriveAppContainerSidFromAppContainerName: {err}"),
        }
    })?;

    Ok(Sid(derived))
}

pub struct Capabilities {
    sid: Sid,
    net_sid: Option<PSID>,
    _list: Vec<SID_AND_ATTRIBUTES>,
    sec: SECURITY_CAPABILITIES,
}

impl Capabilities {
    pub fn new(caps: &[Capability]) -> SandboxResult<Self> {
        let sid = profile_sid()?;

        let net_sid = match caps.contains(&Capability::InternetClient) {
            true => Some(net_capability_sid()?),
            false => None,
        };

        let mut list: Vec<SID_AND_ATTRIBUTES> = net_sid
            .iter()
            .map(|s| SID_AND_ATTRIBUTES {
                Sid: PSID(s.0),
                Attributes: 0,
            })
            .collect();

        let sec = SECURITY_CAPABILITIES {
            AppContainerSid: sid.0,
            Capabilities: list.as_mut_ptr(),
            CapabilityCount: list.len() as u32,
            Reserved: 0,
        };

        Ok(Self {
            sid,
            net_sid,
            _list: list,
            sec,
        })
    }

    pub fn as_ptr(&self) -> *const SECURITY_CAPABILITIES {
        &self.sec as *const _
    }

    pub fn size(&self) -> usize {
        std::mem::size_of::<SECURITY_CAPABILITIES>()
    }

    pub fn net(&self) -> bool {
        self.net_sid.is_some()
    }
}

pub struct AttrList {
    buf: Vec<u8>,
    ptr: windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttrList {
    pub fn new(caps: &Capabilities) -> SandboxResult<Self> {
        use windows::Win32::System::Threading::{
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, UpdateProcThreadAttribute,
        };

        let mut size = 0usize;

        // Pass 1 asks how much room the list needs. A null pointer with a zero
        // count is the documented way to do this.
        // SAFETY: the null pointer is never dereferenced on this call.
        unsafe {
            InitializeProcThreadAttributeList(
                LPPROC_THREAD_ATTRIBUTE_LIST(std::ptr::null_mut()),
                1,
                0,
                &mut size,
            )
        }
        .map_err(|err| SandboxError::Apply {
            backend: NAME,
            why: format!("InitializeProcThreadAttributeList(size): {err}"),
        })?;

        if size == 0 {
            return Err(SandboxError::Apply {
                backend: NAME,
                why: "the kernel reported a zero-sized attribute list".into(),
            });
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
            InitializeProcThreadAttributeList(ptr, 1, 0, &mut size).map_err(|err| {
                SandboxError::Apply {
                    backend: NAME,
                    why: format!("InitializeProcThreadAttributeList: {err}"),
                }
            })?;
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
        .map_err(|err| SandboxError::Apply {
            backend: NAME,
            why: format!("UpdateProcThreadAttribute(SECURITY_CAPABILITIES): {err}"),
        })?;

        Ok(Self { buf, ptr })
    }

    pub fn as_ptr(&self) -> windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST {
        self.ptr
    }
}

// Dropping this before CreateProcessW is a use-after-free.
impl Drop for AttrList {
    fn drop(&mut self) {
        use windows::Win32::System::Threading::DeleteProcThreadAttributeList;

        // SAFETY: the list was initialized and is dropped exactly once.
        unsafe { DeleteProcThreadAttributeList(self.ptr) };
    }
}
