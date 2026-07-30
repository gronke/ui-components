//! The terminal's WebRTC peer (ADR 0028): `web/pair.ts`'s symmetric swap
//! in Rust — one negotiated data channel, candidates gathered completely
//! before encoding, the peer's answer synthesized locally from its compact
//! payload. Pairing is a mutual exchange with no third party: each side
//! sends its token, opens the other's, and connects (ADR 0028).
//!
//! `drive_session` executes `uic_sync::session`'s pure pairing machine
//! with these swaps: the machine decides, this loop performs.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc, oneshot};
use uic_sync::pair::{
    build_sdp, decode_ctrl, decode_payload, encode_ctrl, encode_payload, parse_sdp, Compact, Setup,
};
use uic_sync::session::{Command, Effect, Event as SessionEvent, Gen, PanelState, Session};
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice::mdns::MulticastDnsMode;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

/// The same plain message pair.ts rejects unreachable peers with.
const UNREACHABLE: &str = "the peers could not reach each other — on one network, check \
     that devices may talk to each other; across networks this demo ships no TURN relay";

type PairResult<T> = Result<T, String>;

/// The state-bridge endpoints a wire pumps: state snapshots in, control
/// frames split off beside them, outbound text onto the channel, and the
/// latest snapshot for the greet. One session shares a bridge across every
/// wire it ever runs — a handover's fresh wire pumps the same one.
#[derive(Clone)]
struct Bridge {
    inbound: mpsc::UnboundedSender<String>,
    ctrl: mpsc::UnboundedSender<String>,
    outbound: broadcast::Sender<String>,
    latest: Arc<Mutex<String>>,
}

/// One side of the symmetric swap: the offer is made and gathered, the
/// compact payload ready to travel.
struct Swap {
    pc: Arc<RTCPeerConnection>,
    channel: Arc<RTCDataChannel>,
    compact: Compact,
    payload: String,
}

impl Swap {
    /// Gathers candidates completely (no trickle), then reduces the local
    /// offer to the compact payload — the terminal twin of pair.ts's
    /// `swap()`.
    ///
    /// The terminal runs as an ICE-LITE agent: the browser's swap always
    /// stays ICE-controlling (it applies a synthesized answer), and
    /// webrtc-rs does not resolve two controlling agents, so the terminal
    /// must take the controlled side — lite is the one arrangement that
    /// yields it. Lite gathers host candidates only (no srflx), so the
    /// terminal peers on a shared network; crossing NATs is out of scope
    /// (ADR 0028). Loopback candidates ride along so a terminal and a
    /// browser on one host pair directly.
    async fn new() -> PairResult<Swap> {
        Self::build(true).await
    }

    /// The full (controlling) flavor, for the loopback test's other side —
    /// the terminal's peer is always full in production (the browser).
    #[cfg(test)]
    async fn full() -> PairResult<Swap> {
        Self::build(false).await
    }

