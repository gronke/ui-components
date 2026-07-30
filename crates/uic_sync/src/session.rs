//! The pairing-session lifecycle a native host drives — the Rust half of
//! ADR 0032, sharing the `pair::Ctrl` control plane with `web/session.ts`
//! (whose own job, cross-tab organization, stays browser-only).
//!
//! [`Session`] is a pure state machine: transport events go in, effects
//! come out, and the WebRTC stack stays with the consumer (the lit-demo
//! drives it through a small tokio loop). Every wire carries a monotone
//! [`Gen`] tag; events from superseded wires are no-ops by construction,
//! which replaces per-wire "deliberate close" flags — the machine simply
//! no longer knows the old wire when its close arrives.
//!
//! The machine also owns every user-facing pairing status and the
//! [`PanelState`] it presents — the shared `<pair-panel>` property
//! contract, the same component both hosts render.

use crate::pair::{self, Ctrl};

/// A monotone wire tag: each minted swap gets the next one, and stale-gen
/// events fall through silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gen(u64);

/// The mode vocabulary the native session produces — `as_str` spells the
/// `<pair-panel>` property values. The TS wizard's union adds the
/// browser-only members (`handed`, `moved`, `nortc`) on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelMode {
    #[default]
    Idle,
    Invite,
    Connected,
    Dropped,
    Failed,
}

impl PanelMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PanelMode::Idle => "idle",
            PanelMode::Invite => "invite",
            PanelMode::Connected => "connected",
            PanelMode::Dropped => "dropped",
            PanelMode::Failed => "failed",
        }
    }
}

/// The pairing wizard's three steps: start one, acknowledge the peer,
/// connect. The panel lights the reachable step and mutes the rest; the
/// `as_u8` value is the prop both hosts read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    #[default]
    Init,
    Acknowledge,
    Connect,
}

impl Step {
    pub fn as_u8(self) -> u8 {
        match self {
            Step::Init => 1,
            Step::Acknowledge => 2,
            Step::Connect => 3,
        }
    }
}

/// The `<pair-panel>` view: what a host mirrors onto the mounted panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelState {
    pub mode: PanelMode,
    pub link: String,
    pub status: String,
    pub connected: Option<bool>,
    pub reset_label: String,
    pub step: Step,
}

/// A panel intent the host reads off the component and feeds back in as
/// [`Event::Command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Start a fresh invite (the reset button).
    Renew,
    /// Connect to a pasted invite — a link or a bare pairing code.
    Connect(String),
    /// The host's clock, ticked while a connect is in flight: fold the
    /// elapsed seconds into the connecting status so the wait shows progress.
    /// The machine stays clockless — the host counts, this only formats.
    Tick { secs: u64 },
}

/// What happened outside the machine: transport outcomes carry the [`Gen`]
/// of the wire they speak for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A [`Effect::Mint`] produced a swap with this payload.
    Minted {
        gen: Gen,
        payload: String,
    },
    MintFailed {
        gen: Gen,
        error: String,
    },
    /// A [`Effect::Connect`] opened its channel.
    Connected {
        gen: Gen,
    },
    ConnectFailed {
        gen: Gen,
        error: String,
    },
    /// The transport closed unsolicited; stale gens are no-ops.
    Closed {
        gen: Gen,
    },
    Command(Command),
    /// A decoded `repair` control frame with the peer's fresh payload.
    Repair {
        peer: String,
    },
}

/// What the host must do next, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Mirror this view onto the panel.
    Present(PanelState),
    /// Mint a fresh swap and answer with `Minted`/`MintFailed`.
    Mint { gen: Gen },
    /// Connect the swap to the peer payload and answer with
    /// `Connected`/`ConnectFailed`; `greet` says whether this side
    /// announces its canonical state on open.
    Connect { gen: Gen, peer: String, greet: bool },
    /// Send a control frame down the standing wire's pump.
    SendCtrl(Ctrl),
    /// Tear the swap down and forget it.
    Close { gen: Gen },
}

