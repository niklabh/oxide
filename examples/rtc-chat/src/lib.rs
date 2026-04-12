//! WebRTC peer-to-peer chat demo.
//!
//! Demonstrates the Oxide RTC API: create a peer connection, exchange
//! SDP offer/answer, trickle ICE candidates, open a data channel, and
//! send/receive text messages — all from a guest `.wasm` module.

use oxide_sdk::*;

const MAX_MESSAGES: usize = 20;

static mut STATE: AppState = AppState::new();

#[allow(dead_code)]
struct AppState {
    peer_id: u32,
    channel_id: u32,
    local_sdp: [u8; 8192],
    local_sdp_len: usize,
    remote_sdp_buf: [u8; 8192],
    remote_sdp_len: usize,
    messages: [(u8, [u8; 256], usize); MAX_MESSAGES],
    msg_count: usize,
    is_offerer: bool,
    connected: bool,
}

impl AppState {
    const fn new() -> Self {
        Self {
            peer_id: 0,
            channel_id: 0,
            local_sdp: [0u8; 8192],
            local_sdp_len: 0,
            remote_sdp_buf: [0u8; 8192],
            remote_sdp_len: 0,
            messages: [(0, [0u8; 256], 0); MAX_MESSAGES],
            msg_count: 0,
            is_offerer: false,
            connected: false,
        }
    }

    fn push_message(&mut self, who: u8, text: &str) {
        if self.msg_count >= MAX_MESSAGES {
            for i in 0..MAX_MESSAGES - 1 {
                self.messages[i] = self.messages[i + 1];
            }
            self.msg_count = MAX_MESSAGES - 1;
        }
        let idx = self.msg_count;
        self.messages[idx].0 = who;
        let bytes = text.as_bytes();
        let len = bytes.len().min(256);
        self.messages[idx].1[..len].copy_from_slice(&bytes[..len]);
        self.messages[idx].2 = len;
        self.msg_count += 1;
    }
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("RTC Chat demo loaded");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let (width, height) = canvas_dimensions();
    let w = width as f32;
    let h = height as f32;

    canvas_clear(25, 25, 40, 255);

    // Title
    canvas_rect(0.0, 0.0, w, 50.0, 45, 35, 75, 255);
    canvas_text(16.0, 14.0, 22.0, 200, 180, 255, 255, "Oxide RTC Chat");

    let state_code = if s.peer_id > 0 {
        rtc_connection_state(s.peer_id)
    } else {
        RTC_STATE_CLOSED
    };
    let state_label = match state_code {
        RTC_STATE_NEW => "New",
        RTC_STATE_CONNECTING => "Connecting...",
        RTC_STATE_CONNECTED => "Connected",
        RTC_STATE_DISCONNECTED => "Disconnected",
        RTC_STATE_FAILED => "Failed",
        _ => "Closed",
    };
    let (sr, sg, sb) = match state_code {
        RTC_STATE_CONNECTED => (100, 220, 100),
        RTC_STATE_CONNECTING => (255, 200, 80),
        RTC_STATE_FAILED | RTC_STATE_DISCONNECTED => (255, 80, 80),
        _ => (150, 150, 150),
    };
    canvas_text(w - 200.0, 18.0, 14.0, sr, sg, sb, 255, state_label);

    if !s.connected && state_code == RTC_STATE_CONNECTED {
        s.connected = true;
        s.push_message(2, "** Connected! **");
    }

    // Step 1: Create peer
    let mut y = 65.0;
    if s.peer_id == 0 {
        canvas_text(
            16.0,
            y,
            14.0,
            180,
            180,
            180,
            255,
            "Step 1: Create a peer connection",
        );
        y += 25.0;
        if ui_button(10, 16.0, y, 160.0, 28.0, "Create as Offerer") {
            let id = rtc_create_peer("");
            if id > 0 {
                s.peer_id = id;
                s.is_offerer = true;
                let ch = rtc_create_data_channel(id, "chat", true);
                s.channel_id = ch;
                match rtc_create_offer(id) {
                    Ok(sdp) => {
                        let bytes = sdp.as_bytes();
                        let len = bytes.len().min(s.local_sdp.len());
                        s.local_sdp[..len].copy_from_slice(&bytes[..len]);
                        s.local_sdp_len = len;
                        s.push_message(2, "Offer created — paste to answerer");
                        log(&format!("SDP Offer:\n{sdp}"));
                    }
                    Err(e) => {
                        s.push_message(2, &format!("Offer error: {e}"));
                    }
                }
            }
        }
        if ui_button(11, 190.0, y, 160.0, 28.0, "Create as Answerer") {
            let id = rtc_create_peer("");
            if id > 0 {
                s.peer_id = id;
                s.is_offerer = false;
                s.push_message(2, "Peer ready — paste the remote offer");
            }
        }
        return;
    }