    async fn build(lite: bool) -> PairResult<Swap> {
        let mut setting = SettingEngine::default();
        // The default, spelled out: resolve the browser's .local host
        // candidates, advertise plain IPs ourselves.
        setting.set_ice_multicast_dns_mode(MulticastDnsMode::QueryOnly);
        setting.set_include_loopback_candidate(true);
        setting.set_lite(lite);
        let api = APIBuilder::new()
            .with_media_engine(MediaEngine::default())
            .with_setting_engine(setting)
            .build();
        // A lite agent gathers host candidates only — STUN would give it
        // nothing — so the config stays empty; its host candidate is what
        // the full peer probes.
        let config = RTCConfiguration::default();
        let pc = Arc::new(
            api.new_peer_connection(config)
                .await
                .map_err(|err| err.to_string())?,
        );
        // The negotiated channel, stream 0 on either end, created BEFORE
        // the offer so the application section exists — the browser's
        // `{ negotiated: true, id: 0 }` is one folded field here.
        let channel = pc
            .create_data_channel(
                "uic-sync",
                Some(RTCDataChannelInit {
                    negotiated: Some(0),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|err| err.to_string())?;
        let offer = pc.create_offer(None).await.map_err(|err| err.to_string())?;
        pc.set_local_description(offer)
            .await
            .map_err(|err| err.to_string())?;
        let mut gathered = pc.gathering_complete_promise().await;
        let _ = gathered.recv().await;
        let local = pc
            .local_description()
            .await
            .ok_or("no local description after gathering")?;
        let compact = parse_sdp(&local.sdp).map_err(|err| err.to_string())?;
        let payload = encode_payload(&compact);
        Ok(Swap {
            pc,
            channel,
            compact,
            payload,
        })
    }

    /// Applies the peer's payload — its ANSWER synthesized locally with
    /// the fingerprint-derived DTLS role — and resolves once the channel
    /// opens, wired into the bridge: state snapshots land in the terminal
    /// loop via `inbound`, `uicc1.` control frames (ADR 0032) split off to
    /// `ctrl`, outbound text pumps onto the channel, and exactly one side
    /// greets — the caller says which (the lexically smaller payload for a
    /// plain pairing; the state-holding side, always, on a handover).
    async fn connect(
        &self,
        peer_payload: &str,
        greet: bool,
        bridge: Bridge,
        closed: Arc<dyn Fn() + Send + Sync>,
    ) -> PairResult<()> {
        let Bridge {
            inbound,
            ctrl,
            outbound,
            latest,
        } = bridge;
        let peer = decode_payload(peer_payload).map_err(|err| err.to_string())?;
        if peer.s != Setup::ActPass {
            return Err("uic-sync pair: swap expects the peer's own swap payload".into());
        }
        if peer.f == self.compact.f {
            return Err(
                "uic-sync pair: that is this side's own payload — send it to the peer and open theirs"
                    .into(),
            );
        }
        let role = if peer.f < self.compact.f {
            Setup::Active
        } else {
            Setup::Passive
        };

        // The open/fail race: whoever fires first takes the slot; a
        // failure after the open reports through `closed` instead.
        let (open_tx, open_rx) = oneshot::channel::<PairResult<()>>();
        let pending = Arc::new(Mutex::new(Some(open_tx)));

        let channel = self.channel.clone();
        {
            let inbound = inbound.clone();
            let ctrl = ctrl.clone();
            self.channel
                .on_message(Box::new(move |message: DataChannelMessage| {
                    let inbound = inbound.clone();
                    let ctrl = ctrl.clone();
                    Box::pin(async move {
                        let text = String::from_utf8_lossy(&message.data).to_string();
                        if text.starts_with(uic_sync::pair::CTRL_PREFIX) {
                            let _ = ctrl.send(text);
                        } else {
                            let _ = inbound.send(text);
                        }
                    })
                }));
        }
        {
            let pending = pending.clone();
            let channel = channel.clone();
            let latest = latest.clone();
            let outbound = outbound.clone();
            self.channel.on_open(Box::new(move || {
                let pending = pending.clone();
                let channel = channel.clone();
                let latest = latest.clone();
                let mut updates = outbound.subscribe();
                Box::pin(async move {
                    if greet {
                        let hello = latest.lock().expect("latest state").clone();
                        let _ = channel.send_text(hello).await;
                    }
                    // The outbound pump lives as long as the channel does.
                    let pump = channel.clone();
                    tokio::spawn(async move {
                        while let Ok(state) = updates.recv().await {
                            if pump.send_text(state).await.is_err() {
                                break;
                            }
                        }
                    });
                    if let Some(sender) = pending.lock().expect("pending slot").take() {
                        let _ = sender.send(Ok(()));
                    }
                })
            }));
        }
        {
            let pending = pending.clone();
            let closed = closed.clone();
            self.pc.on_peer_connection_state_change(Box::new(
                move |state: RTCPeerConnectionState| {
                    let pending = pending.clone();
                    let closed = closed.clone();
                    Box::pin(async move {
                        if matches!(
                            state,
                            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
                        ) {
                            match pending.lock().expect("pending slot").take() {
                                Some(sender) => {
                                    let _ =
                                        sender.send(Err(format!("uic-sync pair: {UNREACHABLE}")));
                                }
                                // The user-facing copy for an unsolicited
                                // close lives with the session machine; the
                                // transport only reports that it happened.
                                None => closed(),
                            }
                        }
                    })
                },
            ));
        }

        // UIC_LIT_DEMO_ICE_DEBUG traces the connectivity for a pairing that
        // will not come up — the honest NAT diagnosis the demo cannot fix.
        if std::env::var_os("UIC_LIT_DEMO_ICE_DEBUG").is_some() {
            eprintln!(
                "[ice] role={}, the peer offers these candidates to reach:",
                role.as_str()
            );
            for (address, port) in &peer.c {
                eprintln!("[ice]   {address} {port}");
            }
            self.pc.on_ice_connection_state_change(Box::new(|state| {
                eprintln!("[ice] connection state: {state}");
                Box::pin(async {})
            }));
        }
        let answer =
            RTCSessionDescription::answer(build_sdp(&peer, role)).map_err(|err| err.to_string())?;
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(|err| err.to_string())?;
        open_rx.await.map_err(|_| "pairing abandoned".to_string())?
    }

    /// Tears the connection down — the terminal calls it before a renew so
    /// the old wire's outbound pump stops and no stale peer keeps mirroring.
    async fn close(&self) {
        let _ = self.pc.close().await;
    }
}

/// Everything the session loop is wired to: the panel view it presents,
/// the commands it obeys, the state pumps its wires share, and the slot
/// carrying the live wire's endpoints for the navbar.
pub(crate) struct Wiring {
    pub panel_state: Arc<Mutex<PanelState>>,
    pub commands: mpsc::UnboundedReceiver<Command>,
    pub inbound: mpsc::UnboundedSender<String>,
    pub outbound: broadcast::Sender<String>,
    pub latest: Arc<Mutex<String>>,
    pub endpoints: Arc<Mutex<Option<String>>>,
}

/// The wire's real route once ICE nominated it — the relay-free story the
/// navbar tells: this peer's address ⇄ the other side's.
async fn selected_pair(swap: &Swap) -> Option<String> {
    let pair = swap
        .pc
        .sctp()
        .transport()
        .ice_transport()
        .get_selected_candidate_pair()
        .await?;
    Some(format!(
        "{}:{} ⇄ {}:{}",
        pair.local.address, pair.local.port, pair.remote.address, pair.remote.port
    ))
}

/// Drives a pairing [`Session`] with this module's swaps: the pure machine
/// decides, this loop performs — mints, closes, control frames and panel
/// views where the effects say. Connects are SPAWNED, not awaited: the wait
/// for both sides to apply each other's payload can be long, and commands
/// (a disconnect, a conflict-modal accept) must not queue behind it. Each
/// completion reports its wire's [`Gen`]; superseded wires' completions and
/// close notices die as stale events inside the machine.
pub(crate) async fn drive_session(page: String, opener: Option<String>, wiring: Wiring) {
    let Wiring {
        panel_state,
        mut commands,
        inbound,
        outbound,
        latest,
        endpoints,
    } = wiring;
    // Control frames (ADR 0032) split off the data channel into this pair;
    // unsolicited transport closes report their wire's gen through the
    // other. One bridge serves the session's every wire — a handover's
    // fresh wire pumps the same channels.
    let (ctrl_tx, mut ctrl_rx) = mpsc::unbounded_channel::<String>();
    let (closed_tx, mut closed_rx) = mpsc::unbounded_channel::<Gen>();
    // A spawned connect reports here: Ok carries the nominated route for the
    // navbar, Err the failure. Gen-tagged so the machine drops stale ones.
    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<(Gen, PairResult<Option<String>>)>();
    let bridge = Bridge {
        inbound,
        ctrl: ctrl_tx,
        outbound: outbound.clone(),
        latest,
    };
    let mut swaps: HashMap<Gen, Arc<Swap>> = HashMap::new();
    let (mut session, effects) = Session::start(page, opener);
    let mut queue: VecDeque<Effect> = effects.into();
    // A connect started here; while it stands, a one-second ticker feeds the
    // machine the elapsed seconds so the pairing screen counts up (the pure
    // machine has no clock of its own). Cleared when the connect resolves.
    let mut connecting_since: Option<Instant> = None;
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        while let Some(effect) = queue.pop_front() {
            match effect {
                Effect::Present(view) => *panel_state.lock().expect("panel state") = view,
                Effect::Mint { gen } => {
                    let event = match Swap::new().await {
                        Ok(swap) => {
                            let payload = swap.payload.clone();
                            swaps.insert(gen, Arc::new(swap));
                            SessionEvent::Minted { gen, payload }
                        }
                        Err(error) => SessionEvent::MintFailed { gen, error },
                    };
                    queue.extend(session.on(event));
                }
                Effect::Connect { gen, peer, greet } => {
                    let Some(swap) = swaps.get(&gen).cloned() else {
                        continue;
                    };
                    // Start the connecting clock; the machine shows it only
                    // while it stays in its connecting phase.
                    connecting_since = Some(Instant::now());
                    let notice = closed_tx.clone();
                    let closed: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                        let _ = notice.send(gen);
                    });
                    let bridge = bridge.clone();
                    let done = done_tx.clone();
                    // The connect runs off the loop; its completion arrives
                    // as an event. A Close on this gen tears the swap down
                    // meanwhile, so a superseded connect errors out and its
                    // stale-gen event is a no-op.
                    tokio::spawn(async move {
                        let outcome = match swap.connect(&peer, greet, bridge, closed).await {
                            Ok(()) => Ok(selected_pair(&swap).await),
                            Err(error) => Err(error),
                        };
                        let _ = done.send((gen, outcome));
                    });
                }
                Effect::SendCtrl(ctrl) => {
                    let _ = bridge.outbound.send(encode_ctrl(&ctrl));
                }
                Effect::Close { gen } => {
                    if let Some(swap) = swaps.remove(&gen) {
                        // The in-flight connect (if any) holds its own Arc;
                        // closing the pc makes it error and report stale.
                        swap.close().await;
                    }
                }
            }
        }
        // Effects drained: wait for the next external event.
        let event = tokio::select! {
            command = commands.recv() => match command {
                Some(command) => SessionEvent::Command(command),
                // The terminal loop ended; the session goes with it.
                None => return,
            },
            frame = ctrl_rx.recv() => {
                let Some(frame) = frame else { continue };
                let Some(message) = decode_ctrl(&frame) else { continue };
                if message.t != "repair" {
                    continue;
                }
                let Some(peer) = message.payload else { continue };
                SessionEvent::Repair { peer }
            }
            done = done_rx.recv() => {
                let Some((gen, outcome)) = done else { continue };
                // The connect resolved — stop the clock (whichever way it went).
                connecting_since = None;
                match outcome {
                    Ok(route) => {
                        // The navbar shows the route only while connected
                        // (the loop filters on mode), so a stale success
                        // writing here is harmless — the next Present wins.
                        *endpoints.lock().expect("endpoints slot") = route;
                        SessionEvent::Connected { gen }
                    }
                    Err(error) => SessionEvent::ConnectFailed { gen, error },
                }
            }
            gen = closed_rx.recv() => {
                let Some(gen) = gen else { continue };
                SessionEvent::Closed { gen }
            }
            _ = ticker.tick() => {
                // The pairing screen counts the seconds while a connect stands;
                // idle otherwise (the immediate first tick and any tick between
                // connects carries no clock).
                match connecting_since {
                    Some(since) => SessionEvent::Command(Command::Tick {
                        secs: since.elapsed().as_secs(),
                    }),
                    None => continue,
                }
            }
        };
        queue.extend(session.on(event));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use uic_sync::pair::Compact;

