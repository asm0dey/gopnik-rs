//! Saving and loading: [`Game`] <-> [`Save`], and the two files on disk.
//!
//! ## Where the game saves, and where it does not
//!
//! There is no typed save command; saving happens at exactly two places:
//!
//! * **The mage's paid save.** `Рушель Блаво` asks
//!   `Ты хочешь сохраниться?`, and on `y` charges `district * 50`, writes
//!   the save into the fixed slot `save_r0.sav`, writes the seven
//!   discovery flags into `places.sav`, and prints
//!   `^0Сохранено! ^1Можешь беспредельничать дальше.`
//!   ([`Game::mage`](crate::game::Game)).
//! * **The district-advance autosave.** After the discovery-flag resets it
//!   prints `^0Хочешь сохранить свои достижения?`, and on a `y` writes
//!   `save_r<district>.sav` -- the district *after* the increment, which
//!   is why the shipped corpus is `SAVE_R2`..`SAVE_R5` and has no
//!   `SAVE_R1` -- then prints `^1Сохранено в save_r` + the digit + `.sav`
//!   ([`Game::district_advance`]). This save does not touch `places.sav`,
//!   so slots 2..5 start with their discovery flags clear and only the
//!   class bonus restores any of them.
//!
//! ## The load path
//!
//! On start, the game scans for save files and prints one line per slot
//! found -- `^1Можно начать с ` + digit + ` района`, or
//! `^1Можно начать с того места где ты сохранился` for slot `0` --
//! separated by `^1или`. With none found it goes straight to new-character
//! creation. Otherwise it prints
//! `^0Нажми цифру с какого района начать. 1-начать сначала` and waits for
//! a keypress; only `'2'`, `'3'`, `'4'`, `'5'` and `'0'` are accepted --
//! anything else, `1` included even though the prompt suggests it, starts
//! a new character.
//!
//! An accepted digit opens the matching save file; a read failure prints
//! `^6Чё-то глюкануло - нaверно нет такого сейва, Default:1` and also
//! falls through to creation. On success it prints
//! `^0Загружено из save_r` + the digit, and sets the district from the
//! digit itself -- **the district is not stored in the save record**.
//!
//! Slot `0` is the odd one out: only it reads `places.sav` back, and
//! derives its district as `level div 10 + 1`. Slots 2..5 never touch
//! `places.sav`.

use crate::game::Game;
use crate::locations::Places;
use crate::model::Fighter;
use crate::progress::{self, Progress};
use crate::save::{GrowthSlot, Items, Save, SaveError, GROWTH_LOG_SLOTS};
use crate::term;
use std::io;
use std::path::{Path, PathBuf};

/// The `^7 ` the original prefixes onto every stored name, both on
/// creation and after a rename.
///
/// This port keeps [`Fighter::name`](crate::model::Fighter) *without* the
/// prefix and applies it only here, at the save-format boundary, rather
/// than changing what every combat line renders.
pub const NAME_PREFIX: &str = "^7 ";

/// The slot keys accepted for loading, in the order they're compared.
/// Anything else -- `'1'` included, which is the key the prompt itself
/// suggests -- starts a new character.
///
/// **This is the load-key test and nothing else.** [`present_slots`]'s own
/// scan is a different mechanism with a different alphabet; do not reuse
/// this constant for both.
pub const SLOT_KEYS: [char; 5] = ['2', '3', '4', '5', '0'];

/// The save-file search mask: `save_r?.sav`.
const SLOT_MASK_PREFIX: &str = "save_r";
const SLOT_MASK_SUFFIX: &str = ".sav";

/// The mage's fixed filename: `save_r0.sav`.
pub const MAGE_SAVE: &str = "save_r0.sav";
/// The discovery-flags filename: `places.sav`.
pub const PLACES_SAVE: &str = "places.sav";

/// Seven one-byte reads and seven one-byte writes; [`crate::locations::TRACKED`]
/// is the order.
pub const PLACES_BYTES: usize = 7;

/// `save_r<slot>.sav`, the name both the load scan and the autosave
/// build.
pub fn slot_filename(slot: char) -> String {
    format!("save_r{slot}.sav")
}

