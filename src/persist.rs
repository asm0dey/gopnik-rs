//! Saving and loading: [`Game`] <-> [`Save`], and the two files on disk.
//!
//! Its own module rather than more of `src/game.rs` for the same reason
//! `src/combat_dispatch.rs` is: it is one coherent unit -- the record
//! conversion, the slot scan, and the two writers -- and `game.rs` is
//! already large enough that adding it there would bury it.
//!
//! ## Where the original saves, and where it does not
//!
//! There is **no typed save verb**. `sv` sizes up the enemy
//! (`crate::commands`), and no compare in `entry` or in `FUN_1000_3d11`
//! reaches a file write. Saving happens at exactly two places, both
//! established from flow:
//!
//! * **The mage's paid save**, `1000:75f6`..`1000:773d`. `Рушель Блаво`
//!   asks `Ты хочешь сохраниться?`, and on `y` charges `district * 50`
//!   (`1000:761d`), writes the 694-byte record into the hard-coded name
//!   `save_r0.sav` (`1000:764e` `Rewrite(f, 694)`, `1000:765d`
//!   `BlockWrite` from `DS:369c`), writes the seven discovery flags one
//!   byte at a time into `places.sav` (`1000:766f`..`1000:7724`), and prints
//!   `^0Сохранено! ^1Можешь беспредельничать дальше.` (file `0x8D92`).
//!   [`Game::mage`](crate::game::Game) is the port of that arm.
//! * **The district-advance autosave**, `1000:ab92`..`1000:ad12`. After
//!   `inc [0x3692]` and the discovery-flag resets it prints
//!   `^0Хочешь сохранить свои достижения?` (file `0x9BCD`), reads a line at
//!   its own `\` prompt (`1000:ac31`), compares it against `y` (file
//!   `0x9BF3`) at `1000:ac54`, and on a match writes `save_r<district>.sav`
//!   -- the district *after* the increment, which is why the shipped corpus
//!   is `SAVE_R2`..`SAVE_R5` and has no `SAVE_R1` -- then prints
//!   `^1Сохранено в save_r` + the digit + `.sav` (files `0x9C01`, `0x9BFC`).
//!   **Not wired up in this port**: its `ReadLn` sits at the top of the main
//!   loop in the original, while this port advances the district inside the
//!   post-fight block, which has no input iterator. Recorded in
//!   `docs/re/gaps.md`.
//!
//!   That leaves the port **coherent, not half-implemented**: the increment
//!   and the discovery-flag reset are faithful (`Game::resolve_fight` gets
//!   both gates right), and the two effects that are missing are inert today
//!   -- the prompt has nothing to prompt for, and the ban countdowns it
//!   clears are never set. The practical consequence is that **this port can
//!   only ever produce slot 0**, the mage's. Slots 2..5 exist for the shipped
//!   corpus and for records `tools/savegen.py` writes.
//!
//! `docs/re/gaps.md` used to say "there is no 'saved OK' / 'save failed'
//! string anywhere in `data/strings.json`, so a wrapper could only print
//! composed text". That is **false**, and it is why this port printed
//! nothing on a save: both strings above are in `data/strings.json`, at
//! decimal offsets 36242 and 39937.
//!
//! ## The load path
//!
//! `FUN_1000_6a0d`, `1000:6a62`..`1000:6da0`. `FindFirst` on
//! `<dir>\save_r?.sav` (`1000:6a81`/`1000:6a8a`) counts the slots present,
//! printing one line per slot -- `^1Можно начать с ` + digit + ` района`,
//! or `^1Можно начать с того места где ты сохранился` for slot `0` --
//! separated by `^1или`. With none found (`1000:6b33`) it jumps straight to
//! the new-character block. Otherwise it prints
//! `^0Нажми цифру с какого района начать. 1-начать сначала` (file `0x7C69`)
//! and takes a **`ReadKey`** (`1000:6b56`); `1000:6b5e`..`1000:6b7f` accepts
//! `'2'`, `'3'`, `'4'`, `'5'` and `'0'` and sends anything else -- `1`
//! included, which is what the prompt tells the player to press -- to the
//! new-character block.
//!
//! On an accepted digit it opens `save_r<digit>.sav`, and a non-zero
//! `IOResult` (`1000:6bd4`/`1000:6bdb`) prints
//! `^6Чё-то глюкануло - нaверно нет такого сейва, Default:1` and falls
//! through to creation as well. On success it `BlockRead`s the record,
//! prints `^0Загружено из save_r` + the digit, and sets the district from
//! the digit itself (`1000:6bf9`) -- **the district is not in the record**.
//!
//! Slot `0` is the odd one out, and this port reproduces both halves:
//! `1000:6c50` `cmp byte [0x3692],0` sends only slot 0 on to read
//! `places.sav`, and `1000:6d8c`..`1000:6d9d` then derives its district as
//! `level div 10 + 1`. Slots 2..5 never touch `places.sav` at all, so their
//! discovery flags start clear and only `1000:73bb`'s class bonus puts any
//! back.

