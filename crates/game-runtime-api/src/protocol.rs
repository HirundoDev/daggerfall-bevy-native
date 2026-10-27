//! The newline-delimited JSON-RPC 2.0 wire contract.
//!
//! One request object per line, UTF-8, at most [`MAX_FRAME`] bytes before the
//! newline. The envelope is JSON-RPC 2.0 by *form* (OD-6 = (a) partial), with
//! two deliberate additions: a required `api` field for version negotiation and
//! a **strict** top-level field set. Strictness is the security property here:
//! a client cannot smuggle `uid`, `peer`, `path` or `command` into the envelope,
//! because any field outside the five known ones is `invalid_request`.
//!
//! Duplicate object names are refused: RFC 8259 §4 leaves the effective name
//! of a repeated key undefined, and `serde_json` resolves the ambiguity
//! silently (last one wins). On a security boundary "which value did the server
//! see" must not depend on that, so [`parse_request`] scans the raw line first
//! and rejects any object that names the same key twice, at any depth, with
//! `invalid_request`.
//!
//! Parsing is pure and total: [`parse_request`] never panics, never allocates
//! from an unbounded input (the caller enforces [`MAX_FRAME`]) and returns the
//! id it managed to read, so an error can echo it.

use serde_json::{json, Map, Value};

use crate::api::{ApiVersion, API_LABEL};
use crate::error::RpcError;

/// Largest accepted frame, in bytes, excluding the newline.
pub const MAX_FRAME: usize = 65_536;

/// The only accepted value of the `jsonrpc` field.
pub const JSONRPC_VERSION: &str = "2.0";

/// The five top-level fields a request may carry; nothing else is accepted.
const REQUEST_FIELDS: [&str; 5] = ["jsonrpc", "id", "api", "method", "params"];

/// The v1 method table.
pub mod methods {
    /// `status` - read-only lifecycle and loaded-side projection.
    pub const STATUS: &str = "status";
    /// `metrics` - server counters plus host counters.
    pub const METRICS: &str = "metrics";
    /// `start` - `stopped` -> `running`.
    pub const START: &str = "start";
    /// `stop` - any state -> `stopped` (idempotent).
    pub const STOP: &str = "stop";
    /// `pause` - `running` -> `paused`.
    pub const PAUSE: &str = "pause";
    /// `resume` - `paused` -> `running`.
    pub const RESUME: &str = "resume";
    /// `load-side` - load a side while running.
    pub const LOAD_SIDE: &str = "load-side";
    /// `unload-side` - unload a loaded side while running.
    pub const UNLOAD_SIDE: &str = "unload-side";
    /// `probes.list` - the registered catalogue.
    pub const PROBES_LIST: &str = "probes.list";
    /// `probes.run` - run one probe by name.
    pub const PROBES_RUN: &str = "probes.run";
    /// `events` - pull the bounded event log after a cursor.
    pub const EVENTS: &str = "events";
    /// `screenshot` - host frame metadata (or `null`).
    pub const SCREENSHOT: &str = "screenshot";

    /// Every method of the v1 table, in documentation order.
    pub const ALL: &[&str] = &[
        STATUS,
        METRICS,
        START,
        STOP,
        PAUSE,
        RESUME,
        LOAD_SIDE,
        UNLOAD_SIDE,
        PROBES_LIST,
        PROBES_RUN,
        EVENTS,
        SCREENSHOT,
    ];
}

/// A parsed, version-negotiated request ready for dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The JSON-RPC id, already validated as a number or a string.
    pub id: Value,
    /// The negotiated API version.
    pub api: ApiVersion,
    /// The method name, exactly as sent.
    pub method: String,
    /// Method parameters; `{}` when omitted.
    pub params: Map<String, Value>,
}

/// A request that failed before dispatch, with whatever id was readable.
///
/// The id is echoed into the wire error when it was well-formed, so a client
/// can correlate the refusal. It stays `None` for envelope failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedRequest {
    /// The id to echo, when the envelope was good enough to read one.
    pub id: Option<Value>,
    /// The stable refusal.
    pub error: RpcError,
}

/// Largest JSON nesting depth the duplicate-key scan walks.
///
/// The bound equals `serde_json`'s own recursion limit, so a document deeper
/// than this is refused by the parser anyway; the scan itself therefore can
/// never overflow the stack.
const MAX_SCAN_DEPTH: usize = 128;

/// The outcome of scanning one raw line for repeated object names.
enum Scan {
    /// Every object in the line was walked without a repeated key.
    Clean,
    /// One object named the same key twice; the (decoded) key is reported.
    Duplicate(String),
    /// The line is not JSON this scanner understands. `serde_json` decides.
    Unparsable,
}

