//! Host-side MIDI device enumeration and I/O for Oxide guest modules.
//!
//! Guests call the `api_midi_*` imports to enumerate MIDI input/output ports,
//! open connections, send raw MIDI bytes, and poll for incoming messages.
//!
//! **macOS**: implemented via CoreMIDI (`coremidi` crate). Input callbacks run
//! on a CoreMIDI background thread, split each incoming packet into individual
//! MIDI messages, and push them onto a per-handle bounded [`VecDeque`]. The
//! guest drains the queue by calling `api_midi_recv` each frame — no blocking,
//! no async runtime needed. Each `api_midi_recv` returns exactly one MIDI
//! message; if the queue fills up, the oldest message is dropped.
//!
//! **Other platforms**: all functions return 0 / error codes gracefully (no
//! devices found). MIDI support for Linux and Windows will be added in a later
//! phase.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use wasmtime::{Caller, Linker};

use crate::capabilities::{read_guest_bytes, write_guest_bytes, HostState};

// ── Platform implementation ───────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use coremidi::{Client, Destination, InputPort, OutputPort, PacketList, Source};

    /// Hard cap on queued incoming MIDI messages per input port. If the guest
    /// falls behind, oldest messages are dropped first. 4096 is ~1 MB worst
    /// case with 256-byte SysEx dumps; for typical 3-byte note events it's
    /// ~12 KB — plenty for a ~1 min backlog at max MIDI rate (~30k msg/s).
    const MAX_QUEUED_MESSAGES: usize = 4096;

    /// Split a raw CoreMIDI packet (which may concatenate several MIDI messages
    /// sharing a timestamp) into individual messages and push each onto `out`,
    /// enforcing [`MAX_QUEUED_MESSAGES`] by dropping the oldest on overflow.
    ///
    /// Handles channel voice (0x80–0xEF), System Common (0xF1–0xF6),
    /// System Real-Time (0xF8–0xFF), and SysEx (0xF0 … 0xF7). Bytes that
    /// appear without a preceding status byte (running status, or junk) are
    /// skipped.
    fn enqueue_messages(data: &[u8], out: &mut VecDeque<Vec<u8>>) {
        let mut i = 0;
        while i < data.len() {
            let status = data[i];
            if status & 0x80 == 0 {
                // Data byte with no status — skip. CoreMIDI normally delivers
                // complete messages, so this only hits on malformed input.
                i += 1;
                continue;
            }
            let end = match status & 0xF0 {
                0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => i + 3,
                0xC0 | 0xD0 => i + 2,
                0xF0 => match status {
                    0xF0 => {
                        // SysEx: consume up to and including the next 0xF7.
                        match data[i + 1..].iter().position(|&b| b == 0xF7) {
                            Some(p) => i + 1 + p + 1,
                            None => data.len(),
                        }
                    }
                    0xF1 | 0xF3 => i + 2,
                    0xF2 => i + 3,
                    // 0xF4–0xF7 (undefined / SysEx end) and 0xF8–0xFF
                    // (real-time) are single-byte messages.
                    _ => i + 1,
                },
                _ => i + 1,
            };
            let end = end.min(data.len());
            let msg: Vec<u8> = data[i..end].to_vec();
            if out.len() >= MAX_QUEUED_MESSAGES {
                out.pop_front();
            }
            out.push_back(msg);
            i = end;
        }
    }

    // ── Per-handle state ──────────────────────────────────────────────────

    pub struct InputConn {
        /// Kept alive — dropping closes the CoreMIDI port.
        _port: InputPort,
        pub queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    }

    pub struct OutputConn {
        port: OutputPort,
        dest_idx: usize,
    }

    // ── MidiState ─────────────────────────────────────────────────────────

    pub struct MidiState {
        client: Client,
        next_handle: u32,
        inputs: HashMap<u32, InputConn>,
        outputs: HashMap<u32, OutputConn>,
    }

    impl MidiState {
        pub fn new() -> Option<Self> {
            let client = Client::new("oxide-browser").ok()?;
            Some(Self {
                client,
                next_handle: 1,
                inputs: HashMap::new(),
                outputs: HashMap::new(),
            })
        }

        fn alloc_handle(&mut self) -> u32 {
            let h = self.next_handle;
            self.next_handle = self.next_handle.wrapping_add(1).max(1);
            h
        }

        pub fn input_count() -> u32 {
            coremidi::Sources::count() as u32
        }

        pub fn output_count() -> u32 {
            coremidi::Destinations::count() as u32
        }

        pub fn input_name(index: u32) -> Option<String> {
            let src = Source::from_index(index as usize)?;
            src.display_name()
                .or_else(|| Some(format!("Input {}", index)))
        }

        pub fn output_name(index: u32) -> Option<String> {
            let dst = Destination::from_index(index as usize)?;
            dst.display_name()
                .or_else(|| Some(format!("Output {}", index)))
        }

        pub fn open_input(&mut self, index: u32) -> u32 {
            let source = match Source::from_index(index as usize) {
                Some(s) => s,
                None => return 0,
            };
            let queue: Arc<Mutex<VecDeque<Vec<u8>>>> = Arc::new(Mutex::new(VecDeque::new()));
            let q = queue.clone();
            let port_name = format!("oxide-in-{}", index);
            let port = match self
                .client
                .input_port(&port_name, move |pkt_list: &PacketList| {
                    let mut lock = q.lock().unwrap();
                    for pkt in pkt_list.iter() {
                        let bytes = pkt.data();
                        if !bytes.is_empty() {
                            enqueue_messages(bytes, &mut lock);
                        }
                    }
                }) {
                Ok(p) => p,
                Err(_) => return 0,
            };
            if port.connect_source(&source).is_err() {
                return 0;
            }
            let handle = self.alloc_handle();
            self.inputs.insert(handle, InputConn { _port: port, queue });
            handle
        }

        pub fn open_output(&mut self, index: u32) -> u32 {
            let port_name = format!("oxide-out-{}", index);
            let port = match self.client.output_port(&port_name) {
                Ok(p) => p,
                Err(_) => return 0,
            };
            let handle = self.alloc_handle();
            self.outputs.insert(
                handle,
                OutputConn {
                    port,
                    dest_idx: index as usize,
                },
            );
            handle
        }

        pub fn send(&mut self, handle: u32, data: &[u8]) -> bool {
            let out = match self.outputs.get_mut(&handle) {
                Some(o) => o,
                None => return false,
            };
            let dest = match Destination::from_index(out.dest_idx) {
                Some(d) => d,
                None => return false,
            };
            // Build a single-packet PacketList from raw bytes.
            let packets = coremidi::PacketBuffer::new(0, data);
            out.port.send(&dest, &packets).is_ok()
        }

        /// Length in bytes of the front queued message without popping it.
        /// Used by `api_midi_recv` to decide whether the guest's buffer is
        /// large enough before dequeuing.
        pub fn peek_len(&self, handle: u32) -> Option<usize> {
            self.inputs
                .get(&handle)?
                .queue
                .lock()
                .unwrap()
                .front()
                .map(|m| m.len())
        }

        pub fn recv(&self, handle: u32) -> Option<Vec<u8>> {
            self.inputs.get(&handle)?.queue.lock().unwrap().pop_front()
        }

        pub fn close(&mut self, handle: u32) {
            self.inputs.remove(&handle);
            self.outputs.remove(&handle);
        }
    }
}

