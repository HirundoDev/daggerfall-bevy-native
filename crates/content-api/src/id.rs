//! Opaque, versioned content ids - the identity law of the contract.
//!
//! Canonical form: `dfc1:<kind>/<namespace>:<local>@<revision>`
//!
//! | Part | Grammar |
//! |:--|:--|
//! | `kind` | `item` \| `npc` \| `quest` \| `guild` \| `region` |
//! | `namespace` | `base` (the MAIN baseline) or a side id `^[a-z][a-z0-9-]{1,39}$` (reserved: `base`, `dfc1`) |
//! | `local` | `^[a-z0-9][a-z0-9._-]{0,63}$` |
//! | `revision` | `^[1-9][0-9]{0,8}$` |
//!
//! Changing the local name or the revision is a breaking change: an id, once
//! used, is never re-pointed nor recycled (plan `I1` §3.2 rule 1).

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::error::ApiError;

/// The five content kinds of v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContentKind {
    /// World items.
    Item,
    /// Non-player characters.
    Npc,
    /// Quests.
    Quest,
    /// Guilds.
    Guild,
    /// Regions.
    Region,
}

impl ContentKind {
    /// Stable lowercase token used inside ids.
    pub fn as_str(self) -> &'static str {
        match self {
            ContentKind::Item => "item",
            ContentKind::Npc => "npc",
            ContentKind::Quest => "quest",
            ContentKind::Guild => "guild",
            ContentKind::Region => "region",
        }
    }

    /// Parses the stable lowercase token.
    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        match raw {
            "item" => Ok(ContentKind::Item),
            "npc" => Ok(ContentKind::Npc),
            "quest" => Ok(ContentKind::Quest),
            "guild" => Ok(ContentKind::Guild),
            "region" => Ok(ContentKind::Region),
            _ => Err(ApiError::InvalidId {
                raw: raw.to_string(),
                reason: "kind must be one of item|npc|quest|guild|region",
            }),
        }
    }
}

/// The id of a side that owns a namespace.
///
/// `base` and `dfc1` are reserved by the contract and are rejected here, which
/// is what makes "a side can never author MAIN content" structural instead of
/// a convention.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SideId(String);

impl SideId {
    /// Parses and validates `^[a-z][a-z0-9-]{1,39}$`, rejecting reserved words.
    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        let invalid = |reason: &'static str| ApiError::InvalidSide {
            raw: raw.to_string(),
            reason,
        };
        let bytes = raw.as_bytes();
        if bytes.len() < 2 || bytes.len() > 40 {
            return Err(invalid("side id must be 2..=40 characters"));
        }
        if !bytes[0].is_ascii_lowercase() {
            return Err(invalid("side id must start with a lowercase letter"));
        }
        if !bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            return Err(invalid("side id may only contain a-z, 0-9 and -"));
        }
        if raw == "base" || raw == "dfc1" {
            return Err(invalid("side id is reserved by the contract"));
        }
        Ok(SideId(raw.to_string()))
    }

    /// The canonical text of the side id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The owner of a record: MAIN's baseline or one side.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Namespace {
    /// The MAIN baseline (`base`).
    Base,
    /// A side-owned namespace.
    Side(SideId),
}

/// A monotonic revision of one id (`>= 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(u32);

impl Revision {
    /// The numeric revision.
    pub fn get(&self) -> u32 {
        self.0
    }
}

/// A stable, opaque, versioned content id.
///
/// Ordering is `(kind, namespace, local, revision)` and is the order used for
/// keyset pagination and for the state hash, so it is part of the contract.
#[derive(Debug, Clone, Eq)]
pub struct ContentId {
    kind: ContentKind,
    namespace: Namespace,
    local: String,
    revision: u32,
    raw: String,
}

impl ContentId {
    /// The scheme prefix of every v1 id.
    pub const PREFIX: &str = "dfc1";

