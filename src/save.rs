//! GOPNIK .SAV parsing. 694 bytes, Borland Pascal record layout.
//!
//! Round-trip must be byte-exact, which constrains two things:
//!   * shortstring padding past the length byte is NOT cleared by Borland,
//!     so we retain the original bytes rather than zero-filling;
//!   * every byte past the known fields is preserved verbatim in `tail`.
//!
//! The offsets below are hand-mirrored from `tools/decode_save.py` (the
//! Task 5 Python reference decoder) and from the layout it emits at
//! `data/save_layout.json`. `tests/save_roundtrip.rs` reads that JSON and
//! asserts these constants agree with it, so the two copies cannot
//! silently drift apart.

use encoding_rs::{EncoderResult, IBM866};
use std::fmt;

pub const SIZE: usize = 694;
pub const OFF_MAGIC: usize = 0x000;
pub const OFF_NAME: usize = 0x100;
pub const OFF_STATE: usize = 0x200;
pub const OFF_HP: usize = OFF_STATE + 0x10;
pub const OFF_HPMAX: usize = OFF_STATE + 0x12;
pub const OFF_TAIL: usize = OFF_STATE + 0x14;
const PSTRING_CAP: usize = 255;

#[derive(Debug)]
pub enum SaveError {
    /// Input was not exactly `SIZE` bytes.
    BadSize(usize),
    /// A `pstring` field's bytes did not decode as valid CP866.
    ///
    /// In practice unreachable: IBM866/CP866 is a total single-byte
    /// encoding under WHATWG's definition (every byte 0x00-0xFF maps to
    /// some character), so `encoding_rs`'s strict decoder cannot fail on
    /// it. Kept because the decode API is fallible (`Option`) and must be
    /// handled rather than unwrapped.
    BadCp866Bytes,
    /// A character being encoded into a save has no CP866 representation
    /// (for example a player-typed name containing non-Cyrillic script).
    Unmappable(char),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::BadSize(n) => write!(f, "expected {SIZE} bytes, got {n}"),
            SaveError::BadCp866Bytes => write!(f, "bytes are not valid CP866"),
            SaveError::Unmappable(c) => {
                write!(f, "character {c:?} has no CP866 representation")
            }
        }
    }
}

impl std::error::Error for SaveError {}

/// Decode CP866 bytes to a `String`, using the strict (non-lossy) decoder.
fn cp866_decode(bytes: &[u8]) -> Result<String, SaveError> {
    IBM866
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(|cow| cow.into_owned())
        .ok_or(SaveError::BadCp866Bytes)
}

/// Encode a `&str` to CP866 bytes, using the strict (non-lossy) encoder.
///
/// `Encoding::encode` is unsuitable here: per the WHATWG spec it silently
/// replaces unmappable characters with an HTML numeric character reference
/// (e.g. `"漢"` becomes the literal bytes `&#28450;`), which would write a
/// corrupt save that still round-trips as bytes without ever reporting an
/// error. The encoder used here reports the offending character instead.
fn cp866_encode(s: &str) -> Result<Vec<u8>, SaveError> {
    let mut encoder = IBM866.new_encoder();
    let mut out = Vec::with_capacity(
        encoder
            .max_buffer_length_from_utf8_without_replacement(s.len())
            .unwrap_or(s.len()),
    );
    match encoder.encode_from_utf8_to_vec_without_replacement(s, &mut out, true) {
        (EncoderResult::InputEmpty, _) => Ok(out),
        (EncoderResult::Unmappable(c), _) => Err(SaveError::Unmappable(c)),
        (EncoderResult::OutputFull, _) => {
            unreachable!("buffer sized via max_buffer_length_from_utf8_without_replacement")
        }
    }
}

pub struct Save {
    pub magic: String,
    pub name: String,
    pub stats: [u16; 8],
    pub hp: u16,
    pub hpmax: u16,
    pub tail: Vec<u8>,
    raw: Vec<u8>,
}

fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn get_pstring(b: &[u8], off: usize) -> Result<String, SaveError> {
    let n = b[off] as usize;
    cp866_decode(&b[off + 1..off + 1 + n])
}

fn put_pstring(buf: &mut [u8], off: usize, s: &str) -> Result<(), SaveError> {
    let raw = cp866_encode(s)?;
    assert!(raw.len() <= PSTRING_CAP);
    buf[off] = raw.len() as u8;
    buf[off + 1..off + 1 + raw.len()].copy_from_slice(&raw);
    // Bytes past the length are left exactly as they were.
    Ok(())
}

impl Save {
    /// `name` holds the original bytes, markup included, because round-trip
    /// must be byte-exact. Everything user-facing goes through here.
    pub fn display_name(&self) -> String {
        crate::text::strip(&self.name)
    }

    pub fn parse(bytes: &[u8]) -> Result<Save, SaveError> {
        if bytes.len() != SIZE {
            return Err(SaveError::BadSize(bytes.len()));
        }
        let mut stats = [0u16; 8];
        for (i, s) in stats.iter_mut().enumerate() {
            *s = u16le(bytes, OFF_STATE + 2 * i);
        }
        Ok(Save {
            magic: get_pstring(bytes, OFF_MAGIC)?,
            name: get_pstring(bytes, OFF_NAME)?,
            stats,
            hp: u16le(bytes, OFF_HP),
            hpmax: u16le(bytes, OFF_HPMAX),
            tail: bytes[OFF_TAIL..].to_vec(),
            raw: bytes.to_vec(),
        })
    }

    /// Serialise back to the 694-byte on-disk format.
    ///
    /// Fallible rather than panicking: `magic` and `name` are public
    /// fields, and `name` in particular can be assigned directly from a
    /// player-typed string (Task 11's game loop). A `Save` obtained via
    /// `parse` is always safe to serialise, since `parse` already rejects
    /// non-CP866 input -- but a hand-built or hand-edited `Save` is not
    /// guaranteed encodable, so this must report failure rather than
    /// `.expect()`-panic the whole game on an unlucky name.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveError> {
        let mut buf = self.raw.clone();
        put_pstring(&mut buf, OFF_MAGIC, &self.magic)?;
        put_pstring(&mut buf, OFF_NAME, &self.name)?;
        for (i, s) in self.stats.iter().enumerate() {
            buf[OFF_STATE + 2 * i..OFF_STATE + 2 * i + 2].copy_from_slice(&s.to_le_bytes());
        }
        buf[OFF_HP..OFF_HP + 2].copy_from_slice(&self.hp.to_le_bytes());
        buf[OFF_HPMAX..OFF_HPMAX + 2].copy_from_slice(&self.hpmax.to_le_bytes());
        buf[OFF_TAIL..].copy_from_slice(&self.tail);
        Ok(buf)
    }
}
