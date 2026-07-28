//! The compact pairing payload in Rust — the byte-for-byte twin of
//! `web/pair.ts` (ADR 0028): one contract, two languages, so a terminal
//! peer and a browser peer exchange the same pairing codes. This module
//! is codec only; the WebRTC stack stays with the consumer.

use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

// The payload wears no prefix: in a link the fragment position is the
// discriminator, and the binary layout below self-validates (length
// prefixes must fit, the setup byte and address tags are constrained, the
// bytes must be fully consumed) — structural checks replace a marker.

/// The control-frame marker on a state wire (`web/session.ts`'s twin):
/// protocol messages ride the live data channel as `uicc1.` + JSON, and
/// both ends filter them off before state application — a session hands
/// over to another tab by re-signaling a fresh pairing through its own
/// wire (ADR 0032).
pub const CTRL_PREFIX: &str = "uicc1.";

/// A control-plane message: `repair` carries the new tab's fresh payload,
/// `repair-answer` this side's fresh payload back, `repair-done` the cue to
/// drop the old wire. Unknown kinds are ignored, forward-compatible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ctrl {
    pub t: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

/// Encodes a control frame for the wire.
pub fn encode_ctrl(ctrl: &Ctrl) -> String {
    format!(
        "{CTRL_PREFIX}{}",
        serde_json::to_string(ctrl).expect("a plain struct serializes")
    )
}

/// Decodes a control frame; `None` for state text or an unparseable frame.
pub fn decode_ctrl(text: &str) -> Option<Ctrl> {
    let json = text.strip_prefix(CTRL_PREFIX)?;
    serde_json::from_str(json).ok()
}

/// The compact payload: ice credentials (`u`/`p`), the DTLS fingerprint
/// (`f`), the setup role (`s`) and the candidate [address, port] tuples
/// (`c`) — everything a minimal data-channel-only SDP rebuilds from. The
/// wire form is the declared binary layout below (`LAYOUT`), byte-pinned
/// across the twins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compact {
    pub u: String,
    pub p: String,
    pub f: String,
    pub s: Setup,
    pub c: Vec<(String, u16)>,
}

/// The DTLS setup role a payload carries: `ActPass` negotiates (an offer),
/// `Active`/`Passive` commit to a side (an answer). The type makes an
/// invalid role unrepresentable, which keeps `encode_payload` infallible;
/// the TS twin carries the same three spellings as plain strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    ActPass,
    Active,
    Passive,
}

impl Setup {
    /// The SDP spelling — also the layout's enum vocabulary, in wire order.
    pub fn as_str(self) -> &'static str {
        match self {
            Setup::ActPass => "actpass",
            Setup::Active => "active",
            Setup::Passive => "passive",
        }
    }

    fn from_str(text: &str) -> Option<Setup> {
        match text {
            "actpass" => Some(Setup::ActPass),
            "active" => Some(Setup::Active),
            "passive" => Some(Setup::Passive),
            _ => None,
        }
    }
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

/// Classifies a payload text; `None` when it decodes to no payload at all.
pub fn payload_role(text: &str) -> Option<Role> {
    let compact = decode_payload(text).ok()?;
    match compact.s {
        Setup::ActPass => Some(Role::Offer),
        Setup::Active | Setup::Passive => Some(Role::Answer),
    }
}

/// The wire layout, declaratively — the single place the payload's shape
/// lives. The TS twin mirrors this table verbatim (`web/pair.ts`, `LAYOUT`):
/// field order, kinds and enum values must match byte for byte, and the
/// golden vector pins them.
const LAYOUT: &[(&str, Kind)] = &[
    ("u", Kind::Str8),
    ("p", Kind::Str8),
    ("f", Kind::Hex32),
    ("s", Kind::Enum(&["actpass", "active", "passive"])),
    ("c", Kind::Addrs8),
];

/// The kinds the layout speaks: `Str8` is u8 length + ASCII, `Hex32` 32 raw
/// bytes shown as colon-hex, `Enum` a u8 index into its values, `Addrs8` a
/// u8 count of tagged address + big-endian u16 port entries (IPv4 = 4
/// bytes, IPv6 = 16, an mDNS `<uuid>.local` = its 16 uuid bytes, anything
/// else length-prefixed ASCII).
enum Kind {
    Str8,
    Hex32,
    Enum(&'static [&'static str]),
    Addrs8,
}

