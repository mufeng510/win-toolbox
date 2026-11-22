//! 平台相关的底层能力(从 HashRename 精简移植)。
//!
//! 只保留核心业务需要的部分:内部文件前缀、大小写敏感性、进程存活探测、
//! no-replace 重命名、带重试的重命名、随机 token。
//! 右键菜单注册 / 控制台附加等外壳能力已随 GUI/CLI 一并移除。

use std::io;
use std::path::Path;
use std::time::Duration;

/// 内部文件(临时文件/journal/锁文件)统一前缀。扫描时排除。
pub const INTERNAL_PREFIX: &str = ".hashrename";

/// 文件系统对文件名是否大小写敏感(用于冲突检测)。
pub fn case_sensitive_fs() -> bool {
    cfg!(target_os = "linux")
}

/// 当前进程 ID。
pub fn current_pid() -> u32 {
    std::process::id()
}

/// 判断目标名是否与已占用名冲突(根据平台大小写敏感性)。
#[allow(dead_code)]
pub fn names_conflict(a: &str, b: &str) -> bool {
    if case_sensitive_fs() {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}

/// 以不覆盖已有文件的方式重命名。目标已存在时返回 AlreadyExists 错误,
/// 绝不覆盖用户文件。
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    rename_no_replace_impl(from, to)
}

#[cfg(target_os = "linux")]
fn rename_no_replace_impl(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_from = CString::new(from.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "路径包含 NUL 字节"))?;
    let c_to = CString::new(to.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "路径包含 NUL 字节"))?;

    let ret = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            c_from.as_ptr(),
            libc::AT_FDCWD,
            c_to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if ret == 0 {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    match err.raw_os_error() {
        // ENOSYS: 内核太老;EINVAL: 文件系统/内核不支持该标志 → 回退
        Some(libc::ENOSYS) | Some(libc::EINVAL) => fallback_stat_then_rename(from, to),
        _ => Err(err),
    }
}

#[cfg(target_os = "macos")]
fn rename_no_replace_impl(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_from = CString::new(from.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "路径包含 NUL 字节"))?;
    let c_to = CString::new(to.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "路径包含 NUL 字节"))?;

    let ret = unsafe { libc::renamex_np(c_from.as_ptr(), c_to.as_ptr(), libc::RENAME_EXCL) };
    if ret == 0 {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::ENOSYS) | Some(libc::EINVAL) => fallback_stat_then_rename(from, to),
        _ => Err(err),
    }
}

#[cfg(windows)]
fn rename_no_replace_impl(from: &Path, to: &Path) -> io::Result<()> {
    fn to_wide(p: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        let mut v: Vec<u16> = p.as_os_str().encode_wide().collect();
        v.push(0);
        v
    }
    // 不带 MOVEFILE_REPLACE_EXISTING:目标存在时系统 API 直接失败,不会覆盖。
    let ret = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            to_wide(from).as_ptr(),
            to_wide(to).as_ptr(),
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    };
    if ret == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn rename_no_replace_impl(from: &Path, to: &Path) -> io::Result<()> {
    fallback_stat_then_rename(from, to)
}

/// 回退方案:先检查目标不存在再 rename(存在极小 TOCTOU 窗口)。
#[allow(dead_code)]
fn fallback_stat_then_rename(from: &Path, to: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "目标已存在"));
    }
    std::fs::rename(from, to)
}

/// 带简单重试的重命名(Windows 上文件可能被杀毒软件/索引服务短暂占用)。
/// AlreadyExists / NotFound 不重试(那是真实状态,不是暂时锁)。
pub fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut last = match std::fs::rename(from, to) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    for _ in 0..2 {
        match last.kind() {
            io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound => return Err(last),
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(80));
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// 判断进程是否存活(用于目录锁的陈旧检测)。
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0):0 表示仅探测。ESRCH = 不存在;EPERM = 存在但属其他用户。
        let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if r == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            CloseHandle(h);
            true
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

/// 生成一次运行的随机 token(用于临时文件/journal 命名,避免多实例冲突)。
pub fn gen_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h1 = std::collections::hash_map::RandomState::new().build_hasher();
    h1.write_u128(nanos);
    let a = h1.finish();
    let mut h2 = std::collections::hash_map::RandomState::new().build_hasher();
    h2.write_u64(a);
    let b = h2.finish();
    format!("{a:016x}{b:016x}")
}
