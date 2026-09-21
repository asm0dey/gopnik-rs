//! GOPNIK .SAV parsing and writing. A 694-byte fixed-layout record: each
//! byte occupies a specific field, mirroring the game's own in-memory
//! character record.

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

pub const RECORD_BASE: usize = 0x369c;

/// The temporary-buff countdown.
pub const OFF_BUFF_COUNTDOWN: usize = 0x231;
/// XP not yet spent on a level.
pub const OFF_XP: usize = 0x232;
/// XP needed for the next level.
pub const OFF_THRESHOLD: usize = 0x234;
/// The growth log: 40 entries, three bytes each.
pub const OFF_GROWTH_LOG: usize = 0x236;
/// Levels the growth log has a slot for, and the record's own `MAX_LEVEL`.
pub const GROWTH_LOG_SLOTS: usize = 40;
/// One slot: a Pascal `string[2]` -- length byte, then two code bytes.
pub const GROWTH_SLOT_LEN: usize = 3;

/// `^4Gopnik: ^7version 1.02 june,sept 2003`, the `magic` a new character
/// starts with.
///
/// This is per-save state that every save happens to agree on, not a
/// constant the format reserves -- a `Save` this port builds has to write
/// it, or the player sees a blank banner instead.
pub const MAGIC: &str = "^4Gopnik: ^7version 1.02 june,sept 2003";

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
    /// A `pstring` field's CP866-encoded bytes exceed the format's 255-byte
    /// cap (a Pascal `string[255]` length prefix is a single byte, so this
    /// is a real property of the on-disk format, not an internal invariant).
    /// Carries the actual encoded length. Note this is a byte count, not a
    /// `char` count: CP866 is one byte per character, but the same string
    /// as Rust `String` (UTF-8) can be up to two bytes per character for
    /// non-ASCII (e.g. Cyrillic) text.
    TooLong(usize),
    /// A byte the record holds as a boolean was neither 0 nor 1.
    ///
    /// Not defensiveness: it is what keeps the round trip **total**. The
    /// 23 flag bytes are carried as `bool`, so a 2 could not survive
    /// re-serialisation, and silently rewriting it as 1 would be a round
    /// trip that is byte-exact for every file the game writes and quietly
    /// lossy for one it does not. The original itself never writes
    /// anything but 0 or 1 to these bytes, so a hand-edited file is
    /// refused rather than mangled.
    NotBoolean { off: usize, value: u8 },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::BadSize(n) => write!(f, "expected {SIZE} bytes, got {n}"),
            SaveError::BadCp866Bytes => write!(f, "bytes are not valid CP866"),
            SaveError::Unmappable(c) => {
                write!(f, "character {c:?} has no CP866 representation")
            }
            SaveError::TooLong(n) => {
                write!(
                    f,
                    "encoded length {n} exceeds {PSTRING_CAP}-byte shortstring cap"
                )
            }
            SaveError::NotBoolean { off, value } => write!(
                f,
                "offset 0x{off:03x} (20ae:{:04x}) holds {value}, and the original \
                 only ever stores 0 or 1 there",
                RECORD_BASE + off
            ),
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

