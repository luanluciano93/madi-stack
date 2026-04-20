//! Process supervisor for Nginx, PHP-CGI and MariaDB.
//!
//! One [`Supervisor`] instance owns all managed children. Each running child
//! gets its own Windows Job Object, so:
//!
//! * If the supervisor is dropped (e.g. the GUI crashes) every attached
//!   process is terminated by the kernel — no orphan `nginx.exe` sitting on
//!   port 80, no stray `mysqld.exe` holding the `ibdata1` lock.
//! * [`Supervisor::stop`] tries the service-specific graceful path first
//!   (`nginx -s quit`, `mysqladmin shutdown`) before falling back to
//!   `TerminateProcess` via `Child::kill`.
//!
//! The supervisor is Windows-only. A stub is provided on other platforms so
//! the workspace still builds under `cargo check --workspace` on Linux CI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use madi_core::{Component, PortConfig, ServiceStatus};
use madi_logs::LogBuffer;
use parking_lot::Mutex;

use crate::{ServiceError, ServiceResult};

/// Default timeout for graceful shutdown before we fall back to kill().
const GRACEFUL_TIMEOUT: Duration = Duration::from_secs(10);

/// Handle returned when a service is started. Cheap to clone.
#[derive(Debug, Clone)]
pub struct ServiceHandle {
    pub component: Component,
    pub pid: u32,
    pub working_dir: PathBuf,
}

#[cfg(windows)]
mod imp {
    use super::{
        Arc, Component, Duration, HashMap, LogBuffer, Mutex, Path, PathBuf, PortConfig,
        ServiceError, ServiceHandle, ServiceResult, ServiceStatus, GRACEFUL_TIMEOUT,
    };
    use std::net::Ipv4Addr;
    use std::process::Stdio;

    use madi_logs::LogStream;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::{Child, Command};
    use tokio::time::timeout;
    use tracing::{info, warn};

    use crate::job::Job;

