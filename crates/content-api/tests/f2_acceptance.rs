//! F2 acceptance suite - plan `I1` §9 F2, criteria (a)(b)(c).
//!
//! Deterministic fixture: a 5-record baseline (one per [`ContentKind`], namespace
//! `base`, revision `@1`) built through [`base_record`], and side overlays built
//! through [`side_record`]. No sleeps, no I/O, no environment.

use daggerfall_content_api::query::MAX_PAGE;
use daggerfall_content_api::{
    ApiError, AttrValue, Baseline, ContentApi, ContentId, ContentKind, Namespace, Overlay, Query,
    Record, SchemaVersion, SideId, SCHEMA_VERSION_LABEL,
};

const KINDS: [ContentKind; 5] = [
    ContentKind::Item,
    ContentKind::Npc,
    ContentKind::Quest,
    ContentKind::Guild,
    ContentKind::Region,
];

/// `base`-namespace record at revision `@1` with two typed attributes.
fn base_record(kind: ContentKind, local: &str) -> Record {
    let id = ContentId::new(kind, Namespace::Base, local, 1).expect("valid base id");
    Record::new(
        id,
        [
            ("name".to_string(), AttrValue::Text(local.to_string())),
            ("level".to_string(), AttrValue::Int(7)),
        ],
    )
    .expect("valid record")
}

/// `side`-namespace record at revision `@1`.
fn side_record(side: &SideId, kind: ContentKind, local: &str) -> Record {
    let id = ContentId::new(kind, Namespace::Side(side.clone()), local, 1).expect("valid side id");
    Record::new(
        id,
        [(
            "name".to_string(),
            AttrValue::Text(format!("{}/{local}", side.as_str())),
        )],
    )
    .expect("valid record")
}

fn side(raw: &str) -> SideId {
    SideId::parse(raw).expect("valid side id")
}

fn baseline() -> Baseline {
    let records = KINDS
        .iter()
        .map(|kind| base_record(*kind, &format!("{}-one", kind.as_str())))
        .collect();
    Baseline::new(records).expect("valid baseline")
}

fn fresh_api() -> ContentApi {
    ContentApi::new(baseline(), SchemaVersion::CURRENT).expect("negotiated schema")
}

fn overlay(side: &SideId, label: &str, entries: Vec<Record>) -> Overlay {
    Overlay::new(side.clone(), label, entries).expect("valid overlay")
}

fn id_of(record: &Record) -> ContentId {
    record.id().clone()
}

/// (a) The schema is published and negotiated, and every page carries it.
#[test]
fn a_schema_version_is_published_and_negotiated() {
    assert_eq!(SCHEMA_VERSION_LABEL, "1.0");
    assert_eq!(
        SchemaVersion::parse("1.0").expect("canonical"),
        SchemaVersion::CURRENT
    );
    assert_eq!(SchemaVersion::CURRENT.label(), SCHEMA_VERSION_LABEL);
    assert!(SchemaVersion::CURRENT.accepts(SchemaVersion::CURRENT));

    let api = fresh_api();
    assert_eq!(api.consumer(), SchemaVersion::CURRENT);
    let item = base_record(ContentKind::Item, "item-one");
    let page = api
        .query(&Query::ById { id: id_of(&item) })
        .expect("known id");
    assert_eq!(page.schema_version, SchemaVersion::CURRENT);
    let page = api
        .query(&Query::ByKind {
            kind: ContentKind::Npc,
            after: None,
            limit: 1,
        })
        .expect("valid page");
    assert_eq!(page.schema_version, SchemaVersion::CURRENT);

    let newer = SchemaVersion::parse("2.0").expect("parsable");
    let err = ContentApi::new(baseline(), newer).expect_err("major mismatch must fail");
    assert_eq!(
        err,
        ApiError::UnsupportedSchema {
            declared: newer,
            supported: SchemaVersion::CURRENT,
        }
    );
    for raw in ["1", "1.x", "01.0", "", "1.0.0"] {
        assert!(
            matches!(SchemaVersion::parse(raw), Err(ApiError::InvalidRecord { .. })),
            "`{raw}` must not parse"
        );
    }
}