/// The item, condition and purse block.
///
/// One struct for two spans because they are one set: the character sheet
/// prints them interleaved, and two of the four hand weapons live in each
/// span.
///
/// **Kinds come from the code, not from the five saves.** The 23 `bool`
/// fields are booleans because every direct store to any of them writes 0
/// or 1 and nothing else. The five `i16` fields are signed because every
/// compare against them uses a signed conditional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Items {
    /// `^4Сломана челюсть  `.
    pub broken_jaw: bool,
    /// `^4Сломана нога  `.
    pub broken_leg: bool,
    /// `^2Броня #    `, subtracted from incoming damage.
    pub armour: u8,
    /// `^1У тебя есть тёмные очки`.
    pub dark_glasses: bool,
    /// `^1Костюм Abibas(+1) `.
    pub suit_abibas: bool,
    /// `^1Бутсы(+1) `.
    pub boots: bool,
    /// `^1Кожанка(+2) `.
    pub jacket: bool,
    /// `^1Костюм Adidas(+2) `.
    pub suit_adidas: bool,
    /// `^1Понтовые бутсы(Урон+2) `.
    pub boots_pontovye: bool,
    /// `^1Крутая кожанка(+4) `.
    pub jacket_krutaya: bool,
    /// `^1Кастет(+2) `.
    pub kastet: bool,
    /// `^1У тебя есть мобильник`.
    pub mobile: bool,
    /// `^1На тебе зоновская наколка`.
    pub prison_tattoo: bool,
    /// `^1Крестик(Удача +2) `.
    pub krestik: bool,
    /// `^1Кольцо "Гс"(Удача +1) `.
    pub ring_gs: bool,
    /// `^1Кольцо "Пг"(Всё +1) `.
    pub ring_pg: bool,
    /// `^1Мега Кольцо(Всё +4) `.
    pub mega_ring: bool,
    /// `^1Кольцо "Гп"(Самолечение) `. The fifth post-kill one-shot: it
    /// grants no stat delta.
    pub ring_gp: bool,
    /// `^1Нож(+6) `.
    pub nozh: bool,
    /// `Пиво #.#л.`, in half-litres: the sheet prints `value div 2` with
    /// a `.5` for the odd half.
    pub beer_half_litres: i16,
    /// `Косяки #`.
    pub joints: i16,
    /// `Бабки #`. Every shop row's affordability test compares against
    /// this value.
    pub money: i16,
    /// `Хлам #`.
    pub junk: i16,
    /// понтовость на улице, not the level. Gates the hospital rescue
    /// (>= 10) and wander draw 2's message (>= 100). The one field of the
    /// span the character sheet does not print.
    pub street_cred: i16,
    /// `^1Зубная защита  `.
    pub tooth_guard: bool,
    /// `^1Дубинка(+4)  `.
    pub dubinka: bool,
    /// `^1Тесак(Урон+9) `.
    pub tesak: bool,
    /// `^1У тебя есть пистолет`.
    pub pistol: bool,
    /// `^1 с гушителем`.
    pub silencer: bool,
    /// `^1! патронов - #`. Stored as a word, not a byte: one menu row's
    /// purchase adds three rounds at once.
    pub cartridges: i16,
    /// The church's sermon stage, 0..2.
    pub church_stage: u8,
}

/// One growth-log slot: a Pascal `string[2]` -- length byte, then two stat
/// code bytes (`'1'`..`'4'`).
///
/// All three bytes are state, not just the two codes: the writer appends one
/// code at a time so the length is 1 mid-level-up, and the flee penalty
/// clears **only** the length byte and leaves the payload behind.
pub type GrowthSlot = [u8; GROWTH_SLOT_LEN];

pub struct Save {
    pub magic: String,
    pub name: String,
    /// The eight words at `OFF_STATE`. Named by index rather than split into
    /// eight struct fields because they mirror the fighter record's own
    /// layout. Index -> meaning:
    ///
    /// 0. `rank_index` -- the class; the stored word is the creation
    ///    prompt's answer plus 3.
    /// 1. `strength`
    /// 2. `agility`
    /// 3. `vitality`
    /// 4. `luck`
    /// 5. `level` ("понтовость", 0..40)
    /// 6. `dmg_min`
    /// 7. `dmg_max`
    pub stats: [u16; 8],
    pub hp: u16,
    pub hpmax: u16,
    /// `0x214`..`0x230` and `0x2ae`..`0x2b5`.
    pub items: Items,
    /// The joint buff's countdown.
    pub buff_countdown: u8,
    pub xp: u16,
    pub threshold: u16,
    /// `array[1..40] of string[2]`. Slot `i` here
    /// is the original's element `i + 1`; there is no element 0.
    pub growth_log: [GrowthSlot; GROWTH_LOG_SLOTS],
    /// The 255 payload bytes of the `magic` slot, exactly as they were.
    /// Only the bytes past the declared length are used on write; the rest
    /// is overwritten by `magic`. See the module doc.
    magic_pad: [u8; PSTRING_CAP],
    /// The same, for `name`.
    name_pad: [u8; PSTRING_CAP],
}

fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn i16le(b: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([b[off], b[off + 1]])
}

/// A record byte the original only ever stores 0 or 1 into.
fn boolean(b: &[u8], off: usize) -> Result<bool, SaveError> {
    match b[off] {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(SaveError::NotBoolean { off, value }),
    }
}

/// Read a shortstring: `n` payload bytes after the length byte.
///
/// **Deliberately unchecked, unlike [`SaveError::NotBoolean`].** `n` comes
/// from a single byte so it is at most 255, and both slots are 256 wide at
/// `0x000` and `0x100`, so the read ends at `0x100` or `0x200` at the
/// widest -- inside any input [`Save::parse`] accepts, since `parse` rejects
/// anything that is not exactly `SIZE` bytes before reaching here. There is
/// no malformed-input case to refuse: every 694-byte blob has a valid
/// shortstring in both slots by construction. `NotBoolean` exists because a
/// flag byte genuinely can hold a value the format cannot represent; a
/// length byte cannot.
fn get_pstring(b: &[u8], off: usize) -> Result<String, SaveError> {
    let n = b[off] as usize;
    cp866_decode(&b[off + 1..off + 1 + n])
}

