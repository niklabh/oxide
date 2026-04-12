//! Host-side WebRTC: peer connections, data channels, media tracks, and signaling.
//!
//! Guests call [`register_rtc_functions`] imports from the `oxide` module to create
//! peer-to-peer connections with SDP offer/answer exchange, ICE candidate trickle,
//! data channel messaging, and media track attachment. A lightweight HTTP-based
//! signaling client is included for bootstrapping connections.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::runtime::Runtime;
use wasmtime::{Caller, Linker};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use crate::capabilities::{
    console_log, read_guest_bytes, read_guest_string, write_guest_bytes, ConsoleLevel, HostState,
};

/// Incoming message queued by a data channel's on_message callback.
struct IncomingMessage {
    channel_id: u32,
    is_binary: bool,
    data: Vec<u8>,
}

/// Metadata about a remotely-created data channel that the guest hasn't accepted yet.
struct PendingChannel {
    channel_id: u32,
    label: String,
}

/// Metadata about a remote media track received via `on_track`.
struct PendingTrack {
    kind: u32,
    id: String,
    stream_id: String,
}

/// Per-peer state: the connection object plus event queues polled by the guest.
struct PeerState {
    conn: Arc<RTCPeerConnection>,
    data_channels: Arc<Mutex<HashMap<u32, Arc<RTCDataChannel>>>>,
    incoming_messages: Arc<Mutex<VecDeque<IncomingMessage>>>,
    pending_channels: Arc<Mutex<VecDeque<PendingChannel>>>,
    pending_tracks: Arc<Mutex<VecDeque<PendingTrack>>>,
    ice_candidates: Arc<Mutex<VecDeque<String>>>,
    connection_state: Arc<Mutex<u32>>,
    next_channel_id: u32,
}

/// HTTP-based signaling session for bootstrapping peer connections.
struct SignalingSession {
    base_url: String,
    room: String,
    client: reqwest::blocking::Client,
}

/// All RTC state for a tab. Lazily initialised on first `api_rtc_*` call.
pub struct RtcState {
    runtime: Runtime,
    peers: HashMap<u32, PeerState>,
    next_peer_id: u32,
    signaling: Option<SignalingSession>,
}

/// Connection state constants exposed to guests.
const STATE_NEW: u32 = 0;
const STATE_CONNECTING: u32 = 1;
const STATE_CONNECTED: u32 = 2;
const STATE_DISCONNECTED: u32 = 3;
const STATE_FAILED: u32 = 4;
const STATE_CLOSED: u32 = 5;

fn map_connection_state(s: RTCPeerConnectionState) -> u32 {
    match s {
        RTCPeerConnectionState::New => STATE_NEW,
        RTCPeerConnectionState::Connecting => STATE_CONNECTING,
        RTCPeerConnectionState::Connected => STATE_CONNECTED,
        RTCPeerConnectionState::Disconnected => STATE_DISCONNECTED,
        RTCPeerConnectionState::Failed => STATE_FAILED,
        RTCPeerConnectionState::Closed => STATE_CLOSED,
        _ => STATE_NEW,
    }
}

impl RtcState {
    pub fn new() -> Option<Self> {
        let runtime = Runtime::new().ok()?;
        Some(Self {
            runtime,
            peers: HashMap::new(),
            next_peer_id: 1,
            signaling: None,
        })
    }

    fn alloc_peer_id(&mut self) -> u32 {
        let id = self.next_peer_id;
        self.next_peer_id = self.next_peer_id.wrapping_add(1).max(1);
        id
    }

