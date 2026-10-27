//! Anti-drift guard (plan `I1` 9 F3): the published document and the "0 TCP"
//! structural claim are part of the contract, so both are checked at test time.
//!
//! The document is compiled in with `include_str!`: deleting it or renaming it
//! fails the build, and editing either side of the contract alone fails here.
//! The wire examples are not prose - they are compared against the encoder's
//! actual output, so a serializer or envelope change cannot leave stale docs.

use daggerfall_game_runtime_api::protocol::{encode_error, encode_success, parse_request, methods};
use daggerfall_game_runtime_api::{ApiVersion, RpcError, API_LABEL, ERROR_CODES, MAX_FRAME};
use serde_json::json;

const DOC: &str =
    include_str!("../../../docs/sintergic/game-runtime-api/GAME-RUNTIME-API-v1.md");

const NUMERIC_CODES: [(&str, i64); 12] = [
    ("invalid_request", -32600),
    ("method_not_found", -32601),
    ("invalid_params", -32602),
    ("internal_error", -32603),
    ("unauthorized_peer", -32010),
    ("unsupported_api", -32011),
    ("illegal_transition", -32012),
    ("probe_unknown", -32013),
    ("invalid_probe_name", -32014),
    ("host_error", -32015),
    ("unknown_side", -32016),
    ("frame_too_large", -32017),
];

/// Every published method, error code, frame bound, version and required
/// section appears in the normative document.
#[test]
fn published_doc_matches_the_v1_surface() {
    assert_eq!(API_LABEL, "1.0");
    assert_eq!(ApiVersion::CURRENT.label(), API_LABEL);
    assert!(DOC.starts_with("---\n"), "the doc must open with frontmatter");
    assert!(DOC.contains("schema_version: 1.0"));
    assert!(DOC.contains("owner: main"));
    assert!(DOC.contains("medium: unix-domain-socket"));

    for method in methods::ALL {
        assert!(
            DOC.contains(&format!("`{method}`")),
            "method `{method}` is missing from GAME-RUNTIME-API-v1.md"
        );
    }

    for code in ERROR_CODES {
        assert!(
            DOC.contains(&format!("`{code}`")),
            "error code `{code}` is missing from GAME-RUNTIME-API-v1.md"
        );
    }

    assert!(
        DOC.contains(&MAX_FRAME.to_string()),
        "the document must pin the frame limit {MAX_FRAME}"
    );
    assert!(DOC.contains("1.0"), "the document must pin API 1.0");
    assert!(DOC.contains("idempotent"), "stop idempotence must be published");
    assert!(DOC.contains("No server push"), "v1 non-goals must be published");
    assert!(DOC.contains("No TCP"), "the transport exclusion must be published");
    assert!(DOC.contains("0600"), "the socket mode must be published");
    assert!(DOC.contains("SO_PEERCRED"), "the peer check must be published");
}

