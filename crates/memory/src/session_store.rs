//! `session_store` — session save format and I/O (M8).
//!
//! **Declared minimum contract** (plan §2 M8, anchor INV-L99-4 / PD4 T2 L120
//! "Session/Saves → heart (loop) + memory", REQ-L99-041): the save/load
//! roundtrip of the played session lives in 🧠 Memory (the persistence
//! organ — F0: "persistence, saves and data access"). This module IS the
//! format: [`SessionSnapshot`] (**primitives-only** DTO — bus-only,
//! 0 dependency on other organs) + write/read with **corruption
//! detection** (magic + version + FNV-1a 64 checksum).
//!
//! File format (canonical, deterministic text — `save → load → save`
//! produces identical bytes):
//!
//! ```text
//! DFL-SESSION 1            ← magic + format version
//! pos_x_mm=1500            ← position in integer millimeters (NFR NUMERIC)
//! pos_z_mm=-2000
//! yaw_mrad=314             ← orientation in integer milliradians
//! clock_year=405           ← ❤️ heart: session GameClock (6 fields)
//! clock_month=5
//! clock_day=0
//! clock_hour=12
//! clock_minute=0
//! clock_second=0
//! level=2 …                ← 🎮 skills: RpgStats (6 fields)
//! gold=55                  ← 📦 inventory: gold + goods
//! goods_count=2
//! goods=hides|1            ← escaped id|quantity (insertion order)
//! goods=potion|1
//! quests_count=2
//! quest=q-vendel-shop|Find Vendel's shop   ← 🎭 behavior
//! chunks_count=5
//! chunk=0|0                ← 🧠 memory: chunks in deterministic order
//! checksum=0123abcd…       ← FNV-1a 64 (hex) over the whole payload
//! ```
//!
//! **Roundtrip false-positive guards:** a missing file fails honestly
//! ([`SaveError::Io`] with `NotFound`), unknown magic → [`SaveError::BadMagic`],
//! unsupported version → [`SaveError::BadVersion`] and any altered byte
//! → [`SaveError::Corrupt`] (the attacker cannot recompute the checksum
//! without rewriting the checksum — and if they rewrite it, the strict
//! parser rejects unknown/duplicate/missing keys).
//!
//! 🧠 Memory organ rules (bevy-bridge-matrix): pure Rust, **0 Bevy**,
//! 0 external dependencies (the crate keeps `cargo tree` edge-free).

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// Format magic (first line of the file).
pub const FORMAT_MAGIC: &str = "DFL-SESSION";
/// Format version supported by this build.
pub const FORMAT_VERSION: i64 = 1;

/// Offset FNV-1a 64.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// Prime FNV-1a 64.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Session snapshot in **primitives only** (persistence DTO —
/// bus-only: the `app` maps the organ types here; this crate does not
/// depend on any other organ).
///
/// The 5 states required by the M8 VE (INV-L99-4): position (integer
/// mm/mrad), stats, inventory, quests and chunks + the ❤️ session time
/// (6 integer `GameClock` fields — heart owns the time loop).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionSnapshot {
    /// Session X position in millimeters (integer — NFR NUMERIC).
    pub pos_x_mm: i64,
    /// Session Z position in millimeters (integer).
    pub pos_z_mm: i64,
    /// Yaw orientation in milliradians (integer).
    pub yaw_mrad: i64,
    /// Game clock year (❤️ heart `GameClock::year`).
    pub clock_year: i64,
    /// Month 0..11 (`GameClock::month`).
    pub clock_month: i64,
    /// Day of month 0..29 (`GameClock::day`).
    pub clock_day: i64,
    /// Hour 0..23 (`GameClock::hour`).
    pub clock_hour: i64,
    /// Minute 0..59 (`GameClock::minute`).
    pub clock_minute: i64,
    /// Second 0..59 (`GameClock::second`).
    pub clock_second: i64,
    /// Character level (🎮 `RpgStats::level`).
    pub level: i64,
    /// Experience within the level (`RpgStats::experience`).
    pub experience: i64,
    /// Current health (`RpgStats::health`).
    pub health: i64,
    /// Maximum health (`RpgStats::max_health`).
    pub max_health: i64,
    /// Current stamina (`RpgStats::stamina`).
    pub stamina: i64,
    /// Maximum stamina (`RpgStats::max_stamina`).
    pub max_stamina: i64,
    /// Gold (📦 `InventoryLite::gold`).
    pub gold: i64,
    /// Goods `(id, quantity)` in insertion order (`InventoryLite::goods`).
    pub goods: Vec<(String, i64)>,
    /// Active quests `(id, title)` in acceptance order (🎭 `QuestLog`).
    pub quests: Vec<(String, String)>,
    /// Loaded chunks in deterministic order (🧠 `ChunkMap::iter`).
    pub chunks: Vec<(i64, i64)>,
}