/// Reports the first object key that appears twice in `line`, at any depth.
///
/// RFC 8259 §4 says the effective name of a duplicated key is undefined, and
/// `serde_json` picks the last one without saying so. A control peer must not
/// be able to disagree with the server about which value was read, so the raw
/// line is scanned before it is deserialised.
///
/// The scan is string- and escape-aware: a `"` inside a string does not open an
/// object key, `\"` does not close one, `\\` is a literal backslash, and
/// `\uXXXX` (including surrogate pairs) is decoded before comparison, so
/// `"name"` and `"\u006eame"` are the same key. Keys are tracked per object, so
/// the same name is legal in two sibling objects.
fn duplicate_object_key(line: &str) -> Option<String> {
    let mut scanner = KeyScanner {
        bytes: line.as_bytes(),
        at: 0,
    };
    match scanner.value(0) {
        Scan::Duplicate(key) => Some(key),
        Scan::Clean | Scan::Unparsable => None,
    }
}

/// A minimal JSON walker whose only job is duplicate-key detection.
///
/// It accepts every document `serde_json` accepts (and may accept a bit more,
/// in which case `serde_json` produces the authoritative error).
struct KeyScanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl KeyScanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Scan {
        if depth > MAX_SCAN_DEPTH {
            return Scan::Unparsable;
        }
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => match self.string() {
                Some(_) => Scan::Clean,
                None => Scan::Unparsable,
            },
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Scan::Unparsable,
        }
    }

    fn object(&mut self, depth: usize) -> Scan {
        self.at += 1; // '{'
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Scan::Clean;
        }
        let mut keys: Vec<String> = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Scan::Unparsable;
            }
            let key = match self.string() {
                Some(key) => key,
                None => return Scan::Unparsable,
            };
            if keys.contains(&key) {
                return Scan::Duplicate(key);
            }
            keys.push(key);
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Scan::Unparsable;
            }
            self.at += 1;
            match self.value(depth) {
                Scan::Clean => {}
                other => return other,
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Scan::Clean;
                }
                _ => return Scan::Unparsable,
            }
        }
    }

    fn array(&mut self, depth: usize) -> Scan {
        self.at += 1; // '['
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Scan::Clean;
        }
        loop {
            match self.value(depth) {
                Scan::Clean => {}
                other => return other,
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Scan::Clean;
                }
                _ => return Scan::Unparsable,
            }
        }
    }

    /// Reads one string token starting at `"`, decoding escapes so two
    /// spellings of the same key compare equal.
    fn string(&mut self) -> Option<String> {
        self.at += 1; // '"'
        let mut decoded: Vec<u8> = Vec::new();
        loop {
            let byte = self.peek()?;
            self.at += 1;
            match byte {
                b'"' => return String::from_utf8(decoded).ok(),
                b'\\' => {
                    let escape = self.peek()?;
                    self.at += 1;
                    match escape {
                        b'"' => decoded.push(b'"'),
                        b'\\' => decoded.push(b'\\'),
                        b'/' => decoded.push(b'/'),
                        b'b' => decoded.push(0x08),
                        b'f' => decoded.push(0x0c),
                        b'n' => decoded.push(b'\n'),
                        b'r' => decoded.push(b'\r'),
                        b't' => decoded.push(b'\t'),
                        b'u' => {
                            let unit = self.hex4()?;
                            let scalar = match unit {
                                0xD800..=0xDBFF => {
                                    if self.peek()? != b'\\' {
                                        return None;
                                    }
                                    self.at += 1;
                                    if self.peek()? != b'u' {
                                        return None;
                                    }
                                    self.at += 1;
                                    let low = self.hex4()?;
                                    if !(0xDC00..=0xDFFF).contains(&low) {
                                        return None;
                                    }
                                    let high = u32::from(unit - 0xD800);
                                    let low = u32::from(low - 0xDC00);
                                    0x1_0000 + (high << 10) + low
                                }
                                0xDC00..=0xDFFF => return None,
                                _ => u32::from(unit),
                            };
                            let ch = char::from_u32(scalar)?;
                            let mut buffer = [0u8; 4];
                            decoded.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                        }
                        _ => return None,
                    }
                }
                0x00..=0x1F => return None,
                other => decoded.push(other),
            }
        }
    }

    fn literal(&mut self, word: &[u8]) -> Scan {
        if self.bytes[self.at..].starts_with(word) {
            self.at += word.len();
            Scan::Clean
        } else {
            Scan::Unparsable
        }
    }

    fn number(&mut self) -> Scan {
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let integer = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if self.at == integer {
            return Scan::Unparsable;
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            let fraction = self.at;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            if self.at == fraction {
                return Scan::Unparsable;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            let exponent = self.at;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            if self.at == exponent {
                return Scan::Unparsable;
            }
        }
        Scan::Clean
    }

    fn hex4(&mut self) -> Option<u16> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            let digit = self.peek()?;
            self.at += 1;
            let nibble = match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => return None,
            };
            value = (value << 4) | u16::from(nibble);
        }
        Some(value)
    }
}