/// (b) Applying then removing an overlay leaves the world hash-identical.
#[test]
fn b_overlay_apply_and_remove_leaves_world_identical() {
    let mut api = fresh_api();
    let h0 = api.state_hash();
    let b0 = api.baseline_hash();
    assert_eq!(b0, api.baseline().hash());

    let baseline_npc = base_record(ContentKind::Npc, "npc-one");
    let before = api
        .query(&Query::ById {
            id: id_of(&baseline_npc),
        })
        .expect("baseline visible")
        .records[0]
        .clone();

    let wiki = side("wiki");
    let entries = vec![
        side_record(&wiki, ContentKind::Item, "iron-sword-v2"),
        side_record(&wiki, ContentKind::Npc, "aldric-v2"),
    ];
    let handle = api
        .apply_overlay(overlay(&wiki, "wiki-core", entries.clone()))
        .expect("overlay applies");
    let h1 = api.state_hash();
    assert_ne!(h1, h0, "the applied overlay must change the view");
    assert_eq!(api.baseline_hash(), b0, "apply must not touch the baseline");
    assert_eq!(api.applied_overlays(), vec![handle]);
    for entry in &entries {
        let page = api
            .query(&Query::ById {
                id: id_of(entry),
            })
            .expect("overlay record visible while applied");
        assert_eq!(page.records, vec![entry.clone()]);
    }

    api.remove_overlay(handle).expect("overlay removes");
    let h2 = api.state_hash();
    eprintln!("state hashes h0={h0} h1={h1} h2={h2}");
    assert_eq!(h2, h0, "remove must restore the exact pre-apply state");
    assert_eq!(api.baseline_hash(), b0);
    assert_eq!(api.baseline().hash(), b0);
    assert!(api.applied_overlays().is_empty());
    for entry in &entries {
        assert_eq!(
            api.query(&Query::ById {
                id: id_of(entry),
            })
            .expect_err("retired ids are no longer visible"),
            ApiError::NotFound { id: id_of(entry) }
        );
    }
    let after = api
        .query(&Query::ById {
            id: id_of(&baseline_npc),
        })
        .expect("baseline visible again")
        .records[0]
        .clone();
    assert_eq!(after, before, "baseline record is byte-identical after the cycle");
}

/// (c) Ids are stable, opaque, versioned - and never reused.
#[test]
fn c_ids_are_stable_opaque_versioned_and_not_reusable() {
    let npc = base_record(ContentKind::Npc, "aldric");
    let id = id_of(&npc);
    assert_eq!(ContentId::parse(id.as_str()).expect("round-trips"), id);
    assert_eq!(id.to_string(), "dfc1:npc/base:aldric@1");
    assert_eq!(id.as_str(), "dfc1:npc/base:aldric@1");
    assert_eq!(id.revision(), 1);
    assert_eq!(id.kind(), ContentKind::Npc);
    assert_eq!(id.local(), "aldric");
    assert_eq!(id.namespace(), &Namespace::Base);
    assert!(id.as_str().starts_with(ContentId::PREFIX));
    assert!(ContentId::parse("dfc1:npc/base:aldric@0").is_err());

    let wiki = side("wiki");
    let entry = side_record(&wiki, ContentKind::Item, "iron-sword");
    let mut api = fresh_api();
    let handle = api
        .apply_overlay(overlay(&wiki, "v1", vec![entry.clone()]))
        .expect("applies");
    api.remove_overlay(handle).expect("removes");
    let err = api
        .apply_overlay(overlay(&wiki, "v1-retry", vec![entry.clone()]))
        .expect_err("retired id must never be reused");
    assert_eq!(err, ApiError::IdReuse { id: id_of(&entry) });

    // A baseline identity cannot be re-pointed: `Overlay::new` refuses a `base`
    // record before `apply_overlay` can even consider it...
    let baseline_quest = base_record(ContentKind::Quest, "quest-one");
    let err = Overlay::new(wiki.clone(), "shadow", vec![baseline_quest.clone()])
        .expect_err("base namespace is never writable by a side");
    assert_eq!(
        err,
        ApiError::NamespaceViolation {
            id: id_of(&baseline_quest),
            side: wiki.clone(),
        }
    );
    // ...and an id that is already visible is refused as a collision.
    let applied = side_record(&wiki, ContentKind::Guild, "fighters");
    let handle = api
        .apply_overlay(overlay(&wiki, "v2", vec![applied.clone()]))
        .expect("applies");
    let err = api
        .apply_overlay(overlay(&wiki, "v2-again", vec![applied.clone()]))
        .expect_err("visible id must not be shadowed");
    assert_eq!(err, ApiError::DuplicateId { id: id_of(&applied) });
    assert_eq!(api.applied_overlays(), vec![handle]);
}