    /// `CREATE_NO_WINDOW` — keeps child consoles off the user's screen.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    struct Running {
        child: Child,
        pid: u32,
        // Dropping the job handle triggers `KILL_ON_JOB_CLOSE` on the child.
        // Kept alive for the lifetime of the Running entry.
        _job: Job,
        working_dir: PathBuf,
    }

    pub struct Supervisor {
        install_dir: PathBuf,
        // Live port config: reads are cheap (parking_lot::RwLock), writes
        // happen from the Configurações "Salvar" path. Keeps running
        // children untouched — port changes apply on the next stop+start,
        // which matches the copy in `frontend/src/routes/Configuracoes.svelte`.
        ports: parking_lot::RwLock<PortConfig>,
        running: Arc<Mutex<HashMap<Component, Running>>>,
        // One persistent LogBuffer per component, kept across stop/start so
        // the GUI can still read backlog from the previous run while the
        // service is offline.
        logs: Arc<Mutex<HashMap<Component, Arc<LogBuffer>>>>,
    }

    impl Supervisor {
        pub fn new(install_dir: PathBuf, ports: PortConfig) -> Self {
            Self {
                install_dir,
                ports: parking_lot::RwLock::new(ports),
                running: Arc::new(Mutex::new(HashMap::new())),
                logs: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        pub fn set_ports(&self, ports: PortConfig) {
            *self.ports.write() = ports;
        }

        /// Get (or lazily create) the persistent log buffer for a component.
        /// Always returns the same `Arc` for a given component, so callers
        /// can `subscribe()` once and keep receiving across restarts.
        pub fn logs(&self, component: Component) -> Arc<LogBuffer> {
            let mut g = self.logs.lock();
            g.entry(component).or_insert_with(LogBuffer::new).clone()
        }

        pub fn install_dir(&self) -> &Path {
            &self.install_dir
        }

        pub fn ports(&self) -> PortConfig {
            *self.ports.read()
        }

        pub async fn start(&self, component: Component) -> ServiceResult<ServiceHandle> {
            if matches!(component, Component::PhpMyAdmin) {
                // phpMyAdmin is static files served by nginx — nothing to spawn.
                return Err(ServiceError::NotInstalled(component));
            }

            if self.running.lock().contains_key(&component) {
                return Err(ServiceError::AlreadyRunning);
            }

            let spec = self.spawn_spec(component)?;
            preflight_port(component, &self.ports.read())?;
            ensure_runtime_dirs(component, &self.install_dir)?;

            if matches!(component, Component::MariaDb) {
                bootstrap_mariadb(&self.install_dir).await?;
                // Idempotent: noop if pma DB already exists or pma not yet
                // extracted. Runs while mysqld is offline (uses --bootstrap).
                bootstrap_phpmyadmin_db(&self.install_dir).await?;
            }

            let mut cmd = Command::new(&spec.program);
            cmd.args(&spec.args)
                .current_dir(&spec.cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(CREATE_NO_WINDOW);

            let mut child = cmd.spawn().map_err(ServiceError::Io)?;
            let pid = child
                .id()
                .ok_or_else(|| ServiceError::Io(std::io::Error::other("child has no pid")))?;

            let job = Job::new().map_err(ServiceError::Io)?;
            let raw = child
                .raw_handle()
                .ok_or_else(|| ServiceError::Io(std::io::Error::other("child has no handle")))?;
            // SAFETY: raw comes from a live tokio Child we still own.
            if let Err(e) = unsafe { job.assign_raw(raw) } {
                // Best-effort kill so we don't leak the process.
                let _ = child.start_kill();
                return Err(ServiceError::Io(e));
            }

            // Hand stdout/stderr off to background readers BEFORE inserting
            // the child into the running map — taking them is &mut, while
            // anything in the map sits behind a Mutex.
            let log_buf = self.logs(component);
            if let Some(stdout) = child.stdout.take() {
                spawn_log_pump(component, stdout, LogStream::Stdout, log_buf.clone());
            }
            if let Some(stderr) = child.stderr.take() {
                spawn_log_pump(component, stderr, LogStream::Stderr, log_buf);
            }

            info!(
                component = component.slug(),
                pid, "service started"
            );

            let handle = ServiceHandle {
                component,
                pid,
                working_dir: spec.cwd.clone(),
            };

            self.running.lock().insert(
                component,
                Running {
                    child,
                    pid,
                    _job: job,
                    working_dir: spec.cwd,
                },
            );

            Ok(handle)
        }

        pub async fn stop(&self, component: Component) -> ServiceResult<()> {
            let Some(mut running) = self.running.lock().remove(&component) else {
                return Err(ServiceError::NotRunning);
            };

            // php-cgi has no graceful path on Windows — TerminateProcess is
            // safe because it's stateless. Kill up-front so the wait() below
            // returns immediately instead of eating GRACEFUL_TIMEOUT. For the
            // other services we run the component-specific graceful helper
            // (`nginx -s quit`, `mysqladmin shutdown`).
            let graceful: Result<(), std::io::Error> = match component {
                Component::Php => {
                    let _ = running.child.start_kill();
                    Ok(())
                }
                _ => self.graceful_stop(component, &running.working_dir).await,
            };

            match timeout(GRACEFUL_TIMEOUT, running.child.wait()).await {
                Ok(Ok(status)) => {
                    info!(
                        component = component.slug(),
                        pid = running.pid,
                        ?status,
                        graceful_ok = graceful.is_ok(),
                        "service stopped gracefully"
                    );
                    Ok(())
                }
                Ok(Err(e)) => Err(ServiceError::Io(e)),
                Err(_) => {
                    warn!(
                        component = component.slug(),
                        pid = running.pid,
                        "graceful shutdown timed out — calling TerminateProcess"
                    );
                    let _ = running.child.start_kill();
                    // Drain the zombie. Bounded — once kill is issued the
                    // kernel tears the process down promptly.
                    let _ = timeout(Duration::from_secs(3), running.child.wait()).await;
                    Err(ServiceError::ShutdownTimeout(GRACEFUL_TIMEOUT.as_secs()))
                }
            }
        }

        pub fn status(&self, component: Component) -> ServiceStatus {
            let mut guard = self.running.lock();
            let Some(r) = guard.get_mut(&component) else {
                return ServiceStatus::Stopped;
            };
            match r.child.try_wait() {
                Ok(Some(_)) => {
                    // Reaped unexpectedly — drop it so the next start() can proceed.
                    guard.remove(&component);
                    ServiceStatus::Crashed
                }
                // Either still alive, or try_wait itself failed — in both
                // cases we treat it as alive rather than silently dropping
                // a live child from our tracking map.
                Ok(None) | Err(_) => ServiceStatus::Running,
            }
        }

        pub async fn stop_all(&self) {
            let components: Vec<Component> = self.running.lock().keys().copied().collect();
            for c in components {
                if let Err(e) = self.stop(c).await {
                    warn!(component = c.slug(), error = %e, "stop_all: stop failed");
                }
            }
        }

        // --- internals -----------------------------------------------------

        fn spawn_spec(&self, component: Component) -> ServiceResult<SpawnSpec> {
            let install = &self.install_dir;
            let config_dir = install.join("config");

            let spec = match component {
                Component::Nginx => {
                    let cwd = install.join("bin").join("nginx");
                    let exe = cwd.join("nginx.exe");
                    require_exe(component, &exe)?;
                    SpawnSpec {
                        program: exe,
                        args: vec![
                            "-p".into(),
                            cwd.to_string_lossy().into_owned(),
                            "-c".into(),
                            config_dir.join("nginx.conf").to_string_lossy().into_owned(),
                        ],
                        cwd,
                    }
                }
                Component::Php => {
                    let cwd = install.join("bin").join("php");
                    let exe = cwd.join("php-cgi.exe");
                    require_exe(component, &exe)?;
                    // nginx always talks to FastCGI on 127.0.0.1 regardless of
                    // PortConfig.bind_address — see PortConfig docs.
                    let bind = format!("{}:{}", Ipv4Addr::LOCALHOST, self.ports.read().php_fcgi);
                    SpawnSpec {
                        program: exe,
                        args: vec![
                            "-b".into(),
                            bind,
                            "-c".into(),
                            config_dir.join("php.ini").to_string_lossy().into_owned(),
                        ],
                        cwd,
                    }
                }
                Component::MariaDb => {
                    let base = install.join("bin").join("mariadb");
                    let cwd = base.join("bin");
                    let exe = cwd.join("mysqld.exe");
                    require_exe(component, &exe)?;
                    SpawnSpec {
                        program: exe,
                        args: vec![
                            format!(
                                "--defaults-file={}",
                                config_dir.join("my.ini").to_string_lossy()
                            ),
                            "--console".into(),
                            "--standalone".into(),
                        ],
                        cwd,
                    }
                }
                Component::PhpMyAdmin => return Err(ServiceError::NotInstalled(component)),
            };

            Ok(spec)
        }

        /// Kick off the component-specific graceful shutdown. Returns once the
        /// helper process exits (or fails to spawn). Does NOT wait for the
        /// supervised child itself — [`stop`] does that with a timeout.
        async fn graceful_stop(
            &self,
            component: Component,
            cwd: &Path,
        ) -> Result<(), std::io::Error> {
            match component {
                Component::Nginx => {
                    let exe = cwd.join("nginx.exe");
                    let config = self.install_dir.join("config").join("nginx.conf");
                    run_helper(
                        &exe,
                        &[
                            "-p".as_ref(),
                            cwd.as_os_str(),
                            "-c".as_ref(),
                            config.as_os_str(),
                            "-s".as_ref(),
                            "quit".as_ref(),
                        ],
                        cwd,
                    )
                    .await
                }
                Component::MariaDb => {
                    let exe = cwd.join("mysqladmin.exe");
                    if !exe.exists() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "mysqladmin.exe not found",
                        ));
                    }
                    let port_arg = format!("--port={}", self.ports.read().mariadb);
                    // Read the root password from the secrets file. Empty
                    // (legacy install or missing file) → omit the -p flag and
                    // hope for password-less auth; if that fails the outer
                    // timeout in stop() will fall back to TerminateProcess.
                    let pw = crate::secrets::load(&self.install_dir)
                        .ok()
                        .flatten()
                        .map(|s| s.mariadb_root_password)
                        .unwrap_or_default();
                    let pw_arg = format!("--password={pw}");
                    let mut args: Vec<&std::ffi::OsStr> = vec![
                        "-u".as_ref(),
                        "root".as_ref(),
                        "--protocol=tcp".as_ref(),
                        port_arg.as_ref(),
                    ];
                    if !pw.is_empty() {
                        args.push(pw_arg.as_ref());
                    }
                    args.push("shutdown".as_ref());
                    run_helper(&exe, &args, cwd).await
                }
                // Php is handled directly in `stop()` because the child has
                // already been removed from `self.running` by the time this
                // runs.
                Component::Php | Component::PhpMyAdmin => Ok(()),
            }
        }
    }

    impl Drop for Supervisor {
        fn drop(&mut self) {
            // On drop, every Job handle in `running` closes and
            // KILL_ON_JOB_CLOSE reaps the children. No blocking wait here —
            // we can't call async code from Drop anyway, and the kernel
            // cleanup is synchronous.
        }
    }

    struct SpawnSpec {
        program: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
    }

    fn require_exe(component: Component, exe: &Path) -> ServiceResult<()> {
        if exe.is_file() {
            Ok(())
        } else {
            Err(ServiceError::NotInstalled(component))
        }
    }

    /// Initialize MariaDB's data directory on first start.
    ///
    /// Runs `mariadb-install-db.exe --datadir=<data/mysql>` if the data dir
    /// has no `mysql` system database yet. Takes 3–10s the first time; fast
    /// enough that we can do it inline from `start()` without returning a
    /// separate "starting" status.
    ///
    /// On first bootstrap a 24-char random root password is generated and
    /// persisted to `madistack-secrets.toml` (gitignored). It is passed to
    /// `mariadb-install-db --password=` so the resulting `mysql.user` row
    /// already carries the hash — no temporary "empty root" window.
    ///
    /// Returns early if:
    /// - data dir already contains a bootstrapped mysql schema, or
    /// - the binary is not present (caller will then fail with NotInstalled).
    async fn bootstrap_mariadb(install_dir: &Path) -> ServiceResult<()> {
        let data_dir = install_dir.join("data").join("mariadb");
        // Presence of the `mysql` system DB folder is a reliable marker —
        // mariadb-install-db creates it and mysqld won't start without it.
        if data_dir.join("mysql").is_dir() {
            return Ok(());
        }

        let tool = install_dir
            .join("bin")
            .join("mariadb")
            .join("bin")
            .join("mariadb-install-db.exe");
        if !tool.is_file() {
            return Err(ServiceError::NotInstalled(Component::MariaDb));
        }

        std::fs::create_dir_all(&data_dir)?;

        // Generate + persist root password BEFORE init so a crash mid-init
        // leaves a recoverable secret on disk instead of a half-initialized
        // server with an unknown password.
        let secrets = crate::secrets::Secrets {
            mariadb_root_password: crate::secrets::generate_password(),
        };
        crate::secrets::save(install_dir, &secrets)?;
        info!(
            data_dir = %data_dir.display(),
            "bootstrapping MariaDB data directory (first run, root password generated)"
        );

        let mut cmd = Command::new(&tool);
        cmd.arg(format!("--datadir={}", data_dir.display()))
            .arg(format!("--password={}", secrets.mariadb_root_password))
            .current_dir(tool.parent().unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);

        let status = timeout(Duration::from_secs(60), cmd.status())
            .await
            .map_err(|_| {
                ServiceError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "mariadb-install-db timed out after 60s",
                ))
            })?
            .map_err(ServiceError::Io)?;

        if !status.success() {
            return Err(ServiceError::Io(std::io::Error::other(format!(
                "mariadb-install-db exited with {status}"
            ))));
        }
        info!("MariaDB data directory initialized");
        Ok(())
    }

    /// Create the `phpmyadmin` system database and tables on first start.
    ///
    /// Uses `mysqld --bootstrap`, which reads SQL from stdin without opening
    /// a network port — runs while the real mysqld is offline (we call it
    /// from `start()` before spawning the supervised mysqld). This avoids
    /// the chicken-and-egg of needing a client connection just to seed the
    /// schema.
    ///
    /// Idempotent — the upstream `create_tables.sql` opens with
    /// `CREATE DATABASE IF NOT EXISTS`, but as a cheap fast-path we skip the
    /// whole call if `data/mariadb/phpmyadmin/` already exists.
    ///
    /// Returns Ok (no-op) when:
    /// - the pma DB folder is already present, or
    /// - phpMyAdmin has not been extracted yet (no `sql/create_tables.sql`).
    ///   The next MariaDB start will retry once pma is downloaded.
    async fn bootstrap_phpmyadmin_db(install_dir: &Path) -> ServiceResult<()> {
        let pma_db_dir = install_dir
            .join("data")
            .join("mariadb")
            .join("phpmyadmin");
        if pma_db_dir.is_dir() {
            return Ok(());
        }

        let sql_file = install_dir
            .join("bin")
            .join("phpmyadmin")
            .join("sql")
            .join("create_tables.sql");
        if !sql_file.is_file() {
            return Ok(());
        }

        let mysqld = install_dir
            .join("bin")
            .join("mariadb")
            .join("bin")
            .join("mysqld.exe");
        if !mysqld.is_file() {
            return Err(ServiceError::NotInstalled(Component::MariaDb));
        }

        let data_dir = install_dir.join("data").join("mariadb");
        let sql_body = tokio::fs::read(&sql_file).await?;

        info!("seeding phpMyAdmin control database");

        let mut cmd = Command::new(&mysqld);
        cmd.arg("--bootstrap")
            .arg(format!("--datadir={}", data_dir.display()))
            .current_dir(mysqld.parent().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(ServiceError::Io)?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&sql_body).await?;
            stdin.shutdown().await?;
        }

        let output = timeout(Duration::from_secs(60), child.wait_with_output())
            .await
            .map_err(|_| {
                ServiceError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "mysqld --bootstrap timed out after 60s",
                ))
            })?
            .map_err(ServiceError::Io)?;

        if !output.status.success() {
            // Surface stderr so the user can see what SQL choked. mysqld
            // --bootstrap prints fairly readable errors with line numbers.
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ServiceError::Io(std::io::Error::other(format!(
                "mysqld --bootstrap failed ({}): {stderr}",
                output.status
            ))));
        }
        info!("phpMyAdmin database seeded");
        Ok(())
    }

    /// Create runtime directories that the spawned process expects to exist.
    /// Nginx, for example, does not mkdir its `error_log` parent and will
    /// refuse to start with a `CreateFile` error.
    fn ensure_runtime_dirs(component: Component, install_dir: &Path) -> ServiceResult<()> {
        let logs = install_dir.join("logs");
        match component {
            Component::Nginx => std::fs::create_dir_all(logs.join("nginx"))?,
            Component::MariaDb => {
                std::fs::create_dir_all(logs.join("mariadb"))?;
                std::fs::create_dir_all(install_dir.join("tmp"))?;
            }
            Component::Php | Component::PhpMyAdmin => {}
        }
        Ok(())
    }

    fn preflight_port(component: Component, ports: &PortConfig) -> ServiceResult<()> {
        let port = match component {
            Component::Nginx => ports.http,
            Component::Php => ports.php_fcgi,
            Component::MariaDb => ports.mariadb,
            Component::PhpMyAdmin => return Ok(()),
        };
        if crate::is_port_available(port) {
            Ok(())
        } else {
            // The bind failure is authoritative; the occupier lookup is
            // best-effort and only improves the error message.
            let occupier = crate::port_occupier(port);
            Err(ServiceError::PortBusy {
                component,
                port,
                occupier,
            })
        }
    }

    /// Drain a child's stdout or stderr line-by-line into the per-component
    /// `LogBuffer`. The task lives until the pipe closes (which happens when
    /// the child exits), so it self-terminates with no explicit cancellation.
    ///
    /// Lines are pushed without timestamping the source — `LogBuffer::push`
    /// stamps with the current wall clock. Some servers (mysqld with
    /// `log-error`) write their own timestamps; that's fine, the GUI just
    /// shows them inline with our stamp.
    fn spawn_log_pump<R>(
        component: Component,
        reader: R,
        stream: LogStream,
        buf: Arc<LogBuffer>,
    ) where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => buf.push(stream, line),
                    Ok(None) => break, // EOF — child exited or closed pipe
                    Err(e) => {
                        warn!(
                            component = component.slug(),
                            ?stream,
                            error = %e,
                            "log pump read error — ending pump"
                        );
                        break;
                    }
                }
            }
        });
    }

    async fn run_helper(
        program: &Path,
        args: &[&std::ffi::OsStr],
        cwd: &Path,
    ) -> Result<(), std::io::Error> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        let status = timeout(Duration::from_secs(5), cmd.status())
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "helper timed out"))??;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "helper exited with {status}"
            )))
        }
    }

    #[cfg(test)]
    mod imp_tests {
        use super::{bootstrap_phpmyadmin_db, Component, ServiceError};

        #[tokio::test]
        async fn pma_bootstrap_skips_when_db_already_exists() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path().join("data").join("mariadb").join("phpmyadmin"))
                .unwrap();
            // Should be a no-op even though no binary or sql file exists.
            bootstrap_phpmyadmin_db(dir.path()).await.unwrap();
        }

        #[tokio::test]
        async fn pma_bootstrap_skips_when_pma_not_installed() {
            let dir = tempfile::tempdir().unwrap();
            // No bin/phpmyadmin/ at all → silent skip, retried on next start.
            bootstrap_phpmyadmin_db(dir.path()).await.unwrap();
        }

        #[tokio::test]
        async fn pma_bootstrap_errors_when_mariadb_missing() {
            let dir = tempfile::tempdir().unwrap();
            let sql = dir
                .path()
                .join("bin")
                .join("phpmyadmin")
                .join("sql")
                .join("create_tables.sql");
            std::fs::create_dir_all(sql.parent().unwrap()).unwrap();
            std::fs::write(&sql, b"-- noop\n").unwrap();
            // pma is "extracted" but mysqld.exe is absent.
            let err = bootstrap_phpmyadmin_db(dir.path()).await.unwrap_err();
            assert!(matches!(err, ServiceError::NotInstalled(Component::MariaDb)));
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{
        Arc, Component, LogBuffer, Path, PathBuf, PortConfig, ServiceError, ServiceHandle,
        ServiceResult,
    };

    pub struct Supervisor {
        install_dir: PathBuf,
        ports: parking_lot::RwLock<PortConfig>,
    }

    impl Supervisor {
        pub fn new(install_dir: PathBuf, ports: PortConfig) -> Self {
            Self {
                install_dir,
                ports: parking_lot::RwLock::new(ports),
            }
        }

        pub fn install_dir(&self) -> &Path {
            &self.install_dir
        }

        pub fn ports(&self) -> PortConfig {
            *self.ports.read()
        }

        pub fn set_ports(&self, ports: PortConfig) {
            *self.ports.write() = ports;
        }

        pub fn logs(&self, _component: Component) -> Arc<LogBuffer> {
            LogBuffer::new()
        }

        pub async fn start(&self, component: Component) -> ServiceResult<ServiceHandle> {
            Err(ServiceError::NotInstalled(component))
        }

        pub async fn stop(&self, _component: Component) -> ServiceResult<()> {
            Err(ServiceError::NotRunning)
        }

        pub fn status(&self, _component: Component) -> madi_core::ServiceStatus {
            madi_core::ServiceStatus::Stopped
        }

        pub async fn stop_all(&self) {}
    }
}

