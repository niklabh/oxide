//! Background worker demo — worker module.
//!
//! Loaded by `worker-demo` via [`spawn_worker`]. Receives a single `u32` limit
//! (little-endian) through `on_message`, counts the primes below it on its own
//! thread, and posts the count back as a little-endian `u64`.
//!
//! # Building
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p worker-demo-bg
//! ```

use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("[worker] ready, waiting for work.");
}

#[no_mangle]
pub extern "C" fn on_message(_len: u32) {
    let mut buf = [0u8; 4];
    let n = worker_message_read(&mut buf);
    if n < 4 {
        return;
    }
    let limit = u32::from_le_bytes(buf);

    let mut count: u64 = 0;
    let mut k: u32 = 2;
    while k < limit {
        if is_prime(k) {
            count += 1;
        }
        k += 1;
    }

    worker_post(&count.to_le_bytes());
}

fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2u32;
    while d * d <= n {
        if n.is_multiple_of(d) {
            return false;
        }
        d += 1;
    }
    true
}