/// Retiring an overlay burns `(kind, namespace, local)`, not one exact id: the
/// same `local` cannot come back as `@2`, while a different `local` still can.
#[test]
fn retired_local_cannot_be_reemitted_at_any_revision() {
    let wiki = side("wiki");
    let mut api = fresh_api();

    let v1 = side_record(&wiki, ContentKind::Item, "lantern");
    let handle = api
        .apply_overlay(overlay(&wiki, "lantern-v1", vec![v1.clone()]))
        .expect("applies");
    api.remove_overlay(handle).expect("removes");

    // Same identity, new revision, different content: still a retired identity.
    let v2_id = ContentId::new(
        ContentKind::Item,
        Namespace::Side(wiki.clone()),
        "lantern",
        2,
    )
    .expect("valid side id");
    let v2 = Record::new(
        v2_id.clone(),
        [("name".to_string(), AttrValue::Text("relit".to_string()))],
    )
    .expect("valid record");
    let err = api
        .apply_overlay(overlay(&wiki, "lantern-v2", vec![v2]))
        .expect_err("a retired local must not be re-emitted at any revision");
    assert_eq!(err, ApiError::IdReuse { id: v2_id });
    assert!(api.applied_overlays().is_empty(), "the apply was all-or-nothing");

    // A different local in the same side and kind is untouched.
    let other = side_record(&wiki, ContentKind::Item, "rope");
    let handle = api
        .apply_overlay(overlay(&wiki, "rope-v1", vec![other.clone()]))
        .expect("a fresh local still applies");
    assert_eq!(api.applied_overlays(), vec![handle]);
}

/// `ById` resolves a baseline record and reports unknown ids loudly.
#[test]
fn query_by_id_returns_baseline_record_and_unknown_is_not_found() {
    let api = fresh_api();
    let region = base_record(ContentKind::Region, "region-one");
    let page = api
        .query(&Query::ById {
            id: id_of(&region),
        })
        .expect("known");
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0], region);
    assert_eq!(page.next, None);

    let missing = ContentId::new(ContentKind::Item, Namespace::Base, "ghost", 1).expect("id");
    assert_eq!(
        api.query(&Query::ById { id: missing.clone() })
            .expect_err("unknown id"),
        ApiError::NotFound { id: missing }
    );
}

