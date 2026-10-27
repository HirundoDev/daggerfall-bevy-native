//! # daggerfall-content-api - the content contract (I1 · F2)
//!
//! Owner: **main**. Medium: `path` dependency, **in-process**. **No network.**
//! Trust model: OD-3 (a) - in-process crate (plan
//! `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §3.2, §8.2).
//!
//! Schema version: **1.0** ([`SCHEMA_VERSION_LABEL`]). The normative published
//! form lives in `docs/sintergic/content-api/CONTENT-API-v1.md` and
//! `docs/sintergic/content-api/content-api-v1.schema.json`; `tests/schema_published.rs`
//! fails if either drifts from this crate.
//!
//! ## Why this crate exists
//!
//! A wiki or a side project that wants to read the world, or add content to it,
//! used to need the game's internal store - or worse, a copy of it. That made
//! "read" and "author" the same privilege and made every mod a fork. This crate
//! splits those privileges into two verbs with different invariants, behind
//! opaque ids that are safe to publish.
//!
//! ## Two verbs, two privileges
//!
//! | Verb | What it does | What it can NEVER do |
//! |:---|:---|:---|
//! | **query** (read) - [`ContentApi::query`] | returns records by stable id or by keyset criterion | expose the store, accept SQL, page by blind offset, mutate anything |
//! | **overlay** (authoring) - [`ContentApi::apply_overlay`] / [`ContentApi::remove_overlay`] | adds a side-namespaced layer on top of the baseline | mutate the baseline, edit MAIN content, delete anything, shadow an id |
//!
//! ## Surface
//!
//! | Item | Contract role |
//! |:---|:---|
//! | [`ContentApi`] | the facade: baseline + applied overlays + retired ids |
//! | [`Baseline`] | immutable sorted `base`-namespace records (`&mut` never exposed) |
//! | [`Record`] / [`AttrValue`] | immutable value + typed attribute (`Text`/`Int`/`Bool`/`Ref`) |
//! | [`ContentId`] | opaque, versioned identity `dfc1:<kind>/<namespace>:<local>@<revision>` |
//! | [`ContentKind`] / [`Namespace`] / [`SideId`] / [`Revision`] | the parts of an id |
//! | [`Query`] / [`Page`] | the read request and its keyset page |
//! | [`Overlay`] / [`OverlayHandle`] | the authoring batch and its opaque handle |
//! | [`SchemaVersion`] | negotiation: same major, `consumer.minor <= current.minor` |
//! | [`StateHash`] | canonical SHA-256 of a whole view |
//! | [`ApiError`] / [`ERROR_CODES`] | the closed failure set, one stable code each |
//!
//! ## Invariants (all enforced by types or constructors, never by convention)
//!
//! 1. **Ids are never recycled.** An id retired by `remove_overlay` is refused
//!    with [`ApiError::IdReuse`], forever.
//! 2. **The baseline is immutable.** [`Baseline`] exposes no `&mut` accessor and
//!    [`ContentApi::baseline_hash`] equals [`Baseline::hash`] for the life of the
//!    instance.
//! 3. **A side cannot author MAIN content.** Every overlay entry must live in the
//!    batch's own [`Namespace::Side`]; `base` and `dfc1` are reserved side ids.
//! 4. **Overlays are additive.** `apply_overlay` refuses an id already visible
//!    ([`ApiError::DuplicateId`]): no shadowing, no edits, no deletes.
//! 5. **Paging is keyset, not offset.** `after` is exclusive over the ascending
//!    [`ContentId`] order, so a page never skips or repeats because an overlay
//!    appeared in between.
//! 6. **Apply/remove is reversible by hash.** `state_hash()` before applying,
//!    while applied and after removing satisfy `h2 == h0`, regardless of which
//!    handle the overlay got or when it was applied.
//!
//! ## Canonical state hash
//!
//! `b"dfc1-content-state\x00"` · len-prefixed schema version · `u64` record count ·
//! per record in `ContentId` order: len-prefixed id, `u32` attribute count, then
//! each attribute in key order (len-prefixed key, one tag byte for the variant and
//! its payload). Handles and application order are excluded on purpose: equal
//! views hash equal. See [`StateHash`] and the normative document.
//!
//! ## What this crate does NOT own
//!
//! - **No transport.** 0 HTTP, 0 UDS, 0 sockets, 0 network, 0 async runtime.
//!   F2 is the contract; serving it is F3's problem.
//! - **No persistence.** 0 SQL, 0 files: state is built from an in-memory
//!   [`Baseline`] and in-memory overlays.
//! - **No bundle resolution.** `content-runtime`, loading order and conflict
//!   diagnostics across bundles are out of scope.
//! - **No promotion gate.** Whether a side may ship is a pipeline decision, not a
//!   content-API decision.
//! - **No real sides, no parser changes, no game engine.** No dependency on a web
//!   framework, a database driver, a serializer or an engine: `sha2` is the only
//!   dependency.
//! - **No identity allocation policy.** It stores ids; it does not invent them for
//!   a side that has no id scheme of its own.
//!
//! ## Conformance
//!
//! `tests/f2_acceptance.rs` (criterion (a) schema published + negotiated,
//! (b) overlay apply/remove leaves the world hash-identical, (c) ids stable,
//! opaque, versioned and not reusable, plus keyset paging, exact typed attribute
//! filtering, limit bounds, namespace guard, uniqueness and handle errors) and
//! `tests/schema_published.rs` (anti-drift against both published documents).
//!
//! ## MSRV and semver
//!
//! `rust-version = "1.97.1"` (workspace MSRV). Below 1.0: a change to an id
//! grammar, to the [`ContentId`] order, to the state-hash encoding or to a
//! published [`ERROR_CODES`] entry is a **breaking** release, because consumers
//! store ids and hashes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod api;
pub mod baseline;
pub mod error;
pub mod id;
pub mod overlay;
pub mod query;
pub mod record;
pub mod schema;

pub use api::ContentApi;
pub use baseline::{Baseline, StateHash};
pub use error::{ApiError, ERROR_CODES};
pub use id::{ContentId, ContentKind, Namespace, Revision, SideId};
pub use overlay::{Overlay, OverlayHandle};
pub use query::{Page, Query};
pub use record::{AttrValue, Record};
pub use schema::{SchemaVersion, SurfaceItem, SCHEMA_VERSION_LABEL, SURFACE};