    fn quiet_closed() -> Arc<dyn Fn() + Send + Sync> {
        Arc::new(|| {})
    }

    /// The whole pairing (both connects) must land inside this deadline: ICE
    /// on a shared CI runner can stall without ever reaching Failed, and a
    /// `connect` that never resolves would wedge the suite until the job
    /// timeout kills it. A stalled attempt fails fast instead, and one fresh
    /// retry absorbs the rare transient stall — a genuine regression still
    /// fails both attempts.
    const PAIR_DEADLINE: Duration = Duration::from_secs(60);

    #[tokio::test(flavor = "multi_thread")]
    async fn two_swaps_pair_over_loopback_and_one_greets() {
        for attempt in 1..=2 {
            match tokio::time::timeout(PAIR_DEADLINE, loopback_pairs()).await {
                Ok(()) => return,
                Err(_) if attempt < 2 => continue,
                Err(_) => panic!("the loopback pairing stalled past {PAIR_DEADLINE:?}, twice"),
            }
        }
    }

    async fn loopback_pairs() {
        // Production shape: one lite (controlled) side — the terminal — and
        // one full (controlling) side, the browser's stand-in. Two lite or
        // two full peers cannot pair (webrtc-rs never resolves a same-role
        // ICE conflict); the terminal is always the lite one.
        let a = Swap::new().await.expect("swap a");
        let b = Swap::full().await.expect("swap b");
        let a_payload = a.payload.clone();
        let b_payload = b.payload.clone();

        let (a_in_tx, mut a_in_rx) = mpsc::unbounded_channel();
        let (b_in_tx, mut b_in_rx) = mpsc::unbounded_channel();
        let (a_ctrl_tx, mut a_ctrl_rx) = mpsc::unbounded_channel();
        let (b_ctrl_tx, _b_ctrl_rx) = mpsc::unbounded_channel();
        let (a_out, _) = broadcast::channel(8);
        let (b_out, _) = broadcast::channel(8);
        let a_latest = Arc::new(Mutex::new(String::from("hello-from-a")));
        let b_latest = Arc::new(Mutex::new(String::from("hello-from-b")));

        // The swap completes only once BOTH sides applied each other's
        // payload — the connects must run concurrently. The greet flag is
        // the caller's: here the plain lexical rule (ADR 0013).
        let (ra, rb) = tokio::join!(
            a.connect(
                &b_payload,
                a_payload < b_payload,
                Bridge {
                    inbound: a_in_tx,
                    ctrl: a_ctrl_tx,
                    outbound: a_out.clone(),
                    latest: a_latest,
                },
                quiet_closed()
            ),
            b.connect(
                &a_payload,
                b_payload < a_payload,
                Bridge {
                    inbound: b_in_tx,
                    ctrl: b_ctrl_tx,
                    outbound: b_out.clone(),
                    latest: b_latest,
                },
                quiet_closed()
            ),
        );
        ra.expect("side a opens");
        rb.expect("side b opens");

        // The nominated route is readable once the wire stands — the
        // navbar's address line rides this.
        let route = selected_pair(&a).await;
        assert!(
            route.as_deref().is_some_and(|r| r.contains(" ⇄ ")),
            "the selected pair reports: {route:?}"
        );

        // Exactly one side greets: the lexically smaller payload announces
        // its snapshot on open (ADR 0013).
        if a_payload < b_payload {
            assert_eq!(recv_soon(&mut b_in_rx).await, "hello-from-a");
        } else {
            assert_eq!(recv_soon(&mut a_in_rx).await, "hello-from-b");
        }

        // The outbound pumps carry snapshots both ways.
        a_out.send("ping".into()).expect("a's pump listens");
        assert_eq!(recv_soon(&mut b_in_rx).await, "ping");
        b_out.send("pong".into()).expect("b's pump listens");
        assert_eq!(recv_soon(&mut a_in_rx).await, "pong");

        // Control frames split off the state stream (ADR 0032): a `uicc1.`
        // frame from b lands on a's ctrl channel, never its inbound.
        let frame = uic_sync::pair::encode_ctrl(&uic_sync::pair::Ctrl {
            t: "repair".into(),
            payload: Some("freshTabPayload".into()),
        });
        b_out.send(frame.clone()).expect("b's pump listens");
        assert_eq!(recv_soon(&mut a_ctrl_rx).await, frame);
        a_out.send("state-after".into()).expect("a's pump listens");
        assert_eq!(
            recv_soon(&mut b_in_rx).await,
            "state-after",
            "state keeps flowing beside the control plane"
        );
        assert!(
            a_in_rx.try_recv().is_err(),
            "the control frame never reached a's state inbound"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_handover_re_pairs_and_the_state_holder_greets() {
        // The same stall guard as the plain loopback: real ICE on a shared
        // CI runner can wedge without reaching Failed.
        for attempt in 1..=2 {
            match tokio::time::timeout(PAIR_DEADLINE, handover_pairs()).await {
                Ok(()) => return,
                Err(_) if attempt < 2 => continue,
                Err(_) => panic!("the handover pairing stalled past {PAIR_DEADLINE:?}, twice"),
            }
        }
    }

    async fn handover_pairs() {
        // The handover shape (ADR 0032): fresh swaps re-pair while the old
        // wire still stands, and the greet is FORCED — the state-holding
        // side (the terminal) announces its canonical snapshot whatever the
        // payload order says, and the fresh tab stays quiet.
        let terminal = Swap::new().await.expect("fresh terminal side");
        let tab = Swap::full().await.expect("fresh tab side");
        let terminal_payload = terminal.payload.clone();
        let tab_payload = tab.payload.clone();

        let (t_in_tx, _t_in_rx) = mpsc::unbounded_channel();
        let (n_in_tx, mut n_in_rx) = mpsc::unbounded_channel();
        let (t_ctrl_tx, _t_ctrl_rx) = mpsc::unbounded_channel();
        let (n_ctrl_tx, _n_ctrl_rx) = mpsc::unbounded_channel();
        let (t_out, _) = broadcast::channel(8);
        let (n_out, _) = broadcast::channel(8);
        let t_latest = Arc::new(Mutex::new(String::from("the-canonical-list")));
        let n_latest = Arc::new(Mutex::new(String::from("an-empty-fresh-tab")));

        let (rt, rn) = tokio::join!(
            terminal.connect(
                &tab_payload,
                true,
                Bridge {
                    inbound: t_in_tx,
                    ctrl: t_ctrl_tx,
                    outbound: t_out.clone(),
                    latest: t_latest,
                },
                quiet_closed()
            ),
            tab.connect(
                &terminal_payload,
                false,
                Bridge {
                    inbound: n_in_tx,
                    ctrl: n_ctrl_tx,
                    outbound: n_out.clone(),
                    latest: n_latest,
                },
                quiet_closed()
            ),
        );
        rt.expect("the terminal side opens");
        rn.expect("the tab side opens");

        // The forced greeter's snapshot arrives; the fresh tab never greets,
        // so nothing else lands on the terminal's inbound.
        assert_eq!(recv_soon(&mut n_in_rx).await, "the-canonical-list");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_closed_swap_refuses_to_connect() {
        // close() tears the peer connection down for good — a later connect
        // reports plainly instead of hanging. (The cross-peer drop NOTICE
        // rides ICE timeouts too slow for a test deadline; the deterministic
        // local property is what pins close() here.)
        let side = Swap::new().await.expect("swap");
        let peer = Swap::new().await.expect("peer swap");
        side.close().await;
        let (tx, _rx) = mpsc::unbounded_channel();
        let (ctrl, _ctrl_rx) = mpsc::unbounded_channel();
        let (out, _) = broadcast::channel(1);
        let latest = Arc::new(Mutex::new(String::new()));
        let bridge = Bridge {
            inbound: tx,
            ctrl,
            outbound: out,
            latest,
        };
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            side.connect(&peer.payload, false, bridge, quiet_closed()),
        )
        .await
        .expect("a closed swap answers fast");
        result.expect_err("a closed swap must not pair");
    }

    async fn recv_soon(rx: &mut mpsc::UnboundedReceiver<String>) -> String {
        tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("a message within the deadline")
            .expect("the channel stays open")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_rejects_the_own_payload() {
        let side = Swap::new().await.expect("swap");
        let own = side.payload.clone();
        let (tx, _rx) = mpsc::unbounded_channel();
        let (ctrl, _ctrl_rx) = mpsc::unbounded_channel();
        let (out, _) = broadcast::channel(1);
        let latest = Arc::new(Mutex::new(String::new()));
        let bridge = Bridge {
            inbound: tx,
            ctrl,
            outbound: out,
            latest,
        };
        let err = side
            .connect(&own, false, bridge, quiet_closed())
            .await
            .expect_err("own payload must not pair");
        assert!(err.contains("own payload"), "{err}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_rejects_answer_role_payloads() {
        let side = Swap::new().await.expect("swap");
        let answer = uic_sync::pair::encode_payload(&Compact {
            u: "u".into(),
            p: "p".into(),
            f: "00:11".into(),
            s: Setup::Active,
            c: vec![("127.0.0.1".into(), 1)],
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        let (ctrl, _ctrl_rx) = mpsc::unbounded_channel();
        let (out, _) = broadcast::channel(1);
        let latest = Arc::new(Mutex::new(String::new()));
        let bridge = Bridge {
            inbound: tx,
            ctrl,
            outbound: out,
            latest,
        };
        let err = side
            .connect(&answer, false, bridge, quiet_closed())
            .await
            .expect_err("an answer payload must not pair");
        assert!(err.contains("swap expects"), "{err}");
    }
}
