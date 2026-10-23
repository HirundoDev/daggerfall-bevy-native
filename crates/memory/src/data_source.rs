//! `data_source` — Mode A / Mode B payload access with an I/O census
//! (PROBE-MODB-ISOLATION-001, INV-L99-6 / REQ-L99-016, M9).
//!
//! 🧠 Memory organ: data access, the only place allowed to touch the payload
//! files. Two custody modes:
//!
//! - **Mode A (`Retail`)** opens real files under `root/ARENA2` with
//!   [`std::fs::File`] (case-insensitive name resolution, same rule as the
//!   app's Mode A resolver) and censuses every successful open, counting
//!   `.BSA` opens separately.
//! - **Mode B (`Custom`)** never opens the disk: every [`DataSource::open`]
//!   records a **violation** and returns
//!   [`DataSourceError::IsolationViolation`] — including when the file does
//!   exist under the custom root. The block is a matter of POLICY, not of
//!   missing bytes.
//!
//! ## Deviation from the lane contract (documented)
//! The contract declared `census(&self) -> &IoCensus`. A `&self` open has to
//! mutate the census, so the census lives in a [`RefCell`] and
//! [`DataSource::census`] returns `Ref<'_, IoCensus>` (same accessor API:
//! `census().opens()`, `census().bsa_opens()`, …). Returning a plain
//! `&IoCensus` from `&self` with interior mutability would require `unsafe`,
//! which this crate does not use.

use std::cell::{Ref, RefCell};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::chunk_content::{generate_chunk, ChunkContent};
use crate::chunks::ChunkCoord;

/// Failure of a data-source read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSourceError {
    /// The payload is not available in this data source.
    NotAvailable(String),
    /// Mode B policy: a custom universe never reads payload files.
    IsolationViolation(String),
}

impl fmt::Display for DataSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAvailable(reason) => write!(f, "data source not available: {reason}"),
            Self::IsolationViolation(reason) => {
                write!(f, "custom universe isolation violation: {reason}")
            }
        }
    }
}

impl std::error::Error for DataSourceError {}

/// I/O census of a data source (evidence that Mode B never read the disk).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IoCensus {
    opens: usize,
    bsa_opens: usize,
    violations: usize,
    generated: usize,
    attempts: Vec<String>,
}

impl IoCensus {
    /// Successful payload opens.
    pub fn opens(&self) -> usize {
        self.opens
    }

    /// Successful `.BSA` opens (subset of [`Self::opens`]).
    pub fn bsa_opens(&self) -> usize {
        self.bsa_opens
    }

    /// Blocked read attempts (Mode B policy).
    pub fn violations(&self) -> usize {
        self.violations
    }

    /// Chunks produced by [`DataSource::generate_world`] (one per chunk,
    /// both modes — a non-trivial world-generation counter).
    pub fn generated(&self) -> usize {
        self.generated
    }

    /// Every attempted read, in call order.
    pub fn attempts(&self) -> Vec<String> {
        self.attempts.clone()
    }
}

/// Payload access under Mode A (retail) or Mode B (custom universe).
#[derive(Debug)]
pub enum DataSource {
    /// Mode A: real files under `root/ARENA2`, read at runtime only.
    Retail {
        /// Installation root (the directory that contains `ARENA2`).
        root: PathBuf,
        /// I/O census (interior mutability: `open` takes `&self`).
        census: RefCell<IoCensus>,
    },
    /// Mode B: custom universe, never reads payload files.
    Custom {
        /// Custom universe root (kept for evidence, never read).
        root: PathBuf,
        /// I/O census (records the policy violations).
        census: RefCell<IoCensus>,
    },
}

impl DataSource {
    /// Mode A source rooted at `root` (the directory that contains
    /// `ARENA2`).
    pub fn retail(root: impl Into<PathBuf>) -> Self {
        Self::Retail {
            root: root.into(),
            census: RefCell::new(IoCensus::default()),
        }
    }

