//! GOPNIK .SAV parsing and writing. 694 bytes, Borland Pascal record layout.
//!
//! ## The file IS guest memory
//!
//! `orig/g.exe` moves the whole record between the file and `DS:369c` with
//! one *untyped* block operation in each direction -- `1000:6c01` /
//! `1000:6c06` (`BlockRead`), `1000:acc3` / `1000:acc8` and `1000:7658` /
//! `1000:765d` (`BlockWrite`), all with `RecSize` = `0x2b6` = 694. So byte
//! `n` of a `.SAV` is `20ae:(0x369c + n)`, which is why every field below
//! carries the DGROUP address it occupies as well as its offset, and why
//! `docs/re/save-format.md` could name all 694 bytes from the disassembly
//! that reads them.
//!
//! ## Round trip must be byte-exact, and what that does NOT prove
//!
//! [`Save::to_bytes`] starts from a **zeroed** buffer and copies exactly two
//! windows through from the source blob: the shortstring padding past each
//! `pstring`'s declared length, which Borland never clears and which carries
//! no meaning. Everything else is rebuilt from a named field.
//!
//! That is deliberate, and it is a change from the previous revision, which
//! started from a copy of the whole input and overwrote only the slices it
//! knew about. Under that shape a field this module *forgot to write* was
//! copied through untouched and the round trip still passed. With the buffer
//! zeroed it comes back as a hole, and
//! `tests/save_roundtrip.rs::all_reference_saves_round_trip_byte_exactly`
//! fails against the five real saves.
//!
//! **A hole is all it catches.** The round trip cannot see a *symmetric*
//! mislocation -- one applied to both [`Save::parse`] and [`Save::to_bytes`]
//! -- because `to_bytes` then writes each byte back exactly where `parse`
//! read it. Measured, not argued: swapping the `joints` and `money` offsets
//! in **both** directions leaves every one of the eleven tests in
//! `tests/save_roundtrip.rs` green, `rust_offsets_match_save_layout_json`
//! and `save_layout_json_fields_tile_the_record` included.
//!
//! What does catch it is `tests/save_load.rs`, and specifically these two,
//! which is where to add a case for a newly named field:
//!
//! * `save_r5_loads_the_character_the_shipped_bytes_describe` (line 82) --
//!   asserts field VALUES against `SAVE_R5`'s documented contents, so a
//!   field reading someone else's byte is wrong even when it round-trips;
//! * `every_named_field_is_actually_written_by_to_bytes` (line 455) --
//!   sets each `data/save_layout.json` field in turn and requires only that
//!   field's own byte span to move.
//!
//! Both go red under the swap above. On the Python side the same job is
//! `tools/test_decode_save.py`'s `test_every_evidence_address_really_
//! references_that_byte`, which resolves each field's cited instruction out
//! of `orig/g.exe`.
//!
//! The offsets below are hand-mirrored from `tools/decode_save.py` (the
//! Task 5 Python reference decoder) and from the layout it emits at
//! `data/save_layout.json`. `tests/save_roundtrip.rs` reads that JSON and
//! asserts these constants agree with it, so the two copies cannot silently
//! drift apart.

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

/// `.SAV` offset + `RECORD_BASE` is the DGROUP address of that byte.
/// Established from flow; see the module doc.
pub const RECORD_BASE: usize = 0x369c;

/// The temporary-buff countdown, `20ae:38cd`.
pub const OFF_BUFF_COUNTDOWN: usize = 0x231;
/// XP not yet spent on a level, `20ae:38ce`.
pub const OFF_XP: usize = 0x232;
/// XP needed for the next level, `20ae:38d0`.
pub const OFF_THRESHOLD: usize = 0x234;
/// `array[1..40] of string[2]`, `20ae:38d2`. Three bytes per level.
pub const OFF_GROWTH_LOG: usize = 0x236;
/// Levels the growth log has a slot for, and the record's own `MAX_LEVEL`.
pub const GROWTH_LOG_SLOTS: usize = 40;
/// One slot: a Pascal `string[2]` -- length byte, then two code bytes.
pub const GROWTH_SLOT_LEN: usize = 3;

/// `^4Gopnik: ^7version 1.02 june,sept 2003`, the `magic` a new character
/// starts with.
///
/// **Established from flow**: `1000:6dcd`..`1000:6ddb` assigns the CS
/// literal at image `0x6489` (file `0x7D59`) into `DS:369c` inside the
/// new-character block, three instructions after `district := 1`. It is
/// therefore per-save state that every save happens to agree on, not a
/// constant the format reserves -- and a `Save` this port builds has to
/// write it, or the original refuses nothing but the player sees a blank
/// banner. Corroborated by all five shipped saves and by
/// `data/probes/saveprobe-fresh-record.json`.
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
    /// A byte the record holds as a Pascal `Boolean` was neither 0 nor 1.
    ///
    /// Not defensiveness: it is what keeps the round trip **total**. The
    /// 23 flag bytes are carried as `bool`, so a 2 could not survive
    /// re-serialisation, and silently rewriting it as 1 would be a
    /// round trip that is byte-exact for every file the game writes and
    /// quietly lossy for one it does not. Every direct store to any of
    /// those bytes image-wide is `mov byte [X],0` or `mov byte [X],1`
    /// (`docs/re/save-format.md`), so the original cannot produce such a
    /// file; a hand-edited one is refused rather than mangled.
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