/// Where the session stands between events.
enum Phase {
    /// The primary swap is minting.
    Minting,
    /// The invite is presented; a peer payload or a command moves on.
    Inviting,
    /// The primary connect is in flight.
    Connecting,
    /// The wire is up; commands and repair rounds apply.
    Standing,
    /// Failed or dropped; commands mint afresh.
    Down,
}

/// A repair round in flight beside the standing wire.
struct Pending {
    gen: Gen,
    peer: String,
}

/// How a peer payload reached us — the detection that shapes step 2 and the
/// failure. A fresh invite means the peer is initiating and waits on our
/// reply (a failed connect cannot honestly retry — a new swap means a new
/// reply link the peer never sees); a reply to our own invite means the peer
/// already applied our payload (a failed connect cannot resume either — their
/// copy is stale the moment our swap dies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerCase {
    FreshInvite,
    ReplyToOurs,
}

/// The peer this session is chasing: their payload and how it reached us,
/// retained across a failed connect so the failure knows what it can honestly
/// say.
#[derive(Debug, Clone)]
struct Pursuit {
    peer: String,
    case: PeerCase,
}

/// Past this many seconds a connect nudges the user to check the peer opened
/// the reply, rather than only counting up. The browser twin's `SLOW_HINT_MS`.
const SLOW_HINT_SECS: u64 = 15;

/// The reset button's label per resting state. The machine owns the panel's
/// copy (the module contract), so the words that recur across arms live in one
/// place rather than inline at each.
const RESET_START_OVER: &str = "start over";
const RESET_INVITE_ANOTHER: &str = "invite somebody else";
const RESET_TRY_AGAIN: &str = "try again";

/// The honest verdict when a fresh-invite connect could not be confirmed. The
/// browser wizard mirrors this line byte for byte (`pair-wizard.ts`), so one
/// Rust anchor keeps the twin from drifting.
const CONFIRM_FAILED_STATUS: &str = "couldn't confirm the connection — the other side may still show connected; start a fresh pairing on both and exchange new links";

/// The pairing session: create an invite, wait for the peer (pairing is a
/// mutual exchange — ADR 0028), connect, stand ready to renew, and answer
/// repair rounds (ADR 0032) with a fresh wire that replaces the old one
/// only after it opened.
pub struct Session {
    page: String,
    /// The peer being chased, if any — its payload and how it reached us,
    /// which shapes the connecting status and the honest failure.
    pursuit: Option<Pursuit>,
    /// A one-shot status the next plain invite carries — the honest word
    /// after a pairing that could not resume.
    carry_status: Option<String>,
    phase: Phase,
    counter: u64,
    /// The primary wire: its gen and, once minted, its payload.
    current: Option<(Gen, String)>,
    pending: Option<Pending>,
    view: PanelState,
}

impl Session {
    /// Starts a session for the pairing page; `opener` carries the peer
    /// payload when an invite link launched this host. The first effect
    /// mints the own swap.
    pub fn start(page: String, opener: Option<String>) -> (Session, Vec<Effect>) {
        // A CLI opener is a fresh invite we are answering — we hold their
        // payload and must send our reply back.
        let pursuit = opener.map(|peer| Pursuit {
            peer,
            case: PeerCase::FreshInvite,
        });
        let mut session = Session {
            page,
            pursuit,
            carry_status: None,
            phase: Phase::Minting,
            counter: 0,
            current: None,
            pending: None,
            view: PanelState::default(),
        };
        let mint = session.mint();
        (session, vec![mint])
    }

