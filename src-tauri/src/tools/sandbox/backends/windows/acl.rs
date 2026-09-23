use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
};
use windows::Win32::Security::Authorization::{
    DENY_ACCESS, EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, GRANT_ACCESS, SE_FILE_OBJECT,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};

use crate::tools::sandbox::plan::{SandboxError, SandboxResult};

use super::NAME;

const FULL: u32 = 0x001F_01FF;

fn w(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

// Merge into the existing DACL. Replacing it would drop the user's own
// permissions on their project directory.
fn patch(path: &Path, sid: PSID, grant: bool) -> SandboxResult<()> {
    let wide = w(path);

    let mut existing: *mut ACL = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR(std::ptr::null_mut());

    // SAFETY: every out-param is a valid, initialised slot. descriptor owns
    // the DACL buffer, so it must outlive `existing`.
    let err = unsafe {
        GetNamedSecurityInfoW(
            windows::core::PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut existing),
            None,
            &mut descriptor,
        )
    };

    if err != windows::Win32::Foundation::WIN32_ERROR(0) {
        return Err(SandboxError::Apply {
            backend: NAME,
            why: format!("GetNamedSecurityInfoW({}): {:#x}", path.display(), err.0),
        });
    }

    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FULL,
        grfAccessMode: match grant {
            true => GRANT_ACCESS,
            false => DENY_ACCESS,
        },
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: windows::Win32::Security::Authorization::NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: windows::core::PWSTR(sid.0 as *mut u16),
        },
    };

    let mut merged: *mut ACL = std::ptr::null_mut();

    // SAFETY: `existing` came from GetNamedSecurityInfoW and the entry's
    // ptstrName points at `sid`, which outlives this call.
    let err = unsafe {
        SetEntriesInAclW(Some(std::slice::from_ref(&entry)), Some(existing), &mut merged)
    };

    if err != windows::Win32::Foundation::WIN32_ERROR(0) {
        free_descriptor(descriptor);

        return Err(SandboxError::Apply {
            backend: NAME,
            why: format!("SetEntriesInAclW({}): {:#x}", path.display(), err.0),
        });
    }

    // SAFETY: `merged` is a valid ACL produced above.
    let err = unsafe {
        SetNamedSecurityInfoW(
            windows::core::PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(merged),
            None,
        )
    };

    // SAFETY: both were allocated by Win32 with LocalAlloc and are freed once.
    unsafe {
        if !merged.is_null() {
            let _ = LocalFree(HLOCAL(merged as *mut std::ffi::c_void));
        }

        free_descriptor(descriptor);
    }

    if err != windows::Win32::Foundation::WIN32_ERROR(0) {
        return Err(SandboxError::Apply {
            backend: NAME,
            why: format!("SetNamedSecurityInfoW({}): {:#x}", path.display(), err.0),
        });
    }

    Ok(())
}

fn free_descriptor(sd: PSECURITY_DESCRIPTOR) {
    if !sd.is_invalid() {
        // SAFETY: only ever called on a descriptor GetNamedSecurityInfoW
        // allocated, and exactly once. The DACL pointer aliases into it, so it
        // must not be freed separately.
        unsafe {
            let _ = LocalFree(HLOCAL(sd.0 as *mut std::ffi::c_void));
        }
    }
}

pub fn grant(path: &Path, sid: PSID) -> SandboxResult<()> {
    patch(path, sid, true)
}

// Not dropping this leaves the project readable after Argus exits.
pub fn revoke(path: &Path, sid: PSID) -> SandboxResult<()> {
    patch(path, sid, false)
}

// A hard kill leaves ACEs no destructor would remove.
pub fn revoke_all(paths: &[std::path::PathBuf], sid: PSID) -> Vec<String> {
    paths
        .iter()
        .filter_map(|p| revoke(p, sid).err().map(|e| e.to_string()))
        .collect()
}
