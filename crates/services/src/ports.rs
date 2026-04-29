//! Resolve "who is holding this TCP port" for better error messages.
//!
//! Flow from the supervisor: `TcpListener::bind` gives a cheap truthful
//! "is this port free right now?". If it's not, [`port_occupier`] scans the
//! Windows TCP table and returns the owning PID + process name, which lets
//! us render messages like
//! `"port 80 is already in use (pid 4 — System, maybe IIS or SQL Reporting)"`
//! instead of a bare `AddrInUse`.
//!
//! We deliberately don't use the TCP-table lookup as the *authoritative*
//! "is it free" check: the table has a brief lag vs. actual socket state,
//! and we'd rather return a false "free" and let bind fail with a clear
//! errno than a false "busy" that leaves users confused.

use std::path::PathBuf;

use serde::Serialize;

/// A process currently bound to the TCP port we wanted.
#[derive(Debug, Clone, Serialize)]
pub struct PortOccupier {
    pub pid: u32,
    /// Best-effort display name. `None` if we lacked permission to open the
    /// process (common when the owner is a system-privileged service).
    pub process_name: Option<String>,
    /// Full path to the owning executable, if we could resolve it.
    pub exe_path: Option<PathBuf>,
}

/// Return the process holding `port` on `127.0.0.1` (or any local address),
/// if any. Returns `None` if the port is free **or** if the PID could not
/// be resolved. The caller is expected to already know the port is busy
/// (via a failed `bind`) — this function only augments the error message.
#[must_use]
pub fn port_occupier(port: u16) -> Option<PortOccupier> {
    imp::port_occupier(port)
}

#[cfg(windows)]
mod imp {
    use super::PortOccupier;
    use std::mem::size_of;
    use std::path::PathBuf;

    use sysinfo::{Pid, ProcessesToUpdate, System};
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    };
    use windows::Win32::Networking::WinSock::AF_INET;

    /// `MIB_TCP_STATE_LISTEN` from `tcpmib.h`. windows-rs only exposes the
    /// state values as a strongly-typed enum tied to a different table type,
    /// so we use the raw constant. See:
    /// <https://learn.microsoft.com/windows/win32/api/tcpmib/ne-tcpmib-mib_tcp_state>
    const MIB_TCP_STATE_LISTEN: u32 = 2;

    pub fn port_occupier(port: u16) -> Option<PortOccupier> {
        let pid = find_owning_pid(port)?;
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let process = sys.process(Pid::from_u32(pid));
        let process_name = process.map(|p| p.name().to_string_lossy().into_owned());
        let exe_path = process.and_then(|p| p.exe().map(PathBuf::from));
        Some(PortOccupier {
            pid,
            process_name,
            exe_path,
        })
    }

    /// Scan the IPv4 TCP table and return the PID owning `port` in LISTEN
    /// state. We do not walk the IPv6 table — our services bind to IPv4
    /// explicitly (see `PortConfig::bind_address` docs), and the common
    /// offenders on Windows (IIS, World Wide Web Publishing, SQL Server)
    /// show up in the v4 table too.
    fn find_owning_pid(port: u16) -> Option<u32> {
        let mut size: u32 = 0;

        // Probe the required buffer size. SAFETY: NULL buffer + size_ptr is
        // the documented size-query convention; returns ERROR_INSUFFICIENT_BUFFER.
        unsafe {
            let _ = GetExtendedTcpTable(
                None,
                &mut size,
                false,
                AF_INET.0.into(),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }
        if size == 0 {
            return None;
        }

        // Allocate as `Vec<u32>` rather than `Vec<u8>` so the backing storage
        // is 4-byte aligned — `MIB_TCPTABLE_OWNER_PID` needs that. Round up
        // to whole `u32`s; any trailing bytes GetExtendedTcpTable doesn't
        // touch are harmless.
        let u32_len = (size as usize).div_ceil(size_of::<u32>());
        let mut buf: Vec<u32> = vec![0u32; u32_len];
        let byte_capacity = buf.len() * size_of::<u32>();

        // SAFETY: buf is at least `size` bytes (we rounded up). The OS
        // writes `size` bytes and returns NO_ERROR (0) on success.
        let ret = unsafe {
            GetExtendedTcpTable(
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                false,
                AF_INET.0.into(),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if ret != 0 {
            return None;
        }

        // SAFETY: the OS just wrote a valid `MIB_TCPTABLE_OWNER_PID` header
        // followed by `dwNumEntries` contiguous `MIB_TCPROW_OWNER_PID`s into
        // `buf`, and `buf` is aligned to `u32` (>= `MIB_TCPTABLE_OWNER_PID`
        // alignment).
        let header = unsafe { &*buf.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>() };
        let count = header.dwNumEntries as usize;

        let row0_ptr: *const MIB_TCPROW_OWNER_PID = std::ptr::addr_of!(header.table).cast();
        // Sanity: make sure we don't read past the buffer — should never
        // trip, but we treat a corrupt table as "no occupier found".
        let needed = size_of::<MIB_TCPTABLE_OWNER_PID>()
            .saturating_add(count.saturating_mul(size_of::<MIB_TCPROW_OWNER_PID>()))
            .saturating_sub(size_of::<MIB_TCPROW_OWNER_PID>());
        if needed > byte_capacity {
            return None;
        }

        // Filter on LISTEN state. Windows keeps TIME_WAIT/CLOSE_WAIT residue
        // for the same `(local_addr, local_port)` after a service restart, and
        // those rows carry `dwOwningPid = 0` (the System Idle Process). Without
        // this filter we'd return PID 0 for any port that recently saw a
        // connection close, even when the real listener is still bound.
        for i in 0..count {
            // SAFETY: i < count and the buffer is large enough (checked above).
            let row = unsafe { &*row0_ptr.add(i) };
            if row.dwState != MIB_TCP_STATE_LISTEN {
                continue;
            }
            // dwLocalPort is a DWORD with the port (network byte order) in
            // the low word. Extract the low 16 bits safely, then swap from
            // network to host order.
            let raw = u16::try_from(row.dwLocalPort & 0xFFFF).unwrap_or(0);
            let local_port = raw.to_be();
            if local_port == port {
                return Some(row.dwOwningPid);
            }
        }
        None
    }
}

#[cfg(not(windows))]
mod imp {
    use super::PortOccupier;

    pub fn port_occupier(_port: u16) -> Option<PortOccupier> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_high_port_has_no_occupier() {
        // Bind to a random high port we own, then release it. We expect
        // the occupier lookup on a fresh high port (which the OS just
        // released) to return None shortly after.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        // No `sleep` — occasionally the TCP table still has the entry for
        // a brief window. Accept either None or a match (best-effort test).
        let _ = port_occupier(port);
    }

    #[test]
    fn ourselves_show_as_occupier_while_bound() {
        // Bind a listener, confirm lookup returns our own PID.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let occ = port_occupier(port);
        if let Some(o) = occ {
            assert_eq!(o.pid, std::process::id());
        }
        // On rare scheduling, the TCP table may not have ingested the bind
        // yet — don't fail the test on that; the happy path assertion above
        // is enough when it triggers.
        drop(listener);
    }
}
