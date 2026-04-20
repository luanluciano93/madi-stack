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

    #[error("{}", format_port_busy(.component, .port, .occupier.as_ref()))]
    PortBusy {
        component: Component,
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
// call sites.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn format_port_busy(
    component: &Component,
    port: &u16,
    occupier: Option<&PortOccupier>,
) -> String {
    let base = match occupier {
        Some(o) => {
            let name = o.process_name.as_deref().unwrap_or("<unknown>");
            format!("port {port} is already in use (pid {}, {name})", o.pid)
        }
        None => format!("port {port} is already in use"),
    };
    match occupier.and_then(|o| competitor_hint(*component, o)) {
        Some(hint) => format!("{base} — {hint}"),
        None => base,
    }
}

/// Recognize common competitors for each component's default port so the UI
/// can tell the user "stop USBWebserver" instead of a bare `AddrInUse`.
///
/// Matching is case-insensitive on the exe filename. We only look at the
/// filename (not path) because portable stacks ship the same exe under many
/// parent directories.
fn competitor_hint(component: Component, occupier: &PortOccupier) -> Option<String> {
    let name = occupier.process_name.as_deref()?.to_ascii_lowercase();

    match component {
        Component::MariaDb => {
            // MariaDB ships as `mariadbd.exe` on newer builds; older and many
            // portable distros keep the MySQL-era `mysqld.exe` / variants.
            if name.starts_with("mysqld") || name.starts_with("mariadbd") {
                Some(
                    "outro MariaDB/MySQL já está rodando (USBWebserver, XAMPP, \
                     WampServer ou o serviço oficial do MySQL). Pare-o antes de \
                     iniciar o MadiStack ou mude a porta em Configurações."
                        .into(),
                )
            } else {
                None
            }
        }
        Component::Nginx => {
            if name == "nginx.exe" {
                Some("outro nginx já está rodando — pare-o ou mude a porta.".into())
            } else if name == "w3wp.exe" || name == "inetinfo.exe" {
                Some(
                    "IIS está escutando na porta 80. Pare o serviço \"World Wide \
                     Web Publishing Service\" ou mude a porta em Configurações."
                        .into(),
                )
            } else if name == "httpd.exe" {
                Some("Apache está escutando nesta porta (XAMPP/WAMP?).".into())
            } else {
                None
            }
        }
        Component::Php => {
            if name.starts_with("php-cgi") || name.starts_with("php-fpm") {
                Some("outro PHP FastCGI já está na porta.".into())
            } else {
                None
            }
        }
        Component::PhpMyAdmin => None,
    }
}