    fn create_peer(&mut self, stun_urls: Vec<String>) -> Result<u32> {
        let config = RTCConfiguration {
            ice_servers: if stun_urls.is_empty() {
                vec![RTCIceServer {
                    urls: vec!["stun:stun.l.google.com:19302".to_string()],
                    ..Default::default()
                }]
            } else {
                vec![RTCIceServer {
                    urls: stun_urls,
                    ..Default::default()
                }]
            },
            ..Default::default()
        };

        let peer_id = self.alloc_peer_id();
        let conn = self.runtime.block_on(async {
            let mut me = MediaEngine::default();
            me.register_default_codecs()?;
            let mut registry = Registry::new();
            registry = register_default_interceptors(registry, &mut me)?;
            let api = APIBuilder::new()
                .with_media_engine(me)
                .with_interceptor_registry(registry)
                .build();
            api.new_peer_connection(config).await
        })?;

        let conn = Arc::new(conn);
        let connection_state = Arc::new(Mutex::new(STATE_NEW));
        let ice_candidates: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        let incoming_messages: Arc<Mutex<VecDeque<IncomingMessage>>> =
            Arc::new(Mutex::new(VecDeque::new()));
        let pending_channels: Arc<Mutex<VecDeque<PendingChannel>>> =
            Arc::new(Mutex::new(VecDeque::new()));
        let pending_tracks: Arc<Mutex<VecDeque<PendingTrack>>> =
            Arc::new(Mutex::new(VecDeque::new()));
        let data_channels: Arc<Mutex<HashMap<u32, Arc<RTCDataChannel>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Wire up connection state callback.
        let cs = connection_state.clone();
        conn.on_peer_connection_state_change(Box::new(move |s| {
            *cs.lock().unwrap() = map_connection_state(s);
            Box::pin(async {})
        }));

        // Wire up ICE candidate gathering.
        let ice = ice_candidates.clone();
        conn.on_ice_candidate(Box::new(move |c| {
            if let Some(candidate) = c {
                if let Ok(json) = serde_json::to_string(&candidate.to_json().unwrap_or_default()) {
                    ice.lock().unwrap().push_back(json);
                }
            }
            Box::pin(async {})
        }));

        // Wire up incoming data channels from the remote peer.
        let pending = pending_channels.clone();
        let msgs = incoming_messages.clone();
        let dc_map = data_channels.clone();
        let next_ch = Arc::new(Mutex::new(1u32));
        conn.on_data_channel(Box::new(move |dc| {
            let ch_id = {
                let mut n = next_ch.lock().unwrap();
                let id = *n;
                *n = n.wrapping_add(1).max(1);
                id
            };
            let label = dc.label().to_string();
            pending.lock().unwrap().push_back(PendingChannel {
                channel_id: ch_id,
                label,
            });

            let msgs2 = msgs.clone();
            dc.on_message(Box::new(move |msg: DataChannelMessage| {
                msgs2.lock().unwrap().push_back(IncomingMessage {
                    channel_id: ch_id,
                    is_binary: !msg.is_string,
                    data: msg.data.to_vec(),
                });
                Box::pin(async {})
            }));

            dc_map.lock().unwrap().insert(ch_id, dc);

            Box::pin(async {})
        }));

        // Wire up incoming remote media tracks.
        let tracks = pending_tracks.clone();
        conn.on_track(Box::new(move |track, _receiver, _transceiver| {
            let kind = match track.kind() {
                webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Audio => 0,
                webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video => 1,
                _ => 2,
            };
            tracks.lock().unwrap().push_back(PendingTrack {
                kind,
                id: track.id().to_string(),
                stream_id: track.stream_id().to_string(),
            });
            Box::pin(async {})
        }));

        self.peers.insert(
            peer_id,
            PeerState {
                conn,
                data_channels,
                incoming_messages,
                pending_channels,
                pending_tracks,
                ice_candidates,
                connection_state,
                next_channel_id: 1,
            },
        );

        Ok(peer_id)
    }

    fn close_peer(&mut self, peer_id: u32) -> bool {
        if let Some(peer) = self.peers.remove(&peer_id) {
            let _ = self.runtime.block_on(peer.conn.close());
            true
        } else {
            false
        }
    }

