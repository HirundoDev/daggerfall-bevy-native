//! Kernel-attested identity of the client behind a UDS connection.
//!
//! `SO_PEERCRED` is filled in by the kernel when the connection is created:
//! the uid, gid and pid belong to the process that called `connect`, and no
//! byte the client writes can influence them. The server reads this before it
//! reads the first request, which is what makes "the filesystem is the
//! capability" enforceable rather than advisory.

use std::os::unix::net::UnixStream;

use nix::sys::socket::{getsockopt, sockopt, UnixCredentials};

use crate::error::RuntimeError;

/// What the kernel says about the client end of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerInfo {
    /// Real user id of the peer process, as recorded by the kernel.
    pub uid: u32,
    /// Real group id of the peer process, as recorded by the kernel.
    pub gid: u32,
    /// Process id of the peer process, as recorded by the kernel.
    pub pid: i32,
}

/// The server's acceptance rule for peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerPolicy {
    /// Only the process running as this uid may connect.
    OnlyUid(u32),
}

impl PeerPolicy {
    /// The uid this policy accepts.
    pub fn expected_uid(self) -> u32 {
        match self {
            PeerPolicy::OnlyUid(uid) => uid,
        }
    }

    /// Whether `info` satisfies this policy.
    pub fn permits(self, info: &PeerInfo) -> bool {
        info.uid == self.expected_uid()
    }
}

/// Reads the kernel credentials of `stream` without applying any policy.
pub fn credentials(stream: &UnixStream) -> Result<PeerInfo, RuntimeError> {
    let credentials: UnixCredentials =
        getsockopt(stream, sockopt::PeerCredentials).map_err(|error| {
            RuntimeError::PeerCredentials {
                reason: format!("SO_PEERCRED unavailable: {error}"),
            }
        })?;
    Ok(PeerInfo {
        uid: credentials.uid(),
        gid: credentials.gid(),
        pid: credentials.pid(),
    })
}

/// Reads the kernel credentials of `stream` and enforces `policy`.
///
/// Returns the attested identity on success. On a policy mismatch the returned
/// error names both uids but the server still answers with the uniform
/// `unauthorized_peer` code, so a client learns nothing it did not already
/// know. An unreadable credential is also a refusal: the server fails closed.
pub fn authorize(stream: &UnixStream, policy: &PeerPolicy) -> Result<PeerInfo, RuntimeError> {
    let info = credentials(stream)?;
    if policy.permits(&info) {
        Ok(info)
    } else {
        Err(RuntimeError::PeerCredentials {
            reason: format!(
                "peer uid {} is not authorized (expected {})",
                info.uid,
                policy.expected_uid()
            ),
        })
    }
}