    /// Mode B source rooted at `root`.
    pub fn custom(root: impl Into<PathBuf>) -> Self {
        Self::Custom {
            root: root.into(),
            census: RefCell::new(IoCensus::default()),
        }
    }

    /// Opens `name`.
    ///
    /// Mode A reads `root/ARENA2/<name>` (case-insensitive) and censuses the
    /// open. Mode B never touches the disk: it censuses a violation and
    /// always returns [`DataSourceError::IsolationViolation`].
    pub fn open(&self, name: &str) -> Result<Vec<u8>, DataSourceError> {
        match self {
            Self::Retail { root, .. } => {
                self.census_mut().attempts.push(format!("retail open {name}"));

                let dir = arena2_dir(root);
                let path = match resolve_case_insensitive(&dir, name) {
                    Some(path) => path,
                    None => {
                        return Err(DataSourceError::NotAvailable(format!(
                            "{name} not found under {dir:?}"
                        )))
                    }
                };
                let mut file = File::open(&path).map_err(|error| {
                    DataSourceError::NotAvailable(format!("{path:?}: {error}"))
                })?;
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).map_err(|error| {
                    DataSourceError::NotAvailable(format!("{path:?}: {error}"))
                })?;

                let mut census = self.census_mut();
                census.opens += 1;
                if is_bsa(name) {
                    census.bsa_opens += 1;
                }
                Ok(bytes)
            }
            Self::Custom { .. } => {
                let mut census = self.census_mut();
                census.violations += 1;
                census
                    .attempts
                    .push(format!("custom open {name} (blocked before I/O)"));
                Err(DataSourceError::IsolationViolation(format!(
                    "custom universe never reads payload files (requested {name})"
                )))
            }
        }
    }

    /// Generates the world content of `coords` and censuses one `generated`
    /// unit per chunk.
    ///
    /// Both modes compute the chunks through the pure
    /// [`crate::chunk_content::generate_chunk`] pipeline and **never open a
    /// payload file**: a generated world is a computation, not a read. Mode B
    /// additionally stays disk-free by policy. The returned chunks are the
    /// real product of the call, so `generated() == coords.len()` is a
    /// non-trivial check (the counter cannot move without producing chunks).
    pub fn generate_world(&self, seed: u64, coords: &[ChunkCoord]) -> Vec<ChunkContent> {
        let chunks: Vec<ChunkContent> = coords
            .iter()
            .map(|coord| generate_chunk(seed, *coord))
            .collect();
        self.census_mut().generated += chunks.len();
        chunks
    }

    /// Census of this source so far.
    pub fn census(&self) -> Ref<'_, IoCensus> {
        match self {
            Self::Retail { census, .. } | Self::Custom { census, .. } => census.borrow(),
        }
    }

    /// Mutable census (interior mutability: the readers/writers take `&self`).
    fn census_mut(&self) -> std::cell::RefMut<'_, IoCensus> {
        match self {
            Self::Retail { census, .. } | Self::Custom { census, .. } => census.borrow_mut(),
        }
    }

    /// True for Mode B (custom universe).
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom { .. })
    }

    /// Root of this source, as declared.
    pub fn root(&self) -> &Path {
        match self {
            Self::Retail { root, .. } | Self::Custom { root, .. } => root,
        }
    }
}

/// True when `name` designates a `.BSA` archive (case-insensitive).
fn is_bsa(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".bsa")
}

/// Resolves the `ARENA2` directory under `root` (both capitalization
/// variants; `root` itself is accepted when it already is `ARENA2`).
fn arena2_dir(root: &Path) -> PathBuf {
    for candidate in ["ARENA2", "arena2"] {
        let path = root.join(candidate);
        if path.is_dir() {
            return path;
        }
    }
    let already_arena2 = root
        .file_name()
        .map(|name| name.eq_ignore_ascii_case("ARENA2"))
        .unwrap_or(false);
    if already_arena2 {
        return root.to_path_buf();
    }
    root.join("ARENA2")
}