const ADDR_V4: u8 = 0;
const ADDR_V6: u8 = 1;
const ADDR_MDNS: u8 = 2;
const ADDR_NAME: u8 = 3;

/// A field's decoded value during a layout walk.
enum Decoded {
    Text(String),
    Addrs(Vec<(String, u16)>),
}

/// Binds a layout field name to its `Compact` member (Rust has no
/// reflection; the TS twin indexes by name directly).
fn field<'a>(compact: &'a Compact, name: &str) -> &'a str {
    match name {
        "u" => &compact.u,
        "p" => &compact.p,
        "f" => &compact.f,
        "s" => compact.s.as_str(),
        other => unreachable!("no text field {other} in the layout"),
    }
}

/// Packs a compact into the declared binary layout, bare base64url —
/// infallible: `Compact` holds no state the layout cannot spell.
pub fn encode_payload(compact: &Compact) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(96);
    for (name, kind) in LAYOUT {
        match kind {
            Kind::Str8 => push_short(&mut out, field(compact, name)),
            Kind::Hex32 => out.extend_from_slice(&fingerprint_bytes(field(compact, name))),
            Kind::Enum(values) => {
                let value = field(compact, name);
                let index = values
                    .iter()
                    .position(|known| *known == value)
                    .expect("every Setup spelling sits in the layout vocabulary");
                out.push(index as u8);
            }
            Kind::Addrs8 => {
                out.push(compact.c.len() as u8);
                for (address, port) in &compact.c {
                    push_addr(&mut out, address);
                    out.extend_from_slice(&port.to_be_bytes());
                }
            }
        }
    }
    URL_SAFE_NO_PAD.encode(out)
}

/// Unpacks a payload text along the declared layout; the structural checks
/// (fitting length prefixes, constrained enum and tag bytes, full
/// consumption, at least one candidate) are what classify text as a
/// payload — there is no prefix marker.
pub fn decode_payload(text: &str) -> Result<Compact, PairError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text.trim())
        .map_err(|err| PairError::Payload(format!("payload is not base64url: {err}")))?;
    let mut at = Cursor {
        bytes: &bytes,
        at: 0,
    };
    let mut fields = Vec::with_capacity(LAYOUT.len());
    for (name, kind) in LAYOUT {
        fields.push(match kind {
            Kind::Str8 => Decoded::Text(at.short()?),
            Kind::Hex32 => Decoded::Text(fingerprint_hex(at.take(32)?)),
            Kind::Enum(values) => {
                let index = at.byte()? as usize;
                let value = values
                    .get(index)
                    .ok_or_else(|| PairError::Payload(format!("unknown {name} byte {index}")))?;
                Decoded::Text((*value).to_string())
            }
            Kind::Addrs8 => {
                let count = at.byte()?;
                let mut addrs = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let address = take_addr(&mut at)?;
                    let port = u16::from_be_bytes(at.take(2)?.try_into().expect("two bytes"));
                    addrs.push((address, port));
                }
                Decoded::Addrs(addrs)
            }
        });
    }
    // Structural validation: every byte consumed, at least one candidate.
    if at.at != bytes.len() {
        return Err(PairError::Payload("payload has trailing bytes".into()));
    }
    let mut fields = fields.into_iter();
    let mut text = || match fields.next() {
        Some(Decoded::Text(value)) => value,
        _ => unreachable!("the layout walk yields texts in order"),
    };
    let compact = Compact {
        u: text(),
        p: text(),
        f: text(),
        // The enum walk validated the byte against the vocabulary already.
        s: Setup::from_str(&text()).expect("the layout vocabulary maps to Setup"),
        c: match fields.next() {
            Some(Decoded::Addrs(addrs)) if !addrs.is_empty() => addrs,
            Some(Decoded::Addrs(_)) => {
                return Err(PairError::Payload("payload carries no candidates".into()));
            }
            _ => unreachable!("the layout walk ends on the addresses"),
        },
    };
    Ok(compact)
}

