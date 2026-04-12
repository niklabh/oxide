#![allow(dead_code)]
//! Lightweight protobuf wire-format encoder/decoder.
//!
//! Matches the wire format produced by `oxide_sdk::proto`, allowing the backend
//! to speak the same binary protocol as WASM guest applications.

const WIRE_VARINT: u32 = 0;
const WIRE_64BIT: u32 = 1;
const WIRE_LEN: u32 = 2;
const WIRE_32BIT: u32 = 5;

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

// ── Encoder ──────────────────────────────────────────────────────────────────

pub struct ProtoEncoder {
    buf: Vec<u8>,
}

impl ProtoEncoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn tag(mut self, field: u32, wire: u32) -> Self {
        encode_varint(&mut self.buf, ((field as u64) << 3) | (wire as u64));
        self
    }

    pub fn uint64(self, field: u32, value: u64) -> Self {
        let mut s = self.tag(field, WIRE_VARINT);
        encode_varint(&mut s.buf, value);
        s
    }

    pub fn uint32(self, field: u32, value: u32) -> Self {
        self.uint64(field, value as u64)
    }

    pub fn bool(self, field: u32, value: bool) -> Self {
        self.uint64(field, value as u64)
    }

    pub fn bytes(self, field: u32, value: &[u8]) -> Self {
        let mut s = self.tag(field, WIRE_LEN);
        encode_varint(&mut s.buf, value.len() as u64);
        s.buf.extend_from_slice(value);
        s
    }

    pub fn string(self, field: u32, value: &str) -> Self {
        self.bytes(field, value.as_bytes())
    }

    pub fn message(self, field: u32, msg: &ProtoEncoder) -> Self {
        self.bytes(field, &msg.buf)
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

// ── Decoder ──────────────────────────────────────────────────────────────────

pub struct ProtoDecoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

pub struct ProtoField<'a> {
    pub number: u32,
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
            FieldData::Bytes(_) => 0,
        }
    }

    pub fn as_u32(&self) -> u32 {
        self.as_u64() as u32
    }

    pub fn as_bool(&self) -> bool {
        self.as_u64() != 0
    }

    pub fn as_str(&self) -> &'a str {
        match &self.data {
            FieldData::Bytes(b) => core::str::from_utf8(b).unwrap_or(""),
            _ => "",
        }
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
            WIRE_VARINT => FieldData::Varint(decode_varint(self.buf, &mut self.pos)?),
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
            _ => return None,
        };

        Some(ProtoField { number, data })
    }
}