/// Every file in `dir` whose name matches the mask `save_r?.sav`, as the
/// character the `?` matched.
///
/// **`?` is a DOS wildcard and matches ANY single character**, not just a
/// slot key. So `save_r1.sav` and `save_rx.sav` are both listed and
/// prompted for, and neither is a key the load menu accepts: typing what
/// they show starts a new character instead. The scan and the key check
/// are two separate steps.
///
/// The game itself only ever writes `save_r0` and `save_r2`..`save_r5`, so
/// nothing in ordinary play produces anything else.
///
/// Two behaviours here are the port's own choice, not the original's:
///
/// * **Order is by name, not directory order.** Directory order is not
///   portable and nothing in the game depends on it -- it only decides
///   which menu line prints first. Sorted is deterministic.
/// * **Both filename cases match.** DOS filenames are case-insensitive;
///   this host is not, and the shipped save files are uppercase while the
///   game writes lowercase.
pub fn present_slots(dir: &Path) -> Vec<char> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, char)> = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_lowercase();
        if !(lower.len() == SLOT_MASK_PREFIX.len() + 1 + SLOT_MASK_SUFFIX.len()
            && lower.starts_with(SLOT_MASK_PREFIX)
            && lower.ends_with(SLOT_MASK_SUFFIX))
        {
            continue;
        }
        // The character the `?` matched, taken from the name AS STORED so
        // the menu prints what is on disk, not a case-folded one.
        let Some(c) = name.chars().nth(SLOT_MASK_PREFIX.len()) else {
            continue;
        };
        out.push((lower, c));
    }
    out.sort();
    out.dedup_by(|a, b| a.1 == b.1);
    out.into_iter().map(|(_, c)| c).collect()
}

/// Read `save_r<slot>.sav`, trying the name the game writes and then the
/// one the shipped corpus carries. See [`present_slots`].
///
/// **Any** failure on the first name falls through to the second, not only
/// `NotFound`. An earlier revision special-cased `NotFound`, which
/// `cargo mutants` reported as a survivor -- and rightly: the distinction is
/// untestable in the ordinary case and it is the wrong one anyway, because
/// the two names denote the same file on the FAT filesystem the original
/// ran on. Whichever name is readable wins. If neither is, the error
/// reported is the LOWERCASE one, because that is the name this port would
/// have created.
fn read_slot(dir: &Path, slot: char) -> io::Result<Vec<u8>> {
    let lower = slot_filename(slot);
    match std::fs::read(dir.join(&lower)) {
        Ok(b) => Ok(b),
        Err(e) => std::fs::read(dir.join(lower.to_uppercase())).map_err(|_| e),
    }
}

impl Game {
    /// The 694-byte record this `Game` would be saved as.
    ///
    /// Every field of [`Save`] is filled from live state: there is no
    /// template and no carried-over blob.
    ///
    /// **What is deliberately NOT here, and why that is faithful.** These
    /// fields live on `Game` but are not part of the saved record, matching
    /// the original, which does not persist them either:
    ///
    /// - [`Game::harder_encounters`]
    /// - [`Game::fight_accepted`]
    /// - [`Game::market_ban_countdown`]
    /// - [`Game::club_ban_countdown`]
    /// - [`Game::den_errand_1_pending`]
    /// - [`Game::den_errand_2_pending`]
    /// - [`Game::club_stake`] -- club-local, reset on every visit
    /// - [`Game::rector_showdown`]
    /// - [`Game::dealer_delivery_counter`]
    /// - [`Game::den_loan_credit`]
    /// - [`Game::district`] -- taken from the save-slot digit on load instead
    /// - [`Game::places`] -- persisted separately, in `places.sav`
    pub fn to_save(&self) -> Save {
        let p = &self.player;
        let mut save = Save::blank();
        // The name field already holds `^7 ` + the typed name (set at
        // creation, or on rename), so the record is a straight copy.
        save.name = p.name.clone();
        // The eight stat words, including class and level, then hp and
        // hpmax.
        save.stats = [
            p.class, p.strength, p.agility, p.vitality, p.luck, p.level, p.dmg_min, p.dmg_max,
        ];
        save.hp = p.hp;
        save.hpmax = p.hpmax;
        // `Progress` widened xp and the threshold to `u32`; the original
        // fields are 16-bit words, so the narrowing here just gives that
        // width back, not a cap. Same for `armor` below (`u16` here, one
        // byte in the record) and for the five `Integer`s further down.
        save.buff_countdown = self.buff_countdown;
        save.xp = self.progress.xp;
        save.threshold = self.progress.threshold;
        save.growth_log = growth_log_to_record(&self.progress);
        save.items = Items {
            broken_jaw: p.broken_jaw,
            broken_leg: p.broken_leg,
            armour: p.armor,
            dark_glasses: self.dark_glasses,
            suit_abibas: self.wear_suit_abibas,
            boots: self.wear_boots,
            jacket: self.wear_jacket,
            suit_adidas: self.wear_suit_adidas,
            boots_pontovye: self.wear_boots_pontovye,
            jacket_krutaya: self.wear_jacket_krutaya,
            kastet: self.weapon_kastet,
            mobile: self.has_mobile,
            prison_tattoo: self.prison_tattoo,
            krestik: self.charm_krestik,
            ring_gs: self.charm_ring,
            ring_pg: self.oneshot_gift_1,
            mega_ring: self.oneshot_gift_2,
            ring_gp: self.ring_gospodi_pomilui,
            nozh: self.weapon_nozhik,
            beer_half_litres: p.beer_dl,
            joints: p.joints,
            money: p.money,
            junk: p.junk,
            street_cred: self.pontovost_street,
            tooth_guard: self.tooth_guard,
            dubinka: self.weapon_dubinka,
            tesak: self.weapon_tesak,
            pistol: self.pistol.owned,
            silencer: self.pistol.silencer,
            cartridges: self.pistol.cartridges,
            church_stage: self.church_visits,
        };
        save
    }

