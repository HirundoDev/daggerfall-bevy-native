//! The in-process facade: one baseline, zero or more applied overlays.
//!
//! [`ContentApi`] owns the whole visible state. The read verb ([`ContentApi::query`])
//! and the authoring verb ([`ContentApi::apply_overlay`] /
//! [`ContentApi::remove_overlay`]) are separate entry points with separate
//! invariants: reading never allocates into the state, authoring never mutates the
//! baseline, and retiring an overlay retires its ids forever.

use std::collections::{BTreeMap, BTreeSet};

use crate::baseline::{state_hash_of, Baseline, StateHash};
use crate::error::ApiError;
use crate::id::{ContentId, ContentKind, Namespace};
use crate::overlay::{Overlay, OverlayHandle};
use crate::query::{validate_limit, Page, Query};
use crate::record::Record;
use crate::schema::SchemaVersion;

/// Largest number of overlays that may be applied at once.
const MAX_APPLIED_OVERLAYS: usize = 64;

/// The identity of a content id, ignoring its revision.
///
/// Retiring an overlay burns this whole triple: a revision versions the content
/// of a live identity, it never resurrects a retired one.
type RetiredKey = (ContentKind, Namespace, String);

/// Projects an id onto its retirement identity `(kind, namespace, local)`.
fn retired_key(id: &ContentId) -> RetiredKey {
    (id.kind(), id.namespace().clone(), id.local().to_string())
}

/// The content contract, in-process.
///
/// Constructed from an immutable [`Baseline`] and a consumer [`SchemaVersion`],
/// then fed overlays. `state_hash` covers the *view* (baseline + applied
/// overlays); `baseline_hash` covers the baseline and is constant for the life
/// of the instance.
#[derive(Debug, Clone)]
pub struct ContentApi {
    baseline: Baseline,
    /// Negotiated consumer version, kept for auditability of the handshake.
    consumer: SchemaVersion,
    overlays: BTreeMap<OverlayHandle, Overlay>,
    /// Burned identities `(kind, namespace, local)` - a revision is never enough
    /// to bring one back.
    retired: BTreeSet<RetiredKey>,
    next_handle: u64,
}

impl ContentApi {
    /// Builds the facade, negotiating the consumer's schema version.
    ///
    /// # Errors
    ///
    /// [`ApiError::UnsupportedSchema`] when the consumer's major differs or its
    /// minor is newer than [`SchemaVersion::CURRENT`].
    pub fn new(baseline: Baseline, consumer: SchemaVersion) -> Result<Self, ApiError> {
        if !SchemaVersion::CURRENT.accepts(consumer) {
            return Err(ApiError::UnsupportedSchema {
                declared: consumer,
                supported: SchemaVersion::CURRENT,
            });
        }
        Ok(ContentApi {
            baseline,
            consumer,
            overlays: BTreeMap::new(),
            retired: BTreeSet::new(),
            next_handle: 1,
        })
    }

    /// The schema version this instance publishes.
    pub fn schema_version(&self) -> SchemaVersion {
        SchemaVersion::CURRENT
    }

    /// The schema version this instance negotiated with its consumer.
    pub fn consumer(&self) -> SchemaVersion {
        self.consumer
    }

    /// Runs one read query over the visible view.
    ///
    /// # Errors
    ///
    /// [`ApiError::NotFound`] for an unknown `ById`;
    /// [`ApiError::LimitOutOfRange`] for a bad page size.
    pub fn query(&self, query: &Query) -> Result<Page, ApiError> {
        match query {
            Query::ById { id } => match self.lookup(id) {
                Some(record) => Ok(Page {
                    schema_version: SchemaVersion::CURRENT,
                    records: vec![record.clone()],
                    next: None,
                }),
                None => Err(ApiError::NotFound { id: id.clone() }),
            },
            Query::ByKind { kind, after, limit } => {
                validate_limit(*limit)?;
                let view = self.view_sorted();
                Ok(page(
                    &view,
                    *limit,
                    after.as_ref(),
                    |record| record.id().kind() == *kind,
                ))
            }
            Query::ByAttr {
                kind,
                key,
                value,
                after,
                limit,
            } => {
                validate_limit(*limit)?;
                let view = self.view_sorted();
                Ok(page(&view, *limit, after.as_ref(), |record| {
                    record.id().kind() == *kind && record.get(key) == Some(value)
                }))
            }
        }
    }

