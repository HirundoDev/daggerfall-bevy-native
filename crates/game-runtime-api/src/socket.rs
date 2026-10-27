//! One Unix domain socket per instance, user-only, cleaned up on drop.
//!
//! The filesystem is the capability (plan 3.3, 3.5): the socket lives at
//! `<dir>/dfr-<instance>.sock`, it is created with `0600`, and a second bind of
//! the same pair is refused rather than reused - two previews must never share
//! control by accident. Abstract sockets are refused on purpose: they have no
//! filesystem entry, hence no permission bits and no second line of defence.

use std::fmt;
use std::fs::Permissions;
use std::io::{ErrorKind, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::RuntimeError;

/// Prefix of every socket file this contract creates.
pub const SOCKET_NAME_PREFIX: &str = "dfr-";

/// Largest accepted resolved path length in bytes.
///
/// `sun_path` is 108 bytes including the terminating NUL; 100 leaves margin for
/// platform differences and for the NUL.
pub const MAX_SOCKET_PATH_BYTES: usize = 100;

/// The permission bits a runtime socket is published with.
pub const SOCKET_MODE: u32 = 0o600;

/// Validated identifier of one runtime instance.
///
/// Grammar `^[a-z0-9][a-z0-9._-]{0,63}$`: lowercase, filename-safe, and it can
/// never contain `@` (abstract-socket syntax) or a path separator.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceId(String);

impl InstanceId {
    /// Parses an instance id, rejecting everything the grammar does not allow.
    pub fn parse(raw: &str) -> Result<Self, RuntimeError> {
        let invalid = |reason: &'static str| RuntimeError::InvalidInstanceId {
            raw: raw.to_string(),
            reason,
        };
        let bytes = raw.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 {
            return Err(invalid("instance id must be 1..=64 characters"));
        }
        let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
        if !first_ok {
            return Err(invalid("instance id must start with a lowercase letter or digit"));
        }
        if !bytes[1..].iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(*b, b'.' | b'_' | b'-')
        }) {
            return Err(invalid("instance id may only contain a-z, 0-9, '.', '_' and '-'"));
        }
        Ok(InstanceId(raw.to_string()))
    }

    /// The canonical text of the instance id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A validated socket path.
///
/// Construction is the only way to obtain one, so every `RuntimeSocket` is
/// guaranteed to have a bounded, non-abstract path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SocketPath(PathBuf);

impl SocketPath {
    /// Resolves `<dir>/dfr-<instance>.sock` and validates the result.
    ///
    /// Refuses a path longer than [`MAX_SOCKET_PATH_BYTES`] (the kernel limit is
    /// 108 including the NUL) and any path containing `@`, the abstract-socket
    /// notation. The directory itself is not created: the caller owns its
    /// workspace.
    pub fn for_instance(dir: &Path, instance: &InstanceId) -> Result<Self, RuntimeError> {
        let path = dir.join(format!("{SOCKET_NAME_PREFIX}{}.sock", instance.as_str()));
        let len = path.as_os_str().as_bytes().len();
        if len > MAX_SOCKET_PATH_BYTES {
            return Err(RuntimeError::SocketPathTooLong { len });
        }
        let text = path.to_string_lossy();
        if text.is_empty() {
            return Err(RuntimeError::InvalidSocketPath {
                path: text.into_owned(),
                reason: "socket path must not be empty",
            });
        }
        if text.contains('@') {
            return Err(RuntimeError::InvalidSocketPath {
                path: text.into_owned(),
                reason: "abstract-socket syntax is not allowed",
            });
        }
        Ok(SocketPath(path))
    }