use crate::game::Game;
use crate::locations::Places;
use crate::model::Fighter;
use crate::progress::{self, Progress};
use crate::save::{GrowthSlot, Items, Save, SaveError, GROWTH_LOG_SLOTS};
use crate::term;
use std::io;
use std::path::{Path, PathBuf};

/// The `^7 ` the original prefixes onto every stored name.
///
/// **Established from flow.** `1000:723a`..`1000:725d` assigns the CS
/// literal at image `0x67f2` (file `0x80C2`) into a temp, appends the
/// just-typed name from `DS:379c`, and writes the result back over
/// `DS:379c`; `1000:ed79` does the same after a `rename`. All five shipped
/// saves carry it (`^7 adg`, `^7 vor`, `^7 Mudila`) and so does a freshly
/// created character's `DS:379c`
/// (`data/probes/saveprobe-fresh-record.json`).
///
/// This port keeps [`Fighter::name`](crate::model::Fighter) *without* the
/// prefix and applies it here, at the format boundary, rather than changing
/// what every combat line renders -- a divergence in where the prefix lives,
/// not in what reaches the disk. `docs/re/gaps.md` records it.
const NAME_PREFIX: &str = "^7 ";

/// The slot keys `1000:6b5e`..`1000:6b7f` accepts, in the order it compares
/// them. Anything else -- `'1'` included, which is the key the prompt itself
/// suggests -- starts a new character.
///
/// **This is the KEY test and nothing else.** The menu's own scan is a
/// different mechanism with a different alphabet -- see [`present_slots`] --
/// and using this constant for both was the defect the Task 19 review
/// caught. It is the same shape as the `exit`/`e` fold the previous branch
/// shipped as a Critical: one constant standing in for two mechanisms of the
/// original that happen to agree on the common case.
pub const SLOT_KEYS: [char; 5] = ['2', '3', '4', '5', '0'];

/// The `FindFirst` mask, CS `0x633f` / file `0x7C0F`: `save_r?.sav`.
const SLOT_MASK_PREFIX: &str = "save_r";
const SLOT_MASK_SUFFIX: &str = ".sav";

/// The mage's fixed filename `save_r0.sav`, CS `0x74ab` / file `0x8D7B`.
pub const MAGE_SAVE: &str = "save_r0.sav";
/// `places.sav`: CS `0x63f2` / file `0x7CC2` on the load side,
/// CS `0x74b7` / file `0x8D87` on the mage's.
pub const PLACES_SAVE: &str = "places.sav";

/// Seven one-byte reads at `1000:6ca2`..`1000:6d0e`, seven one-byte writes
/// at `1000:76ab`..`1000:7717`; `crate::locations::TRACKED` is the order.
pub const PLACES_BYTES: usize = 7;

/// `save_r<slot>.sav`, the name both the load scan and the autosave build
/// (`save_r` + `Str(digit)` + `.sav`; CS `0x63d0`/`0x63d7` and
/// `0x8325`/`0x832c`).
pub fn slot_filename(slot: char) -> String {
    format!("save_r{slot}.sav")
}

