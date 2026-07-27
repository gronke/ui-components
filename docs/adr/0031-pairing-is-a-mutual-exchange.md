# ADR 0031: Pairing is a mutual exchange

## Decision

The `/p2p` pairing is a mutual exchange with no third-party service: each side creates an invite and opens the other's — sharing a link, a QR or a token — and the two connect once each has applied the other's payload.
The ntfy.sh rendezvous relay (ADR 0025) is removed, along with its `uic-broker` and `UIC_LIT_DEMO_BROKER` knobs and the terminal's relay glue.
The invite is a single URL-safe fragment, `…/p2p/#uics1.<payload>` — no `s=`, no `&via=<topic>` — so a chat app linkifies the whole URL and the payload stays in the hash, never sent to the server.
The same-browser handover stays: a link opened in a fresh tab hands its payload to a waiting tab over a `BroadcastChannel`, which is local, not a server.

This supersedes ADR 0025.

## Why

The relay only ever automated the return leg — the invite itself already travels as a link or QR — at the price of a dependency on a public third party that, however briefly, sees each side's connection offer.
Everything a relay-free pairing needs already existed as the "exchange by hand" fallback (the symmetric swap, the paste box, the QR, the terminal's paste-a-token path), so removing the relay is mostly deletion rather than new code.
The both-ways step ADR 0025 found hardest — people losing track of which half already happened — is now guided by the UI instead of a relay: the shared panel shows "share your invite" and "open their invite" as equal halves, with the QR now rendered in both (ADR 0030).

## Consequences

- Every pairing is two-way: both sides share a datum, and the single-scan convenience the relay bought is gone — the deliberate tradeoff for keeping the payload off any third party.
- The invite link is shorter and linkifies cleanly, the topic and its `&`/`=` gone; the `uics1.` payload codec is byte-for-byte unchanged, so browser and terminal peers still interoperate.
- The payload stays in the URL fragment and is consumed once — a load reads it, then `history.replaceState` strips it — so it never reaches the server and never lingers in history.
- TURN stays the only optional infrastructure knob (`uic-ice`), unrelated to the removed relay; the repo still ships no TURN server.
- `reqwest` and `rand` leave the demo crate with the relay code.
