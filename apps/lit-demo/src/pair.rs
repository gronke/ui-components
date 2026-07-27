//! The terminal's WebRTC peer (ADR 0028): `web/pair.ts`'s symmetric swap
//! in Rust — one negotiated data channel, candidates gathered completely
//! before encoding, the peer's answer synthesized locally from its compact
//! payload. Pairing is a mutual exchange with no third party: each side
//! sends its token, opens the other's, and connects (ADR 0031).

use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc, oneshot};
use uic_sync::pair::{build_sdp, decode_payload, encode_payload, parse_sdp, Compact};
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
pub const UNREACHABLE: &str = "the peers could not reach each other — on one network, check \
     that devices may talk to each other; across networks this demo ships no TURN relay";

pub type PairResult<T> = Result<T, String>;

/// One side of the symmetric swap: the offer is made and gathered, the
/// compact payload ready to travel.
pub struct Swap {
    pc: Arc<RTCPeerConnection>,
    channel: Arc<RTCDataChannel>,
    pub compact: Compact,
    pub payload: String,
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
    pub async fn new() -> PairResult<Swap> {
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
    /// opens, wired into the bridge: inbound snapshots land in the
    /// terminal loop, outbound ones pump onto the channel, and exactly one
    /// side greets (the lexically smaller payload, ADR 0024).
    pub async fn connect(
        &self,
        peer_payload: &str,
        inbound: mpsc::UnboundedSender<String>,
        outbound: broadcast::Sender<String>,
        latest: Arc<Mutex<String>>,
        closed: Arc<dyn Fn(String) + Send + Sync>,
    ) -> PairResult<()> {
        let peer = decode_payload(peer_payload).map_err(|err| err.to_string())?;
        if peer.s != "actpass" {
            return Err("uic-sync pair: swap expects the peer's own swap payload".into());
        }
        if peer.f == self.compact.f {
            return Err(
                "uic-sync pair: that is this side's own payload — send it to the peer and open theirs"
                    .into(),
            );
        }
        let role = if peer.f < self.compact.f {
            "active"
        } else {
            "passive"
        };
        let greet = self.payload.as_str() < peer_payload;

        // The open/fail race: whoever fires first takes the slot; a
        // failure after the open reports through `closed` instead.
        let (open_tx, open_rx) = oneshot::channel::<PairResult<()>>();
        let pending = Arc::new(Mutex::new(Some(open_tx)));

        let channel = self.channel.clone();
        {
            let inbound = inbound.clone();
            self.channel
                .on_message(Box::new(move |message: DataChannelMessage| {
                    let inbound = inbound.clone();
                    Box::pin(async move {
                        let text = String::from_utf8_lossy(&message.data).to_string();
                        let _ = inbound.send(text);
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
                                None => {
                                    closed("connection closed — restart to pair again".to_string())
                                }
                            }
                        }
                    })
                },
            ));
        }

        // UIC_LIT_DEMO_ICE_DEBUG traces the connectivity for a pairing that
        // will not come up — the honest NAT diagnosis the demo cannot fix.
        if std::env::var_os("UIC_LIT_DEMO_ICE_DEBUG").is_some() {
            eprintln!("[ice] role={role}, the peer offers these candidates to reach:");
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
    pub async fn close(&self) {
        let _ = self.pc.close().await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use uic_sync::pair::Compact;

    fn quiet_closed() -> Arc<dyn Fn(String) + Send + Sync> {
        Arc::new(|_| {})
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
        let (a_out, _) = broadcast::channel(8);
        let (b_out, _) = broadcast::channel(8);
        let a_latest = Arc::new(Mutex::new(String::from("hello-from-a")));
        let b_latest = Arc::new(Mutex::new(String::from("hello-from-b")));

        // The swap completes only once BOTH sides applied each other's
        // payload — the connects must run concurrently.
        let (ra, rb) = tokio::join!(
            a.connect(&b_payload, a_in_tx, a_out.clone(), a_latest, quiet_closed()),
            b.connect(&a_payload, b_in_tx, b_out.clone(), b_latest, quiet_closed()),
        );
        ra.expect("side a opens");
        rb.expect("side b opens");

        // Exactly one side greets: the lexically smaller payload announces
        // its snapshot on open (ADR 0024).
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
        let (out, _) = broadcast::channel(1);
        let latest = Arc::new(Mutex::new(String::new()));
        let err = side
            .connect(&own, tx, out, latest, quiet_closed())
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
            s: "active".into(),
            c: vec![("127.0.0.1".into(), 1)],
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        let (out, _) = broadcast::channel(1);
        let latest = Arc::new(Mutex::new(String::new()));
        let err = side
            .connect(&answer, tx, out, latest, quiet_closed())
            .await
            .expect_err("an answer payload must not pair");
        assert!(err.contains("swap expects"), "{err}");
    }
}