/// Session save/load error.
#[derive(Debug)]
pub enum SaveError {
    /// Disk I/O (includes `NotFound` — load without save = honest failure).
    Io(std::io::Error),
    /// The file does not start with [`FORMAT_MAGIC`] (not one of our saves).
    BadMagic,
    /// Format version not supported by this build.
    BadVersion(i64),
    /// Unreadable payload: checksum mismatch or invalid structure.
    Corrupt(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Io(err) => write!(f, "save I/O error: {err}"),
            SaveError::BadMagic => write!(f, "the file is not a DFL-SESSION save (magic missing)"),
            SaveError::BadVersion(version) => {
                write!(f, "unsupported save version: {version}")
            }
            SaveError::Corrupt(reason) => write!(f, "corrupt save: {reason}"),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SaveError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl SessionSnapshot {
    /// Serializes to canonical text (same bytes for the same state —
    /// byte-for-byte determinism).
    pub fn to_text(&self) -> String {
        let mut body = String::new();
        body.push_str(FORMAT_MAGIC);
        body.push(' ');
        body.push_str(&FORMAT_VERSION.to_string());
        body.push('\n');
        push_scalar(&mut body, "pos_x_mm", self.pos_x_mm);
        push_scalar(&mut body, "pos_z_mm", self.pos_z_mm);
        push_scalar(&mut body, "yaw_mrad", self.yaw_mrad);
        push_scalar(&mut body, "clock_year", self.clock_year);
        push_scalar(&mut body, "clock_month", self.clock_month);
        push_scalar(&mut body, "clock_day", self.clock_day);
        push_scalar(&mut body, "clock_hour", self.clock_hour);
        push_scalar(&mut body, "clock_minute", self.clock_minute);
        push_scalar(&mut body, "clock_second", self.clock_second);
        push_scalar(&mut body, "level", self.level);
        push_scalar(&mut body, "experience", self.experience);
        push_scalar(&mut body, "health", self.health);
        push_scalar(&mut body, "max_health", self.max_health);
        push_scalar(&mut body, "stamina", self.stamina);
        push_scalar(&mut body, "max_stamina", self.max_stamina);
        push_scalar(&mut body, "gold", self.gold);

        body.push_str(&format!("goods_count={}\n", self.goods.len()));
        for (id, qty) in &self.goods {
            body.push_str(&format!("goods={}|{qty}\n", escape(id)));
        }

        body.push_str(&format!("quests_count={}\n", self.quests.len()));
        for (id, title) in &self.quests {
            body.push_str(&format!("quest={}|{}\n", escape(id), escape(title)));
        }

        body.push_str(&format!("chunks_count={}\n", self.chunks.len()));
        for (x, z) in &self.chunks {
            body.push_str(&format!("chunk={x}|{z}\n"));
        }

        let checksum = fnv1a64(body.as_bytes());
        body.push_str(&format!("checksum={checksum:016x}\n"));
        body
    }

    /// Parses canonical text (strict: magic + version + checksum + complete
    /// structure — any deviation is a [`SaveError`]).
    pub fn from_text(text: &str) -> Result<Self, SaveError> {
        let mut lines = text.lines();
        let header = lines.next().ok_or_else(|| SaveError::Corrupt("empty file".into()))?;
        let (magic, version) = header
            .split_once(' ')
            .ok_or_else(|| SaveError::Corrupt("header without version".into()))?;
        if magic != FORMAT_MAGIC {
            return Err(SaveError::BadMagic);
        }
        let version: i64 = version
            .parse()
            .map_err(|_| SaveError::Corrupt("non-numeric version".into()))?;
        if version != FORMAT_VERSION {
            return Err(SaveError::BadVersion(version));
        }

        // The checksum covers the WHOLE payload before the final line.
        let payload: Vec<&str> = text
            .lines()
            .take(text.lines().count().saturating_sub(1))
            .collect();
        let mut payload_bytes = Vec::new();
        for line in &payload {
            payload_bytes.extend_from_slice(line.as_bytes());
            payload_bytes.push(b'\n');
        }
        let checksum_line = text
            .lines()
            .next_back()
            .ok_or_else(|| SaveError::Corrupt("no checksum line".into()))?;
        let expected_hex = checksum_line
            .strip_prefix("checksum=")
            .ok_or_else(|| SaveError::Corrupt("last line is not a checksum".into()))?;
        let expected: u64 = u64::from_str_radix(expected_hex, 16)
            .map_err(|_| SaveError::Corrupt("non-hexadecimal checksum".into()))?;
        let actual = fnv1a64(&payload_bytes);
        if actual != expected {
            return Err(SaveError::Corrupt(format!(
                "checksum mismatch (expected {expected:016x}, computed {actual:016x})"
            )));
        }

        let mut snap = SessionSnapshot::default();
        let mut seen_goods = false;
        let mut seen_quests = false;
        let mut seen_chunks = false;
        let mut iter = payload.into_iter().skip(1); // (skips the already-consumed header)
        while let Some(line) = iter.next() {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| SaveError::Corrupt(format!("line without '=': {line:?}")))?;
            match key {
                "pos_x_mm" => snap.pos_x_mm = parse_i64(value)?,
                "pos_z_mm" => snap.pos_z_mm = parse_i64(value)?,
                "yaw_mrad" => snap.yaw_mrad = parse_i64(value)?,
                "clock_year" => snap.clock_year = parse_i64(value)?,
                "clock_month" => snap.clock_month = parse_i64(value)?,
                "clock_day" => snap.clock_day = parse_i64(value)?,
                "clock_hour" => snap.clock_hour = parse_i64(value)?,
                "clock_minute" => snap.clock_minute = parse_i64(value)?,
                "clock_second" => snap.clock_second = parse_i64(value)?,
                "level" => snap.level = parse_i64(value)?,
                "experience" => snap.experience = parse_i64(value)?,
                "health" => snap.health = parse_i64(value)?,
                "max_health" => snap.max_health = parse_i64(value)?,
                "stamina" => snap.stamina = parse_i64(value)?,
                "max_stamina" => snap.max_stamina = parse_i64(value)?,
                "gold" => snap.gold = parse_i64(value)?,
                "goods_count" => {
                    seen_goods = true;
                    let count = parse_count(value)?;
                    for _ in 0..count {
                        let line = iter
                            .next()
                            .ok_or_else(|| SaveError::Corrupt("goods_count exceeds the file".into()))?;
                        let (k, v) = split_entry(line, "goods")?;
                        let (id, qty) = v
                            .split_once('|')
                            .ok_or_else(|| SaveError::Corrupt("goods without separator".into()))?;
                        snap.goods.push((unescape(id)?, parse_i64(qty)?));
                        let _ = k;
                    }
                }
                "quests_count" => {
                    seen_quests = true;
                    let count = parse_count(value)?;
                    for _ in 0..count {
                        let line = iter
                            .next()
                            .ok_or_else(|| SaveError::Corrupt("quests_count exceeds the file".into()))?;
                        let (k, v) = split_entry(line, "quest")?;
                        let (id, title) = v
                            .split_once('|')
                            .ok_or_else(|| SaveError::Corrupt("quest without separator".into()))?;
                        snap.quests.push((unescape(id)?, unescape(title)?));
                        let _ = k;
                    }
                }
                "chunks_count" => {
                    seen_chunks = true;
                    let count = parse_count(value)?;
                    for _ in 0..count {
                        let line = iter
                            .next()
                            .ok_or_else(|| SaveError::Corrupt("chunks_count exceeds the file".into()))?;
                        let (k, v) = split_entry(line, "chunk")?;
                        let (x, z) = v
                            .split_once('|')
                            .ok_or_else(|| SaveError::Corrupt("chunk without separator".into()))?;
                        snap.chunks.push((parse_i64(x)?, parse_i64(z)?));
                        let _ = k;
                    }
                }
                other => {
                    return Err(SaveError::Corrupt(format!("unknown key: {other:?}")));
                }
            }
        }
        if !(seen_goods && seen_quests && seen_chunks) {
            return Err(SaveError::Corrupt(
                "missing mandatory sections (goods/quests/chunks)".into(),
            ));
        }
        Ok(snap)
    }

    /// Writes the snapshot to `path` (creates missing parent directories).
    pub fn save_to_path(&self, path: &Path) -> Result<(), SaveError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(SaveError::Io)?;
            }
        }
        fs::write(path, self.to_text().as_bytes()).map_err(SaveError::Io)
    }

    /// Reads the snapshot from `path` (absent → honest [`SaveError::Io`]).
    pub fn load_from_path(path: &Path) -> Result<Self, SaveError> {
        let bytes = fs::read(path).map_err(SaveError::Io)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| SaveError::Corrupt("the save is not valid UTF-8".into()))?;
        Self::from_text(&text)
    }

    /// Is the error a missing file (load without save = honest failure)?
    pub fn is_not_found(error: &SaveError) -> bool {
        matches!(error, SaveError::Io(err) if err.kind() == ErrorKind::NotFound)
    }
}

