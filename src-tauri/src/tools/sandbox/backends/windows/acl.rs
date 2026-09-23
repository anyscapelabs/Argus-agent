use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{LocalFree, HLOCAL, WIN32_ERROR};
use windows::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
    GRANT_ACCESS, NO_MULTIPLE_TRUSTEE, SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
    TRUSTEE_W,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
};

use crate::tools::sandbox::plan::{SandboxError, SandboxResult};

use super::NAME;

fn w(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn why(call: &str, path: &Path, err: WIN32_ERROR) -> SandboxError {
    SandboxError::Apply {
        backend: NAME,
        why: format!("{call}({}): {:#x}", path.display(), err.0),
    }
}

fn free(sd: PSECURITY_DESCRIPTOR) {
    if !sd.is_invalid() {
        // SAFETY: only ever called on a descriptor GetNamedSecurityInfoW
        // allocated, exactly once. The DACL pointer aliases into it, so it
        // must not be freed separately.
        unsafe {
            let _ = LocalFree(HLOCAL(sd.0 as *mut std::ffi::c_void));
        }
    }
}

// Merge into the existing DACL. Replacing it would drop the user's own
// permissions on their own project directory.
fn patch(path: &Path, sid: PSID, mask: u32, grant: bool) -> SandboxResult<()> {
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

    if err != WIN32_ERROR(0) {
        return Err(why("GetNamedSecurityInfoW", path, err));
    }

    // Revoking with DENY_ACCESS would leave a deny ACE behind, breaking the
    // next run. SET_ACCESS with a zero mask is what actually deletes the ACEs
    // this trustee holds.
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: match grant {
            true => mask,
            false => 0,
        },
        grfAccessMode: match grant {
            true => GRANT_ACCESS,
            false => SET_ACCESS,
        },
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: windows::core::PWSTR(sid.0 as *mut u16),
        },
    };

    let mut merged: *mut ACL = std::ptr::null_mut();

    // SAFETY: `existing` came from GetNamedSecurityInfoW and the entry's
    // ptstrName points at `sid`, which outlives this call.
    let err = unsafe {
        SetEntriesInAclW(
            Some(std::slice::from_ref(&entry)),
            Some(existing),
            &mut merged,
        )
    };

    if err != WIN32_ERROR(0) {
        free(descriptor);

        return Err(why("SetEntriesInAclW", path, err));
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

    // SAFETY: both came from Win32 LocalAlloc and are freed once.
    unsafe {
        if !merged.is_null() {
            let _ = LocalFree(HLOCAL(merged as *mut std::ffi::c_void));
        }

        free(descriptor);
    }

    if err != WIN32_ERROR(0) {
        return Err(why("SetNamedSecurityInfoW", path, err));
    }

    Ok(())
}

pub fn grant(path: &Path, sid: PSID, mask: u32) -> SandboxResult<()> {
    patch(path, sid, mask, true)
}

// Not dropping this leaves the project readable by a stale SID after Argus
// exits, and a hard kill skips every Drop that would have cleaned up.
pub fn revoke(path: &Path, sid: PSID) -> SandboxResult<()> {
    patch(path, sid, 0, false)
}

pub fn revoke_all(paths: &[PathBuf], sid: PSID) -> Vec<String> {
    paths
        .iter()
        .filter_map(|p| revoke(p, sid).err().map(|e| e.to_string()))
        .collect()
}
