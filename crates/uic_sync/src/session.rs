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

/// The `<pair-panel>` view: what a host mirrors onto the mounted panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelState {
    pub mode: PanelMode,
    pub link: String,
    pub status: String,
    pub connected: Option<bool>,
    pub reset_label: String,
}

/// A panel intent the host reads off the component and feeds back in as
/// [`Event::Command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Start a fresh invite (the reset button).
    Renew,
    /// Connect to a pasted invite — a link or a bare pairing code.
    Connect(String),
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

/// The pairing session: create an invite, wait for the peer (pairing is a
/// mutual exchange — ADR 0028), connect, stand ready to renew, and answer
/// repair rounds (ADR 0032) with a fresh wire that replaces the old one
/// only after it opened.
pub struct Session {
    page: String,
    opener: Option<String>,
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
        let mut session = Session {
            page,
            opener,
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
        // invite and opens the other's. An opener already holds the peer's
        // payload from the link; an inviter waits for the pasted token.
        let reply_to = self.opener.as_deref().map(pair::reply_digest);
        let answering = self.opener.is_some();
        let link = pair::invite_link(&self.page, &payload, reply_to.as_deref());
        let invite = PanelState {
            mode: PanelMode::Invite,
            link,
            status: if answering {
                "opened their invite — send yours back so they connect too".into()
            } else {
                "the card and the code carry the same invite — you connect when a peer answers"
                    .into()
            },
            connected: None,
            reset_label: "start over".into(),
        };
        let mut effects = vec![self.present(invite)];
        if let Some(peer) = self.opener.take() {
            effects.push(self.present_status("connecting…".into()));
            self.phase = Phase::Connecting;
            effects.push(Effect::Connect {
                gen,
                // Exactly one side greets: the lexically smaller payload
                // (the plain rule of ADR 0013) — a fresh pairing has no
                // canonical-state holder yet.
                greet: payload < peer,
                peer,
            });
        } else {
            self.phase = Phase::Inviting;
        }
        effects
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
            reset_label: "try again".into(),
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
        self.phase = Phase::Standing;
        vec![self.present(PanelState {
            mode: PanelMode::Connected,
            status: "paired — one list, two ends".into(),
            connected: Some(true),
            reset_label: "invite somebody else".into(),
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
        self.phase = Phase::Down;
        vec![self.present(PanelState {
            mode: PanelMode::Failed,
            status: format!("pairing failed: {error}"),
            reset_label: "start a new pairing".into(),
            ..PanelState::default()
        })]
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
            reset_label: "invite somebody else".into(),
            ..PanelState::default()
        })]
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        match command {
            Command::Renew => {
                self.opener = None;
                self.recycle()
            }
            Command::Connect(text) => {
                let peer = pair::link_payload(&text);
                if let (Phase::Inviting, Some((gen, payload))) = (&self.phase, self.current.clone())
                {
                    // The presented invite's own swap is fresh — connect it
                    // directly, the mutual exchange completing.
                    self.phase = Phase::Connecting;
                    return vec![
                        self.present_status("connecting…".into()),
                        Effect::Connect {
                            gen,
                            greet: payload < peer,
                            peer,
                        },
                    ];
                }
                // Standing or down: the next mint consumes the peer like an
                // opened link would.
                self.opener = Some(peer);
                self.recycle()
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
    fn a_fresh_session_mints_and_presents_the_invite() {
        let (mut session, effects) = Session::start("https://host/p2p/".into(), None);
        let effects = minted(&mut session, &effects, "bbb");
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Invite);
        assert_eq!(view.link, "https://host/p2p/#bbb");
        assert!(view.status.contains("you connect when a peer answers"));
        assert_eq!(view.reset_label, "start over");
        // No opener: the session waits — no connect yet.
        assert!(!effects.iter().any(|e| matches!(e, Effect::Connect { .. })));
    }

    #[test]
    fn an_opener_connects_at_once_and_its_link_carries_the_reply_digest() {
        let (mut session, effects) =
            Session::start("https://host/p2p/".into(), Some("peerPayload".into()));
        let effects = minted(&mut session, &effects, "own");
        let (gen, peer, greet) = connect_of(&effects);
        assert_eq!(peer, "peerPayload");
        // Lexical greet: "own" < "peerPayload".
        assert!(greet);
        assert_eq!(gen, mint_gen(&[Effect::Mint { gen }]));
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Invite);
        assert_eq!(view.status, "connecting…");
        assert_eq!(
            view.link,
            format!(
                "https://host/p2p/#own.{}",
                crate::pair::reply_digest("peerPayload")
            )
        );
    }

    #[test]
    fn the_pasted_token_connects_the_presented_invite() {
        let (mut session, effects) = Session::start("https://host/p2p/".into(), None);
        let _ = minted(&mut session, &effects, "zzz");
        let gen = {
            // The invite stands; a pasted link connects the same swap.
            let effects = session.on(Event::Command(Command::Connect(
                "https://host/p2p/#aaa.12345678".into(),
            )));
            let (gen, peer, greet) = connect_of(&effects);
            assert_eq!(peer, "aaa", "the link grammar reduces to the payload");
            assert!(!greet, "\"zzz\" is not the lexically smaller payload");
            assert_eq!(presented(&effects).status, "connecting…");
            gen
        };
        let effects = session.on(Event::Connected { gen });
        let view = presented(&effects);
        assert_eq!(view.mode, PanelMode::Connected);
        assert_eq!(view.connected, Some(true));
        assert_eq!(view.link, "", "the consumed invite leaves the card");
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
        assert_eq!(view.reset_label, "try again");
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
        assert_eq!(view.reset_label, "invite somebody else");

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