    /// Rebuild a `Game` from a loaded record.
    ///
    /// `district` and `places` come from the caller because **neither is in
    /// the record**: the district is derived from the save-slot digit, or
    /// `level div 10 + 1` for slot `0`, and the seven discovery flags live in
    /// `places.sav`. Slots 2..5 never read that file, so their `places` is
    /// all-clear.
    ///
    /// The district-5 rector-showdown arm, the class bonus, and the den
    /// loan credit run on **every** entry into the game, new character or
    /// loaded save, so they are all re-applied here, after BOTH `places`
    /// and `district` are installed -- the district-5 arm reads
    /// `district`, which must therefore be set before this call, not after
    /// it.
    pub fn from_save(save: &Save, places: Places, district: u8, seed: u32) -> Game {
        let it = &save.items;
        let player = Fighter {
            // The record's name string is copied in with its prefix
            // included -- no strip.
            name: save.name.clone(),
            class: save.stats[0],
            strength: save.stats[1],
            agility: save.stats[2],
            vitality: save.stats[3],
            luck: save.stats[4],
            level: save.stats[5],
            dmg_min: save.stats[6],
            dmg_max: save.stats[7],
            hp: save.hp,
            hpmax: save.hpmax,
            armor: it.armour,
            broken_jaw: it.broken_jaw,
            broken_leg: it.broken_leg,
            // This is a signed value and the original can keep a negative
            // one, so there is nothing to clamp.
            joints: it.joints,
            // `crate::model::Fighter::stoned` and `Game::buff_countdown`
            // are two models of one variable; the countdown is the one the
            // original keeps, so it decides.
            stoned: save.buff_countdown != 0,
            // This is a signed value and the original can keep a negative
            // one, so there is nothing to clamp.
            beer_dl: it.beer_half_litres,
            money: it.money,
            // Likewise.
            junk: it.junk,
        };
        let progress = Progress {
            xp: save.xp,
            threshold: save.threshold,
            growth_log: growth_log_from_record(&save.growth_log),
        };
        let mut g = Game::new(player, progress, seed);
        // `Game::new` is the NEW-character path and marks the vet and the
        // market as found. A load never does that, so the flags are
        // replaced wholesale here rather than added to.
        g.places = places;
        // `district` MUST be set before the re-applied `apply_class_bonus`
        // call below: it reads `district` to decide whether this load arms
        // `rector_showdown`. `Game::new`'s own internal call ran with
        // `district: 1`, which is right for a NEW character but would be
        // wrong here, before this field is replaced with the loaded value.
        g.district = district;
        g.apply_class_bonus();
        g.has_mobile = it.mobile;
        g.dark_glasses = it.dark_glasses;
        g.prison_tattoo = it.prison_tattoo;
        g.oneshot_gift_1 = it.ring_pg;
        g.oneshot_gift_2 = it.mega_ring;
        g.ring_gospodi_pomilui = it.ring_gp;
        g.pontovost_street = it.street_cred;
        g.buff_countdown = save.buff_countdown;
        g.tooth_guard = it.tooth_guard;
        g.charm_krestik = it.krestik;
        g.charm_ring = it.ring_gs;
        g.weapon_kastet = it.kastet;
        g.weapon_dubinka = it.dubinka;
        g.weapon_nozhik = it.nozh;
        g.weapon_tesak = it.tesak;
        g.wear_suit_abibas = it.suit_abibas;
        g.wear_boots = it.boots;
        g.wear_jacket = it.jacket;
        g.wear_suit_adidas = it.suit_adidas;
        g.wear_boots_pontovye = it.boots_pontovye;
        g.wear_jacket_krutaya = it.jacket_krutaya;
        g.church_visits = it.church_stage;
        g.pistol.owned = it.pistol;
        g.pistol.silencer = it.silencer;
        g.pistol.cartridges = it.cartridges;
        g
    }