/// Parses one line into a [`Request`].
///
/// Order is fixed and security-relevant: envelope shape, `jsonrpc`, `id`,
/// **`api`**, then `method` and `params`. Version negotiation therefore happens
/// before any method is looked up, and an old client always receives
/// [`RpcError::UnsupportedApi`] rather than different behaviour.
///
/// A line whose JSON names the same key twice in one object, at any depth, is
/// [`RpcError::InvalidRequest`] before any field is read: the value a client
/// meant and the value the server reads must not be able to differ.
pub fn parse_request(line: &str) -> Result<Request, RejectedRequest> {
    let reject = |error: RpcError| RejectedRequest { id: None, error };
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(reject(RpcError::InvalidRequest {
            detail: "empty frame".to_string(),
        }));
    }
    if let Some(key) = duplicate_object_key(trimmed) {
        return Err(reject(RpcError::InvalidRequest {
            detail: format!(
                "duplicate object key `{key}`: RFC 8259 §4 leaves the effective name undefined"
            ),
        }));
    }
    let value: Value = serde_json::from_str(trimmed).map_err(|error| {
        reject(RpcError::InvalidRequest {
            detail: format!("malformed JSON: {error}"),
        })
    })?;
    let object = match value {
        Value::Object(object) => object,
        _ => {
            return Err(reject(RpcError::InvalidRequest {
                detail: "request must be a JSON object".to_string(),
            }))
        }
    };

    if let Some(unknown) = object
        .keys()
        .find(|key| !REQUEST_FIELDS.contains(&key.as_str()))
    {
        return Err(reject(RpcError::InvalidRequest {
            detail: format!("unknown top-level field `{unknown}`"),
        }));
    }

    match object.get("jsonrpc") {
        Some(Value::String(version)) if version == JSONRPC_VERSION => {}
        Some(_) => {
            return Err(reject(RpcError::InvalidRequest {
                detail: "`jsonrpc` must be \"2.0\"".to_string(),
            }))
        }
        None => {
            return Err(reject(RpcError::InvalidRequest {
                detail: "missing `jsonrpc` field".to_string(),
            }))
        }
    }

    let id = match object.get("id") {
        Some(id @ Value::String(_)) | Some(id @ Value::Number(_)) => id.clone(),
        Some(_) | None => {
            return Err(reject(RpcError::InvalidRequest {
                detail: "`id` is required and must be a number or a string".to_string(),
            }))
        }
    };

    let api = match object.get("api") {
        Some(Value::String(raw)) => {
            let declared = ApiVersion::parse(raw).map_err(|error| RejectedRequest {
                id: Some(id.clone()),
                error,
            })?;
            if !ApiVersion::CURRENT.accepts(declared) {
                return Err(RejectedRequest {
                    id: Some(id.clone()),
                    error: RpcError::UnsupportedApi {
                        declared: raw.clone(),
                        supported: API_LABEL.to_string(),
                    },
                });
            }
            declared
        }
        Some(other) => {
            return Err(RejectedRequest {
                id: Some(id.clone()),
                error: RpcError::UnsupportedApi {
                    declared: other.to_string(),
                    supported: API_LABEL.to_string(),
                },
            })
        }
        None => {
            return Err(RejectedRequest {
                id: Some(id.clone()),
                error: RpcError::UnsupportedApi {
                    declared: String::new(),
                    supported: API_LABEL.to_string(),
                },
            })
        }
    };

    let method = match object.get("method") {
        Some(Value::String(method)) if !method.is_empty() => method.clone(),
        Some(Value::String(_)) | None => {
            return Err(RejectedRequest {
                id: Some(id),
                error: RpcError::InvalidRequest {
                    detail: "missing `method` field".to_string(),
                },
            })
        }
        Some(_) => {
            return Err(RejectedRequest {
                id: Some(id),
                error: RpcError::InvalidRequest {
                    detail: "`method` must be a string".to_string(),
                },
            })
        }
    };

    let params = match object.get("params") {
        None => Map::new(),
        Some(Value::Object(params)) => params.clone(),
        Some(_) => {
            return Err(RejectedRequest {
                id: Some(id),
                error: RpcError::InvalidParams {
                    detail: "`params` must be an object".to_string(),
                },
            })
        }
    };

    Ok(Request {
        id,
        api,
        method,
        params,
    })
}

/// Encodes a success response as one line (without the trailing newline).
pub fn encode_success(id: &Value, result: &Value) -> String {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result,
    })
    .to_string()
}

/// Encodes an error response as one line (without the trailing newline).
///
/// `id` is `None` when the envelope was unreadable; JSON-RPC requires a `null`
/// id in that case and the server sends exactly that.
pub fn encode_error(id: Option<&Value>, error: &RpcError) -> String {
    let id = id.cloned().unwrap_or(Value::Null);
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": error.numeric(),
            "message": error.to_string(),
            "data": error.data(),
        },
    })
    .to_string()
}
