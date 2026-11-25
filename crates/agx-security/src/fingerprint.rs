//! OpenSSH-style SHA-256 fingerprints computed without any external tool.
use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0);
    for byte in text.bytes().take_while(|byte| *byte != b'=') {
        let value = ALPHABET.iter().position(|candidate| *candidate == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn encode_unpadded(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let value = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (index, byte)| acc | (*byte as u32) << (16 - 8 * index));
        for index in 0..=chunk.len() {
            out.push(ALPHABET[(value >> (18 - 6 * index) & 63) as usize] as char);
        }
    }
    out
}

/// `SHA256:…` for an OpenSSH public key line (`ssh-ed25519 AAAA… comment`), as `ssh-keygen -l`
/// prints it. Returns None for anything that is not a well-formed key line.
pub fn sha256_fingerprint(line: &str) -> Option<String> {
    let mut fields = line.split_whitespace();
    let kind = fields.next()?;
    let blob = decode(fields.next()?)?;
    // The blob begins with the same algorithm name as the line's first field.
    let length = u32::from_be_bytes(blob.get(..4)?.try_into().ok()?) as usize;
    if blob.get(4..4 + length)? != kind.as_bytes() {
        return None;
    }
    Some(format!("SHA256:{}", encode_unpadded(&Sha256::digest(&blob))))
}