    /// Builds an id from its parts, validating `local` and `revision`.
    pub fn new(
        kind: ContentKind,
        namespace: Namespace,
        local: &str,
        revision: u32,
    ) -> Result<Self, ApiError> {
        if !is_valid_local(local) {
            return Err(ApiError::InvalidId {
                raw: local.to_string(),
                reason: "local must match ^[a-z0-9][a-z0-9._-]{0,63}$",
            });
        }
        if revision == 0 || revision > 999_999_999 {
            return Err(ApiError::InvalidId {
                raw: revision.to_string(),
                reason: "revision must match ^[1-9][0-9]{0,8}$",
            });
        }
        let raw = match &namespace {
            Namespace::Base => {
                format!("{}:{}/base:{local}@{revision}", Self::PREFIX, kind.as_str())
            }
            Namespace::Side(side) => format!(
                "{}:{}/{}:{local}@{revision}",
                Self::PREFIX,
                kind.as_str(),
                side.as_str()
            ),
        };
        Ok(ContentId {
            kind,
            namespace,
            local: local.to_string(),
            revision,
            raw,
        })
    }

    /// Parses the canonical `dfc1:<kind>/<namespace>:<local>@<revision>` form.
    ///
    /// Only the canonical spelling is accepted: any other text (wrong prefix,
    /// extra separators, leading zeros in the revision) is [`ApiError::InvalidId`].
    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        let invalid = |reason: &'static str| ApiError::InvalidId {
            raw: raw.to_string(),
            reason,
        };
        let rest = raw
            .strip_prefix("dfc1:")
            .ok_or_else(|| invalid("id must start with `dfc1:`"))?;
        let (head, revision_raw) = rest
            .split_once('@')
            .ok_or_else(|| invalid("id must contain `@<revision>`"))?;
        let (kind_raw, namespace_and_local) = head
            .split_once('/')
            .ok_or_else(|| invalid("id must contain `<kind>/<namespace>`"))?;
        let (namespace_raw, local) = namespace_and_local
            .split_once(':')
            .ok_or_else(|| invalid("id must contain `<namespace>:<local>`"))?;
        let kind = ContentKind::parse(kind_raw)?;
        let namespace = if namespace_raw == "base" {
            Namespace::Base
        } else {
            Namespace::Side(SideId::parse(namespace_raw)?)
        };
        if !is_valid_local(local) {
            return Err(invalid("local must match ^[a-z0-9][a-z0-9._-]{0,63}$"));
        }
        let revision = parse_revision(revision_raw)
            .ok_or_else(|| invalid("revision must match ^[1-9][0-9]{0,8}$"))?;
        ContentId::new(kind, namespace, local, revision)
    }

    /// The canonical text of the id.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The content kind.
    pub fn kind(&self) -> ContentKind {
        self.kind
    }

    /// The owning namespace.
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    /// The side-local name.
    pub fn local(&self) -> &str {
        &self.local
    }

    /// The numeric revision.
    pub fn revision(&self) -> u32 {
        self.revision
    }
}

impl PartialEq for ContentId {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.namespace == other.namespace
            && self.local == other.local
            && self.revision == other.revision
    }
}

impl Ord for ContentId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind
            .cmp(&other.kind)
            .then_with(|| self.namespace.cmp(&other.namespace))
            .then_with(|| self.local.cmp(&other.local))
            .then_with(|| self.revision.cmp(&other.revision))
    }
}

impl PartialOrd for ContentId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for ContentId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.namespace.hash(state);
        self.local.hash(state);
        self.revision.hash(state);
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

fn is_valid_local(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes[1..].iter().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'.' || *b == b'_' || *b == b'-'
    })
}

fn parse_revision(raw: &str) -> Option<u32> {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.len() > 9 {
        return None;
    }
    if !bytes.iter().all(u8::is_ascii_digit) || bytes[0] == b'0' {
        return None;
    }
    raw.parse::<u32>().ok()
}
