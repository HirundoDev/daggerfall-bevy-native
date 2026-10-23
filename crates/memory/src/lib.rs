//! Memory organ — persistence, saves and data access (F0 scaffold).
//!
//! M7: [`chunks`] — `ChunkMap` (chunks loaded in the session), source of the
//! PD4 T7 Automap read model (REQ-L99-039, declared minimum contract).
//!
//! M8: [`session_store`] — session save format and I/O (REQ-L99-041,
//! INV-L99-4): `SessionSnapshot` (primitives only, bus-only) with
//! save/load, magic, version and FNV-1a 64 checksum (corruption
//! detection). The session loop (❤️ heart) and the ECS mapping live in `app`.
//!
//! M9: [`chunk_content`] — pure deterministic chunk payload
//! (INV-L99-2): per-cell integer generation, canonical bytes, per-chunk
//! digest and an order-invariant set digest, plus [`ChunkSet`]. And
//! [`data_source`] — Mode A retail access vs Mode B custom-universe
//! isolation with an I/O census (INV-L99-6 / REQ-L99-016).

pub mod chunk_content;
pub mod chunks;
pub mod data_source;
pub mod session_store;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "memory";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
