//! `config.toml` — the remembered data root (`$XDG_CONFIG_HOME`, §4.5).
//!
//! Written **only** after L2 passed ([`crate::data_root::DataRoot::remember`]):
//! a root that cannot be validated is never remembered, so a broken mount
//! cannot poison the next launch. The write is atomic (tmp file in the same
//! directory + `fsync` + `rename` + directory `fsync`), so a crash leaves
//! either the old file or the new one, never a truncated one.
//!
//! **Structure read here** (a deliberate TOML subset — the engine takes no
//! TOML dependency):
//!
//! ```toml
//! schema = 1
//! [data]
//! root = "/path/to/Daggerfall/app"
//! strict = false
//! ```
//!
//! Anything this module does not own (`[mods]` is the mods lane's, §5) is kept
//! **verbatim** and re-emitted on rewrite, so persisting a root never destroys
//! another lane's section. An unknown `schema`, a parse error or an
//! unreadable file is *ignored with a warning* and never rewritten blindly
//! (§4.5) — the caller records `W-DATA-001` and continues with the next rank.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::data_root::Warning;

/// Config schema this build understands (§4.5).
pub const CONFIG_SCHEMA: i64 = 1;

/// Directory under the XDG config root owned by the engine.
pub const CONFIG_DIR: &str = "daggerfall-bevy-native";
/// File name of the remembered root.
pub const CONFIG_FILE: &str = "config.toml";

/// The `schema` value plus the entries this lane owns, and the raw lines of
/// every section it does not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigFile {
    /// `schema` (≥ 1). Always [`CONFIG_SCHEMA`] for a file that was accepted.
    pub schema: i64,
    /// `[data].root` — the remembered data folder.
    pub root: Option<PathBuf>,
    /// `[data].strict` — default of `--strict-data`.
    pub strict: Option<bool>,
    /// Lines of sections this lane does not own, kept verbatim for rewrites.
    extra: Vec<String>,
}

/// Outcome of reading the config: a usable file, no file, or a file that was
/// ignored (with the reason to report as `W-DATA-001`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigState {
    /// No `config.toml` on this machine.
    Missing,
    /// Parsed and accepted.
    Loaded(ConfigFile),
    /// Present but unusable; it is **not** rewritten.
    Ignored {
        /// File that was read.
        path: PathBuf,
        /// Why it was ignored (printed to the player).
        reason: String,
    },
}

impl ConfigState {
    /// The accepted file, when there is one.
    pub fn loaded(&self) -> Option<&ConfigFile> {
        match self {
            Self::Loaded(file) => Some(file),
            _ => None,
        }
    }

    /// The warning to surface, if the file was ignored.
    pub fn warning(&self) -> Option<Warning> {
        match self {
            Self::Ignored { path, reason } => Some(Warning {
                code: "W-DATA-001",
                message: format!(
                    "config \"{}\" ignored: {reason}. Not rewriting it.",
                    path.display()
                ),
            }),
            _ => None,
        }
    }
}

/// `$XDG_CONFIG_HOME/daggerfall-bevy-native/config.toml`, falling back to
/// `$HOME/.config/daggerfall-bevy-native/config.toml` (§4.5).
///
/// `None` when neither variable is set: the engine then runs without a
/// remembered root instead of inventing a location.
pub fn config_path(xdg_config_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let base = xdg_config_home
        .map(Path::to_path_buf)
        .or_else(|| home.map(|home| home.join(".config")))?;
    Some(base.join(CONFIG_DIR).join(CONFIG_FILE))
}

/// Reads the config at `path` (see [`ConfigState`] for the three outcomes).
pub fn load(path: &Path) -> ConfigState {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ConfigState::Missing,
        Err(err) => {
            return ConfigState::Ignored {
                path: path.to_path_buf(),
                reason: format!("unreadable ({err})"),
            };
        }
    };
    match parse(&text) {
        Ok(file) if file.schema == CONFIG_SCHEMA => ConfigState::Loaded(file),
        Ok(file) => ConfigState::Ignored {
            path: path.to_path_buf(),
            reason: format!(
                "schema {} is not supported by this build (expected {CONFIG_SCHEMA})",
                file.schema
            ),
        },
        Err(reason) => ConfigState::Ignored {
            path: path.to_path_buf(),
            reason,
        },
    }
}

