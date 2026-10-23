//! Linux process ownership. Keep the leader unreaped until its group is signalled,
//! so a recycled PID can never become the target of cleanup.
use nix::{
    errno::Errno,
    sys::{
        signal::{Signal, killpg},
        wait::{Id, WaitPidFlag, WaitStatus, waitid},
    },
    unistd::Pid,
};
use std::{
    fmt,
    fs::File,
    io::{self, Read},
    os::{
        fd::OwnedFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) struct OwnedChild {
    child: Child,
    group: Pid,
    reaped: bool,
}

pub(crate) struct Capture {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub error: Option<String>,
}

/// Cleanup failures remain values until the controller explicitly renders them.
/// In particular, the Drop fallback must not allocate a diagnostic String.
#[derive(Debug)]
pub(crate) enum CleanupError {
    AlreadyReaped,
    Group(Errno),
    Wait(io::Error),
}

impl fmt::Display for CleanupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyReaped => f.write_str("XADBTEST-CLEANUP: child already reaped"),
            Self::Group(error) => write!(f, "XADBTEST-CLEANUP-GROUP: {error}"),
            Self::Wait(error) => write!(f, "XADBTEST-CLEANUP-WAIT: {error}"),
        }
    }
}

impl OwnedChild {
    /// Streaming stdin and independently drained stdout/stderr. The controller owns
    /// every nonblocking endpoint and uses the same unreaped process-group lifetime.
    pub fn spawn_stream(
        binary: &Path,
        directory: &Path,
        args: &[String],
        control_socket: &Path,
    ) -> io::Result<(Self, UnixStream, UnixStream, UnixStream)> {
        let (stdin, in_reader) = UnixStream::pair()?;
        let (stdout, out_writer) = UnixStream::pair()?;
        let (stderr, err_writer) = UnixStream::pair()?;
        stdin.set_nonblocking(true)?;
        stdout.set_nonblocking(true)?;
        stderr.set_nonblocking(true)?;
        let child = Command::new(binary)
            .args(args)
            .current_dir(directory)
            .env_clear()
            .env("LANG", "C")
            .env("TZ", "UTC")
            .env("TMPDIR", directory)
            .env("XADBTEST_CONTROL_SOCKET", control_socket)
            .stdin(Stdio::from(OwnedFd::from(in_reader)))
            .stdout(Stdio::from(OwnedFd::from(out_writer)))
            .stderr(Stdio::from(OwnedFd::from(err_writer)))
            .process_group(0)
            .spawn()?;
        let group = Pid::from_raw(child.id() as i32);
        Ok((
            Self {
                child,
                group,
                reaped: false,
            },
            stdin,
            stdout,
            stderr,
        ))
    }

    /// Observe termination without reaping or releasing PID ownership.
    pub fn exited(&self) -> io::Result<bool> {
        if self.reaped {
            return Err(io::Error::other("child already reaped"));
        }
        match waitid(
            Id::Pid(self.group),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        ) {
            Ok(WaitStatus::StillAlive) => Ok(false),
            Ok(_) => Ok(true),
            Err(Errno::EINTR) => Ok(false),
            Err(error) => Err(io::Error::from_raw_os_error(error as i32)),
        }
    }

