//! # daggerfall-content-runtime - the bundle contract (I1 · F3)
//!
//! Owner: **main**. Medium: a **versioned file tree**. **No network.**
//! Trust model: OD-2 (a) side state is side-owned, never in the save envelope;
//! plan `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §3.4, §8.2, §9 F3.
//!
//! ## Why this crate exists
//!
//! Before F3 there was no override or manifest layer for assets (GAP-PC-03): a
//! side could only ship code, and whatever it wrote into the world had no
//! identity a reader could check. A *bundle* gives content the same treatment the
//! content API gives records: an explicit manifest, a version, a digest and a
//! namespace. The four rules of §3.4 are enforced here, not by review:
//! immutable publication, offline verification, side namespacing, and zero
//! executable payload.
//!
//! ## Surface
//!
//! | Item | Contract role |
//! |:---|:---|
//! | [`BundleBuilder`] | incremental, validating constructor ([`BundleBuilder::add_file`]) |
//! | [`Bundle`] | immutable manifest + payload; [`Bundle::verify`], [`Bundle::publish`], [`Bundle::open`] |
//! | [`Manifest`] / [`ManifestEntry`] | the canonical manifest and its entry table |
//! | [`BundlePath`] | validated relative path, always under the bundle's side id |
//! | [`Version`] | strict `major.minor.patch`, used for the bundle and the core pin |
//! | [`VerifyReport`] | entries, bytes and manifest digest proved by [`Bundle::verify`] |
//! | [`PublishOutcome`] | `Published` vs `AlreadyPublished` (idempotent republication) |
//! | [`detect_executable`] | magic-first, extension-second executable detection |
//! | [`BundleError`] / [`ERROR_CODES`] | the closed failure set, one stable code each |
//!
//! ## Invariants (enforced by types, constructors and tests)
//!
//! 1. **A published bundle is never edited.** [`Bundle::publish`] writes to a
//!    sibling temporary directory and renames it into place; an existing
//!    `(side, version)` with a different digest yields
//!    [`BundleError::VersionConflict`] and is left byte-identical. A new version
//!    is the only way to change content.
//! 2. **Verification is offline.** [`Bundle::open`] re-hashes `manifest.json` and
//!    every payload file against `manifest.sha256`, with no network, registry or
//!    clock involved.
//! 3. **Paths are namespaced.** Every [`BundlePath`] starts with the bundle's own
//!    side id, so two sides can never address the same file.
//! 4. **No executable payload.** Bytes are refused by magic (a renamed binary is
//!    still caught) and by extension, both at add time and again in
//!    [`Bundle::verify`], so tampering cannot smuggle code in later.
//! 5. **The manifest is canonical.** Keys and entries are sorted and the JSON is
//!    compact; [`Manifest::parse`] rejects any bytes that are not identical to
//!    their own canonical form, and [`Manifest::sha256`] digests exactly those
//!    bytes.
//! 6. **The core pin is exact.** [`Bundle::check_core`] accepts only the core
//!    version recorded in `compat.core`; there are no ranges.
//!
//! ## On disk
//!
//! `<root>/<side>/<version>/manifest.json`,
//! `<root>/<side>/<version>/manifest.sha256` (lowercase hex plus one newline) and
//! `<root>/<side>/<version>/payload/<bundle path>`. Directories are created as
//! needed; the payload tree mirrors the namespaced bundle paths.
//!
//! The normative published form lives in
//! `docs/sintergic/content-runtime/CONTENT-RUNTIME-v1.md`; `tests/surface_published.rs`
//! fails if the document drifts from [`ERROR_CODES`], the layout strings, the
//! magic table or the four rule headings.
//!
//! ## What this crate does NOT own
//!
//! - **No loading order or cross-bundle conflict resolution.** v1 publishes and
//!   verifies one bundle at a time; composing several bundles is out of scope.
//! - **No registry, no network, no signatures.** Integrity is content-addressed
//!   by SHA-256 only; authenticity beyond a hash is not claimed.
//! - **No side state persistence.** A side that needs to persist writes in its
//!   own workspace; the save envelope is closed (OD-2 (a)).
//! - **No plugin payload.** A plugin is a crate, not a bundle.
//! - **No promotion gate.** Whether a side may ship is a pipeline decision.
//! - **No engine, no async runtime, no transport, no database.** The only
//!   dependencies are `sha2`, `serde_json` and the content contract.
//!
//! ## Conformance
//!
//! `tests/f3_bundle_acceptance.rs` carries one executing test per criterion of
//! plan §9 F3 and per invariant above; `tests/surface_published.rs` is the
//! anti-drift guard over the published document and over these sources (zero
//! network, zero process execution, zero engine dependency).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod bundle;
pub mod error;
pub mod executable;
pub mod manifest;
pub mod path;
pub mod species_sprites;
pub mod version;

pub use bundle::{Bundle, BundleBuilder, PublishOutcome, VerifyReport};
pub use error::{BundleError, ERROR_CODES};
pub use executable::{detect_executable, EXECUTABLE_EXTENSIONS};
pub use manifest::{
    Manifest, ManifestEntry, COMPAT_ENGINE_KEY, ENGINE_PIN, MANIFEST_FILE, MANIFEST_HASH_FILE,
    PAYLOAD_DIR, SCHEMA_VERSION_LABEL,
};
pub use path::{BundlePath, MAX_PATH_BYTES, MAX_SEGMENT_BYTES};
pub use version::Version;
