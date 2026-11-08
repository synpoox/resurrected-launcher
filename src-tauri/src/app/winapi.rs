//! Shell, registry and process FFI for the slice 4 commands (#74): raw
//! `extern "system"` declarations as in `game::winapi`; no new crate.

use std::io;
use std::path::PathBuf;

/// The per-user Run key Start with Windows writes its one value to.
pub(super) const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
#[cfg(windows)]
const ERROR_FILE_NOT_FOUND: i32 = 2;

#[cfg(windows)]
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
    fn RegSetKeyValueW(
        hkey: isize,
        sub_key: *const u16,
        value: *const u16,
        kind: u32,
        data: *const u16,
        bytes: u32,
    ) -> i32;
    fn RegDeleteKeyValueW(hkey: isize, sub_key: *const u16, value: *const u16) -> i32;
}

/// `HKCU\<key>\<name>` as a string, `None` when absent.
#[cfg(windows)]
pub(super) fn hkcu_value(key: &str, name: &str) -> io::Result<Option<String>> {
    const RRF_RT_REG_SZ: u32 = 0x2;
    let (key, name) = (wide(key), wide(name));
    let mut bytes = 0u32;
    // SAFETY: NUL-terminated strings; a null data pointer queries the size.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut bytes,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status));
    }
    let mut buf = vec![0u16; bytes as usize / 2 + 1];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `bytes` bytes; RegGetValueW NUL-terminates REG_SZ.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            &mut bytes,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status));
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(Some(String::from_utf16_lossy(&buf[..len])))
}

/// Writes `HKCU\<key>\<name>` = `value` (REG_SZ), or deletes it for `None`.
#[cfg(windows)]
pub(crate) fn set_hkcu_value(key: &str, name: &str, value: Option<&str>) -> io::Result<()> {
    const REG_SZ: u32 = 1;
    let (key, name) = (wide(key), wide(name));
    let status = match value {
        Some(v) => {
            let data = wide(v);
            // SAFETY: NUL-terminated strings; `bytes` covers `data` with its NUL.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    data.as_ptr(),
                    (data.len() * 2) as u32,
                )
            }
        }
        // SAFETY: NUL-terminated strings.
        None => match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) }
        {
            ERROR_FILE_NOT_FOUND => 0,
            s => s,
        },
    };
    match status {
        0 => Ok(()),
        s => Err(io::Error::from_raw_os_error(s)),
    }
}

/// `SHGetKnownFolderPath(FOLDERID_SavedGames)`.
#[cfg(windows)]
pub(super) fn saved_games() -> io::Result<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    #[repr(C)]
    struct Guid(u32, u16, u16, [u8; 8]);
    #[link(name = "shell32")]
    extern "system" {
        fn SHGetKnownFolderPath(
            id: *const Guid,
            flags: u32,
            token: isize,
            path: *mut *mut u16,
        ) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        fn CoTaskMemFree(p: *mut std::ffi::c_void);
    }
    // {4C5C32FF-BB9D-43B0-B5B4-2D72E54EAAA4}
    const SAVED_GAMES: Guid = Guid(
        0x4C5C_32FF,
        0xBB9D,
        0x43B0,
        [0xB5, 0xB4, 0x2D, 0x72, 0xE5, 0x4E, 0xAA, 0xA4],
    );
    let mut p = std::ptr::null_mut();
    // SAFETY: valid GUID and out pointer; the returned string is freed below.
    let hr = unsafe { SHGetKnownFolderPath(&SAVED_GAMES, 0, 0, &mut p) };
    let result = if hr == 0 {
        // SAFETY: on success `p` is a NUL-terminated wide string.
        let len = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
        // SAFETY: `len` wide chars were just read from `p`.
        let s = unsafe { std::slice::from_raw_parts(p, len) };
        Ok(PathBuf::from(OsString::from_wide(s)))
    } else {
        Err(io::Error::other(format!(
            "SHGetKnownFolderPath failed: {hr:#x}"
        )))
    };
    // SAFETY: CoTaskMemFree accepts null and the shell-allocated pointer.
    unsafe { CoTaskMemFree(p.cast()) };
    result
}