/// Every file in `dir` whose name matches the mask `save_r?.sav`, as the
/// character the `?` matched.
///
/// **`?` is a DOS wildcard and matches ANY single character**, not just a
/// slot key. `1000:6a81`/`1000:6a8a` pass the mask at CS `0x633f` (file
/// `0x7C0F`) to `FindFirst`, and `1000:6ada` reads `[0x3d2b]` -- the
/// seventh byte of the name `FindFirst` returned -- to print the menu line.
/// So `save_r1.sav` and `save_rx.sav` are both listed and both prompted for
/// by the original, and neither is a key `1000:6b5e`..`1000:6b7f` accepts:
/// typing what they show starts a new character. The scan and the key test
/// are two mechanisms, and this function is only the first of them.
///
/// An earlier revision filtered on [`SLOT_KEYS`] here, which made those two
/// files invisible to the port and turned the listing order into compare
/// order. The game itself only ever writes `save_r0` and `save_r2`..`save_r5`
/// (`crate::persist`'s module doc), so nothing in ordinary play produced
/// one -- but `tools/savegen.py` writes whatever it is told to.
///
/// Two **port decisions** here, neither a property of the original:
///
/// * **Order is by name, not directory order.** `FindFirst`/`FindNext`
///   (`1000:6a8a`, `1000:6b2b`) walk the FAT directory in on-disk order,
///   which no portable API exposes and which nothing in the game depends on
///   -- it decides only which menu line prints first. Sorted is
///   deterministic, which a test can assert.
/// * **Both filename cases match.** The DOS mask is case-insensitive on
///   FAT; this host is not, and the shipped corpus is uppercase while the
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
        // the menu prints what is on disk -- `1000:6ada` reads the byte
        // `FindFirst` returned, not a case-folded one.
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
    /// template and no carried-over blob, which is exactly what
    /// `Game::write_save` used to refuse over. The `Unsupported` error it
    /// returned named `.SAV` offsets `0x214` and `0x2ae` as the blocker;
    /// both spans are established now (`docs/re/save-format.md`), so the
    /// blocker is gone rather than worked around.
    ///
    /// **What is deliberately NOT here, and why that is faithful.** The
    /// record is `20ae:369c`..`20ae:3951` and nothing else; every other
    /// global this port carries sits outside it and is therefore *not saved
    /// by the original either*:
    ///
    /// | field | address | side of the record |
    /// |---|---|---|
    /// | [`Game::flag_3693`] | `20ae:3693` | below `369c` |
    /// | [`Game::market_ban_countdown`] | `20ae:3b76` | above `3951` |
    /// | [`Game::club_ban_countdown`] | `20ae:3b77` | above |
    /// | [`Game::den_errand_1_pending`] | `20ae:3b78` | above |
    /// | [`Game::den_errand_2_pending`] | `20ae:3b79` | above |
    /// | [`Game::rector_showdown`] | `20ae:3c83` | above |
    /// | [`Game::dealer_delivery_counter`] | `20ae:3e32` | above |
    /// | [`Game::den_loan_credit`] | `20ae:3e35` | above |
    /// | [`Game::district`] | `20ae:3692` | below -- the load path takes it from the slot digit (`1000:6bf9`) |
    /// | [`Game::places`] | `20ae:3694`..`369a` | below -- `places.sav`, seven separate byte writes |
    ///
    /// `tests/save_load.rs::every_in_record_address_named_in_game_rs_is_persisted`
    /// is the executable form of that claim: it re-derives the set of
    /// `20ae:` addresses `src/game.rs`'s field docs name, keeps the ones
    /// inside the record, and requires each to be cited here. Adding a
    /// `Game` field for an in-record byte and forgetting to persist it fails
    /// that test rather than silently losing the byte.
    pub fn to_save(&self) -> Save {
        let p = &self.player;
        let mut save = Save::blank();
        save.name = format!("{NAME_PREFIX}{}", p.name);
        // The eight stat words, `20ae:389c` (class), `20ae:389e`,
        // `20ae:38a0`, `20ae:38a2`, `20ae:38a4`, `20ae:38a6` (level),
        // `20ae:38a8`, `20ae:38aa`; then `20ae:38ac`/`20ae:38ae` for hp and
        // hpmax. `docs/re/combat.md`, "The fighter record".
        save.stats = [
            p.class, p.strength, p.agility, p.vitality, p.luck, p.level, p.dmg_min, p.dmg_max,
        ];
        save.hp = p.hp;
        save.hpmax = p.hpmax;
        // `20ae:38cd`, `38ce`, `38d0`, `38d2`. `Progress` widened xp and the
        // threshold to `u32`; the original's are 16-bit words, so the
        // narrowing here is the widening being given back, not a cap. Same
        // for `armor` below (`u16` here, one byte at `20ae:38b2`) and for
        // the five `Integer`s further down.
        save.buff_countdown = self.buff_countdown;
        save.xp = self.progress.xp as u16;
        save.threshold = self.progress.threshold as u16;
        save.growth_log = growth_log_to_record(&self.progress);
        save.items = Items {
            broken_jaw: p.broken_jaw,                      // 20ae:38b0
            broken_leg: p.broken_leg,                      // 20ae:38b1
            armour: p.armor as u8,                         // 20ae:38b2
            dark_glasses: self.dark_glasses,               // 20ae:38b3
            suit_abibas: self.wear_suit_abibas_38b4,       // 20ae:38b4
            boots: self.wear_boots_38b5,                   // 20ae:38b5
            jacket: self.wear_jacket_38b6,                 // 20ae:38b6
            suit_adidas: self.wear_suit_adidas_38b7,       // 20ae:38b7
            boots_pontovye: self.wear_boots_pontovye_38b8, // 20ae:38b8
            jacket_krutaya: self.wear_jacket_krutaya_38b9, // 20ae:38b9
            kastet: self.weapon_kastet_38ba,               // 20ae:38ba
            mobile: self.has_mobile,                       // 20ae:38bb
            prison_tattoo: self.prison_tattoo,             // 20ae:38bc
            krestik: self.charm_krestik_38bd,              // 20ae:38bd
            ring_gs: self.charm_ring_38be,                 // 20ae:38be
            ring_pg: self.oneshot_gift_1,                  // 20ae:38bf
            mega_ring: self.oneshot_gift_2,                // 20ae:38c0
            ring_gp: self.ring_gospodi_pomilui,            // 20ae:38c1
            nozh: self.weapon_nozhik_38c2,                 // 20ae:38c2
            beer_half_litres: p.beer_dl as i16,            // 20ae:38c3
            joints: p.joints as i16,                       // 20ae:38c5
            money: p.money as i16,                         // 20ae:38c7
            junk: p.junk as i16,                           // 20ae:38c9
            street_cred: self.pontovost_street as i16,     // 20ae:38cb
            tooth_guard: self.tooth_guard,                 // 20ae:394a
            dubinka: self.weapon_dubinka_394b,             // 20ae:394b
            tesak: self.weapon_tesak_394c,                 // 20ae:394c
            pistol: self.pistol.owned,                     // 20ae:394d
            silencer: self.pistol.silencer,                // 20ae:394e
            cartridges: self.pistol.cartridges,            // 20ae:394f
            church_stage: self.church_visits,              // 20ae:3951
        };
        save
    }

    /// Rebuild a `Game` from a loaded record.
    ///
    /// `district` and `places` come from the caller because **neither is in
    /// the record**: the district is `Val` of the slot digit
    /// (`1000:6bf9`), or `level div 10 + 1` for slot `0` (`1000:6d93`), and
    /// the seven discovery flags live in `places.sav`. Slots 2..5 never read
    /// that file (`1000:6c50` gates it on `district == 0`), so their
    /// `places` is all-clear.
    ///
    /// `1000:73bb`'s class bonus runs on **every** entry into the game, new
    /// character or loaded save (`docs/re/wander.md`, "What reaches
    /// `1000:73bb`"), so it is re-applied here after `places` is installed.
    pub fn from_save(save: &Save, places: Places, district: u8, seed: u32) -> Game {
        let it = &save.items;
        let player = Fighter {
            name: save
                .name
                .strip_prefix(NAME_PREFIX)
                .unwrap_or(&save.name)
                .to_string(),
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
            armor: u16::from(it.armour),
            broken_jaw: it.broken_jaw,
            broken_leg: it.broken_leg,
            joints: it.joints.max(0) as u16,
            // `crate::model::Fighter::stoned` and `Game::buff_countdown`
            // are two models of one variable (`docs/re/gaps.md`); the
            // countdown is the one the original keeps, so it decides.
            stoned: save.buff_countdown != 0,
            beer_dl: it.beer_half_litres.max(0) as u16,
            money: i32::from(it.money),
            junk: it.junk.max(0) as u16,
        };
        let progress = Progress {
            xp: u32::from(save.xp),
            threshold: u32::from(save.threshold),
            growth_log: growth_log_from_record(&save.growth_log),
        };
        let mut g = Game::new(player, progress, seed);
        // `Game::new` is the NEW-character path: `1000:6dc3`/`1000:6dc8`
        // mark the vet and the market found. A load never reaches those two
        // stores -- `1000:6da0` jumps past them -- so the flags are replaced
        // wholesale here rather than added to.
        g.places = places;
        g.apply_class_bonus();
        g.district = district;
        g.has_mobile = it.mobile;
        g.dark_glasses = it.dark_glasses;
        g.prison_tattoo = it.prison_tattoo;
        g.oneshot_gift_1 = it.ring_pg;
        g.oneshot_gift_2 = it.mega_ring;
        g.ring_gospodi_pomilui = it.ring_gp;
        g.pontovost_street = i32::from(it.street_cred);
        g.buff_countdown = save.buff_countdown;
        g.tooth_guard = it.tooth_guard;
        g.charm_krestik_38bd = it.krestik;
        g.charm_ring_38be = it.ring_gs;
        g.weapon_kastet_38ba = it.kastet;
        g.weapon_dubinka_394b = it.dubinka;
        g.weapon_nozhik_38c2 = it.nozh;
        g.weapon_tesak_394c = it.tesak;
        g.wear_suit_abibas_38b4 = it.suit_abibas;
        g.wear_boots_38b5 = it.boots;
        g.wear_jacket_38b6 = it.jacket;
        g.wear_suit_adidas_38b7 = it.suit_adidas;
        g.wear_boots_pontovye_38b8 = it.boots_pontovye;
        g.wear_jacket_krutaya_38b9 = it.jacket_krutaya;
        g.church_visits = it.church_stage;
        g.pistol.owned = it.pistol;
        g.pistol.silencer = it.silencer;
        g.pistol.cartridges = it.cartridges;
        g
    }

    /// Write the 694-byte record, and **only** that.
    ///
    /// The two writers differ here and the difference is real: the mage
    /// writes `places.sav` as well (`1000:766f`..`1000:7724`) and the
    /// district-advance autosave does **not** -- `1000:acc8`'s `BlockWrite`
    /// is followed straight by `1000:acd5 Close` and then the confirmation
    /// line. So the flags are [`Game::write_places`]'s job, called by the
    /// writer that actually does it.
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

    /// The seven discovery flags, one byte each, `1000:766f`..`1000:7724`.
    /// `crate::locations::TRACKED` fixes the order from the reader's own
    /// destinations (`docs/re/gaps.md`, "`PLACES.SAV`'s byte order").
    pub fn write_places(&self, dir: &Path) -> io::Result<PathBuf> {
        let path = dir.join(PLACES_SAVE);
        std::fs::write(&path, self.places.to_bytes())?;
        Ok(path)
    }

    /// The mage's paid save, `1000:7621`..`1000:773d`: the record, then the
    /// flags, then the confirmation.
    ///
    /// The money has already left by the time this is called -- `1000:761d`
    /// debits before the file is opened, and the original does not refund a
    /// failed write either.
    pub fn mage_save(&self) -> io::Result<PathBuf> {
        let dir = self.save_dir.clone();
        let path = self.write_save_as(&dir, MAGE_SAVE)?;
        self.write_places(&dir)?;
        // 1000:7729, CS 0x74c2, file 0x8D92.
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
/// flee penalty clears only the length byte (`1000:497d`) and leaves the
/// payload, a "length 0, codes still there" state no `Progress` value maps
/// to. A record parsed into a `Game` and written back out therefore
/// normalises such a slot to three zero bytes. It is a port limitation, not
/// a finding about the original, and it is why `Save`'s own round trip
/// (which never goes through `Progress`) is the one the byte-exactness test
/// uses.
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

/// The three arms of `1000:6a67`..`1000:6b81`, which are three different
/// things and not two levels of "maybe".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotMenu {
    /// The directory holds no `save_r?.sav`. `1000:6b33`'s
    /// `cmp byte [0x3d04],0` / `ja 0x6b3d` falls straight through to the
    /// new-character block **printing nothing at all**, which is the
    /// ordinary case for a clean checkout.
    NoSaves,
    /// A menu was printed and the key pressed was none of `0`, `2`..`5`
    /// (`1000:6b5e`..`1000:6b7f`) -- `1` included, which is the key the
    /// prompt itself suggests. `1000:6b81` jumps to the new-character block.
    NewCharacter,
    /// A menu was printed and an accepted digit was pressed.
    Load(char),
}

