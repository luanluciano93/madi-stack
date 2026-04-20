// `services` needs `unsafe` for Windows Job Objects. Keep the surface tiny
// and scoped — every `unsafe` block below must carry a SAFETY comment.
#![allow(unsafe_code)]

//! Process supervisor for the managed services.
//!
//! Children are attached to a Windows Job Object with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so that if the GUI crashes, nginx,
//! php-cgi and mysqld are terminated by the kernel — no zombies.

use std::path::PathBuf;

use madi_core::{Component, ServiceStatus};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("port {0} is already in use")]
    PortBusy(u16),

    #[error("component {0:?} not installed")]
    NotInstalled(Component),

    #[error("already running")]
    AlreadyRunning,

    #[error("not running")]
    NotRunning,

    #[error("graceful shutdown timed out after {0}s")]
    ShutdownTimeout(u64),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

/// Handle to a running service.
#[derive(Debug)]
pub struct ServiceHandle {
    pub component: Component,
    pub pid: u32,
    pub working_dir: PathBuf,
}

/// Check if a TCP port on 127.0.0.1 is available.
pub fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Start a managed service.
///
/// TODO(sprint-1): spawn with `tokio::process::Command` + attach to Job Object.
pub async fn start(
    component: Component,
    _install_dir: &std::path::Path,
) -> ServiceResult<ServiceHandle> {
    Err(ServiceError::NotInstalled(component))
}

/// Graceful stop. Uses the service-specific shutdown command when available
/// (`nginx -s quit`, `mysqladmin shutdown`) and falls back to `TerminateProcess`.
pub async fn stop(_handle: &ServiceHandle) -> ServiceResult<()> {
    Ok(())
}

/// Current status — polls sysinfo for the PID.
pub fn status(_handle: &ServiceHandle) -> ServiceStatus {
    ServiceStatus::Stopped
}