    pub fn spawn(
        binary: &Path,
        directory: &Path,
        args: &[String],
    ) -> io::Result<(Self, UnixStream, UnixStream)> {
        let (stdout, out_writer) = UnixStream::pair()?;
        let (stderr, err_writer) = UnixStream::pair()?;
        stdout.set_nonblocking(true)?;
        stderr.set_nonblocking(true)?;
        let child = Command::new(binary)
            .args(args)
            .current_dir(directory)
            .env_clear()
            .env("LANG", "C")
            .env("TZ", "UTC")
            .env("TMPDIR", directory)
            .stdin(File::open(directory.join("request.json"))?)
            .stdout(Stdio::from(OwnedFd::from(out_writer)))
            .stderr(Stdio::from(OwnedFd::from(err_writer)))
            .process_group(0)
            .spawn()?;
        let group = Pid::from_raw(child.id() as i32);
        Ok((
            Self {
                child,
                group,
                reaped: false,
            },
            stdout,
            stderr,
        ))
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn capture(
        &mut self,
        mut stdout: UnixStream,
        mut stderr: UnixStream,
        timeout_ms: u64,
        limit: usize,
    ) -> Capture {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut capture = Capture {
            stdout: vec![],
            stderr: vec![],
            timed_out: false,
            error: None,
        };
        loop {
            if Instant::now() >= deadline {
                capture.timed_out = true;
                break;
            }
            let streams_closed =
                match drain(&mut stdout, &mut capture.stdout, limit).and_then(|out_eof| {
                    drain(&mut stderr, &mut capture.stderr, limit).map(|err_eof| out_eof && err_eof)
                }) {
                    Ok(closed) => closed,
                    Err(e) => {
                        capture.error = Some(format!("XADBTEST-OUTPUT: {e}"));
                        break;
                    }
                };
            let exited = match waitid(
                Id::Pid(self.group),
                WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
            ) {
                Ok(WaitStatus::StillAlive) => false,
                Ok(_) => true,
                Err(Errno::EINTR) => continue,
                Err(e) => {
                    capture.error = Some(format!("XADBTEST-WAIT: {e}"));
                    break;
                }
            };
            if Instant::now() >= deadline {
                capture.timed_out = true;
                break;
            }
            // Descendants can inherit either writer. Leader exit alone is not EOF.
            if exited && streams_closed {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        capture
    }

    /// Signal the owned group before reaping its leader. May block in kernel wait.
    /// Errors never imply successful group cleanup. Drop retries best-effort without
    /// formatting diagnostics; callers must use this explicit result for qualification.
    pub fn finish(&mut self) -> Result<ExitStatus, CleanupError> {
        if self.reaped {
            return Err(CleanupError::AlreadyReaped);
        }
        // Always signal before wait/reap, including a normally exited leader.
        match killpg(self.group, Signal::SIGKILL) {
            Ok(()) | Err(Errno::ESRCH) => {
                // Group success only covers its current members. A moved leader
                // is still ours, and its unreaped PID cannot have been recycled.
                let _ = self.child.kill();
            }
            Err(e) => {
                // Best effort for the directly owned handle, but do not hide group cleanup failure.
                let _ = self.child.kill();
                if self.child.wait().is_ok() {
                    self.reaped = true;
                }
                return Err(CleanupError::Group(e));
            }
        }
        let status = self.child.wait().map_err(CleanupError::Wait)?;
        self.reaped = true;
        Ok(status)
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish();
        }
    }
}

fn drain(stream: &mut UnixStream, bytes: &mut Vec<u8>, limit: usize) -> io::Result<bool> {
    // Each call has a finite byte budget even if a worker writes continuously.
    let mut buffer = [0; 8192];
    loop {
        let capacity = buffer.len().min(limit.saturating_sub(bytes.len()) + 1);
        match stream.read(&mut buffer[..capacity]) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                let keep = count.min(limit - bytes.len());
                bytes.extend_from_slice(&buffer[..keep]);
                if count > keep {
                    return Err(io::Error::other("output byte limit exceeded"));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, os::unix::process::ExitStatusExt};

    // Both processes are direct children of the test, so no orphan reaper is needed.
    struct FixtureChild(Child);

    impl Drop for FixtureChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    #[ignore = "subprocess fixture, invoked with a private directory and stdin handshake"]
    fn moved_worker_fixture() {
        use nix::unistd::{getpgid, getppid, setpgid};

        io::stdin().read_exact(&mut [0]).unwrap();
        let parent_group = getpgid(Some(getppid())).unwrap();
        setpgid(Pid::from_raw(0), parent_group).unwrap();
        std::fs::write("worker.moved", b"").unwrap();
        // Bound the original bug's wait without a watchdog signalling a reused PID.
        thread::sleep(Duration::from_secs(5));
        std::fs::write("worker.returned", b"").unwrap();
    }

    fn moved_worker_with_retained_group(directory: &Path) -> (OwnedChild, FixtureChild) {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::tests::moved_worker_fixture",
                "--ignored",
            ])
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut worker = OwnedChild {
            group: Pid::from_raw(child.id() as i32),
            child,
            reaped: false,
        };
        let keeper = FixtureChild(
            Command::new("/bin/sleep")
                .arg("30")
                .process_group(worker.group.as_raw())
                .spawn()
                .unwrap(),
        );
        worker.child.stdin.take().unwrap().write_all(&[1]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !directory.join("worker.moved").exists() {
            assert!(Instant::now() < deadline, "worker did not move groups");
            thread::sleep(Duration::from_millis(2));
        }
        assert_ne!(
            nix::unistd::getpgid(Some(Pid::from_raw(worker.id() as i32))).unwrap(),
            worker.group
        );
        assert_eq!(
            nix::unistd::getpgid(Some(Pid::from_raw(keeper.0.id() as i32))).unwrap(),
            worker.group
        );
        (worker, keeper)
    }

    #[test]
    fn retained_group_does_not_hide_a_moved_worker_from_finish() {
        for _ in 0..3 {
            let work = tempfile::tempdir().unwrap();
            let (mut worker, mut keeper) = moved_worker_with_retained_group(work.path());
            let pid = Pid::from_raw(worker.id() as i32);
            let status = worker.finish().unwrap();
            assert_eq!(
                keeper.0.wait().unwrap().signal(),
                Some(Signal::SIGKILL as i32)
            );
            assert_eq!(status.signal(), Some(Signal::SIGKILL as i32));
            assert!(!work.path().join("worker.returned").exists());
            assert_eq!(
                waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG),
                Err(Errno::ECHILD)
            );
        }
    }

    #[test]
    fn retained_group_does_not_hide_a_moved_worker_from_drop() {
        for _ in 0..3 {
            let work = tempfile::tempdir().unwrap();
            let (worker, mut keeper) = moved_worker_with_retained_group(work.path());
            let pid = Pid::from_raw(worker.id() as i32);
            drop(worker);
            assert_eq!(
                keeper.0.wait().unwrap().signal(),
                Some(Signal::SIGKILL as i32)
            );
            assert!(!work.path().join("worker.returned").exists());
            assert_eq!(
                waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG),
                Err(Errno::ECHILD)
            );
        }
    }