    fn create_offer(&mut self, peer_id: u32) -> Result<String> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;
        let offer = self.runtime.block_on(peer.conn.create_offer(None))?;
        self.runtime
            .block_on(peer.conn.set_local_description(offer.clone()))?;
        Ok(offer.sdp)
    }

    fn create_answer(&mut self, peer_id: u32) -> Result<String> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;
        let answer = self.runtime.block_on(peer.conn.create_answer(None))?;
        self.runtime
            .block_on(peer.conn.set_local_description(answer.clone()))?;
        Ok(answer.sdp)
    }

    fn set_local_description(&mut self, peer_id: u32, sdp: &str, is_offer: bool) -> Result<()> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;
        let desc = if is_offer {
            RTCSessionDescription::offer(sdp.to_string())?
        } else {
            RTCSessionDescription::answer(sdp.to_string())?
        };
        self.runtime
            .block_on(peer.conn.set_local_description(desc))?;
        Ok(())
    }

    fn set_remote_description(&mut self, peer_id: u32, sdp: &str, is_offer: bool) -> Result<()> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;
        let desc = if is_offer {
            RTCSessionDescription::offer(sdp.to_string())?
        } else {
            RTCSessionDescription::answer(sdp.to_string())?
        };
        self.runtime
            .block_on(peer.conn.set_remote_description(desc))?;
        Ok(())
    }

    fn add_ice_candidate(&mut self, peer_id: u32, candidate_json: &str) -> Result<()> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;
        let init: RTCIceCandidateInit = serde_json::from_str(candidate_json)?;
        self.runtime.block_on(peer.conn.add_ice_candidate(init))?;
        Ok(())
    }

    fn connection_state(&self, peer_id: u32) -> u32 {
        self.peers
            .get(&peer_id)
            .map(|p| *p.connection_state.lock().unwrap())
            .unwrap_or(STATE_CLOSED)
    }

    fn poll_ice_candidate(&self, peer_id: u32) -> Option<String> {
        self.peers
            .get(&peer_id)
            .and_then(|p| p.ice_candidates.lock().unwrap().pop_front())
    }

    fn create_data_channel(&mut self, peer_id: u32, label: &str, ordered: bool) -> Result<u32> {
        let peer = self
            .peers
            .get_mut(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;

        let opts = if ordered {
            None
        } else {
            Some(
                webrtc::data_channel::data_channel_init::RTCDataChannelInit {
                    ordered: Some(false),
                    ..Default::default()
                },
            )
        };

        let dc = self.runtime.block_on(async {
            if let Some(opts) = opts {
                peer.conn.create_data_channel(label, Some(opts)).await
            } else {
                peer.conn.create_data_channel(label, None).await
            }
        })?;

        let ch_id = peer.next_channel_id;
        peer.next_channel_id = peer.next_channel_id.wrapping_add(1).max(1);

        let msgs = peer.incoming_messages.clone();
        let ch_id_copy = ch_id;
        dc.on_message(Box::new(move |msg: DataChannelMessage| {
            msgs.lock().unwrap().push_back(IncomingMessage {
                channel_id: ch_id_copy,
                is_binary: !msg.is_string,
                data: msg.data.to_vec(),
            });
            Box::pin(async {})
        }));

        peer.data_channels.lock().unwrap().insert(ch_id, dc);
        Ok(ch_id)
    }

    fn send_data(&self, peer_id: u32, channel_id: u32, data: &[u8], is_binary: bool) -> Result<()> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;
        let dc = peer
            .data_channels
            .lock()
            .unwrap()
            .get(&channel_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown channel"))?;
        if is_binary {
            self.runtime
                .block_on(dc.send(&bytes::Bytes::copy_from_slice(data)))?;
        } else {
            let text = String::from_utf8_lossy(data).to_string();
            self.runtime.block_on(dc.send_text(text))?;
        }
        Ok(())
    }

    fn recv_message(&self, peer_id: u32) -> Option<IncomingMessage> {
        self.peers
            .get(&peer_id)
            .and_then(|p| p.incoming_messages.lock().unwrap().pop_front())
    }

    fn recv_from_channel(&self, peer_id: u32, channel_id: u32) -> Option<IncomingMessage> {
        let peer = self.peers.get(&peer_id)?;
        let mut q = peer.incoming_messages.lock().unwrap();
        if let Some(pos) = q.iter().position(|m| m.channel_id == channel_id) {
            q.remove(pos)
        } else {
            None
        }
    }

    fn poll_new_channel(&self, peer_id: u32) -> Option<PendingChannel> {
        self.peers
            .get(&peer_id)
            .and_then(|p| p.pending_channels.lock().unwrap().pop_front())
    }

    fn poll_track(&self, peer_id: u32) -> Option<PendingTrack> {
        self.peers
            .get(&peer_id)
            .and_then(|p| p.pending_tracks.lock().unwrap().pop_front())
    }

    fn add_track(&mut self, peer_id: u32, kind: u32) -> Result<u32> {
        let peer = self
            .peers
            .get_mut(&peer_id)
            .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;

        let track_id = format!("track-{kind}-{}", peer.next_channel_id);
        peer.next_channel_id = peer.next_channel_id.wrapping_add(1).max(1);

        let codec_type = if kind == 0 {
            webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Audio
        } else {
            webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video
        };

        let track = Arc::new(
            webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP::new(
                webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability {
                    mime_type: if kind == 0 {
                        webrtc::api::media_engine::MIME_TYPE_OPUS.to_string()
                    } else {
                        webrtc::api::media_engine::MIME_TYPE_VP8.to_string()
                    },
                    ..Default::default()
                },
                track_id,
                format!("oxide-{}", if kind == 0 { "audio" } else { "video" }),
            ),
        );

        let _sender = self.runtime.block_on(async {
            peer.conn
                .add_track(track as Arc<dyn webrtc::track::track_local::TrackLocal + Send + Sync>)
                .await
        })?;

        let _ = codec_type;
        Ok(1)
    }

    // ── Signaling helpers ───────────────────────────────────────────

    fn signal_connect(&mut self, url: &str) -> bool {
        self.signaling = Some(SignalingSession {
            base_url: url.trim_end_matches('/').to_string(),
            room: String::new(),
            client: reqwest::blocking::Client::new(),
        });
        true
    }

    fn signal_join_room(&mut self, room: &str) -> bool {
        if let Some(ref mut sig) = self.signaling {
            sig.room = room.to_string();
            let url = format!("{}/rooms/{}/join", sig.base_url, room);
            sig.client.post(&url).send().ok();
            true
        } else {
            false
        }
    }

    fn signal_send(&self, data: &[u8]) -> bool {
        if let Some(ref sig) = self.signaling {
            let url = if sig.room.is_empty() {
                format!("{}/signal", sig.base_url)
            } else {
                format!("{}/rooms/{}/signal", sig.base_url, sig.room)
            };
            sig.client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(data.to_vec())
                .send()
                .is_ok()
        } else {
            false
        }
    }

    fn signal_recv(&self) -> Option<Vec<u8>> {
        let sig = self.signaling.as_ref()?;
        let url = if sig.room.is_empty() {
            format!("{}/signal", sig.base_url)
        } else {
            format!("{}/rooms/{}/signal", sig.base_url, sig.room)
        };
        let resp = sig.client.get(&url).send().ok()?;
        if resp.status().is_success() {
            resp.bytes().ok().map(|b| b.to_vec())
        } else {
            None
        }
    }
}

