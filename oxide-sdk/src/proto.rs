//! Lightweight protobuf wire-format encoder/decoder.
//!
//! Produces bytes fully compatible with the Protocol Buffers binary wire
//! format (no .proto file or code-generation required).  This makes
//! protobuf the *native* serialisation layer for Oxide guest applications.
//!
//! ## Encoding
//!
//! ```rust,ignore
//! use oxide_sdk::proto::ProtoEncoder;
//!
//! let data = ProtoEncoder::new()
//!     .string(1, "alice")
//!     .uint64(2, 42)
//!     .bool(3, true)
//!     .bytes(4, &[0xCA, 0xFE])
//!     .finish();
//! ```
//!
//! ## Decoding
//!
//! ```rust,ignore
//! use oxide_sdk::proto::ProtoDecoder;
//!
//! let mut decoder = ProtoDecoder::new(&data);
//! while let Some(field) = decoder.next() {
//!     match field.number {
//!         1 => log(&format!("name = {}", field.as_str())),
//!         2 => log(&format!("age  = {}", field.as_u64())),
//!         _ => {}
//!     }
//! }
//! ```

// ── Wire types ───────────────────────────────────────────────────────────────

const WIRE_VARINT: u32 = 0;
const WIRE_64BIT: u32 = 1;
const WIRE_LEN: u32 = 2;
const WIRE_32BIT: u32 = 5;

// ── Varint helpers ───────────────────────────────────────────────────────────

fn encode_varint(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        if v < 0x80 {
            buf.push(v as u8);
            return;
        }
        buf.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
}

fn decode_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift = 0u32;
    loop {
        if *pos >= buf.len() {
            return None;
        }
        let byte = buf[*pos];
        *pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte < 0x80 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

fn zigzag_encode(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn zigzag_decode(v: u64) -> i64 {
    ((v >> 1) as i64) ^ (-((v & 1) as i64))
}

// ── Encoder ──────────────────────────────────────────────────────────────────

/// Builds a protobuf-compatible binary message field by field.
pub struct ProtoEncoder {
    buf: Vec<u8>,
}

impl ProtoEncoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
        }
    }

    fn tag(self, field: u32, wire: u32) -> Self {
        let mut s = self;
        encode_varint(&mut s.buf, ((field as u64) << 3) | (wire as u64));
        s
    }

    // ── Varint types ────────────────────────────────────────────────

    pub fn uint64(self, field: u32, value: u64) -> Self {
        let mut s = self.tag(field, WIRE_VARINT);
        encode_varint(&mut s.buf, value);
        s
    }

    pub fn uint32(self, field: u32, value: u32) -> Self {
        self.uint64(field, value as u64)
    }

    pub fn int64(self, field: u32, value: i64) -> Self {
        self.uint64(field, value as u64)
    }

    pub fn int32(self, field: u32, value: i32) -> Self {
        self.uint64(field, value as u64)
    }

    pub fn sint64(self, field: u32, value: i64) -> Self {
        self.uint64(field, zigzag_encode(value))
    }

    pub fn sint32(self, field: u32, value: i32) -> Self {
        self.sint64(field, value as i64)
    }

    pub fn bool(self, field: u32, value: bool) -> Self {
        self.uint64(field, value as u64)
    }

    // ── Length-delimited types ───────────────────────────────────────

    pub fn bytes(self, field: u32, value: &[u8]) -> Self {
        let mut s = self.tag(field, WIRE_LEN);
        encode_varint(&mut s.buf, value.len() as u64);
        s.buf.extend_from_slice(value);
        s
    }

    pub fn string(self, field: u32, value: &str) -> Self {
        self.bytes(field, value.as_bytes())
    }

    /// Embed a sub-message (another `ProtoEncoder`'s output).
    pub fn message(self, field: u32, msg: &ProtoEncoder) -> Self {
        self.bytes(field, &msg.buf)
    }

    // ── Fixed-width types ───────────────────────────────────────────

    pub fn fixed64(self, field: u32, value: u64) -> Self {
        let mut s = self.tag(field, WIRE_64BIT);
        s.buf.extend_from_slice(&value.to_le_bytes());
        s
    }

    pub fn sfixed64(self, field: u32, value: i64) -> Self {
        self.fixed64(field, value as u64)
    }

    pub fn double(self, field: u32, value: f64) -> Self {
        self.fixed64(field, value.to_bits())
    }

    pub fn fixed32(self, field: u32, value: u32) -> Self {
        let mut s = self.tag(field, WIRE_32BIT);
        s.buf.extend_from_slice(&value.to_le_bytes());
        s
    }

    pub fn sfixed32(self, field: u32, value: i32) -> Self {
        self.fixed32(field, value as u32)
    }

    pub fn float(self, field: u32, value: f32) -> Self {
        self.fixed32(field, value.to_bits())
    }

    // ── Finalise ────────────────────────────────────────────────────

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl Default for ProtoEncoder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Decoder ──────────────────────────────────────────────────────────────────

