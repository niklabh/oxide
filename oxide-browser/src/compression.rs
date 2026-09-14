//! Compression capabilities for Oxide guest modules.
//!
//! Guests call `api_compress` / `api_decompress` with a format code
//! (0 = gzip, 1 = raw deflate, 2 = zlib), mirroring the web platform's
//! `CompressionStream` formats. Both functions return the *total* output
//! size: if it fits in `out_cap` the data was written, otherwise the guest
//! should retry with a buffer of the returned size.

use std::io::{Read, Write};

use anyhow::Result;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use flate2::write::{DeflateEncoder, GzEncoder, ZlibEncoder};
use flate2::Compression;
use wasmtime::{Caller, Linker};

use crate::capabilities::{read_guest_bytes, write_guest_bytes, HostState};

/// Gzip (RFC 1952), matches web `CompressionStream("gzip")`.
pub const FORMAT_GZIP: u32 = 0;
/// Raw deflate (RFC 1951), matches `CompressionStream("deflate-raw")`.
pub const FORMAT_DEFLATE: u32 = 1;
/// Zlib (RFC 1950), matches `CompressionStream("deflate")`.
pub const FORMAT_ZLIB: u32 = 2;

/// Hard cap on decompressed output to defend against zip bombs.
/// Guests only get 256 MB of linear memory anyway.
const MAX_DECOMPRESSED: u64 = 128 * 1024 * 1024;

fn compress(format: u32, data: &[u8]) -> Option<Vec<u8>> {
    let level = Compression::default();
    match format {
        FORMAT_GZIP => {
            let mut enc = GzEncoder::new(Vec::new(), level);
            enc.write_all(data).ok()?;
            enc.finish().ok()
        }
        FORMAT_DEFLATE => {
            let mut enc = DeflateEncoder::new(Vec::new(), level);
            enc.write_all(data).ok()?;
            enc.finish().ok()
        }
        FORMAT_ZLIB => {
            let mut enc = ZlibEncoder::new(Vec::new(), level);
            enc.write_all(data).ok()?;
            enc.finish().ok()
        }
        _ => None,
    }
}

fn decompress(format: u32, data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    // Read one byte past the cap so we can tell "exactly at cap" from "over".
    let limit = MAX_DECOMPRESSED + 1;
    let n = match format {
        FORMAT_GZIP => GzDecoder::new(data).take(limit).read_to_end(&mut out),
        FORMAT_DEFLATE => DeflateDecoder::new(data).take(limit).read_to_end(&mut out),
        FORMAT_ZLIB => ZlibDecoder::new(data).take(limit).read_to_end(&mut out),
        _ => return None,
    }
    .ok()?;
    if n as u64 > MAX_DECOMPRESSED {
        return None;
    }
    Some(out)
}

/// Register `api_compress` and `api_decompress` host functions.
pub fn register_compression_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // api_compress(format, data_ptr, data_len, out_ptr, out_cap) -> i64
    //   Returns the total compressed size. If it is <= out_cap the output was
    //   written to out_ptr; otherwise call again with a bigger buffer.
    //   Returns -1 on unknown format, -2 on codec error.
    linker.func_wrap(
        "oxide",
        "api_compress",
        |mut caller: Caller<'_, HostState>,
         format: u32,
         data_ptr: u32,
         data_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i64 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            if format > FORMAT_ZLIB {
                return -1;
            }
            let out = match compress(format, &data) {
                Some(o) => o,
                None => return -2,
            };
            if out.len() <= out_cap as usize
                && write_guest_bytes(&mem, &mut caller, out_ptr, &out).is_err()
            {
                return -2;
            }
            out.len() as i64
        },
    )?;

    // api_decompress(format, data_ptr, data_len, out_ptr, out_cap) -> i64
    //   Same contract as api_compress. Output is capped at 128 MB.
    //   Returns -1 on unknown format, -2 on corrupt data or oversized output.
    linker.func_wrap(
        "oxide",
        "api_decompress",
        |mut caller: Caller<'_, HostState>,
         format: u32,
         data_ptr: u32,
         data_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i64 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            if format > FORMAT_ZLIB {
                return -1;
            }
            let out = match decompress(format, &data) {
                Some(o) => o,
                None => return -2,
            };
            if out.len() <= out_cap as usize
                && write_guest_bytes(&mem, &mut caller, out_ptr, &out).is_err()
            {
                return -2;
            }
            out.len() as i64
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_formats() {
        let data = b"hello hello hello hello hello oxide".repeat(100);
        for format in [FORMAT_GZIP, FORMAT_DEFLATE, FORMAT_ZLIB] {
            let packed = compress(format, &data).unwrap();
            assert!(packed.len() < data.len());
            let unpacked = decompress(format, &packed).unwrap();
            assert_eq!(unpacked, data);
        }
    }

    #[test]
    fn unknown_format_rejected() {
        assert!(compress(99, b"x").is_none());
        assert!(decompress(99, b"x").is_none());
    }

    #[test]
    fn corrupt_data_rejected() {
        assert!(decompress(FORMAT_GZIP, b"not gzip at all").is_none());
    }
}