// ── Stub for non-macOS ────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
mod platform {
    /// Placeholder — MIDI is not yet supported on this platform.
    pub struct MidiState;

    impl MidiState {
        pub fn new() -> Option<Self> {
            Some(Self)
        }
        pub fn input_count() -> u32 {
            0
        }
        pub fn output_count() -> u32 {
            0
        }
        pub fn input_name(_index: u32) -> Option<String> {
            None
        }
        pub fn output_name(_index: u32) -> Option<String> {
            None
        }
        pub fn open_input(&mut self, _index: u32) -> u32 {
            0
        }
        pub fn open_output(&mut self, _index: u32) -> u32 {
            0
        }
        pub fn send(&mut self, _handle: u32, _data: &[u8]) -> bool {
            false
        }
        pub fn peek_len(&self, _handle: u32) -> Option<usize> {
            None
        }
        pub fn recv(&self, _handle: u32) -> Option<Vec<u8>> {
            None
        }
        pub fn close(&mut self, _handle: u32) {}
    }
}

// ── Public re-export ──────────────────────────────────────────────────────────

pub use platform::MidiState;

// ── Lazy initialisation helper ────────────────────────────────────────────────

fn ensure_midi(state: &Arc<Mutex<Option<MidiState>>>) {
    let mut g = state.lock().unwrap();
    if g.is_none() {
        *g = MidiState::new();
    }
}

// ── Host function registration ────────────────────────────────────────────────

