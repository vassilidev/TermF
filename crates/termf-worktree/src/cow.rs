//! Copy-on-write clones: a file or a whole folder at almost no disk cost, or an error, never a plain copy.

use std::path::Path;

use anyhow::Result;

/// APFS's `clonefile(2)`, which clones a folder in one call. The standard library has no wrapper for it, and
/// `cp -c` falls back to a plain copy where cloning fails.
#[cfg(target_os = "macos")]
pub(crate) fn clone(src: &Path, dest: &Path) -> Result<()> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn clonefile(src: *const c_char, dst: *const c_char, flags: u32) -> c_int;
    }
    const CLONE_NOFOLLOW: u32 = 1;
    let src = CString::new(src.as_os_str().as_bytes())?;
    let dst = CString::new(dest.as_os_str().as_bytes())?;
    // SAFETY: both are valid NUL-terminated paths that outlive the call.
    match unsafe { clonefile(src.as_ptr(), dst.as_ptr(), CLONE_NOFOLLOW) } {
        0 => Ok(()),
        _ => Err(std::io::Error::last_os_error().into()),
    }
}

/// btrfs and XFS reflinks, through GNU cp, which fails rather than copy when it cannot clone.
#[cfg(target_os = "linux")]
pub(crate) fn clone(src: &Path, dest: &Path) -> Result<()> {
    anyhow::ensure!(!dest.exists(), "{} already exists", dest.display());
    let out = crate::command("cp")
        .args(["-a", "--reflink=always", "--"])
        .arg(src)
        .arg(dest)
        .stdin(std::process::Stdio::null())
        .output()?;
    if !out.status.success() {
        if dest.is_dir() {
            let _ = std::fs::remove_dir_all(dest);
        } else {
            let _ = std::fs::remove_file(dest);
        }
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn clone(_: &Path, _: &Path) -> Result<()> {
    anyhow::bail!("copy-on-write clones are not supported on this system")
}