    #[test]
    fn drop_terminates_and_reaps_the_owned_child_without_an_original_group() {
        for _ in 0..3 {
            let child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
            let pid = Pid::from_raw(child.id() as i32);
            drop(OwnedChild {
                group: pid,
                child,
                reaped: false,
            });
            assert_eq!(
                waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG),
                Err(Errno::ECHILD)
            );
        }
    }

    #[test]
    fn drop_after_successful_finish_leaves_no_waitable_child() {
        for _ in 0..3 {
            let work = tempfile::tempdir().unwrap();
            std::fs::write(work.path().join("request.json"), b"").unwrap();
            let (mut child, _, _) =
                OwnedChild::spawn(Path::new("/bin/sleep"), work.path(), &["10".into()]).unwrap();
            let pid = Pid::from_raw(child.id() as i32);
            assert!(!child.finish().unwrap().success());
            drop(child);
            assert_eq!(
                waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG),
                Err(Errno::ECHILD)
            );
        }
    }

    #[test]
    fn repeated_finish_reports_already_reaped_without_touching_the_child() {
        for _ in 0..3 {
            let child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
            let mut owned = OwnedChild {
                group: Pid::from_raw(child.id() as i32),
                child,
                reaped: false,
            };
            owned.finish().unwrap();
            assert!(matches!(owned.finish(), Err(CleanupError::AlreadyReaped)));
        }
    }

    #[test]
    fn exited_leader_does_not_discard_late_bytes_from_inherited_stream() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join("request.json"), b"").unwrap();
        let (mut child, _, _) =
            OwnedChild::spawn(Path::new("/bin/true"), work.path(), &[]).unwrap();
        let (stdout, mut writer) = UnixStream::pair().unwrap();
        let (stderr, err_writer) = UnixStream::pair().unwrap();
        stdout.set_nonblocking(true).unwrap();
        stderr.set_nonblocking(true).unwrap();
        drop(err_writer);
        // An independently owned thread holds the inherited-writer equivalent, without orphaning a process.
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            let _ = writer.write_all(b"late invalid observation\n");
        });
        let capture = child.capture(stdout, stderr, 1000, 1024);
        assert!(child.finish().unwrap().success());
        writer.join().unwrap();
        assert_eq!(capture.stdout, b"late invalid observation\n");
        assert!(!capture.timed_out);
    }

    #[test]
    fn exited_leader_with_open_output_still_reaches_timeout() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join("request.json"), b"").unwrap();
        let (mut child, _, _) =
            OwnedChild::spawn(Path::new("/bin/true"), work.path(), &[]).unwrap();
        let (stdout, _writer) = UnixStream::pair().unwrap();
        let (stderr, _err_writer) = UnixStream::pair().unwrap();
        stdout.set_nonblocking(true).unwrap();
        stderr.set_nonblocking(true).unwrap();
        let capture = child.capture(stdout, stderr, 50, 1024);
        assert!(child.finish().unwrap().success());
        assert!(capture.timed_out);
    }

    #[test]
    fn missing_original_group_still_terminates_the_owned_leader() {
        // No process group with this child's PID exists: it inherits our group.
        let child = Command::new("/bin/sleep").arg("1").spawn().unwrap();
        let mut owned = OwnedChild {
            group: Pid::from_raw(child.id() as i32),
            child,
            reaped: false,
        };
        assert!(!owned.finish().unwrap().success());
    }
}