/// Iterates over protobuf-encoded fields one at a time.
pub struct ProtoDecoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

/// A single decoded protobuf field.
pub struct ProtoField<'a> {
    pub number: u32,
    pub wire_type: u32,
    data: FieldData<'a>,
}

enum FieldData<'a> {
    Varint(u64),
    Fixed64([u8; 8]),
    Bytes(&'a [u8]),
    Fixed32([u8; 4]),
}

impl<'a> ProtoField<'a> {
    pub fn as_u64(&self) -> u64 {
        match &self.data {
            FieldData::Varint(v) => *v,
            FieldData::Fixed64(b) => u64::from_le_bytes(*b),
            FieldData::Fixed32(b) => u32::from_le_bytes(*b) as u64,
            FieldData::Bytes(b) => {
                let mut arr = [0u8; 8];
                let n = b.len().min(8);
                arr[..n].copy_from_slice(&b[..n]);
                u64::from_le_bytes(arr)
            }
        }
    }

    pub fn as_i64(&self) -> i64 {
        self.as_u64() as i64
    }

    pub fn as_u32(&self) -> u32 {
        self.as_u64() as u32
    }

    pub fn as_i32(&self) -> i32 {
        self.as_u64() as i32
    }

    pub fn as_sint64(&self) -> i64 {
        zigzag_decode(self.as_u64())
    }

    pub fn as_sint32(&self) -> i32 {
        self.as_sint64() as i32
    }

    pub fn as_bool(&self) -> bool {
        self.as_u64() != 0
    }

    pub fn as_f64(&self) -> f64 {
        match &self.data {
            FieldData::Fixed64(b) => f64::from_bits(u64::from_le_bytes(*b)),
            _ => self.as_u64() as f64,
        }
    }

    pub fn as_f32(&self) -> f32 {
        match &self.data {
            FieldData::Fixed32(b) => f32::from_bits(u32::from_le_bytes(*b)),
            _ => self.as_u64() as f32,
        }
    }

    pub fn as_bytes(&self) -> &'a [u8] {
        match &self.data {
            FieldData::Bytes(b) => b,
            _ => &[],
        }
    }

    pub fn as_str(&self) -> &'a str {
        core::str::from_utf8(self.as_bytes()).unwrap_or("")
    }

    /// Decode this field's bytes as a nested message.
    pub fn as_message(&self) -> ProtoDecoder<'a> {
        ProtoDecoder::new(self.as_bytes())
    }
}

impl<'a> ProtoDecoder<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn next(&mut self) -> Option<ProtoField<'a>> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let tag = decode_varint(self.buf, &mut self.pos)?;
        let wire_type = (tag & 0x07) as u32;
        let number = (tag >> 3) as u32;

        let data = match wire_type {
            WIRE_VARINT => {
                let v = decode_varint(self.buf, &mut self.pos)?;
                FieldData::Varint(v)
            }
            WIRE_64BIT => {
                if self.pos + 8 > self.buf.len() {
                    return None;
                }
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&self.buf[self.pos..self.pos + 8]);
                self.pos += 8;
                FieldData::Fixed64(arr)
            }
            WIRE_LEN => {
                let len = decode_varint(self.buf, &mut self.pos)? as usize;
                if self.pos + len > self.buf.len() {
                    return None;
                }
                let slice = &self.buf[self.pos..self.pos + len];
                self.pos += len;
                FieldData::Bytes(slice)
            }
            WIRE_32BIT => {
                if self.pos + 4 > self.buf.len() {
                    return None;
                }
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
                self.pos += 4;
                FieldData::Fixed32(arr)
            }
            _ => return None, // unknown wire type — stop
        };

        Some(ProtoField {
            number,
            wire_type,
            data,
        })
    }

    /// Collect all fields into a `Vec` for random-access lookup.
    pub fn collect_fields(&mut self) -> Vec<ProtoField<'a>> {
        let mut fields = Vec::new();
        while let Some(f) = self.next() {
            fields.push(f);
        }
        fields
    }
}