/// The lines `1000:6a67`..`1000:6b30`'s loop writes for a given slot set.
///
/// A function returning the lines rather than printing them, for the reason
/// `docs/superpowers/RESUME.md` gives under "the highest-value cleanup
/// left": `crate::term` writes straight to this process's stdout and nothing
/// in the crate can capture it, so text that only reaches `term` has no
/// executable assertion at all. Every string here is a verbatim byte range
/// of `orig/g.exe` and this project pins those.
///
/// The loop body, per iteration of `1000:6a8f`..`1000:6b30`:
///
/// * `1000:6a99` `cmp byte [0x3d04],0` / `jbe 0x6ab9` -- the counter is the
///   number of slots printed SO FAR, so `^1или` (CS `0x634b`, file `0x7C1B`)
///   is written before every entry except the first, never after the last.
/// * `1000:6abd` `cmp byte [0x3d2b],0x30` / `jz 0x6b06` -- the digit in the
///   found filename. Not `'0'`: `^1Можно начать с ` (CS `0x6351`) + the
///   digit + ` района` (CS `0x6363`). `'0'`: `1000:6b06` re-tests it and
///   `1000:6b0d` writes `^1Можно начать с того места где ты сохранился`
///   (CS `0x636b`, file `0x7C3B`) instead.
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