/// Case-insensitive resolution of `name` inside `dir`: an exact-case match
/// wins, then the sorted scan (deterministic precedence).
fn resolve_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidates: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|file_name| file_name.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scratch directory removed on drop (0 bytes left behind).
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn create(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "df-memory-datasource-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(path.join("ARENA2")).unwrap();
            Self { path }
        }

        fn arena2(&self) -> PathBuf {
            self.path.join("ARENA2")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn custom_never_reads_even_when_the_file_exists() {
        let scratch = Scratch::create("custom");
        std::fs::write(scratch.arena2().join("MAPS.BSA"), [0u8; 32]).unwrap();

        let source = DataSource::custom(&scratch.path);
        assert!(source.is_custom());
        let error = source.open("MAPS.BSA").unwrap_err();
        assert!(matches!(error, DataSourceError::IsolationViolation(_)));
        assert!(error.to_string().contains("isolation violation"));

        let census = source.census();
        assert_eq!(census.opens(), 0);
        assert_eq!(census.bsa_opens(), 0);
        assert_eq!(census.violations(), 1);
        assert_eq!(census.attempts().len(), 1);
        assert!(census.attempts()[0].contains("blocked before I/O"));
    }

    #[test]
    fn retail_opens_and_censuses_bsa() {
        let scratch = Scratch::create("retail");
        let payload = vec![0xABu8; 96];
        std::fs::write(scratch.arena2().join("MAPS.BSA"), &payload).unwrap();
        // Lowercase on disk, uppercase request: case-insensitive resolution.
        std::fs::write(scratch.arena2().join("arch3d.bsa"), [0u8; 8]).unwrap();

        let source = DataSource::retail(&scratch.path);
        assert!(!source.is_custom());
        assert_eq!(source.open("MAPS.BSA").unwrap(), payload);
        assert_eq!(source.open("ARCH3D.BSA").unwrap().len(), 8);
        assert!(source.open("MISSING.BSA").is_err());

        let census = source.census();
        assert_eq!(census.opens(), 2);
        assert_eq!(census.bsa_opens(), 2);
        assert_eq!(census.violations(), 0);
        assert_eq!(census.attempts().len(), 3);
    }

    #[test]
    fn generate_world_counts_chunks_in_both_modes_without_opens() {
        let scratch = Scratch::create("generate");
        let coords = [
            ChunkCoord::new(0, 0),
            ChunkCoord::new(1, 0),
            ChunkCoord::new(0, 1),
            ChunkCoord::new(1, 1),
        ];

        for source in [
            DataSource::custom(&scratch.path),
            DataSource::retail(&scratch.path),
        ] {
            let world = source.generate_world(5, &coords);
            assert_eq!(world.len(), coords.len());
            assert_eq!(world[0].heights.len(), crate::chunk_content::CELL_COUNT);
            let census = source.census();
            assert_eq!(census.generated(), coords.len());
            assert_eq!(census.opens(), 0, "world generation must not open files");
            assert_eq!(census.bsa_opens(), 0);
            assert_eq!(census.violations(), 0);
        }

        // Deltas accumulate; the counter is not a fixed echo of the slice.
        let source = DataSource::custom(&scratch.path);
        source.generate_world(5, &coords[..2]);
        source.generate_world(6, &coords);
        assert_eq!(source.census().generated(), 2 + coords.len());
        assert_eq!(source.census().opens(), 0);
    }

    #[test]
    fn missing_file_is_reported_as_not_available() {
        let scratch = Scratch::create("missing");
        let source = DataSource::retail(&scratch.path);
        let error = source.open("NOPE.BSA").unwrap_err();
        assert!(matches!(error, DataSourceError::NotAvailable(_)));
        assert_eq!(source.census().opens(), 0);
    }
}
