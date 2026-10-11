//! Platform seams the CLI needs from the host operating system.
//!
//! Three things the rest of the CLI asks of the host are genuinely different on Unix and Windows:
//! identifying and stopping a process, holding a lock file exclusively, and renaming a file without
//! replacing an occupied destination. Everything else is portable `std`. Each difference lives
//! here once, behind a `cfg`, so the command code above reads the same on both platforms.
//!
//! The Unix side keeps the project's original choices (`ps`, `kill`, `flock`, `renameat_with`) so
//! nothing about the Linux/macOS behaviour changes; the Windows side reaches the equivalent Win32
//! API directly rather than shelling to `tasklist`/`taskkill`, which would reintroduce the parsing
//! and locale hazards the Unix code's comments already argue against.

use std::fs::File;
use std::io;
use std::path::Path;

/// A stable identity for a live PID, or `None` if no such process exists.
///
/// The identity is `<start-marker> <name>`, where `name` is a single whitespace-free token (an
/// executable file name) so [`crate::proc::start_signature`] can split it off. Unix reads the
/// absolute start time and command name out of `ps`; Windows reads the process creation time and
/// image name out of the Win32 API.
#[cfg(unix)]
pub fn process_identity(pid: u32) -> Option<String> {
    // `lstart` is the absolute start time; `comm` the executable name. Both are present on Linux
    // and macOS. An exited PID makes `ps` print nothing and exit nonzero.
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart=,comm="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let identity = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!identity.is_empty()).then_some(identity)
}

#[cfg(windows)]
pub fn process_identity(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: the handle is checked before use and closed exactly once; every out-parameter points
    // at a live, correctly typed local.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        let times_ok =
            GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0;

        let mut buffer = [0u16; 260];
        let mut len = buffer.len() as u32;
        // Flags 0 == PROCESS_NAME_WIN32: the Win32 path, which is what an operator sees.
        let name_ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut len) != 0;
        CloseHandle(handle);

        if !times_ok || !name_ok {
            return None;
        }
        let full = String::from_utf16_lossy(&buffer[..len as usize]);
        let name = full.rsplit(['\\', '/']).next().unwrap_or(&full);
        let start = ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
        Some(format!("{start} {name}"))
    }
}

/// Ask a process to stop. Callers MUST validate process identity first.
#[cfg(unix)]
pub fn terminate(pid: u32) -> io::Result<()> {
    // POSIX `kill` rather than a libc dependency; default signal is TERM.
    let status = std::process::Command::new("kill")
        .arg(pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("could not signal PID {pid}")))
    }
}

/// Ask a process to stop. Callers MUST validate process identity first.
///
/// Windows has no `exec`. The `spacetime` version-manager shim SPAWNS the versioned server as a
/// child and waits, where Unix would have replaced its own process image — so the PID this CLI
/// recorded is the shim, and stopping only it would orphan the node still holding :3000. This
/// therefore stops the whole tree: every descendant first, then the recorded root.
#[cfg(windows)]
pub fn terminate(pid: u32) -> io::Result<()> {
    let mut targets = descendants(pid);
    targets.push(pid);

    let mut root_stopped = false;
    for target in targets {
        match terminate_one(target) {
            Ok(()) => root_stopped |= target == pid,
            // A PID that no longer exists has already stopped: success for the root, and simply
            // nothing left to do for a descendant.
            Err(error) if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) => {
                root_stopped |= target == pid;
            }
            Err(_) => {}
        }
    }

    if root_stopped {
        Ok(())
    } else {
        Err(io::Error::other(format!("could not signal PID {pid}")))
    }
}

/// `ERROR_INVALID_PARAMETER`: what `OpenProcess` reports for a PID that does not exist.
#[cfg(windows)]
const ERROR_INVALID_PARAMETER: i32 = 87;

#[cfg(windows)]
fn terminate_one(pid: u32) -> io::Result<()> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    // SAFETY: the handle is checked before use and closed exactly once.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let ok = TerminateProcess(handle, 1);
        CloseHandle(handle);
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Every descendant of `pid`, breadth-first. Best-effort: a snapshot that cannot be taken yields an
/// empty list, so the caller still terminates the root.
#[cfg(windows)]
fn descendants(pid: u32) -> Vec<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut pairs: Vec<(u32, u32)> = Vec::new();
    // SAFETY: the snapshot handle is checked and closed exactly once; `entry` is a live, correctly
    // sized, zero-initialised `PROCESSENTRY32W` whose `dwSize` the API requires to be set.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }

    let mut result: Vec<u32> = Vec::new();
    let mut frontier = vec![pid];
    while let Some(parent) = frontier.pop() {
        for &(child, child_parent) in &pairs {
            if child_parent == parent && child != 0 && !result.contains(&child) {
                result.push(child);
                frontier.push(child);
            }
        }
    }
    result
}

/// Open (creating if necessary) an exclusive advisory lock at `path`. The lock is held for as long
/// as the returned file stays open; a second caller fails immediately rather than blocking.
#[cfg(unix)]
pub fn open_exclusive_lock(path: &Path) -> io::Result<File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(|error| io::Error::from_raw_os_error(error.raw_os_error()))
}

#[cfg(windows)]
pub fn open_exclusive_lock(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    // `share_mode(0)` denies every other open (and delete) of this path until the handle closes —
    // exactly the "one Package install at a time" guarantee `flock` gives on Unix, with no separate
    // unlock step and no `OVERLAPPED` to keep alive.
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
}

/// Move `from` onto `to`, failing with [`io::ErrorKind::AlreadyExists`] if `to` is occupied.
///
/// The guarantee both the staged install and the enable/disable moves need: an occupied destination
/// must fail the whole operation rather than merge two Packages or overwrite one.
#[cfg(unix)]
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        from,
        rustix::fs::CWD,
        to,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|error| io::Error::from_raw_os_error(error.raw_os_error()))
}

#[cfg(windows)]
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let from = wide(from);
    let to = wide(to);
    // Deliberately WITHOUT `MOVEFILE_REPLACE_EXISTING`: an occupied destination makes this fail
    // (ERROR_ALREADY_EXISTS -> `io::ErrorKind::AlreadyExists`) instead of overwriting, which is the
    // guarantee callers report on.
    // SAFETY: both pointers are NUL-terminated buffers that outlive the call.
    let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
