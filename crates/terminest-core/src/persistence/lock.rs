use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::PersistenceError;

#[cfg(windows)]
mod sys {
    use std::os::windows::io::RawHandle;

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    #[allow(clippy::upper_case_acronyms)]
    pub struct OVERLAPPED {
        pub internal: usize,
        pub internal_high: usize,
        pub offset: u32,
        pub offset_high: u32,
        pub h_event: RawHandle,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn LockFileEx(
            hFile: RawHandle,
            dwFlags: u32,
            dwReserved: u32,
            nNumberOfBytesToLockLow: u32,
            nNumberOfBytesToLockHigh: u32,
            lpOverlapped: *mut OVERLAPPED,
        ) -> i32;

        pub fn UnlockFileEx(
            hFile: RawHandle,
            dwReserved: u32,
            nNumberOfBytesToUnlockLow: u32,
            nNumberOfBytesToUnlockHigh: u32,
            lpOverlapped: *mut OVERLAPPED,
        ) -> i32;

        pub fn GetLastError() -> u32;
    }

    pub const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x00000002;
    pub const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x00000001;
    pub const ERROR_LOCK_VIOLATION: u32 = 33;
    pub const ERROR_SHARING_VIOLATION: u32 = 32;
}

#[cfg(unix)]
mod sys {
    use std::os::unix::io::RawFd;

    #[link(name = "c")]
    extern "C" {
        pub fn flock(fd: RawFd, operation: std::ffi::c_int) -> std::ffi::c_int;
    }

    pub const LOCK_SH: std::ffi::c_int = 1;
    pub const LOCK_EX: std::ffi::c_int = 2;
    pub const LOCK_NB: std::ffi::c_int = 4;
    pub const LOCK_UN: std::ffi::c_int = 8;
}

/// An OS-level advisory file lock providing cross-process mutual exclusion.
///
/// Guaranteed properties:
/// 1. Coordinated across independent OS processes sharing the filesystem.
/// 2. Automatically released by the OS kernel if the owning process crashes or exits.
/// 3. Does NOT routinely delete lock files on unlock (preventing uncoordinated recreate races).
/// 4. Provides RAII release semantics on drop.
#[derive(Debug)]
pub struct FileLock {
    path: PathBuf,
    file: File,
    is_locked: bool,
}

impl FileLock {
    /// Attempts to acquire an exclusive lock on `path` immediately without blocking.
    /// Returns `Ok(Some(FileLock))` if acquired, `Ok(None)` if already locked by another process.
    pub fn try_acquire_exclusive(path: impl AsRef<Path>) -> Result<Option<Self>, PersistenceError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(PersistenceError::Io)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(PersistenceError::Io)?;

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let mut overlapped = std::mem::MaybeUninit::<sys::OVERLAPPED>::zeroed();
            let flags = sys::LOCKFILE_EXCLUSIVE_LOCK | sys::LOCKFILE_FAIL_IMMEDIATELY;
            let ret = unsafe {
                sys::LockFileEx(
                    file.as_raw_handle(),
                    flags,
                    0,
                    1,
                    0,
                    overlapped.as_mut_ptr(),
                )
            };