/// Keyset paging: stable under repetition, `after` is exclusive, `next` only when
/// more matches remain.
#[test]
fn query_by_kind_is_keyset_paginated_and_stable() {
    // The paging fixture needs several records of ONE kind; the shared 5-record
    // fixture has exactly one per kind, so this test builds five items.
    let records: Vec<Record> = (0..5)
        .map(|index| base_record(ContentKind::Item, &format!("item-{index}")))
        .collect();
    let api = ContentApi::new(
        Baseline::new(records.clone()).expect("valid baseline"),
        SchemaVersion::CURRENT,
    )
    .expect("negotiated");

    let mut collected = Vec::new();
    let mut after = None;
    let mut pages = 0;
    let mut pages_with_next = 0;
    loop {
        let page = api
            .query(&Query::ByKind {
                kind: ContentKind::Item,
                after: after.clone(),
                limit: 2,
            })
            .expect("valid page");
        assert!(page.records.len() <= 2);
        pages += 1;
        collected.extend(page.records.iter().map(id_of));
        match page.next {
            Some(cursor) => {
                pages_with_next += 1;
                assert_eq!(cursor, id_of(page.records.last().expect("non-empty page")));
                after = Some(cursor);
            }
            None => break,
        }
        assert!(pages < 10, "pagination must terminate");
    }
    assert_eq!(pages, 3, "5 records at limit 2 -> 2/2/1");
    assert_eq!(pages_with_next, 2, "only the last page omits `next`");
    assert_eq!(collected.len(), 5);

    let sorted = {
        let mut ids: Vec<ContentId> = records.iter().map(id_of).collect();
        ids.sort();
        ids
    };
    assert_eq!(collected, sorted, "pages concatenate in ascending id order");

    // Repetition is deterministic: same query, same bytes.
    let first = api
        .query(&Query::ByKind {
            kind: ContentKind::Item,
            after: None,
            limit: 2,
        })
        .expect("page");
    let second = api
        .query(&Query::ByKind {
            kind: ContentKind::Item,
            after: None,
            limit: 2,
        })
        .expect("page");
    assert_eq!(first, second);

    // `after` is exclusive: starting at the second id skips it.
    let cursor = id_of(&records[1]);
    let page = api
        .query(&Query::ByKind {
            kind: ContentKind::Item,
            after: Some(cursor.clone()),
            limit: MAX_PAGE,
        })
        .expect("page");
    assert!(!page.records.iter().any(|record| record.id() == &cursor));
    assert_eq!(page.records.len(), 3);
    assert_eq!(page.next, None);
}

/// `ByAttr` compares the exact typed value, never a coerced string.
#[test]
fn query_by_attr_filters_typed_values() {
    let api = fresh_api();
    let exact = api
        .query(&Query::ByAttr {
            kind: ContentKind::Npc,
            key: "level".to_string(),
            value: AttrValue::Int(7),
            after: None,
            limit: MAX_PAGE,
        })
        .expect("valid page");
    assert_eq!(exact.records, vec![base_record(ContentKind::Npc, "npc-one")]);

    for mismatched in [AttrValue::Int(8), AttrValue::Text("7".to_string())] {
        let page = api
            .query(&Query::ByAttr {
                kind: ContentKind::Npc,
                key: "level".to_string(),
                value: mismatched,
                after: None,
                limit: MAX_PAGE,
            })
            .expect("valid page");
        assert!(page.records.is_empty(), "a different value or type never matches");
        assert_eq!(page.next, None);
    }

    let missing = api
        .query(&Query::ByAttr {
            kind: ContentKind::Npc,
            key: "no_such_key".to_string(),
            value: AttrValue::Int(7),
            after: None,
            limit: MAX_PAGE,
        })
        .expect("valid page");
    assert!(missing.records.is_empty());
}

/// Both ends of the page-size range are rejected.
#[test]
fn limit_out_of_range_is_rejected() {
    let api = fresh_api();
    for limit in [0, MAX_PAGE + 1] {
        assert_eq!(
            api.query(&Query::ByKind {
                kind: ContentKind::Item,
                after: None,
                limit,
            })
            .expect_err("limit out of range"),
            ApiError::LimitOutOfRange {
                limit,
                max: MAX_PAGE
            }
        );
        assert_eq!(
            api.query(&Query::ByAttr {
                kind: ContentKind::Item,
                key: "level".to_string(),
                value: AttrValue::Int(7),
                after: None,
                limit,
            })
            .expect_err("limit out of range"),
            ApiError::LimitOutOfRange {
                limit,
                max: MAX_PAGE
            }
        );
    }
}

/// An overlay can only contain its own side's namespace.
#[test]
fn overlay_cannot_write_base_or_another_sides_namespace() {
    let wiki = side("wiki");
    let other = side("other");
    let base_entry = base_record(ContentKind::Item, "item-one");
    let err = Overlay::new(wiki.clone(), "trespass", vec![base_entry.clone()])
        .expect_err("base is not writable");
    assert_eq!(
        err,
        ApiError::NamespaceViolation {
            id: id_of(&base_entry),
            side: wiki.clone(),
        }
    );

    let foreign = side_record(&other, ContentKind::Item, "foreign-sword");
    let err = Overlay::new(wiki.clone(), "trespass", vec![foreign.clone()])
        .expect_err("another side is not writable");
    assert_eq!(
        err,
        ApiError::NamespaceViolation {
            id: id_of(&foreign),
            side: wiki,
        }
    );
}