/// Write a shortstring: the length byte, then the payload.
///
/// The cap is `>`, not `>=`: a Pascal `string[255]`'s length byte holds
/// `0..=255`, so **255 payload bytes is legal** -- it is the longest string
/// the format can express, and rejecting it would refuse a name the original
/// accepts.
///
/// **No slot can overflow.** The two `pstring` slots are 256 bytes each at
/// `OFF_MAGIC` = `0x000` and `OFF_NAME` = `0x100`, so the widest possible
/// write -- one length byte plus 255 payload bytes -- ends at `0x100` and
/// `0x200` respectively, both inside the 694-byte record. That is a property
/// of the layout, not of the caller, which is why there is no bound check
/// here: one could never fire.
fn put_pstring(buf: &mut [u8], off: usize, s: &str) -> Result<(), SaveError> {
    let raw = cp866_encode(s)?;
    if raw.len() > PSTRING_CAP {
        return Err(SaveError::TooLong(raw.len()));
    }
    buf[off] = raw.len() as u8;
    buf[off + 1..off + 1 + raw.len()].copy_from_slice(&raw);
    // Bytes past the length are left exactly as the padding put them.
    Ok(())
}

fn put_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_i16(buf: &mut [u8], off: usize, v: i16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

impl Save {
    /// `name` holds the original bytes, markup included, because round-trip
    /// must be byte-exact. Everything user-facing goes through here.
    pub fn display_name(&self) -> String {
        crate::text::strip(&self.name)
    }

    /// The record a brand-new character starts with, before any field is
    /// filled in.
    ///
    /// Every byte outside the stat words, `magic`, `name` and `threshold`
    /// is zero -- both shortstring paddings included -- so a save this port
    /// writes for a fresh character is byte-identical to one the original
    /// would write.
    pub fn blank() -> Save {
        Save {
            magic: MAGIC.to_string(),
            name: String::new(),
            stats: [0; 8],
            hp: 0,
            hpmax: 0,
            items: Items::default(),
            buff_countdown: 0,
            xp: 0,
            threshold: 0,
            growth_log: [[0; GROWTH_SLOT_LEN]; GROWTH_LOG_SLOTS],
            magic_pad: [0; PSTRING_CAP],
            name_pad: [0; PSTRING_CAP],
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Save, SaveError> {
        if bytes.len() != SIZE {
            return Err(SaveError::BadSize(bytes.len()));
        }
        let mut stats = [0u16; 8];
        for (i, s) in stats.iter_mut().enumerate() {
            *s = u16le(bytes, OFF_STATE + 2 * i);
        }
        let mut growth_log = [[0u8; GROWTH_SLOT_LEN]; GROWTH_LOG_SLOTS];
        for (i, slot) in growth_log.iter_mut().enumerate() {
            let at = OFF_GROWTH_LOG + i * GROWTH_SLOT_LEN;
            slot.copy_from_slice(&bytes[at..at + GROWTH_SLOT_LEN]);
        }
        let items = Items {
            broken_jaw: boolean(bytes, 0x214)?,
            broken_leg: boolean(bytes, 0x215)?,
            armour: bytes[0x216],
            dark_glasses: boolean(bytes, 0x217)?,
            suit_abibas: boolean(bytes, 0x218)?,
            boots: boolean(bytes, 0x219)?,
            jacket: boolean(bytes, 0x21a)?,
            suit_adidas: boolean(bytes, 0x21b)?,
            boots_pontovye: boolean(bytes, 0x21c)?,
            jacket_krutaya: boolean(bytes, 0x21d)?,
            kastet: boolean(bytes, 0x21e)?,
            mobile: boolean(bytes, 0x21f)?,
            prison_tattoo: boolean(bytes, 0x220)?,
            krestik: boolean(bytes, 0x221)?,
            ring_gs: boolean(bytes, 0x222)?,
            ring_pg: boolean(bytes, 0x223)?,
            mega_ring: boolean(bytes, 0x224)?,
            ring_gp: boolean(bytes, 0x225)?,
            nozh: boolean(bytes, 0x226)?,
            beer_half_litres: i16le(bytes, 0x227),
            joints: i16le(bytes, 0x229),
            money: i16le(bytes, 0x22b),
            junk: i16le(bytes, 0x22d),
            street_cred: i16le(bytes, 0x22f),
            tooth_guard: boolean(bytes, 0x2ae)?,
            dubinka: boolean(bytes, 0x2af)?,
            tesak: boolean(bytes, 0x2b0)?,
            pistol: boolean(bytes, 0x2b1)?,
            silencer: boolean(bytes, 0x2b2)?,
            cartridges: i16le(bytes, 0x2b3),
            church_stage: bytes[0x2b5],
        };
        let mut magic_pad = [0u8; PSTRING_CAP];
        magic_pad.copy_from_slice(&bytes[OFF_MAGIC + 1..OFF_MAGIC + 1 + PSTRING_CAP]);
        let mut name_pad = [0u8; PSTRING_CAP];
        name_pad.copy_from_slice(&bytes[OFF_NAME + 1..OFF_NAME + 1 + PSTRING_CAP]);
        Ok(Save {
            magic: get_pstring(bytes, OFF_MAGIC)?,
            name: get_pstring(bytes, OFF_NAME)?,
            stats,
            hp: u16le(bytes, OFF_HP),
            hpmax: u16le(bytes, OFF_HPMAX),
            items,
            buff_countdown: bytes[OFF_BUFF_COUNTDOWN],
            xp: u16le(bytes, OFF_XP),
            threshold: u16le(bytes, OFF_THRESHOLD),
            growth_log,
            magic_pad,
            name_pad,
        })
    }

    /// Serialise back to the 694-byte on-disk format.
    ///
    /// Fallible rather than panicking: `magic` and `name` are public
    /// fields, and `name` in particular can be assigned directly from a
    /// player-typed string. A `Save` obtained via `parse` is always safe to
    /// serialise, since `parse` already rejects non-CP866 input -- but a
    /// hand-built or hand-edited `Save` is not guaranteed encodable, so
    /// this must report failure rather than `.expect()`-panic the whole
    /// game on an unlucky name.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveError> {
        let mut buf = vec![0u8; SIZE];
        // The ONLY bytes copied through rather than rebuilt: shortstring
        // padding, which carries no meaning (see the module doc). Whatever
        // `put_pstring` does not overwrite stays as it was.
        buf[OFF_MAGIC + 1..OFF_MAGIC + 1 + PSTRING_CAP].copy_from_slice(&self.magic_pad);
        buf[OFF_NAME + 1..OFF_NAME + 1 + PSTRING_CAP].copy_from_slice(&self.name_pad);
        put_pstring(&mut buf, OFF_MAGIC, &self.magic)?;
        put_pstring(&mut buf, OFF_NAME, &self.name)?;
        for (i, s) in self.stats.iter().enumerate() {
            put_u16(&mut buf, OFF_STATE + 2 * i, *s);
        }
        put_u16(&mut buf, OFF_HP, self.hp);
        put_u16(&mut buf, OFF_HPMAX, self.hpmax);

        let it = &self.items;
        buf[0x214] = it.broken_jaw.into();
        buf[0x215] = it.broken_leg.into();
        buf[0x216] = it.armour;
        buf[0x217] = it.dark_glasses.into();
        buf[0x218] = it.suit_abibas.into();
        buf[0x219] = it.boots.into();
        buf[0x21a] = it.jacket.into();
        buf[0x21b] = it.suit_adidas.into();
        buf[0x21c] = it.boots_pontovye.into();
        buf[0x21d] = it.jacket_krutaya.into();
        buf[0x21e] = it.kastet.into();
        buf[0x21f] = it.mobile.into();
        buf[0x220] = it.prison_tattoo.into();
        buf[0x221] = it.krestik.into();
        buf[0x222] = it.ring_gs.into();
        buf[0x223] = it.ring_pg.into();
        buf[0x224] = it.mega_ring.into();
        buf[0x225] = it.ring_gp.into();
        buf[0x226] = it.nozh.into();
        put_i16(&mut buf, 0x227, it.beer_half_litres);
        put_i16(&mut buf, 0x229, it.joints);
        put_i16(&mut buf, 0x22b, it.money);
        put_i16(&mut buf, 0x22d, it.junk);
        put_i16(&mut buf, 0x22f, it.street_cred);

        buf[OFF_BUFF_COUNTDOWN] = self.buff_countdown;
        put_u16(&mut buf, OFF_XP, self.xp);
        put_u16(&mut buf, OFF_THRESHOLD, self.threshold);
        for (i, slot) in self.growth_log.iter().enumerate() {
            let at = OFF_GROWTH_LOG + i * GROWTH_SLOT_LEN;
            buf[at..at + GROWTH_SLOT_LEN].copy_from_slice(slot);
        }

        buf[0x2ae] = it.tooth_guard.into();
        buf[0x2af] = it.dubinka.into();
        buf[0x2b0] = it.tesak.into();
        buf[0x2b1] = it.pistol.into();
        buf[0x2b2] = it.silencer.into();
        put_i16(&mut buf, 0x2b3, it.cartridges);
        buf[0x2b5] = it.church_stage;
        Ok(buf)
    }
}
