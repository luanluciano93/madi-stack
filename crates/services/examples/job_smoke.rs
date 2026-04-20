//! Smoke test for the Job Object wrapper.
//!
//! Spawns a long-lived child (`ping -n 60 127.0.0.1`), attaches it to a Job,
//! drops the job handle, and verifies the child was killed by the kernel
//! within 2 seconds. Dropping the handle simulates exactly what happens when
//! the parent process crashes — so "PASS" here means "if MadiStack dies,
//! nginx/php/mysqld die with it".
//!
//! Run with:
//!
//! ```pwsh
//! cargo run -p madi-services --example job_smoke
//! ```

#[cfg(not(windows))]
fn main() {
    eprintln!("job_smoke is Windows-only");
    std::process::exit(0);
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use madi_services::job::Job;
    use sysinfo::{Pid, ProcessesToUpdate, System};

    let mut child = Command::new("cmd")
        .args(["/c", "ping -n 60 127.0.0.1 > NUL"])
        .spawn()?;
    let pid = child.id();
    println!("[1/4] spawned child pid={pid}");

    {
        let job = Job::new()?;
        println!("[2/4] job created with KILL_ON_JOB_CLOSE");
        job.assign(&child)?;
        println!("[3/4] child attached to job");
        // Job handle drops at end of scope → kernel terminates the child.
    }
    println!("[4/4] job handle dropped — polling for child death...");

    let started = Instant::now();
    let deadline = Duration::from_secs(2);
    loop {
        sleep(Duration::from_millis(100));
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let alive = sys.process(Pid::from_u32(pid)).is_some();
        if !alive {
            println!(
                "PASS — child terminated by Job Object in {:?}",
                started.elapsed()
            );
            let _ = child.wait();
            return Ok(());
        }
        if started.elapsed() > deadline {
            // Don't leak the process on failure.
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("FAIL — child {pid} still alive after {deadline:?}").into());
        }
    }
}
