//! Registry and PE version FFI for [`super::SystemGameHost`] (#57): raw
//! `extern "system"` declarations, the same pattern as `GetDiskFreeSpaceExW` in
//! `install::preflight`; no new crate. Reads only.

use std::io;
use std::path::{Path, PathBuf};

/// Battle.net's D2R uninstall entry, under `...\CurrentVersion\Uninstall`.
/// shortcut: the fixed key name Battle.net uses; enumerate every uninstall key by
/// DisplayName if a store or locale ever names it differently.
#[cfg(windows)]
const D2R_UNINSTALL_KEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Diablo II Resurrected";

#[cfg(windows)]
fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(Some(0)).collect()
}

/// See [`super::GameHost::battle_net_install_locations`].
#[cfg(windows)]
pub(super) fn battle_net_install_locations() -> Vec<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            hkey: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut u16,
            bytes: *mut u32,
        ) -> i32;
    }
    // Predefined keys are sign-extended `(HKEY)(LONG)0x8000000N`.
    const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
    const HKEY_LOCAL_MACHINE: isize = 0x8000_0002_u32 as i32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x2;
    const RRF_SUBKEY_WOW6464KEY: u32 = 0x1_0000;
    const RRF_SUBKEY_WOW6432KEY: u32 = 0x2_0000;

    let key = wide(D2R_UNINSTALL_KEY.as_ref());
    let value = wide("InstallLocation".as_ref());
    let mut found = Vec::new();
    for hkey in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [RRF_SUBKEY_WOW6464KEY, RRF_SUBKEY_WOW6432KEY] {
            let flags = RRF_RT_REG_SZ | view;
            let mut bytes = 0u32;
            // SAFETY: NUL-terminated strings; a null data pointer queries the size.
            let status = unsafe {
                RegGetValueW(
                    hkey,
                    key.as_ptr(),
                    value.as_ptr(),
                    flags,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut bytes,
                )
            };
            if status != 0 || bytes < 2 {
                continue;
            }
            let mut buf = vec![0u16; bytes as usize / 2];
            // SAFETY: `buf` holds `bytes` bytes; RegGetValueW NUL-terminates REG_SZ.
            let status = unsafe {
                RegGetValueW(
                    hkey,
                    key.as_ptr(),
                    value.as_ptr(),
                    flags,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr(),
                    &mut bytes,
                )
            };
            if status != 0 {
                continue;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let text = OsString::from_wide(&buf[..len]);
            if !text.to_string_lossy().trim().is_empty() {
                found.push(PathBuf::from(text));
            }
        }
    }
    found
}

#[cfg(not(windows))]
pub(super) fn battle_net_install_locations() -> Vec<PathBuf> {
    // shortcut: the product targets Windows only; non-Windows dev builds detect nothing.
    Vec::new()
}

/// See [`super::GameHost::product_version`] (`GetFileVersionInfoW`/`VerQueryValueW`).
#[cfg(windows)]
pub(super) fn product_version(file: &Path) -> io::Result<Option<String>> {
    #[link(name = "version")]
    extern "system" {
        fn GetFileVersionInfoSizeW(file: *const u16, handle: *mut u32) -> u32;
        fn GetFileVersionInfoW(file: *const u16, handle: u32, len: u32, data: *mut u8) -> i32;
        fn VerQueryValueW(
            block: *const u8,
            sub_block: *const u16,
            buffer: *mut *const u16,
            len: *mut u32,
        ) -> i32;
    }
    const ERROR_RESOURCE_DATA_NOT_FOUND: i32 = 1812;
    const ERROR_RESOURCE_TYPE_NOT_FOUND: i32 = 1813;

    let path = wide(file.as_os_str());
    let mut ignored = 0u32;
    // SAFETY: NUL-terminated path and a valid out-pointer.
    let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), &mut ignored) };
    if size == 0 {
        let e = io::Error::last_os_error();
        return match e.raw_os_error() {
            Some(ERROR_RESOURCE_DATA_NOT_FOUND | ERROR_RESOURCE_TYPE_NOT_FOUND) => Ok(None),
            _ => Err(e),
        };
    }
    // u32 elements keep the block aligned for the u16 reads below.
    let mut block = vec![0u32; (size as usize).div_ceil(4)];
    let data = block.as_mut_ptr().cast::<u8>();
    // SAFETY: `block` holds at least `size` bytes.
    if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, data) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // Copies `units` u16s of a sub-block out of `block`; `bytes` says whether the
    // returned length counts bytes (binary values) or characters (strings).
    let query = |sub: &str, bytes: bool| -> Option<Vec<u16>> {
        let sub = wide(sub.as_ref());
        let mut ptr = std::ptr::null::<u16>();
        let mut len = 0u32;
        // SAFETY: `data` is the block GetFileVersionInfoW filled; out-pointers valid.
        let ok = unsafe { VerQueryValueW(data, sub.as_ptr(), &mut ptr, &mut len) };
        if ok == 0 || ptr.is_null() {
            return None;
        }
        let units = if bytes { len / 2 } else { len } as usize;
        // SAFETY: on success `ptr` points at `len` bytes or characters inside `block`.
        Some(unsafe { std::slice::from_raw_parts(ptr, units) }.to_vec())
    };
    // Each Translation entry is (language, code page).
    let mut tables: Vec<String> = query(r"\VarFileInfo\Translation", true)
        .map(|t| {
            t.chunks_exact(2)
                .map(|p| format!("{:04x}{:04x}", p[0], p[1]))
                .collect()
        })
        .unwrap_or_default();
    tables.extend(["040904b0".into(), "040904e4".into()]);
    for table in tables {
        if let Some(chars) = query(&format!(r"\StringFileInfo\{table}\ProductVersion"), false) {
            let text = String::from_utf16_lossy(&chars);
            let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
            if !text.is_empty() {
                return Ok(Some(text.to_string()));
            }
        }
    }
    Ok(None)
}

#[cfg(not(windows))]
pub(super) fn product_version(_file: &Path) -> io::Result<Option<String>> {
    // shortcut: the product targets Windows only; non-Windows dev builds read no versions.
    Ok(None)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn reads_product_version_of_a_signed_windows_binary() {
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        let version = product_version(notepad)
            .unwrap()
            .expect("notepad has a version");
        // e.g. 10.0.26100.1; the first component is the Windows major version.
        assert!(version.starts_with("10.0."), "{version}");
    }

    #[test]
    fn a_file_without_version_info_is_none_and_a_missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.exe");
        std::fs::write(&plain, b"not a PE file").unwrap();
        assert_eq!(product_version(&plain).unwrap(), None);
        assert!(product_version(&dir.path().join("absent.exe")).is_err());
    }

    #[test]
    fn registry_lookup_never_fails() {
        // No assertion on contents: the machine may or may not have D2R installed.
        for path in battle_net_install_locations() {
            assert!(!path.as_os_str().is_empty());
        }
    }
}