fn ensure_rtc(state: &Arc<Mutex<Option<RtcState>>>) -> bool {
    let mut g = state.lock().unwrap();
    if g.is_none() {
        *g = RtcState::new();
    }
    g.is_some()
}

/// Register all `api_rtc_*` host functions on the given linker.
pub fn register_rtc_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // ── Peer Connection ──────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_rtc_create_peer",
        |caller: Caller<'_, HostState>, stun_ptr: u32, stun_len: u32| -> u32 {
            let console = caller.data().console.clone();
            let rtc = caller.data().rtc.clone();
            if !ensure_rtc(&rtc) {
                console_log(&console, ConsoleLevel::Error, "[RTC] Init failed".into());
                return 0;
            }
            let stun_config = if stun_len > 0 {
                let mem = caller.data().memory.expect("memory not set");
                read_guest_string(&mem, &caller, stun_ptr, stun_len).unwrap_or_default()
            } else {
                String::new()
            };
            let stun_urls: Vec<String> = if stun_config.is_empty() {
                Vec::new()
            } else {
                stun_config
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            };
            let mut g = rtc.lock().unwrap();
            match g.as_mut().unwrap().create_peer(stun_urls) {
                Ok(id) => {
                    console_log(
                        &console,
                        ConsoleLevel::Log,
                        format!("[RTC] Peer {id} created"),
                    );
                    id
                }
                Err(e) => {
                    console_log(
                        &console,
                        ConsoleLevel::Error,
                        format!("[RTC] Create peer: {e}"),
                    );
                    0
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_close_peer",
        |caller: Caller<'_, HostState>, peer_id: u32| -> u32 {
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            if let Some(r) = g.as_mut() {
                if r.close_peer(peer_id) {
                    1
                } else {
                    0
                }
            } else {
                0
            }
        },
    )?;

    // ── SDP Offer / Answer ───────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_rtc_create_offer",
        |mut caller: Caller<'_, HostState>, peer_id: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let console = caller.data().console.clone();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            let r = match g.as_mut() {
                Some(r) => r,
                None => return -1,
            };
            match r.create_offer(peer_id) {
                Ok(sdp) => {
                    let bytes = sdp.as_bytes();
                    let write_len = bytes.len().min(out_cap as usize);
                    if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).is_err() {
                        return -4;
                    }
                    write_len as i32
                }
                Err(e) => {
                    console_log(&console, ConsoleLevel::Error, format!("[RTC] Offer: {e}"));
                    -2
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_create_answer",
        |mut caller: Caller<'_, HostState>, peer_id: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let console = caller.data().console.clone();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            let r = match g.as_mut() {
                Some(r) => r,
                None => return -1,
            };
            match r.create_answer(peer_id) {
                Ok(sdp) => {
                    let bytes = sdp.as_bytes();
                    let write_len = bytes.len().min(out_cap as usize);
                    if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).is_err() {
                        return -4;
                    }
                    write_len as i32
                }
                Err(e) => {
                    console_log(&console, ConsoleLevel::Error, format!("[RTC] Answer: {e}"));
                    -2
                }
            }
        },
    )?;

    // ── SDP set local/remote ─────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_rtc_set_local_description",
        |caller: Caller<'_, HostState>,
         peer_id: u32,
         sdp_ptr: u32,
         sdp_len: u32,
         is_offer: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let console = caller.data().console.clone();
            let sdp = read_guest_string(&mem, &caller, sdp_ptr, sdp_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            match g.as_mut() {
                Some(r) => match r.set_local_description(peer_id, &sdp, is_offer != 0) {
                    Ok(()) => 0,
                    Err(e) => {
                        console_log(
                            &console,
                            ConsoleLevel::Error,
                            format!("[RTC] Set local desc: {e}"),
                        );
                        -2
                    }
                },
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_set_remote_description",
        |caller: Caller<'_, HostState>,
         peer_id: u32,
         sdp_ptr: u32,
         sdp_len: u32,
         is_offer: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let console = caller.data().console.clone();
            let sdp = read_guest_string(&mem, &caller, sdp_ptr, sdp_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            match g.as_mut() {
                Some(r) => match r.set_remote_description(peer_id, &sdp, is_offer != 0) {
                    Ok(()) => 0,
                    Err(e) => {
                        console_log(
                            &console,
                            ConsoleLevel::Error,
                            format!("[RTC] Set remote desc: {e}"),
                        );
                        -2
                    }
                },
                None => -1,
            }
        },
    )?;

    // ── ICE Candidates ───────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_rtc_add_ice_candidate",
        |caller: Caller<'_, HostState>, peer_id: u32, cand_ptr: u32, cand_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let console = caller.data().console.clone();
            let candidate =
                read_guest_string(&mem, &caller, cand_ptr, cand_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            match g.as_mut() {
                Some(r) => match r.add_ice_candidate(peer_id, &candidate) {
                    Ok(()) => 0,
                    Err(e) => {
                        console_log(
                            &console,
                            ConsoleLevel::Error,
                            format!("[RTC] Add ICE candidate: {e}"),
                        );
                        -2
                    }
                },
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_connection_state",
        |caller: Caller<'_, HostState>, peer_id: u32| -> u32 {
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => r.connection_state(peer_id),
                None => STATE_CLOSED,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_poll_ice_candidate",
        |mut caller: Caller<'_, HostState>, peer_id: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => match r.poll_ice_candidate(peer_id) {
                    Some(json) => {
                        let bytes = json.as_bytes();
                        let write_len = bytes.len().min(out_cap as usize);
                        if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len])
                            .is_err()
                        {
                            return -4;
                        }
                        write_len as i32
                    }
                    None => 0,
                },
                None => -1,
            }
        },
    )?;

    // ── Data Channels ────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_rtc_create_data_channel",
        |caller: Caller<'_, HostState>,
         peer_id: u32,
         label_ptr: u32,
         label_len: u32,
         ordered: u32|
         -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let console = caller.data().console.clone();
            let label = read_guest_string(&mem, &caller, label_ptr, label_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            match g.as_mut() {
                Some(r) => match r.create_data_channel(peer_id, &label, ordered != 0) {
                    Ok(ch) => ch,
                    Err(e) => {
                        console_log(
                            &console,
                            ConsoleLevel::Error,
                            format!("[RTC] Create data channel: {e}"),
                        );
                        0
                    }
                },
                None => 0,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_send",
        |caller: Caller<'_, HostState>,
         peer_id: u32,
         channel_id: u32,
         data_ptr: u32,
         data_len: u32,
         is_binary: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let console = caller.data().console.clone();
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => match r.send_data(peer_id, channel_id, &data, is_binary != 0) {
                    Ok(()) => data.len() as i32,
                    Err(e) => {
                        console_log(&console, ConsoleLevel::Error, format!("[RTC] Send: {e}"));
                        -2
                    }
                },
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_recv",
        |mut caller: Caller<'_, HostState>,
         peer_id: u32,
         channel_id: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i64 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => {
                    let msg = if channel_id == 0 {
                        r.recv_message(peer_id)
                    } else {
                        r.recv_from_channel(peer_id, channel_id)
                    };
                    match msg {
                        Some(m) => {
                            let write_len = m.data.len().min(out_cap as usize);
                            if write_guest_bytes(&mem, &mut caller, out_ptr, &m.data[..write_len])
                                .is_err()
                            {
                                return -4;
                            }
                            let flags = if m.is_binary { 1u64 << 32 } else { 0 };
                            let ch = (m.channel_id as u64) << 48;
                            (ch | flags | write_len as u64) as i64
                        }
                        None => 0,
                    }
                }
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_poll_data_channel",
        |mut caller: Caller<'_, HostState>, peer_id: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => match r.poll_new_channel(peer_id) {
                    Some(ch) => {
                        let info = format!("{}:{}", ch.channel_id, ch.label);
                        let bytes = info.as_bytes();
                        let write_len = bytes.len().min(out_cap as usize);
                        if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len])
                            .is_err()
                        {
                            return -4;
                        }
                        write_len as i32
                    }
                    None => 0,
                },
                None => -1,
            }
        },
    )?;

    // ── Media Tracks ─────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_rtc_add_track",
        |caller: Caller<'_, HostState>, peer_id: u32, kind: u32| -> u32 {
            let console = caller.data().console.clone();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            match g.as_mut() {
                Some(r) => match r.add_track(peer_id, kind) {
                    Ok(id) => id,
                    Err(e) => {
                        console_log(
                            &console,
                            ConsoleLevel::Error,
                            format!("[RTC] Add track: {e}"),
                        );
                        0
                    }
                },
                None => 0,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_poll_track",
        |mut caller: Caller<'_, HostState>, peer_id: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => match r.poll_track(peer_id) {
                    Some(t) => {
                        let info = format!("{}:{}:{}", t.kind, t.id, t.stream_id);
                        let bytes = info.as_bytes();
                        let write_len = bytes.len().min(out_cap as usize);
                        if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len])
                            .is_err()
                        {
                            return -4;
                        }
                        write_len as i32
                    }
                    None => 0,
                },
                None => -1,
            }
        },
    )?;

    // ── Signaling ────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_rtc_signal_connect",
        |caller: Caller<'_, HostState>, url_ptr: u32, url_len: u32| -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let console = caller.data().console.clone();
            let url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            if !ensure_rtc(&rtc) {
                console_log(&console, ConsoleLevel::Error, "[RTC] Init failed".into());
                return 0;
            }
            let mut g = rtc.lock().unwrap();
            if g.as_mut().unwrap().signal_connect(&url) {
                console_log(
                    &console,
                    ConsoleLevel::Log,
                    format!("[RTC] Signaling connected to {url}"),
                );
                1
            } else {
                0
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_signal_join_room",
        |caller: Caller<'_, HostState>, room_ptr: u32, room_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let room = read_guest_string(&mem, &caller, room_ptr, room_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            let mut g = rtc.lock().unwrap();
            match g.as_mut() {
                Some(r) => {
                    if r.signal_join_room(&room) {
                        0
                    } else {
                        -2
                    }
                }
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_signal_send",
        |caller: Caller<'_, HostState>, data_ptr: u32, data_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => {
                    if r.signal_send(&data) {
                        0
                    } else {
                        -2
                    }
                }
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_rtc_signal_recv",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let rtc = caller.data().rtc.clone();
            let g = rtc.lock().unwrap();
            match g.as_ref() {
                Some(r) => match r.signal_recv() {
                    Some(data) => {
                        let write_len = data.len().min(out_cap as usize);
                        if write_guest_bytes(&mem, &mut caller, out_ptr, &data[..write_len])
                            .is_err()
                        {
                            return -4;
                        }
                        write_len as i32
                    }
                    None => 0,
                },
                None => -1,
            }
        },
    )?;

    Ok(())
}