    /// Write the 694-byte record, and **only** that.
    ///
    /// The two writers differ here and the difference is real: the mage
    /// writes `places.sav` as well, and the district-advance autosave does
    /// **not**. So the flags are [`Game::write_places`]'s job, called by
    /// the writer that actually does it.
    ///
    /// Returns the path so the caller can print it; the mage's own
    /// confirmation names no file, but the district autosave's does.
    pub fn write_save_as(&self, dir: &Path, name: &str) -> io::Result<PathBuf> {
        let bytes = self
            .to_save()
            .to_bytes()
            .map_err(|e: SaveError| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let path = dir.join(name);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    /// The seven discovery flags, one byte each.
    /// `crate::locations::TRACKED` fixes their order.
    pub fn write_places(&self, dir: &Path) -> io::Result<PathBuf> {
        let path = dir.join(PLACES_SAVE);
        std::fs::write(&path, self.places.to_bytes())?;
        Ok(path)
    }

    /// The mage's paid save: the record, then the flags, then the
    /// confirmation.
    ///
    /// The money has already left by the time this is called -- it debits
    /// before the file is opened, and the original does not refund a
    /// failed write either.
    pub fn mage_save(&self) -> io::Result<PathBuf> {
        let dir = self.save_dir.clone();
        let path = self.write_save_as(&dir, MAGE_SAVE)?;
        self.write_places(&dir)?;
        term::println("^0Сохранено! ^1Можешь беспредельничать дальше.");
        Ok(path)
    }
}

/// `Progress::growth_log`'s two codes per level -> the record's
/// `array[1..40] of string[2]`.
///
/// The port's `Progress` keeps slot 0 to preserve the original's 1-based
/// indexing and holds only the two code bytes, so the Pascal length byte is
/// derived: 2 when both codes are set, 1 when only the first is, 0 when
/// neither. **That loses one distinction the original can express** -- the
/// flee penalty can clear only the length byte and leave the payload, a
/// "length 0, codes still there" state no `Progress` value maps to. A
/// record parsed into a `Game` and written back out therefore normalises
/// such a slot to three zero bytes. It is a port limitation, not a finding
/// about the original.
fn growth_log_to_record(p: &Progress) -> [GrowthSlot; GROWTH_LOG_SLOTS] {
    let mut out = [[0u8; 3]; GROWTH_LOG_SLOTS];
    for (i, slot) in out.iter_mut().enumerate() {
        let entry = p.growth_log.get(i + 1).copied().unwrap_or_default();
        let len = entry.iter().take_while(|&&c| c != 0).count() as u8;
        *slot = [len, entry[0], entry[1]];
    }
    out
}

/// The inverse. The length byte is dropped: a code byte is only meaningful
/// while it is inside the declared length, so anything past it is read as
/// absent, which is what `progress::Stat::from_code` already does with a
/// `0`.
fn growth_log_from_record(rec: &[GrowthSlot; GROWTH_LOG_SLOTS]) -> [progress::GrowthEntry; 41] {
    let mut out = [[0u8; 2]; 41];
    for (i, slot) in rec.iter().enumerate() {
        let len = usize::from(slot[0]).min(2);
        out[i + 1][..len].copy_from_slice(&slot[1..1 + len]);
    }
    out
}

/// The three arms below are three different things, not two levels of
/// "maybe".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotMenu {
    /// The directory holds no `save_r?.sav`. This falls straight through to
    /// the new-character block **printing nothing at all**, which is the
    /// ordinary case for a clean checkout.
    NoSaves,
    /// A menu was printed and the key pressed was none of `0`, `2`..`5` --
    /// `1` included, which is the key the prompt itself suggests. This
    /// jumps to the new-character block.
    NewCharacter,
    /// A menu was printed and an accepted digit was pressed.
    Load(char),
}

/// The lines the slot-menu loop writes for a given slot set.
///
/// Per slot, in order:
///
/// * `^1или` is written before every entry except the first, never after
///   the last.
/// * The digit in the found filename decides the line: not `'0'` writes
///   `^1Можно начать с ` + the digit + ` района`; `'0'` writes
///   `^1Можно начать с того места где ты сохранился` instead.
pub fn slot_menu_lines(slots: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, &slot) in slots.iter().enumerate() {
        if i > 0 {
            out.push("^1или".to_string());
        }
        out.push(if slot == '0' {
            "^1Можно начать с того места где ты сохранился".to_string()
        } else {
            format!("^1Можно начать с {slot} района")
        });
    }
    out
}