            if ret != 0 {
                Ok(Some(Self {
                    path,
                    file,
                    is_locked: true,
                }))
            } else {
                let err = unsafe { sys::GetLastError() };
                if err == sys::ERROR_LOCK_VIOLATION || err == sys::ERROR_SHARING_VIOLATION {
                    Ok(None)
                } else {
                    Err(PersistenceError::Io(std::io::Error::from_raw_os_error(
                        err as i32,
                    )))
                }
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let ret = unsafe { sys::flock(file.as_raw_fd(), sys::LOCK_EX | sys::LOCK_NB) };
            if ret == 0 {
                Ok(Some(Self {
                    path,
                    file,
                    is_locked: true,
                }))
            } else {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EWOULDBLOCK)
                    || err.raw_os_error() == Some(libc::EAGAIN)
                {
                    Ok(None)
                } else {
                    Err(PersistenceError::Io(err))
                }
            }
        }
    }

    /// Acquires an exclusive lock on `path`, blocking the current thread with periodic retries
    /// until `timeout` expires.
    pub fn acquire_exclusive(
        path: impl AsRef<Path>,
        timeout: Duration,
    ) -> Result<Self, PersistenceError> {
        let path = path.as_ref();
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(15);

        loop {
            match Self::try_acquire_exclusive(path)? {
                Some(lock) => return Ok(lock),
                None => {
                    if start.elapsed() >= timeout {
                        return Err(PersistenceError::LockTimeout(path.to_path_buf(), timeout));
                    }
                    std::thread::sleep(poll_interval);
                }
            }
        }
    }

    /// Acquires an exclusive lock on `path` asynchronously, yielding to the Tokio runtime
    /// between retry attempts until `timeout` expires.
    pub async fn acquire_exclusive_async(
        path: impl AsRef<Path>,
        timeout: Duration,
    ) -> Result<Self, PersistenceError> {
        let path = path.as_ref();
        let start = tokio::time::Instant::now();
        let poll_interval = Duration::from_millis(15);

        loop {
            match Self::try_acquire_exclusive(path)? {
                Some(lock) => return Ok(lock),
                None => {
                    if start.elapsed() >= timeout {
                        return Err(PersistenceError::LockTimeout(path.to_path_buf(), timeout));
                    }
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    }

    /// Returns the path to the underlying lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Explicitly unlocks the file.
    pub fn unlock(&mut self) -> Result<(), PersistenceError> {
        if !self.is_locked {
            return Ok(());
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let mut overlapped = std::mem::MaybeUninit::<sys::OVERLAPPED>::zeroed();
            let ret = unsafe {
                sys::UnlockFileEx(self.file.as_raw_handle(), 0, 1, 0, overlapped.as_mut_ptr())
            };
            self.is_locked = false;
            if ret != 0 {
                Ok(())
            } else {
                let err = unsafe { sys::GetLastError() };
                Err(PersistenceError::Io(std::io::Error::from_raw_os_error(
                    err as i32,
                )))
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let ret = unsafe { sys::flock(self.file.as_raw_fd(), sys::LOCK_UN) };
            self.is_locked = false;
            if ret == 0 {
                Ok(())
            } else {
                Err(PersistenceError::Io(std::io::Error::last_os_error()))
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_file_lock_mutual_exclusion_same_process() {
        let temp = tempdir().expect("tempdir");
        let lock_path = temp.path().join("test.lock");

        let lock1 = FileLock::acquire_exclusive(&lock_path, Duration::from_secs(1))
            .expect("lock1 acquired");

        // Attempting to acquire lock2 on the same path must return None
        let lock2_try = FileLock::try_acquire_exclusive(&lock_path).expect("try acquire");
        assert!(lock2_try.is_none());

        // Drop lock1 to release
        drop(lock1);

        // Now lock2 can be acquired
        let lock2 = FileLock::try_acquire_exclusive(&lock_path)
            .expect("try acquire")
            .expect("acquired after drop");
        drop(lock2);
    }

    #[test]
    fn test_file_lock_timeout() {
        let temp = tempdir().expect("tempdir");
        let lock_path = temp.path().join("timeout.lock");

        let _lock1 = FileLock::acquire_exclusive(&lock_path, Duration::from_secs(1))
            .expect("lock1 acquired");

        let start = std::time::Instant::now();
        let res = FileLock::acquire_exclusive(&lock_path, Duration::from_millis(60));
        assert!(res.is_err());
        assert!(matches!(
            res.unwrap_err(),
            PersistenceError::LockTimeout(_, _)
        ));
        assert!(start.elapsed() >= Duration::from_millis(50));
    }
}