    /// Feeds one event and returns the effects to execute, in order.
    pub fn on(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Minted { gen, payload } => self.minted(gen, payload),
            Event::MintFailed { gen, error } => self.mint_failed(gen, &error),
            Event::Connected { gen } => self.opened(gen),
            Event::ConnectFailed { gen, error } => self.connect_failed(gen, &error),
            Event::Closed { gen } => self.closed(gen),
            Event::Command(command) => self.command(command),
            Event::Repair { peer } => self.repair(peer),
        }
    }

    fn next_gen(&mut self) -> Gen {
        self.counter += 1;
        Gen(self.counter)
    }

    fn mint(&mut self) -> Effect {
        let gen = self.next_gen();
        self.phase = Phase::Minting;
        self.current = Some((gen, String::new()));
        self.pending = None;
        Effect::Mint { gen }
    }

    fn present(&mut self, view: PanelState) -> Effect {
        self.view = view.clone();
        Effect::Present(view)
    }

    fn present_status(&mut self, status: String) -> Effect {
        self.view.status = status;
        Effect::Present(self.view.clone())
    }

    fn is_current(&self, gen: Gen) -> bool {
        self.current.as_ref().is_some_and(|(own, _)| *own == gen)
    }

    fn is_pending(&self, gen: Gen) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.gen == gen)
    }

    fn minted(&mut self, gen: Gen, payload: String) -> Vec<Effect> {
        if self.is_pending(gen) {
            // The repair round's fresh wire: the answer rides the standing
            // wire's pump, and the fresh connect greets — this side holds
            // the canonical state, whatever the payload order says.
            let peer = self.pending.as_ref().expect("pending checked").peer.clone();
            return vec![
                Effect::SendCtrl(Ctrl {
                    t: "repair-answer".into(),
                    payload: Some(payload),
                }),
                Effect::Connect {
                    gen,
                    peer,
                    greet: true,
                },
            ];
        }
        if !self.is_current(gen) {
            return Vec::new();
        }
        self.current = Some((gen, payload.clone()));
        // Pairing is a mutual exchange (ADR 0028): each side sends its
        // invite and opens the other's. A pursuit means we already hold
        // the peer's fresh invite (ReplyToOurs connects directly from
        // Inviting and never re-mints), so step 2 shows the reply link the
        // peer must open and the connect rides in the background.
        match self.pursuit.clone() {
            Some(pursuit) => {
                let reply_to = pair::reply_digest(&pursuit.peer);
                let link = pair::invite_link(&self.page, &payload, Some(&reply_to));
                let status = self.connecting_status(0);
                self.phase = Phase::Connecting;
                vec![
                    self.present(PanelState {
                        mode: PanelMode::Invite,
                        link,
                        status,
                        connected: None,
                        reset_label: RESET_START_OVER.into(),
                        step: Step::Acknowledge,
                    }),
                    Effect::Connect {
                        gen,
                        // Exactly one side greets: the lexically smaller
                        // payload (ADR 0013) — a fresh pairing has no
                        // canonical-state holder yet.
                        greet: payload < pursuit.peer,
                        peer: pursuit.peer,
                    },
                ]
            }
            None => {
                let link = pair::invite_link(&self.page, &payload, None);
                self.phase = Phase::Inviting;
                let status = self.carry_status.take().unwrap_or_else(|| {
                    "the card and the code carry the same invite — you connect when a peer answers"
                        .into()
                });
                vec![self.present(PanelState {
                    mode: PanelMode::Invite,
                    link,
                    status,
                    connected: None,
                    reset_label: RESET_START_OVER.into(),
                    step: Step::Init,
                })]
            }
        }
    }

    fn mint_failed(&mut self, gen: Gen, error: &str) -> Vec<Effect> {
        if self.is_pending(gen) {
            self.pending = None;
            return vec![self.present_status(format!(
                "the handover failed to set up ({error}) — still on the old wire"
            ))];
        }
        if !self.is_current(gen) {
            return Vec::new();
        }
        self.phase = Phase::Down;
        self.current = None;
        vec![self.present(PanelState {
            mode: PanelMode::Failed,
            status: format!("pairing setup failed: {error}"),
            reset_label: RESET_TRY_AGAIN.into(),
            ..PanelState::default()
        })]
    }

    fn opened(&mut self, gen: Gen) -> Vec<Effect> {
        if self.is_pending(gen) {
            // The old wire closes only now, after the new one opened — a
            // failed handover loses nothing. Its later close event carries
            // a stale gen and falls through silently.
            let pending = self.pending.take().expect("pending checked");
            let old = self.current.replace((pending.gen, String::new()));
            self.phase = Phase::Standing;
            let mut effects = Vec::new();
            if let Some((old_gen, _)) = old {
                effects.push(Effect::Close { gen: old_gen });
            }
            effects.push(
                self.present_status("the other side moved to a new tab — reconnected".into()),
            );
            return effects;
        }
        if !self.is_current(gen) {
            return Vec::new();
        }
        // The wire stands — the pursuit is over, no retry to keep.
        self.pursuit = None;
        self.phase = Phase::Standing;
        vec![self.present(PanelState {
            mode: PanelMode::Connected,
            status: "paired — one list, two ends".into(),
            connected: Some(true),
            reset_label: RESET_INVITE_ANOTHER.into(),
            step: Step::Connect,
            ..PanelState::default()
        })]
    }

    fn connect_failed(&mut self, gen: Gen, error: &str) -> Vec<Effect> {
        if self.is_pending(gen) {
            self.pending = None;
            return vec![
                Effect::Close { gen },
                self.present_status(format!(
                    "the handover failed ({error}) — still on the old wire"
                )),
            ];
        }
        if !self.is_current(gen) {
            return Vec::new();
        }
        // Neither case can honestly retry. A fresh-invite connect that failed
        // cannot re-mint: a new swap means a new reply link, and the peer —
        // who opened the first — never sees it, so a retry only invalidates
        // the link they are about to open. A reply-to-ours failure cannot
        // resume either: the peer's copy of our payload went stale with our
        // swap. Both fail honestly; a fresh pairing is the way back.
        match self.pursuit.take() {
            Some(Pursuit {
                case: PeerCase::FreshInvite,
                ..
            }) => {
                self.phase = Phase::Down;
                self.current = None;
                vec![self.present(PanelState {
                    mode: PanelMode::Failed,
                    status: CONFIRM_FAILED_STATUS.into(),
                    reset_label: "start a fresh pairing".into(),
                    ..PanelState::default()
                })]
            }
            Some(Pursuit {
                case: PeerCase::ReplyToOurs,
                ..
            }) => {
                // The next plain invite carries the honest word.
                self.carry_status = Some(
                    "that exchange can't resume — the peer's copy of your invite went stale; share this fresh invite instead".into(),
                );
                self.recycle()
            }
            None => {
                self.phase = Phase::Down;
                vec![self.present(PanelState {
                    mode: PanelMode::Failed,
                    status: format!("pairing failed: {error}"),
                    reset_label: "start a new pairing".into(),
                    ..PanelState::default()
                })]
            }
        }
    }

    fn closed(&mut self, gen: Gen) -> Vec<Effect> {
        // Only the standing wire's unsolicited close means anything: a
        // renewed, replaced or already-failed wire is stale by gen, which
        // is the whole deliberate-close story.
        if !matches!(self.phase, Phase::Standing) || !self.is_current(gen) {
            return Vec::new();
        }
        self.phase = Phase::Down;
        self.current = None;
        vec![self.present(PanelState {
            mode: PanelMode::Dropped,
            status: "connection closed — restart to pair again".into(),
            connected: Some(false),
            reset_label: RESET_INVITE_ANOTHER.into(),
            ..PanelState::default()
        })]
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        match command {
            Command::Renew => {
                self.pursuit = None;
                self.carry_status = None;
                self.recycle()
            }
            Command::Connect(text) => {
                let peer = pair::link_payload(&text);
                let reply = pair::link_reply(&text);
                if let (Phase::Inviting, Some((gen, payload))) = (&self.phase, self.current.clone())
                {
                    // A reply naming OUR invite means the peer already
                    // applied our payload — connecting completes at once
                    // (step 3). A reply naming a DIFFERENT invite is neither
                    // ours to answer nor a fresh invite — say so, touch
                    // nothing. No digest is a fresh invite: the peer is
                    // initiating and waits on our reply, so step 2 shows the
                    // reply link they must open while the connect rides
                    // behind it.
                    match reply {
                        Some(ref digest) if *digest == pair::reply_digest(&payload) => {
                            self.pursuit = Some(Pursuit {
                                peer: peer.clone(),
                                case: PeerCase::ReplyToOurs,
                            });
                            self.phase = Phase::Connecting;
                            let mut view = self.view.clone();
                            view.status = self.connecting_status(0);
                            view.step = Step::Connect;
                            return vec![
                                self.present(view),
                                Effect::Connect {
                                    gen,
                                    greet: payload < peer,
                                    peer,
                                },
                            ];
                        }
                        Some(_) => {
                            return vec![self.present_status(
                                "that link answers a different invite — ask them to open your current one"
                                    .into(),
                            )];
                        }
                        None => {
                            self.pursuit = Some(Pursuit {
                                peer: peer.clone(),
                                case: PeerCase::FreshInvite,
                            });
                            self.phase = Phase::Connecting;
                            let reply_to = pair::reply_digest(&peer);
                            let link = pair::invite_link(&self.page, &payload, Some(&reply_to));
                            let status = self.connecting_status(0);
                            return vec![
                                self.present(PanelState {
                                    mode: PanelMode::Invite,
                                    link,
                                    status,
                                    connected: None,
                                    reset_label: RESET_START_OVER.into(),
                                    step: Step::Acknowledge,
                                }),
                                Effect::Connect {
                                    gen,
                                    greet: payload < peer,
                                    peer,
                                },
                            ];
                        }
                    }
                }
                // Standing or down: the next mint consumes the peer like an
                // opened invite would — a paste here starts a fresh chase.
                self.pursuit = Some(Pursuit {
                    peer,
                    case: PeerCase::FreshInvite,
                });
                self.recycle()
            }
            Command::Tick { secs } => {
                // Only a live connect has a clock worth showing; anything else
                // ignores the host's tick.
                if matches!(self.phase, Phase::Connecting) {
                    let status = self.connecting_status(secs);
                    vec![self.present_status(status)]
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// The connecting status the panel shows while a connect is in flight,
    /// the host's elapsed seconds folded in so the wait shows progress. A
    /// fresh-invite opener still owes the reply link (and, past the slow mark,
    /// a nudge to check the peer opened it); a reply-to-ours side is simply
    /// the one connecting now.
    fn connecting_status(&self, secs: u64) -> String {
        match self.pursuit.as_ref().map(|pursuit| pursuit.case) {
            Some(PeerCase::ReplyToOurs) => {
                format!("they opened your invite — connecting… {secs}s")
            }
            _ if secs > SLOW_HINT_SECS => {
                format!("still connecting {secs}s — make sure they opened your reply link")
            }
            _ => {
                format!(
                    "connecting {secs}s — send this reply back; you pair the moment they open it"
                )
            }
        }
    }

    /// Closes whatever wires stand and mints afresh.
    fn recycle(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Some(pending) = self.pending.take() {
            effects.push(Effect::Close { gen: pending.gen });
        }
        if let Some((gen, _)) = self.current.take() {
            effects.push(Effect::Close { gen });
        }
        effects.push(self.mint());
        effects
    }

    fn repair(&mut self, peer: String) -> Vec<Effect> {
        // A repair round makes sense only over a standing wire (the frames
        // arrive through it); one round at a time.
        if !matches!(self.phase, Phase::Standing) || self.pending.is_some() {
            return Vec::new();
        }
        let gen = self.next_gen();
        self.pending = Some(Pending { gen, peer });
        vec![
            self.present_status("the other side is moving to a new tab — re-pairing…".into()),
            Effect::Mint { gen },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minted(session: &mut Session, effects: &[Effect], payload: &str) -> Vec<Effect> {
        let gen = mint_gen(effects);
        session.on(Event::Minted {
            gen,
            payload: payload.into(),
        })
    }

    fn mint_gen(effects: &[Effect]) -> Gen {
        effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Mint { gen } => Some(*gen),
                _ => None,
            })
            .expect("a mint effect")
    }

    fn connect_of(effects: &[Effect]) -> (Gen, String, bool) {
        effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Connect { gen, peer, greet } => Some((*gen, peer.clone(), *greet)),
                _ => None,
            })
            .expect("a connect effect")
    }

    fn presented(effects: &[Effect]) -> PanelState {
        effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::Present(state) => Some(state.clone()),
                _ => None,
            })
            .expect("a present effect")
    }

    #[test]
    fn a_fresh_session_mints_and_presents_step_one() {
        let (mut session, effects) = Session::start("https://host/p2p/".into(), None);
        let effects = minted(&mut session, &effects, "bbb");
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Invite);
        assert_eq!(view.step, Step::Init);
        assert_eq!(view.link, "https://host/p2p/#bbb");
        assert!(view.status.contains("you connect when a peer answers"));
        assert_eq!(view.reset_label, RESET_START_OVER);
        // No pursuit: the session waits — no connect yet.
        assert!(!effects.iter().any(|e| matches!(e, Effect::Connect { .. })));
    }

    #[test]
    fn an_opener_lands_on_step_two_with_the_reply_link() {
        let (mut session, effects) =
            Session::start("https://host/p2p/".into(), Some("peerPayload".into()));
        let effects = minted(&mut session, &effects, "own");
        let (_, peer, greet) = connect_of(&effects);
        assert_eq!(peer, "peerPayload");
        // Lexical greet: "own" < "peerPayload".
        assert!(greet);
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Invite);
        assert_eq!(view.step, Step::Acknowledge);
        // The opener sees a live connect (with the seconds counter) beside the
        // instruction to send the reply — not one or the other.
        assert!(view.status.contains("connecting"));
        assert!(view.status.contains("send this reply back"));
        assert_eq!(
            view.link,
            format!(
                "https://host/p2p/#own.{}",
                crate::pair::reply_digest("peerPayload")
            )
        );
    }

    #[test]
    fn a_fresh_invite_paste_moves_to_step_two_and_upgrades_the_link() {
        let (mut session, effects) = Session::start("https://host/p2p/".into(), None);
        let _ = minted(&mut session, &effects, "zzz");
        // A bare invite (no digest) is a fresh invite: the peer initiates,
        // so we go to step 2 and present the reply link they must open.
        let effects = session.on(Event::Command(Command::Connect(
            "https://host/p2p/#aaa".into(),
        )));
        let (gen, peer, greet) = connect_of(&effects);
        assert_eq!(peer, "aaa");
        assert!(!greet, "\"zzz\" is not the lexically smaller payload");
        let view = presented(&effects);
        assert_eq!(view.step, Step::Acknowledge);
        assert_eq!(
            view.link,
            format!("https://host/p2p/#zzz.{}", crate::pair::reply_digest("aaa")),
            "the presented link becomes the reply the peer opens"
        );
        let effects = session.on(Event::Connected { gen });
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Connected);
        assert_eq!(view.link, "", "the consumed invite leaves the card");
    }

    #[test]
    fn a_reply_to_our_invite_skips_to_step_three() {
        let (mut session, effects) = Session::start("https://host/p2p/".into(), None);
        let _ = minted(&mut session, &effects, "own");
        // A reply naming our own invite: the peer already applied our
        // payload, so we connect at once at step 3, the link unchanged.
        let reply = format!(
            "https://host/p2p/#peer.{}",
            crate::pair::reply_digest("own")
        );
        let effects = session.on(Event::Command(Command::Connect(reply)));
        let (_, peer, _) = connect_of(&effects);
        assert_eq!(peer, "peer");
        let view = presented(&effects);
        assert_eq!(view.step, Step::Connect);
        assert!(view.status.contains("they opened your invite"));
    }

    #[test]
    fn a_reply_for_a_different_invite_is_refused_without_connecting() {
        let (mut session, effects) = Session::start("https://host/p2p/".into(), None);
        let _ = minted(&mut session, &effects, "own");
        // A reply whose digest names some other invite — neither ours to
        // answer nor a fresh invite. Say so, connect nothing.
        let effects = session.on(Event::Command(Command::Connect(
            "https://host/p2p/#peer.deadbeef".into(),
        )));
        assert!(!effects.iter().any(|e| matches!(e, Effect::Connect { .. })));
        assert!(presented(&effects).status.contains("a different invite"));
        // The invite still stands — a correct paste next still works.
        assert_eq!(presented(&effects).mode, PanelMode::Invite);
    }

    #[test]
    fn a_renew_closes_and_mints_afresh() {
        let (mut session, effects) = Session::start("p".into(), None);
        let first = mint_gen(&effects);
        let _ = minted(&mut session, &effects, "own");
        let effects = session.on(Event::Command(Command::Renew));
        assert_eq!(effects[0], Effect::Close { gen: first });
        let second = mint_gen(&effects);
        assert_ne!(first, second);
    }

    #[test]
    fn a_repair_round_replaces_the_wire_and_the_old_close_is_silent() {
        let (mut session, effects) = Session::start("p".into(), Some("peer".into()));
        let effects = minted(&mut session, &effects, "own");
        let (old_gen, ..) = connect_of(&effects);
        session.on(Event::Connected { gen: old_gen });

        // The remote's new tab asks for a repair.
        let effects = session.on(Event::Repair {
            peer: "freshPeer".into(),
        });
        assert!(presented(&effects).status.contains("re-pairing"));
        let fresh_gen = mint_gen(&effects);

        // The fresh swap answers through the standing wire and connects
        // with a FORCED greet — this side holds the canonical state.
        let effects = session.on(Event::Minted {
            gen: fresh_gen,
            payload: "freshOwn".into(),
        });
        assert_eq!(
            effects[0],
            Effect::SendCtrl(Ctrl {
                t: "repair-answer".into(),
                payload: Some("freshOwn".into()),
            })
        );
        let (gen, peer, greet) = connect_of(&effects);
        assert_eq!((gen, peer.as_str(), greet), (fresh_gen, "freshPeer", true));

        // The old wire closes only after the new one opened.
        let effects = session.on(Event::Connected { gen: fresh_gen });
        assert_eq!(effects[0], Effect::Close { gen: old_gen });
        assert!(presented(&effects).status.contains("reconnected"));

        // The old wire's close event is stale by gen — no dropped state.
        assert_eq!(session.on(Event::Closed { gen: old_gen }), Vec::new());
    }

    #[test]
    fn a_failed_handover_keeps_the_old_wire() {
        let (mut session, effects) = Session::start("p".into(), Some("peer".into()));
        let effects = minted(&mut session, &effects, "own");
        let (old_gen, ..) = connect_of(&effects);
        session.on(Event::Connected { gen: old_gen });

        let effects = session.on(Event::Repair {
            peer: "freshPeer".into(),
        });
        let fresh_gen = mint_gen(&effects);
        let effects = session.on(Event::Minted {
            gen: fresh_gen,
            payload: "freshOwn".into(),
        });
        let _ = connect_of(&effects);
        let effects = session.on(Event::ConnectFailed {
            gen: fresh_gen,
            error: "unreachable".into(),
        });
        assert_eq!(effects[0], Effect::Close { gen: fresh_gen });
        assert!(presented(&effects).status.contains("still on the old wire"));

        // The standing wire still owns the session: its close still counts.
        let effects = session.on(Event::Closed { gen: old_gen });
        assert_eq!(presented(&effects).mode, PanelMode::Dropped);
    }

    #[test]
    fn a_fresh_invite_connect_failure_stops_honestly() {
        let (mut session, effects) =
            Session::start("https://host/p2p/".into(), Some("peer".into()));
        let first = minted(&mut session, &effects, "own");
        let (gen, ..) = connect_of(&first);

        // A fresh-invite connect that failed cannot honestly retry: a new
        // swap means a new reply link the peer (who opened the first) never
        // sees. Fail honestly — no re-mint, no re-connect, no bald claim.
        let effects = session.on(Event::ConnectFailed {
            gen,
            error: "the peers could not reach each other".into(),
        });
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Failed);
        assert!(view.status.contains("couldn't confirm"));
        assert!(
            !view.status.contains("could not reach"),
            "the bald unreachable claim stays off the screen: {}",
            view.status
        );
        assert_eq!(view.reset_label, "start a fresh pairing");
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Mint { .. } | Effect::Connect { .. })),
            "an honest failure neither re-mints nor re-connects"
        );
    }

    #[test]
    fn a_tick_counts_the_seconds_while_connecting() {
        let (mut session, effects) =
            Session::start("https://host/p2p/".into(), Some("peer".into()));
        let _ = minted(&mut session, &effects, "own");

        // While a connect stands, the host's tick folds the elapsed seconds
        // into the status — the wait shows progress and the action still reads.
        let effects = session.on(Event::Command(Command::Tick { secs: 12 }));
        let view = presented(&effects);
        assert_eq!(view.step, Step::Acknowledge);
        assert!(
            view.status.contains("12"),
            "the counter shows: {}",
            view.status
        );
        assert!(view.status.contains("send this reply back"));

        // A plain invite (no connect in flight) has no clock — a tick is inert.
        let (mut idle, effects) = Session::start("https://host/p2p/".into(), None);
        let _ = minted(&mut idle, &effects, "own");
        assert!(
            idle.on(Event::Command(Command::Tick { secs: 5 }))
                .is_empty(),
            "a tick off a live connect presents nothing"
        );
    }

    #[test]
    fn a_reply_to_ours_connect_failure_renews_a_plain_invite() {
        let (mut session, effects) = Session::start("https://host/p2p/".into(), None);
        let _ = minted(&mut session, &effects, "own");
        let reply = format!(
            "https://host/p2p/#peer.{}",
            crate::pair::reply_digest("own")
        );
        let effects = session.on(Event::Command(Command::Connect(reply)));
        let (gen, ..) = connect_of(&effects);

        // Their copy of our payload is stale the moment our swap dies —
        // no self-serving retry. Renew to a plain invite (step 1), honestly.
        let effects = session.on(Event::ConnectFailed {
            gen,
            error: "ice failed".into(),
        });
        let fresh = mint_gen(&effects);
        let effects = session.on(Event::Minted {
            gen: fresh,
            payload: "own2".into(),
        });
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Invite);
        assert_eq!(view.step, Step::Init);
        assert!(view.status.contains("can't resume"));
        assert!(
            !view.status.contains("could not reach"),
            "the bald unreachable claim stays off the screen: {}",
            view.status
        );
        assert_eq!(view.link, "https://host/p2p/#own2");
        assert!(
            !effects.iter().any(|e| matches!(e, Effect::Connect { .. })),
            "a plain invite waits for a peer — no auto-connect"
        );
    }

    #[test]
    fn mint_failure_presents_failed_and_a_command_retries() {
        let (mut session, effects) = Session::start("p".into(), None);
        let gen = mint_gen(&effects);
        let effects = session.on(Event::MintFailed {
            gen,
            error: "no sockets".into(),
        });
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Failed);
        assert!(view.status.contains("pairing setup failed"));
        assert_eq!(view.reset_label, RESET_TRY_AGAIN);
        let effects = session.on(Event::Command(Command::Renew));
        assert_ne!(mint_gen(&effects), gen);
    }

    #[test]
    fn a_drop_presents_dropped_and_a_pasted_link_re_pairs() {
        let (mut session, effects) = Session::start("p".into(), Some("peer".into()));
        let effects = minted(&mut session, &effects, "own");
        let (gen, ..) = connect_of(&effects);
        session.on(Event::Connected { gen });

        let effects = session.on(Event::Closed { gen });
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Dropped);
        assert_eq!(view.connected, Some(false));
        assert_eq!(view.reset_label, RESET_INVITE_ANOTHER);

        // A pasted link from the dropped state stashes the peer and mints;
        // the fresh mint then connects as an opener would.
        let effects = session.on(Event::Command(Command::Connect("#newPeer".into())));
        let fresh = mint_gen(&effects);
        let effects = session.on(Event::Minted {
            gen: fresh,
            payload: "aaaOwn".into(),
        });
        let (_, peer, greet) = connect_of(&effects);
        assert_eq!(peer, "newPeer");
        assert!(greet, "\"aaaOwn\" greets \"newPeer\" lexically");
    }
}