/// The item, condition and purse block: `.SAV 0x214`..`0x230` and
/// `0x2ae`..`0x2b5`, i.e. `20ae:38b0`..`38cc` and `20ae:394a`..`3951`.
///
/// One struct for two spans because they are one set: the character sheet
/// (`FUN_1000_1a03`) prints them interleaved, and two of the four hand
/// weapons live in each span. Every field's evidence is the sheet's own flag
/// line -- the guard's operand IS the DGROUP address and the label sits
/// inside the arm that guard selects -- with the addresses in
/// `docs/re/save-format.md` and in `data/save_layout.json`'s `evidence`.
///
/// **Kinds come from the code, not from the five saves.** The 23 `bool`
/// fields are Pascal `Boolean` because every direct store to any of them
/// image-wide writes 0 or 1 and nothing else. The five `i16` fields are
/// Pascal `Integer` because every compare against them is a word compare
/// followed by a *signed* conditional; that is also why `20ae:38c4`,
/// `38c6`, `38c8`, `38ca`, `38cc` and `3950` have no reference of their own
/// anywhere in the image -- they are high halves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Items {
    /// `0x214` / `20ae:38b0` -- `^4Сломана челюсть  ` (`1000:2037`).
    pub broken_jaw: bool,
    /// `0x215` / `20ae:38b1` -- `^4Сломана нога  ` (`1000:2099`).
    pub broken_leg: bool,
    /// `0x216` / `20ae:38b2` -- `^2Броня #    ` (`1000:227b`), subtracted
    /// from incoming damage at `1000:4769`.
    pub armour: u8,
    /// `0x217` / `20ae:38b3` -- `^1У тебя есть тёмные очки` (`1000:1cf8`).
    pub dark_glasses: bool,
    /// `0x218` / `20ae:38b4` -- `^1Костюм Abibas(+1) ` (`1000:22a1`).
    pub suit_abibas: bool,
    /// `0x219` / `20ae:38b5` -- `^1Бутсы(+1) ` (`1000:1e81`).
    pub boots: bool,
    /// `0x21a` / `20ae:38b6` -- `^1Кожанка(+2) ` (`1000:2323`).
    pub jacket: bool,
    /// `0x21b` / `20ae:38b7` -- `^1Костюм Adidas(+2) ` (`1000:22fc`).
    pub suit_adidas: bool,
    /// `0x21c` / `20ae:38b8` -- `^1Понтовые бутсы(Урон+2) ` (`1000:1ecf`).
    pub boots_pontovye: bool,
    /// `0x21d` / `20ae:38b9` -- `^1Крутая кожанка(+4) ` (`1000:237e`).
    pub jacket_krutaya: bool,
    /// `0x21e` / `20ae:38ba` -- `^1Кастет(+2) ` (`1000:1eef`).
    pub kastet: bool,
    /// `0x21f` / `20ae:38bb` -- `^1У тебя есть мобильник` (`1000:1cd8`).
    pub mobile: bool,
    /// `0x220` / `20ae:38bc` -- `^1На тебе зоновская наколка` (`1000:1d18`).
    pub prison_tattoo: bool,
    /// `0x221` / `20ae:38bd` -- `^1Крестик(Удача +2) ` (`1000:1be9`).
    pub krestik: bool,
    /// `0x222` / `20ae:38be` -- `^1Кольцо "Гс"(Удача +1) ` (`1000:1c09`).
    pub ring_gs: bool,
    /// `0x223` / `20ae:38bf` -- `^1Кольцо "Пг"(Всё +1) ` (`1000:1c69`).
    pub ring_pg: bool,
    /// `0x224` / `20ae:38c0` -- `^1Мега Кольцо(Всё +4) ` (`1000:1c89`).
    pub mega_ring: bool,
    /// `0x225` / `20ae:38c1` -- `^1Кольцо "Гп"(Самолечение) ` (`1000:1ca9`).
    /// The **fifth** post-kill one-shot: it grants no stat delta, which is
    /// why `data/xp.json`'s `post_kill_stat_events` stops at `0x224`.
    pub ring_gp: bool,
    /// `0x226` / `20ae:38c2` -- `^1Нож(+6) ` (`1000:1fb5`).
    pub nozh: bool,
    /// `0x227` / `20ae:38c3` -- `Пиво #.#л.` (`1000:23d5`), in HALF-litres:
    /// the sheet prints `value div 2` with a `.5` for the odd half.
    pub beer_half_litres: i16,
    /// `0x229` / `20ae:38c5` -- `Косяки #` (`1000:23b4`).
    pub joints: i16,
    /// `0x22b` / `20ae:38c7` -- `Бабки #` (`1000:242e`). Every shop row's
    /// affordability test is `cmp ax,[0x38c7]`; 107 references image-wide.
    pub money: i16,
    /// `0x22d` / `20ae:38c9` -- `Хлам #` (`1000:246a`).
    pub junk: i16,
    /// `0x22f` / `20ae:38cb` -- понтовость на улице, **not** the level at
    /// `20ae:38a6`. Gates the hospital rescue (`1000:4fc4`, >= 10) and
    /// wander draw 2's message (`1000:afdc`, >= 100). The one field of the
    /// span the character sheet does not print.
    pub street_cred: i16,
    /// `0x2ae` / `20ae:394a` -- `^1Зубная защита  ` (`1000:2068`).
    pub tooth_guard: bool,
    /// `0x2af` / `20ae:394b` -- `^1Дубинка(+4)  ` (`1000:1f59`).
    pub dubinka: bool,
    /// `0x2b0` / `20ae:394c` -- `^1Тесак(Урон+9) ` (`1000:2003`).
    pub tesak: bool,
    /// `0x2b1` / `20ae:394d` -- `^1У тебя есть пистолет` (`1000:1d38`).
    pub pistol: bool,
    /// `0x2b2` / `20ae:394e` -- `^1 с гушителем` (`1000:1d6a`).
    pub silencer: bool,
    /// `0x2b3` / `20ae:394f` -- `^1! патронов - #` (`1000:1d8a`). A **word**:
    /// the guard is `cmp word`, `bmar` row 7 adds three with
    /// `add word [0x394f],3` at `1000:cd0a`, and `20ae:3950` is referenced
    /// nowhere, so `0x2b4` is this field's high byte and not a 32nd flag.
    pub cartridges: i16,
    /// `0x2b5` / `20ae:3951` -- the church's sermon stage, 0..2. Raised at
    /// `1000:7dc7` and `1000:7f5b`; read at `1000:7c76`/`7ceb`/`7dcb` and at
    /// `1000:8247`.
    pub church_stage: u8,
}