/// The prompt after the menu, `1000:6b3d` (CS `0x6399`, file `0x7C69`).
pub const SLOT_PROMPT: &str = "^0Нажми цифру с какого района начать. 1-начать сначала";

/// Print the slot menu and read the key, `1000:6a67`..`1000:6b81`.
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
    // 1000:6b56 is a ReadKey, not a ReadLn -- the original takes one
    // keystroke with no Enter. This port has no raw-key input anywhere
    // (`crate::term` writes only), so it reads a line and takes its first
    // character. A PORT DECISION, and the one place this path knowingly
    // differs from `1000:6b56`.
    let Some(line) = lines.next() else {
        return Ok(SlotMenu::NewCharacter);
    };
    Ok(match line?.chars().next() {
        Some(k) if SLOT_KEYS.contains(&k) => SlotMenu::Load(k),
        _ => SlotMenu::NewCharacter,
    })
}

/// Load slot `slot` out of `dir`, `1000:6b84`..`1000:6d9d`.
///
/// `Ok(None)` is the original's own fall-through: a `Reset` that leaves
/// `IOResult` non-zero (`1000:6bd4`/`1000:6bdb`) prints
/// `^6Чё-то глюкануло - нaверно нет такого сейва, Default:1` at
/// `1000:6da5` and continues into the new-character block, so a missing or
/// unreadable file is not an error here either.
pub fn load_slot(dir: &Path, slot: char, seed: u32) -> io::Result<Option<Game>> {
    // `1000:6b5e`..`1000:6b81` compares the key against exactly `'2'`,
    // `'3'`, `'4'`, `'5'`, `'0'` and jumps to the new-character block on
    // anything else, so a key outside that set never reaches the open at
    // all. [`choose_slot`] already filters, but this is a `pub` entry point
    // and the same rejection belongs here -- the alternative was
    // `to_digit(10).unwrap_or(1)` further down, which turned a stray
    // character into a plausible-looking district 1.
    let Some(digit) = slot.to_digit(10) else {
        return Ok(None);
    };
    if !SLOT_KEYS.contains(&slot) {
        return Ok(None);
    }
    let bytes = match read_slot(dir, slot) {
        Ok(b) => b,
        Err(_) => {
            // 1000:6da5, CS 0x6451, file 0x7D21:
            // `^6Чё-то глюкануло - нaверно нет такого сейва, Default:1`.
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
    // 1000:6c1e, CS 0x63dc, file 0x7CAC: `^0Загружено из save_r` + digit.
    term::println(&format!("^0Загружено из save_r{slot}"));

    // 1000:6c50 `cmp byte [0x3692],0` -- ONLY slot 0 reads places.sav, and
    // 1000:6d8c..1000:6d9d then derives its district from the level.
    let (places, district) = if slot == '0' {
        let places = match std::fs::read(dir.join(PLACES_SAVE))
            .or_else(|_| std::fs::read(dir.join(PLACES_SAVE.to_uppercase())))
        {
            // `>= PLACES_BYTES`, not `> 0`: the original opens the file with
            // `Reset(f, 1)` and issues SEVEN separate one-byte `Read`s
            // (`1000:6ca2`..`1000:6d0e`), so a file with fewer than seven
            // bytes leaves `IOResult` non-zero and takes the failure arm.
            // It is also the only thing standing between a truncated
            // `PLACES.SAV` and `&b[..7]` panicking.
            Ok(b) if b.len() >= PLACES_BYTES => {
                // 1000:6d20, CS 0x63fd, file 0x7CCD:
                // `^0Загружено из places`.
                term::println("^0Загружено из places");
                Places::from_bytes(&b[..PLACES_BYTES])
            }
            _ => {
                // 1000:6d3b..1000:6d73: the failure arm CLEARS the flags,
                // with three class-keyed exceptions, then prints
                // `^6Чё-то глюкануло - немогу прoгрузить Places:Ресет ту
                // Default` (CS 0x6413, file 0x7CE3). The class bonus that
                // `Game::from_save` re-applies restores exactly those three,
                // so an all-clear set plus the bonus reproduces the arm.
                term::println("^6Чё-то глюкануло - немогу прoгрузить Places:Ресет ту Default");
                Places::from_bytes(&[0u8; 7])
            }
        };
        // `1000:6d93`..`1000:6d9d`: `mov ax,[0x38a6]` / `cwd` / `idiv cx`
        // / `inc ax` / `mov [0x3692],al`. **Signed** division -- `cwd`
        // sign-extends and `idiv` is the signed form -- and `20ae:38a6` is a
        // Pascal `Integer`, corroborated by `1000:ab7f`'s `cmp ax,[0x38a6]`
        // / `jle`, a signed conditional. The `as u8` models `mov [...],al`,
        // the truncation to the low byte.
        //
        // The cast to `i16` is not decoration: a record is not required to
        // hold a level in 0..40, and `tools/savegen.py --set level=0x8000`
        // writes one that is negative -- which is the workflow this branch
        // hands the next several tasks, so "unreachable in play" is not a
        // reason to model it unsigned.
        let level = save.stats[5] as i16;
        (places, (level / 10 + 1) as u8)
    } else {
        // Slots 2..5 never open places.sav; their flags start clear, and the
        // district is the digit itself (`1000:6bf9`).
        (Places::from_bytes(&[0u8; 7]), digit as u8)
    };
    Ok(Some(Game::from_save(&save, places, district, seed)))
}