/// Parses the TOML subset described in the module doc.
///
/// Unknown keys and unknown sections are preserved for the next rewrite, not
/// rejected: a future lane adding `[mods]` must not make this build ignore the
/// whole file. A malformed line *is* rejected (the file is then ignored and
/// left untouched).
pub fn parse(text: &str) -> Result<ConfigFile, String> {
    let mut file = ConfigFile {
        schema: CONFIG_SCHEMA,
        ..ConfigFile::default()
    };
    let mut saw_schema = false;
    let mut section = String::new();
    for (index, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            let name = name.trim().to_string();
            if name == "data" {
                section = name;
            } else {
                if !name.is_empty() {
                    file.extra.push(format!("[{name}]"));
                }
                section = name;
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {} is not `key = value`", index + 1));
        };
        let key = key.trim();
        let value = value.trim();
        match (section.as_str(), key) {
            ("", "schema") => {
                let parsed: i64 = value
                    .parse()
                    .map_err(|_| format!("line {}: schema is not an integer", index + 1))?;
                file.schema = parsed;
                saw_schema = true;
            }
            ("data", "root") => {
                file.root = Some(PathBuf::from(parse_string(value, index + 1)?));
            }
            ("data", "strict") => {
                file.strict = Some(parse_bool(value, index + 1)?);
            }
            // Everything else belongs to another lane: keep it byte-for-byte.
            _ => file.extra.push(format!("{key} = {value}")),
        }
    }
    if !saw_schema {
        return Err("no `schema` key".into());
    }
    Ok(file)
}

/// Serialises the file (owned keys first, preserved lines after).
pub fn render(file: &ConfigFile) -> String {
    let mut out = String::new();
    out.push_str(&format!("schema = {}\n", file.schema));
    out.push_str("[data]\n");
    if let Some(root) = &file.root {
        out.push_str(&format!("root = {}\n", quote(&root.to_string_lossy())));
    }
    if let Some(strict) = file.strict {
        out.push_str(&format!("strict = {strict}\n"));
    }
    if !file.extra.is_empty() {
        out.push('\n');
        for line in &file.extra {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Writes `file` atomically to `path` (tmp + `fsync` + `rename` + dir `fsync`).
pub fn write_atomic(path: &Path, file: &ConfigFile) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("config path without a directory"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config".into()),
        std::process::id()
    ));
    let bytes = render(file);
    {
        let mut handle = std::fs::File::create(&tmp)?;
        handle.write_all(bytes.as_bytes())?;
        handle.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    // Persist the rename itself; a directory cannot be opened for writing, so
    // `File::open` + `sync_all` is how POSIX exposes the directory fsync.
    if let Ok(handle) = std::fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    Ok(())
}

/// Forgets the data decision: removes `[data].root` **and** `[data].strict`
/// (both belong to this lane; keeping a `strict` without a root would leave a
/// half-remembered choice behind), rewriting atomically.
///
/// Returns `Ok(false)` when there was no config (nothing to forget). The file
/// is deleted when nothing of this lane is left in it; another lane's sections
/// are preserved.
pub fn forget_root(path: &Path) -> Result<bool, String> {
    match load(path) {
        ConfigState::Missing => Ok(false),
        ConfigState::Ignored { reason, .. } => Err(format!(
            "not touching \"{}\": {reason} (fix or delete the file by hand)",
            path.display()
        )),
        ConfigState::Loaded(mut file) => {
            file.root = None;
            file.strict = None;
            if file.extra.is_empty() {
                std::fs::remove_file(path).map_err(|err| format!("{}: {err}", path.display()))?;
                return Ok(true);
            }
            write_atomic(path, &file).map_err(|err| format!("{}: {err}", path.display()))?;
            Ok(true)
        }
    }
}

/// Sets `[data].root` (and `strict` when given), preserving other sections.
pub fn remember_root(path: &Path, root: &Path, strict: bool) -> Result<(), String> {
    let mut file = match load(path) {
        ConfigState::Loaded(file) => file,
        // An ignored or absent file is replaced only when the caller explicitly
        // asked to remember a root that already passed L2; the ignored content
        // is not preserved because it could not be understood (§4.5).
        ConfigState::Missing | ConfigState::Ignored { .. } => ConfigFile {
            schema: CONFIG_SCHEMA,
            ..ConfigFile::default()
        },
    };
    file.schema = CONFIG_SCHEMA;
    file.root = Some(root.to_path_buf());
    file.strict = Some(strict);
    write_atomic(path, &file).map_err(|err| format!("{}: {err}", path.display()))
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        match character {
            '\\' if in_string => escaped = !escaped,
            '"' if !escaped => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => escaped = false,
        }
    }
    line
}

fn parse_string(value: &str, line: usize) -> Result<String, String> {
    let trimmed = value.trim();
    let Some(inner) = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return Err(format!("line {line}: value is not a quoted string"));
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => return Err(format!("line {line}: trailing escape")),
        }
    }
    Ok(out)
}