    /// Applies an overlay and returns its handle.
    ///
    /// Checks, in order, per entry: retired identity
    /// `(kind, namespace, local)` ([`ApiError::IdReuse`]), id already visible
    /// ([`ApiError::DuplicateId`]); then the overlay budget
    /// ([`ApiError::TooManyOverlays`]). The first violation aborts the whole
    /// apply - overlays are all-or-nothing.
    ///
    /// # Errors
    ///
    /// See the check list above.
    pub fn apply_overlay(&mut self, overlay: Overlay) -> Result<OverlayHandle, ApiError> {
        for entry in overlay.entries() {
            let id = entry.id();
            if self.retired.contains(&retired_key(id)) {
                return Err(ApiError::IdReuse { id: id.clone() });
            }
            if self.lookup(id).is_some() {
                return Err(ApiError::DuplicateId { id: id.clone() });
            }
        }
        if self.overlays.len() >= MAX_APPLIED_OVERLAYS {
            return Err(ApiError::TooManyOverlays {
                max: MAX_APPLIED_OVERLAYS,
            });
        }
        let handle = OverlayHandle::from_counter(self.next_handle);
        self.next_handle += 1;
        self.overlays.insert(handle, overlay);
        Ok(handle)
    }

    /// Removes an applied overlay and burns every identity `(kind, namespace,
    /// local)` it carried.
    ///
    /// A retired identity is never re-emitted at any revision, so a removed
    /// overlay cannot be silently re-applied - not even as `@2` (plan `I1` §3.2
    /// rule 1).
    ///
    /// # Errors
    ///
    /// [`ApiError::OverlayUnknown`] when the handle is not applied - including a
    /// double remove.
    pub fn remove_overlay(&mut self, handle: OverlayHandle) -> Result<(), ApiError> {
        let overlay = self
            .overlays
            .remove(&handle)
            .ok_or(ApiError::OverlayUnknown { handle })?;
        self.retired
            .extend(overlay.entries().iter().map(|entry| retired_key(entry.id())));
        Ok(())
    }

    /// Applied handles in ascending order.
    pub fn applied_overlays(&self) -> Vec<OverlayHandle> {
        self.overlays.keys().copied().collect()
    }

    /// Canonical hash of the full view (baseline + applied overlays).
    ///
    /// Independent of handles and of the order in which overlays were applied.
    pub fn state_hash(&self) -> StateHash {
        state_hash_of(self.view_sorted().into_iter())
    }

    /// Canonical hash of the baseline alone - always equal to
    /// [`Baseline::hash`] of the baseline this instance was built from.
    pub fn baseline_hash(&self) -> StateHash {
        self.baseline.hash()
    }

    /// The immutable baseline.
    pub fn baseline(&self) -> &Baseline {
        &self.baseline
    }

    fn lookup(&self, id: &ContentId) -> Option<&Record> {
        if let Some(record) = self.baseline.get(id) {
            return Some(record);
        }
        for overlay in self.overlays.values() {
            if let Ok(position) = overlay
                .entries()
                .binary_search_by(|entry| entry.id().cmp(id))
            {
                return Some(&overlay.entries()[position]);
            }
        }
        None
    }

    fn view_sorted(&self) -> Vec<&Record> {
        let mut view: Vec<&Record> = Vec::with_capacity(self.baseline.len());
        view.extend(self.baseline.iter());
        for overlay in self.overlays.values() {
            view.extend(overlay.entries());
        }
        view.sort_by(|left, right| left.id().cmp(right.id()));
        view
    }
}

fn page(
    view: &[&Record],
    limit: usize,
    after: Option<&ContentId>,
    matches: impl Fn(&Record) -> bool,
) -> Page {
    let mut records = Vec::with_capacity(limit.min(view.len()));
    let mut last = None;
    let mut more = false;
    for record in view
        .iter()
        .copied()
        .filter(|record| matches(record))
        .skip_while(|record| match after {
            Some(cursor) => record.id() <= cursor,
            None => false,
        })
    {
        if records.len() == limit {
            more = true;
            break;
        }
        last = Some(record.id().clone());
        records.push(record.clone());
    }
    Page {
        schema_version: SchemaVersion::CURRENT,
        records,
        next: if more { last } else { None },
    }
}
