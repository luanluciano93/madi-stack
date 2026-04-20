//! Windows Job Object wrapper with kill-on-close semantics.
//!
//! A [`Job`] is created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Every child
//! process assigned via [`Job::assign`] is terminated by the kernel the moment
//! the last handle to the job is released — including when the parent (us)
//! crashes. This is the whole point: if MadiStack dies, nginx/php-cgi/mysqld
//! die with it. No zombies holding ports or lock files.
//!
//! # Production caveat
//!
//! For a bulletproof supervisor, children must be spawned with
//! `CREATE_SUSPENDED`, assigned to the job, then resumed. Otherwise a child
//! could fork a grandchild in the microseconds between spawn and assign, and
//! that grandchild would escape the job. Today [`Job::assign`] does the
//! post-spawn attach — fine for services that don't fork (nginx/php-cgi/mysqld),
//! but not a general primitive. TODO(sprint-2): add `spawn_suspended_in_job`.

use std::io;
use std::mem;
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::process::Child;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// RAII handle to a Windows Job Object. Attached processes are terminated
/// when this value is dropped.
pub struct Job(HANDLE);

impl Job {
    /// Create a new unnamed job with `KILL_ON_JOB_CLOSE` set.
    pub fn new() -> io::Result<Self> {
        // SAFETY: FFI call with null security attributes and null name.
        // Returns a valid HANDLE on success; we lift errors into io::Error.
        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(to_io)?;
        let job = Job(handle);

        // SAFETY: `info` is a freshly zeroed POD. We only touch the one
        // LimitFlags field we care about.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        // SAFETY: hjob is the job we just created; the info pointer and size
        // match what JobObjectExtendedLimitInformation expects.
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(info).cast(),
                u32::try_from(mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap(),
            )
        }
        .map_err(to_io)?;

        Ok(job)
    }

    /// Attach an already-spawned child to this job.
    ///
    /// Holds no reference to `child` — the caller keeps ownership and can
    /// still `wait`/`kill` normally.
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        // SAFETY: see `assign_raw`. The handle is borrowed from `child`.
        unsafe { self.assign_raw(child.as_raw_handle()) }
    }

    /// Attach a process to the job by raw handle.
    ///
    /// # Safety
    ///
    /// `handle` must be a currently-valid process handle with
    /// `PROCESS_SET_QUOTA | PROCESS_TERMINATE` rights (the default rights for
    /// a handle returned by `CreateProcess`). Used to support
    /// `tokio::process::Child`, which does not expose `AsRawHandle` directly.
    pub unsafe fn assign_raw(&self, handle: RawHandle) -> io::Result<()> {
        let hproc = HANDLE(handle.cast());
        // SAFETY: caller guarantees `handle` is a live process handle; self.0
        // is a live job handle owned by this struct.
        unsafe { AssignProcessToJobObject(self.0, hproc) }.map_err(to_io)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: self.0 was produced by CreateJobObjectW and has not been
        // closed elsewhere. Closing the last handle triggers
        // KILL_ON_JOB_CLOSE on every attached process.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

// Job Object handles are not thread-affine — standard Win32 kernel handles
// are usable from any thread as long as we serialise our own access.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

fn to_io(e: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(e.code().0)
}
