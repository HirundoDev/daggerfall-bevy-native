//! Executable-content detection - rule 4 of plan `I1` §3.4.
//!
//! A v1 bundle transports content, never code: a plugin is a crate, not a
//! payload. Detection is layered on purpose:
//!
//! 1. **Magic bytes** decide. A binary renamed to `.png` is still a binary, so a
//!    file is refused when its first bytes identify an executable format.
//! 2. **File extensions** are a second, cheaper line of defence. They catch a
//!    payload that *declares* itself executable even if this detector's magic
//!    table would not recognise it.
//!
//! Neither layer tries to be exhaustive over every executable format; together
//! they make "no executable payload" a checked property of the build and of
//! every later `verify()`.

use crate::path::BundlePath;

/// File extensions refused regardless of content, ASCII case-insensitive.
///
/// `bin` is deliberately absent: `.bin` is a legitimate name for data files, and
/// the magic table still catches a real binary under that name.
pub const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "wasm", "o", "a", "class", "jar", "sh", "bash", "bat", "cmd",
    "ps1", "py", "rb", "pl", "php", "js", "mjs", "cjs", "node", "elf",
];

/// Magic-byte prefixes and the stable reason each one produces.
///
/// `CA FE BA BE` is shared by fat Mach-O binaries and JVM class files; the reason
/// names both, so the diagnostic never claims more than the bytes prove.
const MAGIC_PREFIXES: &[(&[u8], &str)] = &[
    (&[0x7F, b'E', b'L', b'F'], "ELF executable header (7F 45 4C 46)"),
    (
        &[0xFE, 0xED, 0xFA, 0xCE],
        "Mach-O binary header (FE ED FA CE)",
    ),
    (
        &[0xFE, 0xED, 0xFA, 0xCF],
        "Mach-O binary header (FE ED FA CF)",
    ),
    (
        &[0xCE, 0xFA, 0xED, 0xFE],
        "Mach-O binary header (CE FA ED FE)",
    ),
    (
        &[0xCF, 0xFA, 0xED, 0xFE],
        "Mach-O binary header (CF FA ED FE)",
    ),
    (
        &[0xCA, 0xFE, 0xBA, 0xBE],
        "Mach-O fat binary or Java class header (CA FE BA BE)",
    ),
    (
        &[0xBE, 0xBA, 0xFE, 0xCA],
        "Mach-O fat binary header (BE BA FE CA)",
    ),
    (
        &[0xCA, 0xFE, 0xBA, 0xBF],
        "Mach-O fat binary header (CA FE BA BF)",
    ),
    (
        b"MZ",
        "PE/DOS executable header (4D 5A)",
    ),
    (
        &[0x00, 0x61, 0x73, 0x6D],
        "WebAssembly module header (00 61 73 6D)",
    ),
    (b"#!", "shebang script header (23 21)"),
];

/// Returns the stable reason why `bytes` / `path` are executable, or `None`.
pub fn detect_executable(bytes: &[u8], path: &BundlePath) -> Option<&'static str> {
    if let Some((_, reason)) = MAGIC_PREFIXES
        .iter()
        .find(|(prefix, _)| bytes.starts_with(prefix))
    {
        return Some(reason);
    }
    executable_extension(path)
}

/// Extension layer: `Some(reason)` when the file name declares an executable
/// extension.
fn executable_extension(path: &BundlePath) -> Option<&'static str> {
    let file_name = path.file_name();
    let (_, extension) = file_name.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    if EXECUTABLE_EXTENSIONS.contains(&extension.as_str()) {
        Some("file extension declares executable content (defence in depth)")
    } else {
        None
    }
}