/// One tagged address entry.
fn push_addr(out: &mut Vec<u8>, address: &str) {
    if let Some(uuid) = mdns_uuid_bytes(address) {
        out.push(ADDR_MDNS);
        out.extend_from_slice(&uuid);
    } else if let Ok(v4) = address.parse::<std::net::Ipv4Addr>() {
        out.push(ADDR_V4);
        out.extend_from_slice(&v4.octets());
    } else if let Ok(v6) = address.parse::<std::net::Ipv6Addr>() {
        out.push(ADDR_V6);
        out.extend_from_slice(&v6.octets());
    } else {
        out.push(ADDR_NAME);
        push_short(out, address);
    }
}

fn take_addr(at: &mut Cursor<'_>) -> Result<String, PairError> {
    Ok(match at.byte()? {
        ADDR_V4 => {
            let octets: [u8; 4] = at.take(4)?.try_into().expect("four bytes");
            std::net::Ipv4Addr::from(octets).to_string()
        }
        ADDR_V6 => {
            let octets: [u8; 16] = at.take(16)?.try_into().expect("sixteen bytes");
            std::net::Ipv6Addr::from(octets).to_string()
        }
        ADDR_MDNS => mdns_uuid_string(at.take(16)?),
        ADDR_NAME => at.short()?,
        other => {
            return Err(PairError::Payload(format!("unknown address tag {other}")));
        }
    })
}

/// A length-prefixed ASCII field (ice credentials, fallback hostnames).
fn push_short(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
}

/// `AA:BB:…` (32 pairs) into raw bytes; the SDP always spells sha-256
/// fingerprints this way.
fn fingerprint_bytes(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (slot, pair) in out.iter_mut().zip(hex.split(':')) {
        *slot = u8::from_str_radix(pair, 16).unwrap_or(0);
    }
    out
}

