//! The compact pairing payload in Rust — the byte-for-byte twin of
//! `web/pair.ts` (ADR 0028): one contract, two languages, so a terminal
//! peer and a browser peer exchange the same `uics1.` strings. This module
//! is codec only; the WebRTC stack stays with the consumer.

use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

pub const PREFIX: &str = "uics1.";

/// The compact payload: ice credentials, DTLS fingerprint, setup role and
/// the candidate [address, port] tuples — everything a minimal
/// data-channel-only SDP rebuilds from. The field order mirrors the TS
/// object literal: serde serializes declaration order and JSON.stringify
/// insertion order, and the encoded bytes must match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compact {
    pub u: String,
    pub p: String,
    pub f: String,
    pub s: String,
    pub c: Vec<(String, u16)>,
}

#[derive(Debug)]
pub enum PairError {
    /// The text is no payload at all.
    Payload(String),
    /// A local description misses a required line.
    Sdp(String),
}

impl fmt::Display for PairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PairError::Payload(message) => write!(f, "uic-sync pair: {message}"),
            PairError::Sdp(what) => {
                write!(f, "uic-sync pair: no {what} in the local description")
            }
        }
    }
}

impl std::error::Error for PairError {}

/// The role a compact payload plays — offers negotiate (`actpass`),
/// answers commit to a side; `None` for text that is no payload at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Offer,
    Answer,
}

pub fn payload_role(text: &str) -> Option<Role> {
    let compact = decode_payload(text).ok()?;
    match compact.s.as_str() {
        "actpass" => Some(Role::Offer),
        "active" | "passive" => Some(Role::Answer),
        _ => None,
    }
}

pub fn encode_payload(compact: &Compact) -> String {
    let json = serde_json::to_vec(compact).expect("a plain struct serializes");
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
}