fn parse_bool(value: &str, line: usize) -> Result<bool, String> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("line {line}: `{other}` is not a boolean")),
    }
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "df-config-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn xdg_config_home_wins_over_home() {
        let path = config_path(Some(Path::new("/xdg")), Some(Path::new("/home/u")))
            .expect("a path");
        assert_eq!(
            path,
            PathBuf::from("/xdg/daggerfall-bevy-native/config.toml")
        );
        let path = config_path(None, Some(Path::new("/home/u"))).expect("a path");
        assert_eq!(
            path,
            PathBuf::from("/home/u/.config/daggerfall-bevy-native/config.toml")
        );
        assert!(config_path(None, None).is_none());
    }

    #[test]
    fn roundtrip_preserves_foreign_sections() {
        let text = "schema = 1\n[data]\nroot = \"/games/df\"\nstrict = true\n\n[mods]\ndir = \"/games/df/mods\"\nload_order = [\"a\", \"b\"]\n";
        let mut file = parse(text).expect("parses");
        assert_eq!(file.root.as_deref(), Some(Path::new("/games/df")));
        assert_eq!(file.strict, Some(true));
        file.root = Some(PathBuf::from("/other/df"));
        let rendered = render(&file);
        let again = parse(&rendered).expect("renders parseable");
        assert_eq!(again.root.as_deref(), Some(Path::new("/other/df")));
        assert!(
            rendered.contains("[mods]") && rendered.contains("load_order = [\"a\", \"b\"]"),
            "the mods lane's section must survive a rewrite:\n{rendered}"
        );
    }

    #[test]
    fn unknown_schema_is_ignored_not_rewritten() {
        let dir = scratch("schema");
        let path = dir.join("config.toml");
        let text = "schema = 99\n[data]\nroot = \"/games/df\"\n";
        std::fs::write(&path, text).expect("seed config");
        let state = load(&path);
        assert!(matches!(state, ConfigState::Ignored { .. }));
        assert_eq!(
            state.warning().map(|warning| warning.code),
            Some("W-DATA-001")
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("still there"),
            text,
            "an ignored config is never rewritten"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_config_is_ignored_not_rewritten() {
        let dir = scratch("corrupt");
        let path = dir.join("config.toml");
        let text = "schema = 1\n[data\nroot = \"/games/df\"\n";
        std::fs::write(&path, text).expect("seed config");
        let state = load(&path);
        assert!(matches!(state, ConfigState::Ignored { .. }));
        assert_eq!(
            std::fs::read_to_string(&path).expect("still there"),
            text
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_then_forget_leaves_no_file() {
        let dir = scratch("remember");
        let path = dir.join("config.toml");
        remember_root(&path, Path::new("/games/df"), false).expect("remembers");
        let loaded = load(&path);
        assert_eq!(
            loaded.loaded().and_then(|file| file.root.clone()).as_deref(),
            Some(Path::new("/games/df"))
        );
        assert!(forget_root(&path).expect("forgets"));
        assert!(!path.exists(), "an emptied config is removed");
        assert!(!forget_root(&path).expect("second forget is a no-op"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forget_refuses_to_rewrite_an_ignored_config() {
        let dir = scratch("forget-ignored");
        let path = dir.join("config.toml");
        std::fs::write(&path, "schema = 7\n").expect("seed config");
        assert!(forget_root(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).expect("still there"),
            "schema = 7\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quoted_paths_with_spaces_and_quotes_roundtrip() {
        let dir = scratch("quote");
        let path = dir.join("config.toml");
        let root = PathBuf::from("/games/GOG Games/\"Daggerfall\"\\app");
        remember_root(&path, &root, true).expect("remembers");
        let loaded = load(&path);
        let file = loaded.loaded().expect("loads");
        assert_eq!(file.root.as_deref(), Some(root.as_path()));
        assert_eq!(file.strict, Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