fn push_scalar(body: &mut String, key: &str, value: i64) {
    body.push_str(key);
    body.push('=');
    body.push_str(&value.to_string());
    body.push('\n');
}

fn parse_i64(value: &str) -> Result<i64, SaveError> {
    value
        .parse()
        .map_err(|_| SaveError::Corrupt(format!("invalid integer: {value:?}")))
}

fn parse_count(value: &str) -> Result<usize, SaveError> {
    let count: i64 = parse_i64(value)?;
    usize::try_from(count).map_err(|_| SaveError::Corrupt(format!("negative count: {count}")))
}

fn split_entry<'a>(line: &'a str, expected: &str) -> Result<(&'a str, &'a str), SaveError> {
    let (key, value) = line
        .split_once('=')
        .ok_or_else(|| SaveError::Corrupt(format!("list line without '=': {line:?}")))?;
    if key != expected {
        return Err(SaveError::Corrupt(format!(
            "expected {expected:?} but got {key:?}"
        )));
    }
    Ok((key, value))
}

/// Escapes `\`, `|` and newlines in text fields (exact roundtrip).
pub(crate) fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '|' => out.push_str("\\p"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn unescape(escaped: &str) -> Result<String, SaveError> {
    let mut out = String::with_capacity(escaped.len());
    let mut chars = escaped.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('p') => out.push('|'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            other => {
                return Err(SaveError::Corrupt(format!(
                    "invalid escape: \\{other:?}"
                )));
            }
        }
    }
    Ok(out)
}