pub fn decode_payload(text: &str) -> Result<Compact, PairError> {
    let trimmed = text.trim();
    let base64 = trimmed
        .strip_prefix(PREFIX)
        .ok_or_else(|| PairError::Payload(format!("payload does not start with {PREFIX:?}")))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(base64)
        .map_err(|err| PairError::Payload(format!("payload is not base64url: {err}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| PairError::Payload(format!("payload is not a compact offer: {err}")))
}

/// Reduces a local description to the compact payload — the same lines
/// pair.ts extracts: component-1 UDP candidates of the host, srflx and
/// relay kinds (deduplicated by address:port), the ice credentials, the
/// sha-256 fingerprint and the setup role.
pub fn parse_sdp(sdp: &str) -> Result<Compact, PairError> {
    let mut candidates: Vec<(String, u16)> = Vec::new();
    for line in sdp.lines() {
        let line = line.trim_end_matches('\r');
        let Some(rest) = line.strip_prefix("a=candidate:") else {
            continue;
        };
        let parts: Vec<&str> = rest.split_ascii_whitespace().collect();
        // <foundation> 1 udp <priority> <address> <port> typ <kind> …
        if parts.len() >= 8
            && parts[1] == "1"
            && parts[2].eq_ignore_ascii_case("udp")
            && parts[6] == "typ"
            && matches!(parts[7], "host" | "srflx" | "relay")
        {
            if let Ok(port) = parts[5].parse::<u16>() {
                let address = parts[4].to_string();
                if !candidates
                    .iter()
                    .any(|(known, at)| known == &address && *at == port)
                {
                    candidates.push((address, port));
                }
            }
        }
    }
    if candidates.is_empty() {
        return Err(PairError::Payload("no usable candidates gathered".into()));
    }
    Ok(Compact {
        u: required(sdp, "a=ice-ufrag:", "ice-ufrag")?,
        p: required(sdp, "a=ice-pwd:", "ice-pwd")?,
        f: required(sdp, "a=fingerprint:sha-256 ", "sha-256 fingerprint")?,
        s: required(sdp, "a=setup:", "setup role")?,
        c: candidates,
    })
}

fn required(sdp: &str, prefix: &str, what: &str) -> Result<String, PairError> {
    sdp.lines()
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix(prefix))
        .next()
        .map(str::to_string)
        .ok_or_else(|| PairError::Sdp(what.to_string()))
}

/// The minimal data-channel-only SDP a peer rebuilds from a compact
/// payload — byte-identical to pair.ts's buildSdp, with the setup role
/// substituted (the answer synthesis picks active/passive).
pub fn build_sdp(compact: &Compact, setup: &str) -> String {
    let mut lines: Vec<String> = vec![
        "v=0".into(),
        "o=- 0 0 IN IP4 127.0.0.1".into(),
        "s=-".into(),
        "t=0 0".into(),
        "a=group:BUNDLE 0".into(),
        "m=application 9 UDP/DTLS/SCTP webrtc-datachannel".into(),
        "c=IN IP4 0.0.0.0".into(),
    ];
    for (index, (address, port)) in compact.c.iter().enumerate() {
        lines.push(format!(
            "a=candidate:{} 1 udp {} {address} {port} typ host",
            index + 1,
            2113937151usize - index
        ));
    }
    lines.push(format!("a=ice-ufrag:{}", compact.u));
    lines.push(format!("a=ice-pwd:{}", compact.p));
    lines.push(format!("a=fingerprint:sha-256 {}", compact.f));
    lines.push(format!("a=setup:{setup}"));
    lines.push("a=mid:0".into());
    lines.push("a=sctp-port:5000".into());
    lines.push("a=max-message-size:262144".into());
    lines.push(String::new());
    lines.join("\r\n")
}

/// The payload carried by a link, pasted text or scanned code. The invite
/// is a single `#uics1.…` fragment, so this finds the `uics1.` prefix and
/// takes the base64url run that follows (any trailing text is cut); a bare
/// token passes through, and text with no payload returns trimmed for the
/// caller to reject.
pub fn link_payload(text: &str) -> String {
    let trimmed = text.trim();
    let Some(at) = trimmed.find(PREFIX) else {
        return trimmed.to_string();
    };
    let rest = &trimmed[at..];
    // base64url is `[A-Za-z0-9_-]`; the payload ends at the first byte
    // outside it (a quote, whitespace, or the string's end).
    let end = rest[PREFIX.len()..]
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .map_or(rest.len(), |offset| PREFIX.len() + offset);
    rest[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A genuine Chrome payload — one mDNS host candidate and one STUN
    /// srflx candidate — with the server-reflexive address rewritten to the
    /// RFC 5737 documentation range (`203.0.113.0/24`). Only that string
    /// changed; the byte layout is Chrome's own, which is the point: it
    /// pins that `JSON.stringify` and serde agree field for field.
    const BROWSER_VECTOR: &str = "uics1.eyJ1IjoiZVVxTCIsInAiOiJMTldRVGNrY1Z2RndRSko5VXZGUWtINkciLCJmIjoiMDc6ODc6MjQ6QUE6Qzc6Q0E6OEI6OEQ6QzU6OUM6RjY6NUU6ODI6QkY6QTk6NjM6QzE6QTc6NEU6MUU6NzA6NkQ6MTE6NDI6MEQ6RjM6MDM6OTI6NkM6N0M6NTk6QTUiLCJzIjoiYWN0cGFzcyIsImMiOltbIjIyZjkwYmZjLTBiNWItNDEyNS1iN2ExLTIzZDcxZDNiYzQ4MS5sb2NhbCIsNTM3NzVdLFsiMjAzLjAuMTEzLjE4NyIsNTM3NzVdXX0";

    fn vector_compact() -> Compact {
        Compact {
            u: "eUqL".into(),
            p: "LNWQTckcVvFwQJJ9UvFQkH6G".into(),
            f: "07:87:24:AA:C7:CA:8B:8D:C5:9C:F6:5E:82:BF:A9:63:C1:A7:4E:1E:70:6D:11:42:0D:F3:03:92:6C:7C:59:A5".into(),
            s: "actpass".into(),
            c: vec![
                ("22f90bfc-0b5b-4125-b7a1-23d71d3bc481.local".into(), 53775),
                ("203.0.113.187".into(), 53775),
            ],
        }
    }

    #[test]
    fn the_browser_vector_round_trips_byte_identically() {
        let decoded = decode_payload(BROWSER_VECTOR).unwrap();
        assert_eq!(decoded, vector_compact());
        // The Rust encoding must reproduce the browser's bytes exactly —
        // field order, compact JSON, unpadded base64url.
        assert_eq!(encode_payload(&decoded), BROWSER_VECTOR);
        assert_eq!(payload_role(BROWSER_VECTOR), Some(Role::Offer));
    }

    #[test]
    fn build_sdp_matches_the_ts_template() {
        let sdp = build_sdp(&vector_compact(), "active");
        let expected = "v=0\r\n\
            o=- 0 0 IN IP4 127.0.0.1\r\n\
            s=-\r\n\
            t=0 0\r\n\
            a=group:BUNDLE 0\r\n\
            m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n\
            c=IN IP4 0.0.0.0\r\n\
            a=candidate:1 1 udp 2113937151 22f90bfc-0b5b-4125-b7a1-23d71d3bc481.local 53775 typ host\r\n\
            a=candidate:2 1 udp 2113937150 203.0.113.187 53775 typ host\r\n\
            a=ice-ufrag:eUqL\r\n\
            a=ice-pwd:LNWQTckcVvFwQJJ9UvFQkH6G\r\n\
            a=fingerprint:sha-256 07:87:24:AA:C7:CA:8B:8D:C5:9C:F6:5E:82:BF:A9:63:C1:A7:4E:1E:70:6D:11:42:0D:F3:03:92:6C:7C:59:A5\r\n\
            a=setup:active\r\n\
            a=mid:0\r\n\
            a=sctp-port:5000\r\n\
            a=max-message-size:262144\r\n";
        assert_eq!(sdp, expected);
    }

    #[test]
    fn parse_sdp_reduces_its_own_build() {
        let compact = vector_compact();
        let parsed = parse_sdp(&build_sdp(&compact, "actpass")).unwrap();
        // The round trip normalizes every candidate to `host`, exactly the
        // TS behavior — everything else survives verbatim.
        assert_eq!(parsed.u, compact.u);
        assert_eq!(parsed.p, compact.p);
        assert_eq!(parsed.f, compact.f);
        assert_eq!(parsed.s, "actpass");
        assert_eq!(parsed.c, compact.c);
    }

    #[test]
    fn parse_sdp_skips_foreign_candidate_shapes() {
        let sdp = "a=candidate:1 2 udp 1 10.0.0.1 1000 typ host\r\n\
            a=candidate:2 1 tcp 1 10.0.0.2 1001 typ host\r\n\
            a=candidate:3 1 UDP 1 10.0.0.3 1002 typ srflx raddr 0.0.0.0 rport 0\r\n\
            a=candidate:4 1 udp 1 10.0.0.3 1002 typ host\r\n\
            a=ice-ufrag:u\r\na=ice-pwd:p\r\na=fingerprint:sha-256 AA\r\na=setup:actpass\r\n";
        let parsed = parse_sdp(sdp).unwrap();
        // Component 2 and tcp drop; the srflx keeps its trailing fields;
        // the duplicate address:port dedupes.
        assert_eq!(parsed.c, vec![("10.0.0.3".to_string(), 1002)]);
    }

    #[test]
    fn links_and_tokens_reduce_to_the_payload() {
        // A full invite link: the single `#uics1.…` fragment, no parameters.
        assert_eq!(link_payload("https://host/p2p/#uics1.abc"), "uics1.abc");
        // A bare token pasted into the box, with stray whitespace.
        assert_eq!(link_payload("  uics1.bare  "), "uics1.bare");
        // Trailing text after the token is cut at the first non-base64url byte.
        assert_eq!(link_payload("uics1.abc and some noise"), "uics1.abc");
        // Text with no payload passes through trimmed (the caller rejects it).
        assert_eq!(link_payload("nonsense"), "nonsense");
    }

    #[test]
    fn wrong_prefixes_and_roles_report_plainly() {
        assert!(decode_payload("nonsense").is_err());
        assert_eq!(payload_role("nonsense"), None);
        let answer = encode_payload(&Compact {
            s: "active".into(),
            ..vector_compact()
        });
        assert_eq!(payload_role(&answer), Some(Role::Answer));
    }
}
