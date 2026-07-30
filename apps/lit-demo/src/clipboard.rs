//! The p2p clipboard: the arboard backend the mocked DOM's
//! `navigator.clipboard` reads, plus the throttle and classification the
//! run loop uses to auto-continue a pairing step. A headless session or
//! `--no-clipboard` leaves the backend inert (the paste path always
//! remains); the classification is pure and the contents are never logged.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use uic_js::ClipboardBackend;
use uic_sync::pair::{decode_payload, link_payload, link_reply, reply_digest, Setup};

/// The system clipboard behind uic_js's backend trait, so `navigator.
/// clipboard` and the host's own read share one arboard connection.
/// Construction fails on a headless host — then the reads yield `None` and
/// the affordance simply never fires.
pub(crate) struct SystemClipboard(RefCell<Option<arboard::Clipboard>>);

impl SystemClipboard {
    /// A backend when `enabled` and a clipboard opens; otherwise inert.
    pub(crate) fn new(enabled: bool) -> SystemClipboard {
        let clipboard = enabled.then(arboard::Clipboard::new).and_then(Result::ok);
        SystemClipboard(RefCell::new(clipboard))
    }
}

impl ClipboardBackend for SystemClipboard {
    fn read(&self) -> Option<String> {
        self.0.borrow_mut().as_mut()?.get_text().ok()
    }

    fn write(&self, text: &str) -> bool {
        self.0
            .borrow_mut()
            .as_mut()
            .is_some_and(|clipboard| clipboard.set_text(text.to_string()).is_ok())
    }
}

/// What the clipboard held that matters: a peer credential, and whether it
/// answers the invite we are currently showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Find {
    pub payload: String,
    pub reply_to_us: bool,
}

/// Whether the text carries a peer's swap offer we could pair with — a
/// decodable `actpass` payload (the `uic1` magic head makes this certain)
/// that is not our own. `reply_to_us` marks a reply naming our current
/// invite (the digest matches), which the loop treats as the peer we
/// already expect rather than a conflict.
pub(crate) fn classify(text: &str, own_payload: &str) -> Option<Find> {
    let payload = link_payload(text);
    let compact = decode_payload(&payload).ok()?;
    if compact.s != Setup::ActPass || payload == own_payload {
        return None;
    }
    let reply_to_us = link_reply(text).is_some_and(|digest| digest == reply_digest(own_payload));
    Some(Find {
        payload,
        reply_to_us,
    })
}

/// The gap between reads — a pairing credential is not time-critical, and a
/// hot loop reading the clipboard would be rude.
const POLL_EVERY: Duration = Duration::from_secs(1);

/// The read throttle: it gates how often the loop reads the clipboard and
/// reports only when the contents CHANGED, so an unchanged clipboard costs
/// nothing and the same credential is never offered twice. It holds no
/// reader — the loop reads through the host's clipboard backend.
#[derive(Default)]
pub(crate) struct ClipboardWatch {
    last: Option<String>,
    next_poll: Option<Instant>,
}

impl ClipboardWatch {
    /// The clipboard text if it changed since the last read and the
    /// throttle has elapsed; `None` otherwise. `read` fetches the current
    /// text (the host's `clipboard_read`); `now` lets the loop own the
    /// clock and tests stay deterministic.
    pub(crate) fn poll(
        &mut self,
        now: Instant,
        read: impl FnOnce() -> Option<String>,
    ) -> Option<String> {
        if self.next_poll.is_some_and(|at| now < at) {
            return None;
        }
        self.next_poll = Some(now + POLL_EVERY);
        let text = read()?;
        if self.last.as_deref() == Some(text.as_str()) {
            return None;
        }
        self.last = Some(text.clone());
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_sync::pair::{encode_payload, Compact};

    fn offer(fingerprint: &str) -> String {
        encode_payload(&Compact {
            u: "u".into(),
            p: "p".into(),
            f: fingerprint.into(),
            s: Setup::ActPass,
            c: vec![("127.0.0.1".into(), 1)],
        })
    }

    fn answer(fingerprint: &str) -> String {
        encode_payload(&Compact {
            u: "u".into(),
            p: "p".into(),
            f: fingerprint.into(),
            s: Setup::Active,
            c: vec![("127.0.0.1".into(), 1)],
        })
    }

    #[test]
    fn classify_accepts_a_peer_offer_and_rejects_the_rest() {
        let own = offer("00:aa");
        let peer = offer("00:bb");

        let found = classify(&peer, &own).expect("a peer offer pairs");
        assert_eq!(found.payload, peer);
        assert!(!found.reply_to_us);

        // Our own payload, an answer role, and plain garbage are no finds.
        assert_eq!(classify(&own, &own), None);
        assert_eq!(classify(&answer("00:bb"), &own), None);
        assert_eq!(classify("just some text", &own), None);
    }

    #[test]
    fn classify_marks_a_reply_naming_our_invite() {
        let own = offer("00:aa");
        let peer = offer("00:bb");
        let reply = format!("https://host/p2p/#{peer}.{}", reply_digest(&own));
        let found = classify(&reply, &own).expect("a reply is still a peer offer");
        assert_eq!(found.payload, peer);
        assert!(found.reply_to_us, "the digest names our invite");

        // A reply for a different invite is a find, but not ours.
        let other = format!("https://host/p2p/#{peer}.deadbeef");
        assert!(!classify(&other, &own).unwrap().reply_to_us);
    }

    #[test]
    fn the_watch_throttles_and_fires_only_on_change() {
        let mut reads = vec!["first", "first", "second"].into_iter();
        let mut next = || reads.next().map(String::from);
        let mut watch = ClipboardWatch::default();
        let t0 = Instant::now();

        // First read fires; a second read inside the throttle window is
        // skipped without even calling the reader.
        assert_eq!(watch.poll(t0, &mut next).as_deref(), Some("first"));
        assert_eq!(watch.poll(t0 + Duration::from_millis(200), &mut next), None);
        // Past the window, an unchanged value is silent…
        assert_eq!(watch.poll(t0 + Duration::from_secs(2), &mut next), None);
        // …and a change fires.
        assert_eq!(
            watch
                .poll(t0 + Duration::from_secs(4), &mut next)
                .as_deref(),
            Some("second")
        );
    }

    #[test]
    fn an_empty_read_is_silent() {
        let mut watch = ClipboardWatch::default();
        assert_eq!(watch.poll(Instant::now(), || None), None);
    }
}