fn fingerprint_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The 16 uuid bytes of an mDNS `<uuid>.local` candidate, or `None` when
/// the address is no such name.
fn mdns_uuid_bytes(address: &str) -> Option<[u8; 16]> {
    let uuid = address.strip_suffix(".local")?;
    let hex: String = uuid.chars().filter(|c| *c != '-').collect();
    if uuid.len() != 36 || hex.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (slot, pair) in out.iter_mut().zip(hex.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *slot = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

fn mdns_uuid_string(bytes: &[u8]) -> String {
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}.local",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A checked reader over the payload bytes: truncation reports plainly.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, PairError> {
        let slice = self.take(1)?;
        Ok(slice[0])
    }

    fn take(&mut self, len: usize) -> Result<&[u8], PairError> {
        let end = self.at + len;
        if end > self.bytes.len() {
            return Err(PairError::Payload("payload is truncated".into()));
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn short(&mut self) -> Result<String, PairError> {
        let len = self.byte()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| PairError::Payload("payload field is not utf-8".into()))
    }
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
    let setup = required(sdp, "a=setup:", "setup role")?;
    Ok(Compact {
        u: required(sdp, "a=ice-ufrag:", "ice-ufrag")?,
        p: required(sdp, "a=ice-pwd:", "ice-pwd")?,
        f: required(sdp, "a=fingerprint:sha-256 ", "sha-256 fingerprint")?,
        s: Setup::from_str(&setup)
            .ok_or_else(|| PairError::Payload(format!("unknown setup role {setup:?}")))?,
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
pub fn build_sdp(compact: &Compact, setup: Setup) -> String {
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
    lines.push(format!("a=setup:{}", setup.as_str()));
    lines.push("a=mid:0".into());
    lines.push("a=sctp-port:5000".into());
    lines.push("a=max-message-size:262144".into());
    lines.push(String::new());
    lines.join("\r\n")
}

/// Builds an invite link the pairing page opens: the payload as a single
/// URL-safe fragment (`#<payload>`), so a chat app linkifies the whole URL.
/// A link answering an opened invite appends the reply digest (`.{digest}`,
/// still one token — parsers cut before the dot), so a browser opening it
/// routes the reply to the exact tab that invited. TS twin: `web/pair.ts`
/// `inviteLink`.
pub fn invite_link(page: &str, payload: &str, reply_to: Option<&str>) -> String {
    match reply_to {
        Some(digest) => format!("{page}#{payload}.{digest}"),
        None => format!("{page}#{payload}"),
    }
}

/// The reply-routing digest (fnv1a-32, 8 hex chars) of an invite payload. A
/// return link answering an invite carries `.{digest}` after its own payload
/// (`#<payload>.<digest>` — still one URL-safe token; `link_payload` cuts
/// before the dot), so the same-browser handover can route the reply to the
/// exact tab that invited. A routing hint only — the payload's own
/// credential guards stay the security; TS twin: `web/pair.ts` `replyDigest`.
pub fn reply_digest(payload: &str) -> String {
    let mut hash: u32 = 0x811c9dc5;
    for byte in payload.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// The payload carried by a link, pasted text or scanned code. The invite
/// is the whole fragment, so the payload sits after `#` when the text is a
/// link and at the front when it is a bare code — the base64url run from
/// there (any trailing text is cut, a reply link's `.{digest}` suffix
/// included); text with no payload returns trimmed for the caller's decode
/// to reject.
pub fn link_payload(text: &str) -> String {
    let trimmed = text.trim();
    let rest = match trimmed.find('#') {
        Some(at) => &trimmed[at + 1..],
        None => trimmed,
    };
    // base64url is `[A-Za-z0-9_-]`; the payload ends at the first byte
    // outside it (the digest's dot, whitespace, or the string's end).
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    if end == 0 {
        return trimmed.to_string();
    }
    rest[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Chrome-shaped payload — one mDNS host candidate and one STUN
    /// srflx candidate, the server-reflexive address in the RFC 5737
    /// documentation range — packed in the declared binary layout, no
    /// prefix. The constant pins the bytes across the twins: `web/pair.ts`
    /// must produce these exact characters for the same fields.
    const BROWSER_VECTOR: &str = "BGVVcUwYTE5XUVRja2NWdkZ3UUpKOVV2RlFrSDZHB4ckqsfKi43FnPZegr-pY8GnTh5wbRFCDfMDkmx8WaUAAgIi-Qv8C1tBJbehI9cdO8SB0g8AywBxu9IP";

    fn vector_compact() -> Compact {
        Compact {
            u: "eUqL".into(),
            p: "LNWQTckcVvFwQJJ9UvFQkH6G".into(),
            f: "07:87:24:AA:C7:CA:8B:8D:C5:9C:F6:5E:82:BF:A9:63:C1:A7:4E:1E:70:6D:11:42:0D:F3:03:92:6C:7C:59:A5".into(),
            s: Setup::ActPass,
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
        // The Rust encoding must reproduce the pinned bytes exactly — the
        // binary layout, unpadded base64url.
        assert_eq!(encode_payload(&decoded), BROWSER_VECTOR);
        assert_eq!(payload_role(BROWSER_VECTOR), Some(Role::Offer));
    }

    #[test]
    fn every_address_shape_round_trips() {
        // IPv6 canonicalizes (RFC 5952, `::1`), plain hostnames ride the
        // length-prefixed fallback, and non-canonical input comes back
        // canonical — the same address either way.
        let compact = Compact {
            u: "u16u16u16u16u16u".into(),
            p: "p32p32p32p32p32p32p32p32".into(),
            f: vector_compact().f,
            s: Setup::Active,
            c: vec![
                ("::1".into(), 9),
                ("somehost".into(), 80),
                ("192.168.1.7".into(), 65535),
            ],
        };
        let decoded = decode_payload(&encode_payload(&compact)).unwrap();
        assert_eq!(decoded, compact);

        let sprawling = Compact {
            c: vec![("0:0:0:0:0:0:0:1".into(), 9)],
            ..compact.clone()
        };
        let canonical = decode_payload(&encode_payload(&sprawling)).unwrap();
        assert_eq!(canonical.c, vec![("::1".to_string(), 9)]);
    }

    #[test]
    fn truncated_and_alien_payloads_report_plainly() {
        // Truncation, trailing bytes, plain words and an empty candidate
        // list all fail the structural validation.
        assert!(decode_payload("AAAA").is_err());
        assert!(decode_payload("nonsense").is_err());
        assert!(decode_payload(&format!("{BROWSER_VECTOR}AAAA")).is_err());
        let hollow = encode_payload(&Compact {
            c: Vec::new(),
            ..vector_compact()
        });
        assert!(decode_payload(&hollow).is_err());
    }

    #[test]
    fn build_sdp_matches_the_ts_template() {
        let sdp = build_sdp(&vector_compact(), Setup::Active);
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
        let parsed = parse_sdp(&build_sdp(&compact, Setup::ActPass)).unwrap();
        // The round trip normalizes every candidate to `host`, exactly the
        // TS behavior — everything else survives verbatim.
        assert_eq!(parsed.u, compact.u);
        assert_eq!(parsed.p, compact.p);
        assert_eq!(parsed.f, compact.f);
        assert_eq!(parsed.s, Setup::ActPass);
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
    fn links_and_codes_reduce_to_the_payload() {
        // A full invite link: the whole fragment is the payload.
        assert_eq!(link_payload("https://host/p2p/#abc123"), "abc123");
        // A reply link's `.{digest}` suffix is cut with the rest.
        assert_eq!(link_payload("https://host/p2p/#abc123.1a2b3c4d"), "abc123");
        // A bare code pasted into the box, with stray whitespace.
        assert_eq!(link_payload("  bareCode42  "), "bareCode42");
        // Trailing text after a bare code is cut at the first
        // non-base64url byte.
        assert_eq!(link_payload("abc123 and some noise"), "abc123");
        // A link with an empty fragment passes through trimmed (the
        // caller's decode rejects it).
        assert_eq!(link_payload("https://host/p2p/#"), "https://host/p2p/#");
    }

    #[test]
    fn control_frames_round_trip_and_state_stays_state() {
        let repair = Ctrl {
            t: "repair".into(),
            payload: Some("abc123".into()),
        };
        let framed = encode_ctrl(&repair);
        assert!(framed.starts_with(CTRL_PREFIX));
        assert_eq!(decode_ctrl(&framed), Some(repair));
        // Payload-less frames stay compact, like the TS twin's encode.
        let done = Ctrl {
            t: "repair-done".into(),
            payload: None,
        };
        assert_eq!(encode_ctrl(&done), r#"uicc1.{"t":"repair-done"}"#);
        // A state snapshot is no control frame; garbage after the prefix
        // drops instead of erroring.
        assert_eq!(decode_ctrl(r#"{"items":[]}"#), None);
        assert_eq!(decode_ctrl("uicc1.not json"), None);
    }

    #[test]
    fn invite_links_carry_the_fragment_and_the_reply_digest() {
        assert_eq!(
            invite_link("https://host/p2p/", "abc", None),
            "https://host/p2p/#abc"
        );
        assert_eq!(
            invite_link("https://host/p2p/", "abc", Some("1a2b3c4d")),
            "https://host/p2p/#abc.1a2b3c4d"
        );
        // The link reduces back to its own payload.
        assert_eq!(
            link_payload(&invite_link("p/", "abc", Some("1a2b3c4d"))),
            "abc"
        );
    }

    #[test]
    fn a_missing_sdp_line_reports_which_one() {
        // parse_sdp requires all four attribute lines; the error names the
        // missing one plainly (PairError::Sdp).
        let sdp = "a=candidate:1 1 udp 1 10.0.0.1 1000 typ host\r\n\
            a=ice-ufrag:u\r\n\
            a=fingerprint:sha-256 AA\r\n\
            a=setup:actpass\r\n";
        let err = parse_sdp(sdp).expect_err("no ice-pwd line");
        assert!(matches!(err, PairError::Sdp(_)));
        assert_eq!(
            err.to_string(),
            "uic-sync pair: no ice-pwd in the local description"
        );
    }

    #[test]
    fn the_reply_digest_is_pinned_across_the_twins() {
        // The fixed vector both languages must produce — the TS twin
        // (`web/pair.ts` replyDigest) carries the same value in its comment.
        assert_eq!(reply_digest("abc"), "1a47e90b");
        assert_eq!(reply_digest("").len(), 8);
    }

    #[test]
    fn wrong_prefixes_and_roles_report_plainly() {
        assert!(decode_payload("nonsense").is_err());
        assert_eq!(payload_role("nonsense"), None);
        let answer = encode_payload(&Compact {
            s: Setup::Active,
            ..vector_compact()
        });
        assert_eq!(payload_role(&answer), Some(Role::Answer));
    }
}