/// An open process handle, kept from Play so the exit code survives the exit.
#[cfg(windows)]
pub(super) struct ProcessHandle(isize);

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn GetExitCodeProcess(process: isize, code: *mut u32) -> i32;
    fn CloseHandle(h: isize) -> i32;
}

#[cfg(windows)]
impl ProcessHandle {
    /// `None` when the process cannot be opened.
    pub(super) fn open(pid: u32) -> Option<Self> {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        // SAFETY: plain call; a zero handle means failure.
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        (h != 0).then_some(Self(h))
    }

    /// The exit code, `None` while it still runs (or reports STILL_ACTIVE).
    pub(super) fn exit_code(&self) -> Option<i32> {
        const STILL_ACTIVE: u32 = 259;
        let mut code = 0u32;
        // SAFETY: `self.0` is an open process handle.
        let ok = unsafe { GetExitCodeProcess(self.0, &mut code) } != 0;
        (ok && code != STILL_ACTIVE).then_some(code as i32)
    }
}

#[cfg(windows)]
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: closes the handle opened in `open`, once.
        unsafe { CloseHandle(self.0) };
    }
}

// SAFETY: a process handle may be used and closed from any thread.
#[cfg(windows)]
unsafe impl Send for ProcessHandle {}

/// For example "Windows 10.0.26200 x86_64", from `RtlGetVersion`.
#[cfg(windows)]
pub(super) fn os_description() -> String {
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        csd: [u16; 128],
    }
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }
    let mut info = OsVersionInfoW {
        size: std::mem::size_of::<OsVersionInfoW>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        csd: [0; 128],
    };
    // SAFETY: `info` is a correctly sized OSVERSIONINFOW.
    unsafe { RtlGetVersion(&mut info) };
    format!(
        "Windows {}.{}.{} {}",
        info.major,
        info.minor,
        info.build,
        std::env::consts::ARCH
    )
}

#[cfg(not(windows))]
mod other {
    use super::*;
    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }
    pub(in super::super) fn hkcu_value(_: &str, _: &str) -> io::Result<Option<String>> {
        unsupported()
    }
    pub(crate) fn set_hkcu_value(_: &str, _: &str, _: Option<&str>) -> io::Result<()> {
        unsupported()
    }
    pub(in super::super) fn saved_games() -> io::Result<PathBuf> {
        unsupported()
    }
    pub(in super::super) struct ProcessHandle;
    impl ProcessHandle {
        pub(in super::super) fn open(_: u32) -> Option<Self> {
            None
        }
        pub(in super::super) fn exit_code(&self) -> Option<i32> {
            None
        }
    }
    pub(in super::super) fn os_description() -> String {
        std::env::consts::OS.to_owned()
    }
}
#[cfg(not(windows))]
pub(crate) use other::*;

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn hkcu_value_round_trips_and_deletes_under_a_scratch_key() {
        let key = format!(r"Software\resurrected-launcher-test-{}", std::process::id());
        assert_eq!(hkcu_value(&key, "v").unwrap(), None);
        set_hkcu_value(&key, "v", Some(r#""C:\a b\x.exe" --autostart"#)).unwrap();
        assert_eq!(
            hkcu_value(&key, "v").unwrap().as_deref(),
            Some(r#""C:\a b\x.exe" --autostart"#)
        );
        set_hkcu_value(&key, "v", None).unwrap();
        set_hkcu_value(&key, "v", None).unwrap();
        assert_eq!(hkcu_value(&key, "v").unwrap(), None);
        #[link(name = "advapi32")]
        extern "system" {
            fn RegDeleteKeyW(hkey: isize, sub_key: *const u16) -> i32;
        }
        // SAFETY: NUL-terminated string; removes the empty scratch key.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(&key).as_ptr()) };
    }

    #[test]
    fn saved_games_and_own_exit_code() {
        assert!(saved_games().unwrap().is_absolute());
        let me = ProcessHandle::open(std::process::id()).unwrap();
        assert_eq!(me.exit_code(), None, "still running");
        assert!(os_description().starts_with("Windows 10."));
    }
}
