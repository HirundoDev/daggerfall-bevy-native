//! Immutable content records and their typed attributes.
//!
//! A record is a value: it carries an id and a sorted map of attributes. Attribute
//! keys are `^[a-z][a-z0-9_]{0,31}$`, unique, at most 64 per record, and text
//! values are capped at 4096 UTF-8 bytes so no single record can smuggle a blob
//! through the query verb.

use std::collections::BTreeMap;

use crate::error::ApiError;
use crate::id::ContentId;

/// Maximum number of attributes in one record.
pub const MAX_ATTRS: usize = 64;

/// Maximum byte length of one [`AttrValue::Text`] value.
pub const MAX_TEXT_BYTES: usize = 4096;

/// A typed attribute value.
///
/// `ByAttr` queries compare these exactly, including the variant: `Int(1)` never
/// matches `Text("1")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    /// UTF-8 text, at most [`MAX_TEXT_BYTES`].
    Text(String),
    /// Signed integer.
    Int(i64),
    /// Boolean.
    Bool(bool),
    /// Reference to another content id.
    Ref(ContentId),
}

/// One content record: mutable only by replacing it with another valid record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    id: ContentId,
    attrs: BTreeMap<String, AttrValue>,
}

impl Record {
    /// Builds a record, validating every attribute.
    ///
    /// # Errors
    ///
    /// [`ApiError::InvalidRecord`] for a malformed key, more than [`MAX_ATTRS`]
    /// attributes, a repeated key or an oversized text value.
    pub fn new(
        id: ContentId,
        attrs: impl IntoIterator<Item = (String, AttrValue)>,
    ) -> Result<Self, ApiError> {
        let mut map = BTreeMap::new();
        for (key, value) in attrs {
            if !is_valid_key(&key) {
                return Err(ApiError::InvalidRecord {
                    reason: format!(
                        "attribute key `{key}` must match ^[a-z][a-z0-9_]{{0,31}}$"
                    ),
                });
            }
            if let AttrValue::Text(text) = &value {
                if text.len() > MAX_TEXT_BYTES {
                    return Err(ApiError::InvalidRecord {
                        reason: format!(
                            "text value for key `{key}` exceeds {MAX_TEXT_BYTES} bytes"
                        ),
                    });
                }
            }
            if map.insert(key.clone(), value).is_some() {
                return Err(ApiError::InvalidRecord {
                    reason: format!("duplicate attribute key `{key}`"),
                });
            }
            if map.len() > MAX_ATTRS {
                return Err(ApiError::InvalidRecord {
                    reason: format!("record exceeds {MAX_ATTRS} attributes"),
                });
            }
        }
        Ok(Record { id, attrs: map })
    }

    /// The record identity.
    pub fn id(&self) -> &ContentId {
        &self.id
    }

    /// Attributes in ascending key order.
    pub fn attrs(&self) -> &BTreeMap<String, AttrValue> {
        &self.attrs
    }

    /// One attribute value by key.
    pub fn get(&self, key: &str) -> Option<&AttrValue> {
        self.attrs.get(key)
    }
}

fn is_valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}