/// The prompt after the menu.
pub const SLOT_PROMPT: &str = "^0Нажми цифру с какого района начать. 1-начать сначала";

/// Print the slot menu and read the key.
pub fn choose_slot(
    dir: &Path,
    lines: &mut dyn Iterator<Item = io::Result<String>>,
) -> io::Result<SlotMenu> {
    let slots = present_slots(dir);
    if slots.is_empty() {
        return Ok(SlotMenu::NoSaves);
    }
    for line in slot_menu_lines(&slots) {
        term::println(&line);
    }
    term::println(SLOT_PROMPT);
    // The original takes one keystroke with no Enter key needed. This
    // port has no raw-key input, so it reads a line and takes its first
    // character -- a deliberate divergence.
    let Some(line) = term::read_line(lines) else {
        return Ok(SlotMenu::NewCharacter);
    };
    Ok(match line?.chars().next() {
        Some(k) if SLOT_KEYS.contains(&k) => SlotMenu::Load(k),
        _ => SlotMenu::NewCharacter,
    })
}

/// Load slot `slot` out of `dir`.
///
/// `Ok(None)` is the original's own fall-through: a failed read prints
/// `^6Чё-то глюкануло - нaверно нет такого сейва, Default:1` and continues
/// into the new-character block, so a missing or unreadable file is not an
/// error here either.
pub fn load_slot(dir: &Path, slot: char, seed: u32) -> io::Result<Option<Game>> {
    // The key must be exactly one of `'2'`, `'3'`, `'4'`, `'5'`, `'0'`;
    // anything else goes to the new-character block instead. [`choose_slot`]
    // already filters, but this is a `pub` entry point and needs the same
    // rejection -- the alternative, folding any stray character to a
    // digit, would turn it into a plausible-looking district 1.
    let Some(digit) = slot.to_digit(10) else {
        return Ok(None);
    };
    if !SLOT_KEYS.contains(&slot) {
        return Ok(None);
    }
    let bytes = match read_slot(dir, slot) {
        Ok(b) => b,
        Err(_) => {
            term::println("^6Чё-то глюкануло - нaверно нет такого сейва, Default:1");
            return Ok(None);
        }
    };
    let save = match Save::parse(&bytes) {
        Ok(s) => s,
        Err(_) => {
            term::println("^6Чё-то глюкануло - нaверно нет такого сейва, Default:1");
            return Ok(None);
        }
    };
    term::println(&format!("^0Загружено из save_r{slot}"));

    // ONLY slot 0 reads places.sav; the others derive their district from
    // the level.
    let (places, district) = if slot == '0' {
        let places = match std::fs::read(dir.join(PLACES_SAVE))
            .or_else(|_| std::fs::read(dir.join(PLACES_SAVE.to_uppercase())))
        {
            // `>= PLACES_BYTES`, not `> 0`: the original reads the file as
            // seven separate one-byte reads, so a file with fewer than
            // seven bytes fails and takes the failure arm. It is also the
            // only thing standing between a truncated `PLACES.SAV` and
            // `&b[..7]` panicking.
            Ok(b) if b.len() >= PLACES_BYTES => {
                term::println("^0Загружено из places");
                Places::from_bytes(&b[..PLACES_BYTES])
            }
            _ => {
                // The failure arm CLEARS the flags, with three class-keyed
                // exceptions, then prints
                // `^6Чё-то глюкануло - немогу прoгрузить Places:Ресет ту
                // Default`. The class bonus that `Game::from_save`
                // re-applies restores exactly those three, so an all-clear
                // set plus the bonus reproduces the arm.
                term::println("^6Чё-то глюкануло - немогу прoгрузить Places:Ресет ту Default");
                Places::from_bytes(&[0u8; 7])
            }
        };
        // Signed division: the level is a signed value, so this uses signed
        // division to derive the district, then truncates to a byte.
        //
        // The cast to `i16` is not decoration: a record is not required to
        // hold a level in 0..40, so a negative level is possible even
        // though it is unreachable in ordinary play.
        let level = save.stats[5] as i16;
        (places, (level / 10 + 1) as u8)
    } else {
        // Slots 2..5 never open places.sav; their flags start clear, and
        // the district is the digit itself.
        (Places::from_bytes(&[0u8; 7]), digit as u8)
    };
    Ok(Some(Game::from_save(&save, places, district, seed)))
}