/// `ERROR_CODES` is exactly the closed set, in the published order, and every
/// `RpcError` variant maps to one of its members and to its published numeric
/// code.
#[test]
fn error_code_set_is_closed_and_matches_the_numeric_table() {
    assert_eq!(
        ERROR_CODES,
        &[
            "invalid_request",
            "method_not_found",
            "invalid_params",
            "internal_error",
            "unauthorized_peer",
            "unsupported_api",
            "illegal_transition",
            "probe_unknown",
            "invalid_probe_name",
            "host_error",
            "unknown_side",
            "frame_too_large",
        ],
        "ERROR_CODES must stay exactly the published closed set"
    );

    let variants = [
        RpcError::InvalidRequest {
            detail: String::new(),
        },
        RpcError::MethodNotFound {
            method: String::new(),
        },
        RpcError::InvalidParams {
            detail: String::new(),
        },
        RpcError::InternalError {
            detail: String::new(),
        },
        RpcError::UnauthorizedPeer {
            detail: String::new(),
        },
        RpcError::UnsupportedApi {
            declared: String::new(),
            supported: API_LABEL.to_string(),
        },
        RpcError::IllegalTransition {
            from: "stopped",
            command: "pause",
        },
        RpcError::ProbeUnknown {
            name: "ALL-999".to_string(),
        },
        RpcError::InvalidProbeName {
            raw: "../bin/sh".to_string(),
            reason: "test",
        },
        RpcError::HostError {
            detail: String::new(),
        },
        RpcError::UnknownSide {
            side: "alpha".to_string(),
        },
        RpcError::FrameTooLarge {
            len: MAX_FRAME + 1,
            max: MAX_FRAME,
        },
    ];

    for variant in &variants {
        let code = variant.code();
        assert!(
            ERROR_CODES.contains(&code),
            "`{code}` is not in ERROR_CODES"
        );
        assert!(
            variant.to_string().starts_with(code),
            "Display must start with the stable code: {variant}"
        );
        let expected = NUMERIC_CODES
            .iter()
            .find(|(candidate, _)| *candidate == code)
            .map(|(_, numeric)| *numeric)
            .unwrap_or_else(|| panic!("`{code}` has no numeric code"));
        assert_eq!(variant.numeric(), expected, "numeric code for `{code}`");
        assert!(
            DOC.contains(&format!("| `{code}` | {expected} |")),
            "the document's numeric table is stale for `{code}`"
        );
    }

    for (code, numeric) in NUMERIC_CODES {
        assert!(
            DOC.contains(&format!("| `{code}` | {numeric} |")),
            "row `{code}` / {numeric} missing from the document"
        );
    }
}

/// The three documented wire examples are byte-for-byte what this build emits,
/// and the documented request parses under the documented rules.
#[test]
fn published_wire_examples_are_what_the_encoder_emits() {
    let request = r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"status"}"#;
    assert!(
        DOC.contains(request),
        "the documented request line is missing from the doc"
    );
    let parsed = parse_request(request).expect("the documented request must parse");
    assert_eq!(parsed.method, methods::STATUS);
    assert_eq!(parsed.api, ApiVersion::CURRENT);
    assert!(parsed.params.is_empty());

    let success = encode_success(&json!(1), &json!({"lifecycle": "stopped"}));
    assert!(
        DOC.contains(&success),
        "documented success response is stale; the encoder emits:\n{success}"
    );

    let error = encode_error(
        Some(&json!(3)),
        &RpcError::IllegalTransition {
            from: "stopped",
            command: "pause",
        },
    );
    assert!(
        DOC.contains(&error),
        "documented error response is stale; the encoder emits:\n{error}"
    );
    let parsed_error: serde_json::Value = serde_json::from_str(&error).expect("error is JSON");
    assert_eq!(parsed_error["error"]["code"], json!(-32012));
    assert_eq!(
        parsed_error["error"]["data"]["code"],
        json!("illegal_transition")
    );

    let unauthorized = encode_error(
        None,
        &RpcError::UnauthorizedPeer {
            detail: "peer uid is not authorized (expected 1001)".to_string(),
        },
    );
    assert!(
        DOC.contains(&unauthorized),
        "documented peer refusal is stale; the encoder emits:\n{unauthorized}"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&unauthorized)
            .expect("refusal is JSON")["error"]["code"],
        json!(-32010)
    );
}

/// Structural evidence for "UDS only, never a network socket": no source file
/// names a TCP/UDP socket type or the `std::net` module.
#[test]
fn crate_sources_contain_no_tcp_or_udp_transport() {
    let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut scanned = 0usize;
    for entry in std::fs::read_dir(&source_dir).expect("src is readable") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        scanned += 1;
        let source = std::fs::read_to_string(&path).expect("source file is readable");
        for forbidden in ["TcpListener", "TcpStream", "UdpSocket", "std::net::"] {
            assert!(
                !source.contains(forbidden),
                "`{}` contains `{forbidden}`: the runtime contract must be UDS-only",
                path.display()
            );
        }
    }
    assert!(
        scanned >= 10,
        "expected the whole crate surface, scanned only {scanned} files"
    );
}
