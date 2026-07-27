# ADR 0025: A rendezvous relay may carry the pairing reply

> **Superseded by [ADR 0031](0031-pairing-is-a-mutual-exchange.md).**
> The rendezvous relay was removed; pairing is now a mutual exchange with no third-party service.
> This record stands for the history.

## Decision

The `/p2p` demo page hands the reply payload of a pairing to a public pub/sub relay (ntfy.sh by default) instead of demanding a second manual exchange.
An invite link carries a one-time random topic (`…&via=<topic>`), the inviter subscribes to it over SSE, and whoever opens the link posts its own payload there — one scan or one opened link connects both sides.
`@schuhkarton/uic-sync` stays relay-free: the wizard element in the lit-demo page owns topic minting, the EventSource ear and the single POST; the library's pairing surface is unchanged.
Two localStorage knobs scope the infrastructure per browser: `uic-broker` (`off` restores the fully manual exchange, any URL points at a self-hosted ntfy) and `uic-ice` (an appended `RTCIceServer` list — a TURN server with credentials makes hostile NATs reachable).
`pair` now keeps `typ relay` candidates in payloads, so TURN-allocated addresses survive the compact encoding.

## Why

ADR 0024 stands on "pairing needs no infrastructure at all", which remains true and remains the fallback — but the both-ways exchange proved to be the demo's hardest step: each side must send one message AND receive one, and people lose track of which half already happened.
The reply is the only leg a relay can automate (the invite itself already travels as a QR code or link), and one payload-sized POST per pairing sits comfortably inside a public relay's terms.
State never touches the relay: only pairing payloads (addresses and a certificate fingerprint) transit it, and the topic secret rides exclusively inside the invite link.

## Consequences

- The default demo experience leans on ntfy.sh availability; the QR/manual path stays fully functional without it, and relay failures fall back to it with explicit hints.
- Whoever holds an invite link can post to its topic; the pairing guards (payload role, own payload, spent swap) decide what gets applied, exactly as with a pasted payload.
- The invite link grows by the topic (~25 characters) and stays QR-friendly.
- TURN stays the operator's affair: the repo ships no credentials and no relay, only the knob.