/// FNV-1a 64 (deterministic, 0 dependencies — corruption detection).
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample() -> SessionSnapshot {
        SessionSnapshot {
            pos_x_mm: 1500,
            pos_z_mm: -2000,
            yaw_mrad: 314,
            clock_year: 405,
            clock_month: 5,
            clock_day: 0,
            clock_hour: 13,
            clock_minute: 1,
            clock_second: 1,
            level: 2,
            experience: 50,
            health: 70,
            max_health: 100,
            stamina: 100,
            max_stamina: 100,
            gold: 55,
            goods: vec![("hides".into(), 1), ("potion".into(), 1)],
            quests: vec![
                ("q-vendel-shop".into(), "Find Vendel's shop".into()),
                ("q-cavern".into(), "Explore the cavern to the east".into()),
            ],
            chunks: vec![(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)],
        }
    }

    fn temp_save_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "daggerfall-session-store-{tag}-{}.dfs",
            std::process::id()
        ))
    }

    #[test]
    fn text_roundtrip_is_byte_stable() {
        let snap = sample();
        let text = snap.to_text();
        let parsed = SessionSnapshot::from_text(&text).expect("valid parse");
        assert_eq!(parsed, snap);
        // Determinism: save → load → save = identical bytes.
        assert_eq!(parsed.to_text(), text);
    }

    #[test]
    fn file_roundtrip_preserves_state() {
        let snap = sample();
        let path = temp_save_path("file");
        snap.save_to_path(&path).expect("save");
        let loaded = SessionSnapshot::load_from_path(&path).expect("load");
        assert_eq!(loaded, snap);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_fails_honestly() {
        let path = temp_save_path("missing");
        let _ = std::fs::remove_file(&path);
        let err = SessionSnapshot::load_from_path(&path).expect_err("must fail");
        assert!(SessionSnapshot::is_not_found(&err), "expected NotFound: {err}");
    }

    #[test]
    fn corrupted_byte_is_detected() {
        let snap = sample();
        let path = temp_save_path("corrupt");
        snap.save_to_path(&path).expect("save");
        let mut bytes = std::fs::read(&path).expect("read");
        // Alters ONE payload byte (inside the gold area).
        let idx = bytes
            .windows(2)
            .position(|w| w == b"55")
            .expect("gold in the file");
        bytes[idx] = b'6';
        std::fs::write(&path, &bytes).expect("corrupt write");
        let err = SessionSnapshot::load_from_path(&path).expect_err("must detect");
        assert!(matches!(err, SaveError::Corrupt(_)), "expected Corrupt: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bad_magic_and_version_are_rejected() {
        let snap = sample();
        let text = snap.to_text();
        let bad_magic = text.replacen(FORMAT_MAGIC, "OTHER-FORMAT", 1);
        assert!(matches!(
            SessionSnapshot::from_text(&bad_magic),
            Err(SaveError::BadMagic)
        ));
        let bad_version = text.replacen(
            &format!("{FORMAT_MAGIC} {FORMAT_VERSION}"),
            &format!("{FORMAT_MAGIC} 999"),
            1,
        );
        assert!(matches!(
            SessionSnapshot::from_text(&bad_version),
            Err(SaveError::BadVersion(999))
        ));
    }

    #[test]
    fn escaping_roundtrips_special_chars() {
        let mut snap = sample();
        snap.quests.push((
            "q-|odd".into(),
            "Title with \\ backslash and | pipe\nand newline".into(),
        ));
        let parsed = SessionSnapshot::from_text(&snap.to_text()).expect("parse");
        assert_eq!(parsed, snap);
    }

    #[test]
    fn unknown_key_is_corrupt() {
        let snap = sample();
        let text = snap.to_text();
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        // Reinserts a valid checksum after injecting a foreign key.
        lines.pop();
        lines.push("gold_extra=1".into());
        let mut payload = lines.join("\n");
        payload.push('\n');
        let checksum = fnv1a64(payload.as_bytes());
        payload.push_str(&format!("checksum={checksum:016x}\n"));
        assert!(matches!(
            SessionSnapshot::from_text(&payload),
            Err(SaveError::Corrupt(_))
        ));
    }

    #[test]
    fn empty_snapshot_roundtrips() {
        let empty = SessionSnapshot::default();
        let parsed = SessionSnapshot::from_text(&empty.to_text()).expect("parse");
        assert_eq!(parsed, empty);
        assert!(parsed.goods.is_empty() && parsed.quests.is_empty() && parsed.chunks.is_empty());
    }
}