    /// The path as a `Path`.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for SocketPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// A bound listening socket that owns its filesystem entry.
///
/// `Drop` unlinks the path, so a crashed test or a restarted preview can rebind
/// the same instance. The final path is only ever published after the socket is
/// already `0600` (see [`RuntimeSocket::bind`]) and is never widened by this
/// crate.
#[derive(Debug)]
pub struct RuntimeSocket {
    path: SocketPath,
    listener: UnixListener,
    mode: u32,
}

impl RuntimeSocket {
    /// Binds `path`, refusing an existing filesystem entry.
    ///
    /// The existing entry is reported as [`RuntimeError::SocketInUse`] even when
    /// it is stale: silently reusing a path could attach control to the wrong
    /// process, and unlinking someone else's socket is not this crate's call.
    ///
    /// Binding at the final path and then calling `chmod` would leave the path
    /// observable, for a moment, with the umask-derived permissions of the
    /// process - a window in which another local user could `connect` (the
    /// kernel checks the mode of the *socket inode* at connect time). So the
    /// socket is bound at an unpredictable sibling name of the same length,
    /// narrowed to `0600`, and then published under the final name atomically.
    /// Publishing uses `link(2)` rather than `rename(2)`: rename would replace a
    /// socket that another process bound between the check and the publish,
    /// while link refuses with `EEXIST`, which is reported as
    /// [`RuntimeError::SocketInUse`]. The listener keeps working because the
    /// published name and the temporary name are the same inode.
    pub fn bind(path: SocketPath) -> Result<Self, RuntimeError> {
        if std::fs::symlink_metadata(path.as_path()).is_ok() {
            return Err(RuntimeError::SocketInUse {
                path: path.to_string(),
            });
        }
        let mut attempt = 0_u32;
        loop {
            let temporary = temporary_sibling(path.as_path())?;
            let listener = match UnixListener::bind(&temporary) {
                Ok(listener) => listener,
                Err(error) if error.kind() == ErrorKind::AlreadyExists && attempt < 32 => {
                    // Never unlink here: the colliding entry is not ours.
                    attempt += 1;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            // From this point the temporary name is ours, so every error path
            // unlinks it: a failed bind must leave the directory as it found it.
            if let Err(error) =
                std::fs::set_permissions(&temporary, Permissions::from_mode(SOCKET_MODE))
            {
                let _ = std::fs::remove_file(&temporary);
                return Err(error.into());
            }
            if let Err(error) = listener.set_nonblocking(true) {
                let _ = std::fs::remove_file(&temporary);
                return Err(error.into());
            }
            match std::fs::hard_link(&temporary, path.as_path()) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    let _ = std::fs::remove_file(&temporary);
                    return Err(RuntimeError::SocketInUse {
                        path: path.to_string(),
                    });
                }
                Err(error) => {
                    let _ = std::fs::remove_file(&temporary);
                    return Err(error.into());
                }
            }
            if let Err(error) = std::fs::remove_file(&temporary) {
                // The published name is the only remaining link; drop it so the
                // caller does not inherit a half-published socket.
                let _ = std::fs::remove_file(path.as_path());
                return Err(error.into());
            }
            let mode = read_mode(path.as_path(), SOCKET_MODE);
            return Ok(RuntimeSocket {
                path,
                listener,
                mode,
            });
        }
    }

    /// The socket path.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// The permission bits read back from the filesystem.
    ///
    /// Falls back to the mode observed at bind if the entry vanished, so the
    /// value is always the last mode this crate published.
    pub fn mode(&self) -> u32 {
        read_mode(self.path.as_path(), self.mode)
    }

    /// The listening socket.
    pub fn listener(&self) -> &UnixListener {
        &self.listener
    }
}

impl Drop for RuntimeSocket {
    fn drop(&mut self) {
        // Best effort: the listener is about to close anyway, and a failed
        // unlink (already removed by an operator) must not abort a destructor.
        let _ = std::fs::remove_file(self.path.as_path());
    }
}

/// Counter used only when `/dev/urandom` cannot be read.
static NONCE_FALLBACK: AtomicU64 = AtomicU64::new(0);

/// A sibling of `path` with a fresh, unpredictable name of the same length.
///
/// Keeping the length identical to the final name is what makes the temporary
/// entry safe to bind: [`SocketPath`] already proved the final path fits
/// `sun_path`, so a name of the same byte length fits too and no validated path
/// becomes unbindable.
fn temporary_sibling(path: &Path) -> Result<PathBuf, RuntimeError> {
    let invalid = |reason: &'static str| RuntimeError::InvalidSocketPath {
        path: path.display().to_string(),
        reason,
    };
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("socket path has no usable file name"))?;
    if file_name.len() < 2 {
        return Err(invalid("socket file name is too short"));
    }
    let width = file_name.len() - 1;
    let nonce = nonce_hex();
    let mut name = String::with_capacity(file_name.len());
    name.push('.');
    while name.len() <= width {
        name.push_str(&nonce);
    }
    name.truncate(width + 1);
    Ok(path.with_file_name(name))
}

/// Sixteen unpredictable hex characters for a temporary socket name.
///
/// The name only has to be unguessable while the socket is being narrowed to
/// `0600`; `/dev/urandom` is preferred and a time/counter/pid mix is the
/// documented fallback when it cannot be opened.
fn nonce_hex() -> String {
    let mut bytes = [0_u8; 8];
    if let Ok(mut source) = std::fs::File::open("/dev/urandom") {
        if source.read_exact(&mut bytes).is_ok() {
            return bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    let counter = NONCE_FALLBACK.fetch_add(1, Ordering::Relaxed);
    let seed = nanos ^ counter.rotate_left(32) ^ u64::from(std::process::id());
    seed.to_be_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_mode(path: &Path, fallback: u32) -> u32 {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            let file_type = metadata.file_type();
            if file_type.is_socket() {
                metadata.permissions().mode() & 0o777
            } else {
                fallback
            }
        }
        Err(_) => fallback,
    }
}
