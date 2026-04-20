// `services` needs `unsafe` for Windows Job Objects. Keep the surface tiny
// and scoped — every `unsafe` block below must carry a SAFETY comment.
#![allow(unsafe_code)]

//! Process supervisor for the managed services.
//!
//! Children are attached to a Windows Job Object with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so that if the GUI crashes, nginx,
//! php-cgi and mysqld are terminated by the kernel — no zombies.
//!
//! The main entry point is [`Supervisor`].

use madi_core::Component;

#[cfg(windows)]
pub mod job;

pub mod ports;
pub mod secrets;
mod supervisor;

pub use ports::{port_occupier, PortOccupier};
pub use secrets::{Secrets, SecretsError};
pub use supervisor::{ServiceHandle, Supervisor};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{}", format_port_busy(.port, .occupier.as_ref()))]
    PortBusy {
        port: u16,
        occupier: Option<PortOccupier>,
    },

    #[error("component {0:?} not installed")]
    NotInstalled(Component),

    #[error("already running")]
    AlreadyRunning,

    #[error("not running")]
    NotRunning,

    #[error("graceful shutdown timed out after {0}s")]
    ShutdownTimeout(u64),

    #[error("secrets error: {0}")]
    Secrets(#[from] SecretsError),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

/// Check if a TCP port on 127.0.0.1 is available.
#[must_use]
pub fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

// thiserror passes the struct field by reference into `{}` — the `&u16` is
// forced by the macro, not something we chose. Clippy's pass-by-ref lint
// would rewrite the signature to take `u16`, breaking the macro-generated
// call site.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn format_port_busy(port: &u16, occupier: Option<&PortOccupier>) -> String {
    match occupier {
        Some(o) => {
            let name = o.process_name.as_deref().unwrap_or("<unknown>");
            format!("port {port} is already in use (pid {}, {name})", o.pid)
        }
        None => format!("port {port} is already in use"),
    }
}