/// One growth-log slot: a Pascal `string[2]` -- length byte, then two stat
/// code bytes (`'1'`..`'4'`).
///
/// All three bytes are state, not just the two codes: the writer appends one
/// code at a time (`1000:2657`/`1000:2661`/`1000:267a`) so the length is 1
/// mid-level-up, and the flee penalty clears **only** the length byte at
/// `1000:497d` and leaves the payload behind.
pub type GrowthSlot = [u8; GROWTH_SLOT_LEN];

pub struct Save {
    pub magic: String,
    pub name: String,
    /// The eight words at `OFF_STATE`. Named by index rather than split into
    /// eight struct fields because they are the same 16-byte block
    /// `tools/capture_combat_vectors.py`'s `FIELDS_U16` reads out of the
    /// live fighter record (`docs/re/combat.md`, "The fighter record") --
    /// keeping one array here mirrors that layout instead of inventing a
    /// second one. Index -> meaning, pinned by Task 9
    /// (`docs/re/save-format.md`):
    ///
    /// 0. `rank_index` -- the class; the stored word is the creation
    ///    prompt's answer plus 3 (`1000:71b8`).
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
    /// `0x231` / `20ae:38cd` -- the joint buff's countdown.
    pub buff_countdown: u8,
    /// `0x232` / `20ae:38ce`.
    pub xp: u16,
    /// `0x234` / `20ae:38d0`.
    pub threshold: u16,
    /// `0x236` / `20ae:38d2` -- `array[1..40] of string[2]`. Slot `i` here
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
/// accepts. Both sides of that boundary are pinned by
/// `tests/save_roundtrip.rs::the_shortstring_cap_admits_255_bytes_and_refuses_256`;
/// `cargo mutants` reported the `>` -> `>=` mutant as a survivor before that
/// test existed.
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
    /// **Observed in the original**, not chosen here:
    /// `data/probes/saveprobe-fresh-record.json` is a dump of `20ae:369c`
    /// taken after driving character creation in `orig/g.exe` under qemu,
    /// and every byte outside the stat words, `magic`, `name` and
    /// `threshold` is zero -- both shortstring paddings included. So the
    /// zeroes below are the original's own, and a save this port writes for
    /// a fresh character can be byte-identical to one the original would
    /// write.
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