/// Register all `api_midi_*` host functions on the given linker.
pub fn register_midi_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // ── midi_input_count ──────────────────────────────────────────────────
    // api_midi_input_count() -> u32
    linker.func_wrap(
        "oxide",
        "api_midi_input_count",
        |_caller: Caller<'_, HostState>| -> u32 { MidiState::input_count() },
    )?;

    // ── midi_output_count ─────────────────────────────────────────────────
    // api_midi_output_count() -> u32
    linker.func_wrap(
        "oxide",
        "api_midi_output_count",
        |_caller: Caller<'_, HostState>| -> u32 { MidiState::output_count() },
    )?;

    // ── midi_input_name ───────────────────────────────────────────────────
    // api_midi_input_name(index: u32, out_ptr: u32, out_cap: u32) -> u32
    // Writes the port name into guest memory. Returns bytes written, or 0
    // if the index is out of range.
    linker.func_wrap(
        "oxide",
        "api_midi_input_name",
        |mut caller: Caller<'_, HostState>, index: u32, out_ptr: u32, out_cap: u32| -> u32 {
            let name = match MidiState::input_name(index) {
                Some(n) => n,
                None => return 0,
            };
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let bytes = name.as_bytes();
            let len = bytes.len().min(out_cap as usize);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..len]).is_err() {
                return 0;
            }
            len as u32
        },
    )?;

    // ── midi_output_name ──────────────────────────────────────────────────
    // api_midi_output_name(index: u32, out_ptr: u32, out_cap: u32) -> u32
    linker.func_wrap(
        "oxide",
        "api_midi_output_name",
        |mut caller: Caller<'_, HostState>, index: u32, out_ptr: u32, out_cap: u32| -> u32 {
            let name = match MidiState::output_name(index) {
                Some(n) => n,
                None => return 0,
            };
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let bytes = name.as_bytes();
            let len = bytes.len().min(out_cap as usize);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..len]).is_err() {
                return 0;
            }
            len as u32
        },
    )?;

    // ── midi_open_input ───────────────────────────────────────────────────
    // api_midi_open_input(index: u32) -> u32
    // Returns a handle (> 0) or 0 on failure.
    linker.func_wrap(
        "oxide",
        "api_midi_open_input",
        |caller: Caller<'_, HostState>, index: u32| -> u32 {
            let midi = caller.data().midi.clone();
            ensure_midi(&midi);
            let mut g = midi.lock().unwrap();
            g.as_mut().map(|s| s.open_input(index)).unwrap_or(0)
        },
    )?;

    // ── midi_open_output ──────────────────────────────────────────────────
    // api_midi_open_output(index: u32) -> u32
    linker.func_wrap(
        "oxide",
        "api_midi_open_output",
        |caller: Caller<'_, HostState>, index: u32| -> u32 {
            let midi = caller.data().midi.clone();
            ensure_midi(&midi);
            let mut g = midi.lock().unwrap();
            g.as_mut().map(|s| s.open_output(index)).unwrap_or(0)
        },
    )?;

    // ── midi_send ─────────────────────────────────────────────────────────
    // api_midi_send(handle: u32, data_ptr: u32, data_len: u32) -> i32
    // Returns 0 on success, -1 on failure.
    linker.func_wrap(
        "oxide",
        "api_midi_send",
        |caller: Caller<'_, HostState>, handle: u32, ptr: u32, len: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -1,
            };
            let data = match read_guest_bytes(&mem, &caller, ptr, len) {
                Ok(b) => b,
                Err(_) => return -1,
            };
            let midi = caller.data().midi.clone();
            let mut g = midi.lock().unwrap();
            if g.as_mut().is_some_and(|s| s.send(handle, &data)) {
                0
            } else {
                -1
            }
        },
    )?;

    // ── midi_recv ─────────────────────────────────────────────────────────
    // api_midi_recv(handle: u32, out_ptr: u32, out_cap: u32) -> i32
    // Dequeues one MIDI message and writes its bytes into guest memory.
    // Returns bytes written on success, -1 if the queue is empty, or -2 if
    // the guest buffer is too small (message stays in the queue so the guest
    // can retry with a larger buffer).
    linker.func_wrap(
        "oxide",
        "api_midi_recv",
        |mut caller: Caller<'_, HostState>, handle: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let midi = caller.data().midi.clone();
            let peek = {
                let g = midi.lock().unwrap();
                g.as_ref().and_then(|s| s.peek_len(handle))
            };
            let msg_len = match peek {
                Some(n) => n,
                None => return -1,
            };
            if msg_len > out_cap as usize {
                return -2;
            }
            let msg = {
                let g = midi.lock().unwrap();
                match g.as_ref().and_then(|s| s.recv(handle)) {
                    Some(m) => m,
                    None => return -1,
                }
            };
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -1,
            };
            if write_guest_bytes(&mem, &mut caller, out_ptr, &msg).is_err() {
                return -1;
            }
            msg.len() as i32
        },
    )?;

    // ── midi_close ────────────────────────────────────────────────────────
    // api_midi_close(handle: u32)
    // Frees host-side resources for the given handle.
    linker.func_wrap(
        "oxide",
        "api_midi_close",
        |caller: Caller<'_, HostState>, handle: u32| {
            let midi = caller.data().midi.clone();
            let mut g = midi.lock().unwrap();
            if let Some(ref mut state) = *g {
                state.close(handle);
            }
        },
    )?;

    Ok(())
}