    // Step 2: SDP exchange
    if !s.connected {
        canvas_text(
            16.0,
            y,
            14.0,
            180,
            180,
            180,
            255,
            if s.is_offerer {
                "Step 2: Exchange SDP — copy your offer, paste the remote answer"
            } else {
                "Step 2: Paste the remote offer, then copy your answer"
            },
        );
        y += 25.0;

        let remote_sdp_text = ui_text_input(100, 16.0, y, w - 180.0, "Paste remote SDP here…");
        if ui_button(12, w - 150.0, y, 140.0, 28.0, "Apply Remote SDP") {
            let sdp = remote_sdp_text.trim();
            if !sdp.is_empty() {
                let is_offer = !s.is_offerer;
                let rc = rtc_set_remote_description(s.peer_id, sdp, is_offer);
                if rc == 0 {
                    s.push_message(2, "Remote SDP applied");
                    if !s.is_offerer {
                        let ch = rtc_create_data_channel(s.peer_id, "chat", true);
                        s.channel_id = ch;
                        match rtc_create_answer(s.peer_id) {
                            Ok(answer) => {
                                let bytes = answer.as_bytes();
                                let len = bytes.len().min(s.local_sdp.len());
                                s.local_sdp[..len].copy_from_slice(&bytes[..len]);
                                s.local_sdp_len = len;
                                s.push_message(2, "Answer created — copy to offerer");
                                log(&format!("SDP Answer:\n{answer}"));
                            }
                            Err(e) => {
                                s.push_message(2, &format!("Answer error: {e}"));
                            }
                        }
                    }
                } else {
                    s.push_message(2, &format!("SDP apply error: {rc}"));
                }
            }
        }
        y += 40.0;

        // Drain gathered ICE candidates and log them
        while let Some(candidate) = rtc_poll_ice_candidate(s.peer_id) {
            log(&format!("ICE candidate: {candidate}"));
        }

        // Check for incoming data channels from remote
        if let Some(ch_info) = rtc_poll_data_channel(s.peer_id) {
            s.channel_id = ch_info.channel_id;
            s.push_message(2, &format!("Remote channel: {}", ch_info.label));
        }
    }

    // Chat area
    let chat_top = if s.connected { y } else { y + 10.0 };
    let chat_bottom = h - 50.0;

    canvas_rect(
        8.0,
        chat_top,
        w - 16.0,
        chat_bottom - chat_top,
        35,
        35,
        50,
        255,
    );
    canvas_text(16.0, chat_top + 4.0, 12.0, 120, 120, 140, 255, "Messages");

    let mut my = chat_top + 22.0;
    for i in 0..s.msg_count {
        if my > chat_bottom - 16.0 {
            break;
        }
        let (who, ref buf, len) = s.messages[i];
        let text = core::str::from_utf8(&buf[..len]).unwrap_or("???");
        let (cr, cg, cb) = match who {
            0 => (100, 200, 255),
            1 => (255, 200, 100),
            _ => (150, 150, 170),
        };
        let prefix = match who {
            0 => "You: ",
            1 => "Them: ",
            _ => "",
        };
        canvas_text(20.0, my, 13.0, cr, cg, cb, 255, &format!("{prefix}{text}"));
        my += 18.0;
    }

    // Message input
    if s.connected && s.channel_id > 0 {
        let msg = ui_text_input(200, 16.0, h - 40.0, w - 140.0, "Type a message…");
        if ui_button(20, w - 110.0, h - 40.0, 100.0, 28.0, "Send") {
            let text = msg.trim();
            if !text.is_empty() {
                let rc = rtc_send_text(s.peer_id, s.channel_id, text);
                if rc >= 0 {
                    s.push_message(0, text);
                } else {
                    s.push_message(2, &format!("Send error: {rc}"));
                }
            }
        }
    }

    // Poll incoming messages
    while let Some(msg) = rtc_recv(s.peer_id, 0) {
        s.push_message(1, &msg.text());
    }
}