pub use imp::Supervisor;

#[cfg(test)]
mod tests {
    use super::*;
    use madi_core::PortConfig;

    #[tokio::test]
    async fn start_fails_when_binaries_absent() {
        let sup = Supervisor::new(PathBuf::from("./__does_not_exist__"), PortConfig::default());
        let err = sup.start(Component::Nginx).await.unwrap_err();
        assert!(matches!(err, ServiceError::NotInstalled(Component::Nginx)));
    }

    #[tokio::test]
    async fn stop_on_idle_returns_not_running() {
        let sup = Supervisor::new(PathBuf::from("."), PortConfig::default());
        let err = sup.stop(Component::Nginx).await.unwrap_err();
        assert!(matches!(err, ServiceError::NotRunning));
    }

    #[test]
    fn status_on_idle_is_stopped() {
        let sup = Supervisor::new(PathBuf::from("."), PortConfig::default());
        assert_eq!(sup.status(Component::MariaDb), ServiceStatus::Stopped);
    }

    #[tokio::test]
    async fn phpmyadmin_cannot_be_started() {
        let sup = Supervisor::new(PathBuf::from("."), PortConfig::default());
        let err = sup.start(Component::PhpMyAdmin).await.unwrap_err();
        assert!(matches!(
            err,
            ServiceError::NotInstalled(Component::PhpMyAdmin)
        ));
    }
}