/// Duplicates inside one batch, and collisions with anything already visible.
#[test]
fn overlay_entries_must_be_unique_and_not_collide_with_baseline() {
    let wiki = side("wiki");
    let entry = side_record(&wiki, ContentKind::Item, "dupe");
    let err = Overlay::new(wiki.clone(), "dupes", vec![entry.clone(), entry.clone()])
        .expect_err("duplicate ids inside one overlay");
    assert_eq!(err, ApiError::DuplicateId { id: id_of(&entry) });

    // A baseline id can never reach the collision check: the namespace guard fires
    // first, which is exactly what "must not collide with the baseline" means
    // structurally.
    let baseline_item = base_record(ContentKind::Item, "item-one");
    assert_eq!(
        Overlay::new(wiki.clone(), "shadow", vec![baseline_item.clone()])
            .expect_err("baseline ids are unreachable from an overlay"),
        ApiError::NamespaceViolation {
            id: id_of(&baseline_item),
            side: wiki.clone(),
        }
    );

    // The collision guard itself (`lookup` covers baseline plus applied overlays)
    // is exercised with an overlay id that is already visible.
    let mut api = fresh_api();
    let applied = side_record(&wiki, ContentKind::Item, "collide");
    api.apply_overlay(overlay(&wiki, "one", vec![applied.clone()]))
        .expect("applies");
    assert_eq!(
        api.apply_overlay(overlay(&wiki, "two", vec![applied.clone()]))
            .expect_err("already visible"),
        ApiError::DuplicateId { id: id_of(&applied) }
    );
    assert_eq!(api.state_hash(), {
        // The rejected apply must not have changed anything.
        let mut probe = fresh_api();
        probe
            .apply_overlay(overlay(&wiki, "one", vec![applied]))
            .expect("applies");
        probe.state_hash()
    });
}

/// Unknown or already-removed handles fail loudly, never silently.
#[test]
fn remove_unknown_overlay_fails_loudly() {
    let wiki = side("wiki");
    let mut api = fresh_api();
    let handle = api
        .apply_overlay(overlay(
            &wiki,
            "once",
            vec![side_record(&wiki, ContentKind::Item, "one-shot")],
        ))
        .expect("applies");
    api.remove_overlay(handle).expect("first remove succeeds");
    assert_eq!(
        api.remove_overlay(handle).expect_err("double remove"),
        ApiError::OverlayUnknown { handle }
    );

    // A handle minted by another instance is unknown here: handles are per-API.
    let mut other_api = fresh_api();
    let foreign = other_api
        .apply_overlay(overlay(
            &wiki,
            "foreign",
            vec![side_record(&wiki, ContentKind::Npc, "borrowed")],
        ))
        .expect("applies");
    assert_eq!(
        api.remove_overlay(foreign)
            .expect_err("handles are not portable"),
        ApiError::OverlayUnknown { handle: foreign }
    );
}

/// The baseline never moves, no matter how many overlays come and go.
#[test]
fn baseline_is_immutable_across_overlay_lifecycle() {
    let mut api = fresh_api();
    let len = api.baseline().len();
    let hash = api.baseline_hash();
    let wiki = side("wiki");
    let entry = side_record(&wiki, ContentKind::Quest, "extra-quest");

    assert_eq!(api.baseline().len(), len);
    assert!(!api.baseline().is_empty());
    let handle = api
        .apply_overlay(overlay(&wiki, "lifecycle", vec![entry]))
        .expect("applies");
    assert_eq!(api.baseline().len(), len);
    assert_eq!(api.baseline_hash(), hash);
    api.remove_overlay(handle).expect("removes");
    assert_eq!(api.baseline().len(), len);
    assert_eq!(api.baseline_hash(), hash);
    assert_eq!(api.state_hash(), hash, "no overlays -> view equals baseline");

    // Still queryable without any overlay applied.
    for kind in KINDS {
        let page = api
            .query(&Query::ByKind {
                kind,
                after: None,
                limit: MAX_PAGE,
            })
            .expect("valid page");
        assert_eq!(page.records.len(), 1);
    }
}
