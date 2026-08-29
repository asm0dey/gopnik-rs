//! The save/load path: `Game` <-> `Save` <-> 694 bytes on disk.
//!
//! **The five shipped saves are the independent check.** A round trip that
//! writes a buffer and reads the same buffer back proves nothing about
//! offsets or meanings; every test below that could be satisfied that way is
//! anchored to `orig/SAVE_R?.SAV`, whose bytes were produced by the original
//! in 2003, or to `data/probes/saveprobe-fresh-record.json`, a dump of
//! `20ae:369c` taken from `orig/g.exe` running under qemu.

use gopnik::game::Game;
use gopnik::locations::{Location, Places};
use gopnik::persist;
use gopnik::save::{Save, SIZE};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn orig(name: &str) -> Vec<u8> {
    let p = root().join("orig").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

const SHIPPED: [&str; 5] = [
    "SAVE_R0.SAV",
    "SAVE_R2.SAV",
    "SAVE_R3.SAV",
    "SAVE_R4.SAV",
    "SAVE_R5.SAV",
];

/// A scratch directory under the target dir. No `tempfile` dependency: the
/// owner's rule is to ask before adding one, and a per-test subdirectory of
/// `target/` costs nothing.
fn scratch(tag: &str) -> PathBuf {
    let d = root().join("target").join("save-load-tests").join(tag);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fresh_game() -> Game {
    let (player, progress) = gopnik::progress::new_character("Гопа", 2);
    Game::new(player, progress, 12345)
}

// ---------------------------------------------------------------------------
// Game <-> Save, checked against the shipped corpus
// ---------------------------------------------------------------------------

/// A save the port LOADS and immediately writes back must be byte-identical.
///
/// This is the real load/save test, and it is not a buffer echo: the bytes
/// travel `.SAV` -> `Save` -> `Game` (33 typed fields, five of them on
/// `Fighter` and one on `Progress`) -> `Save` -> `.SAV`. A field the
/// conversion drops, widens wrongly, or puts on the wrong side comes back as
/// a different byte.
#[test]
fn every_shipped_save_survives_a_whole_load_then_save_cycle() {
    for name in SHIPPED {
        let bytes = orig(name);
        let save = Save::parse(&bytes).unwrap();
        let game = Game::from_save(&save, Places::from_bytes(&[0u8; 7]), 3, 1);
        let back = game.to_save().to_bytes().unwrap();
        assert_eq!(back.len(), SIZE);
        assert_eq!(
            back,
            bytes,
            "{name}: load -> save is not byte-exact; first difference at 0x{:03x}",
            back.iter()
                .zip(&bytes)
                .position(|(a, b)| a != b)
                .unwrap_or(0)
        );
    }
}

/// The one thing the cycle above cannot see, because it holds on both sides:
/// that the values landed in the fields whose names claim them.
#[test]
fn save_r5_loads_the_character_the_shipped_bytes_describe() {
    let save = Save::parse(&orig("SAVE_R5.SAV")).unwrap();
    let g = Game::from_save(&save, Places::from_bytes(&[0u8; 7]), 5, 1);

    // The eight stat words, cross-checked against docs/re/save-format.md's
    // own observed-values table rather than against the parser.
    assert_eq!(g.player.class, 5);
    assert_eq!(g.player.strength, 90);
    assert_eq!(g.player.agility, 120);
    assert_eq!(g.player.vitality, 45);
    assert_eq!(g.player.luck, 49);
    assert_eq!(g.player.level, 40);
    assert_eq!(g.player.hp, 325);
    assert_eq!(g.player.hpmax, 325);
    // The `^7 ` the original prefixes at 1000:723a is stripped at the format
    // boundary, so the live name is what the player typed.
    assert_eq!(g.player.name, "Mudila");

    // The two Task 19 spans. SAVE_R5 is the ONLY shipped save whose owner
    // bought a pistol, which is what makes these three assertions
    // discriminating rather than decorative.
    assert!(g.pistol.owned);
    assert!(g.pistol.silencer);
    assert_eq!(g.pistol.cartridges, 8);
    assert_eq!(g.player.armor, 26);
    assert_eq!(g.player.money, 29);
    assert_eq!(g.pontovost_street, 1508);
    assert_eq!(g.player.beer_dl, 274);
    assert_eq!(g.player.joints, 4);
    assert_eq!(g.player.junk, 0);
    assert_eq!(g.church_visits, 1);
    assert!(
        g.weapon_nozhik_38c2,
        "SAVE_R5 is the only save with the нож"
    );
    assert!(g.wear_jacket_krutaya_38b9, "...and the only крутая кожанка");
    assert!(!g.weapon_dubinka_394b, "...and it has no дубинка");
    assert!(!g.weapon_tesak_394c);
    assert!(g.tooth_guard);
    assert!(g.has_mobile);
    assert!(g.prison_tattoo);
    assert!(g.dark_glasses);
}

/// The complement: the four saves without a pistol must not grow one, and
/// the two with a live buff must carry it. Both are properties no single
/// save can show.
#[test]
fn the_shipped_saves_disagree_where_the_layout_says_they_should() {
    let mut with_pistol = 0;
    let mut with_buff = 0;
    let mut with_tooth_guard = 0;
    for name in SHIPPED {
        let save = Save::parse(&orig(name)).unwrap();
        let g = Game::from_save(&save, Places::from_bytes(&[0u8; 7]), 3, 1);
        with_pistol += usize::from(g.pistol.owned);
        with_buff += usize::from(g.buff_countdown != 0);
        with_tooth_guard += usize::from(g.tooth_guard);
        // Fighter::stoned and Game::buff_countdown are two models of one
        // variable; the countdown is the original's, so it decides.
        assert_eq!(g.player.stoned, g.buff_countdown != 0, "{name}");
    }
    assert_eq!(with_pistol, 1, "only SAVE_R5's owner bought a pistol");
    assert_eq!(with_buff, 2, "SAVE_R2 and SAVE_R4 carry the joint buff");
    assert_eq!(with_tooth_guard, 3, "SAVE_R3, SAVE_R4 and SAVE_R5");
}

/// The growth log survives the trip through `Progress`, which stores only
/// the two codes and derives the Pascal length byte back.
#[test]
fn the_growth_log_survives_the_trip_through_progress() {
    for name in SHIPPED {
        let bytes = orig(name);
        let save = Save::parse(&bytes).unwrap();
        let level = usize::from(save.stats[5]);
        let g = Game::from_save(&save, Places::from_bytes(&[0u8; 7]), 3, 1);
        // Every level the character reached granted two stats.
        for lvl in 1..=level {
            let entry = g.progress.growth_log[lvl];
            assert!(
                entry.iter().all(|&c| (b'1'..=b'4').contains(&c)),
                "{name}: level {lvl} holds {entry:?}"
            );
        }
        for lvl in level + 1..=40 {
            assert_eq!(g.progress.growth_log[lvl], [0, 0], "{name}: level {lvl}");
        }
        assert_eq!(g.to_save().growth_log, save.growth_log, "{name}");
    }
}

// ---------------------------------------------------------------------------
// A freshly created character, against what the original starts one with
// ---------------------------------------------------------------------------

/// `Game::write_save` used to refuse for a fresh character because `.SAV`
/// `0x214` and `0x2ae` were unknown. This is the test that says it does not
/// any more -- and it does not merely check that *some* 694 bytes come out,
/// it checks them against a dump of the original's own `20ae:369c` after
/// character creation.
#[test]
fn a_fresh_record_matches_what_the_original_starts_a_new_character_with() {
    // data/probes/saveprobe-fresh-record.json, class answer 0, empty name:
    // magic assigned at 1000:6dcd, threshold 10 at 1000:6de0, and every
    // other byte past 0x214 zero.
    let (player, progress) = gopnik::progress::new_character("Раз^6дол^4бай", 0);
    let g = Game::new(player, progress, 1);
    let bytes = g.to_save().to_bytes().unwrap();
    assert_eq!(bytes.len(), SIZE);

    let save = Save::parse(&bytes).unwrap();
    assert_eq!(save.magic, gopnik::save::MAGIC);
    assert_eq!(save.name, "^7 Раз^6дол^4бай");
    assert_eq!(save.stats, [3, 3, 3, 3, 3, 0, 1, 3], "the observed record");
    assert_eq!((save.hp, save.hpmax), (28, 28));
    assert_eq!(save.threshold, 10, "1000:6de0 mov word [0x38d0],0xa");
    assert_eq!(save.xp, 0);
    assert_eq!(save.buff_countdown, 0);

    // Both shortstring paddings and both former `unk_` spans are zero in the
    // guest's own record.
    let name_len = usize::from(bytes[0x100]);
    assert!(bytes[1 + usize::from(bytes[0])..0x100]
        .iter()
        .all(|&b| b == 0));
    assert!(bytes[0x101 + name_len..0x200].iter().all(|&b| b == 0));
    assert!(bytes[0x214..0x231].iter().all(|&b| b == 0));
    assert!(bytes[0x236..0x2b6].iter().all(|&b| b == 0));
}

/// ...and the whole 694 bytes, compared against the committed dump directly
/// rather than field by field.
///
/// Only the class-dependent stat words are excluded, because the probe run
/// answered `0` (Пацан) and this test builds the same class -- so nothing is
/// excluded on the grounds of disagreeing.
#[test]
fn a_fresh_record_is_byte_identical_to_the_probe_dump() {
    let p = root()
        .join("data")
        .join("probes")
        .join("saveprobe-fresh-record.json");
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        json["class_answer"].as_u64(),
        Some(0),
        "the probe was taken with a different class than this test builds"
    );
    let hex = json["record_hex"].as_str().expect("record_hex");
    let want: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    assert_eq!(want.len(), SIZE);

    let (player, progress) = gopnik::progress::new_character("Раз^6дол^4бай", 0);
    let g = Game::new(player, progress, 1);
    let got = g.to_save().to_bytes().unwrap();
    assert_eq!(
        got,
        want,
        "the port's fresh record differs from the original's at 0x{:03x}",
        got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(0)
    );
}

// ---------------------------------------------------------------------------
// The files on disk
// ---------------------------------------------------------------------------

#[test]
fn writing_then_loading_a_fresh_character_reproduces_it() {
    let dir = scratch("fresh-round-trip");
    let mut g = fresh_game();
    g.player.money = 4321;
    g.pontovost_street = 777;
    g.pistol.owned = true;
    g.pistol.cartridges = 3;
    g.places.mark_found(Location::Club);
    let path = g.write_save_as(&dir, "save_r0.sav").unwrap();
    assert!(path.is_file());
    assert_eq!(std::fs::read(&path).unwrap().len(), SIZE);
    // `write_save_as` writes the RECORD only -- the district autosave
    // (1000:acc8 -> 1000:acd5) writes nothing else, and only the mage's arm
    // goes on to the flags.
    assert!(!dir.join("places.sav").exists());
    g.write_places(&dir).unwrap();
    assert!(dir.join("places.sav").is_file());

    let back = persist::load_slot(&dir, '0', 1).unwrap().expect("loads");
    assert_eq!(back.player.name, g.player.name);
    assert_eq!(back.player.money, 4321);
    assert_eq!(back.pontovost_street, 777);
    assert!(back.pistol.owned);
    assert_eq!(back.pistol.cartridges, 3);
    assert!(
        back.places.is_found(Location::Club),
        "places.sav round-trips"
    );
    // Slot 0 derives the district from the level (1000:6d93), not from the
    // digit; a level-0 character lands in district 1.
    assert_eq!(back.district, 1);
}

/// The slot menu, `1000:6a62`..`1000:6b81`. All three arms.
#[test]
fn the_slot_menu_reports_what_the_directory_holds() {
    let dir = scratch("slot-menu");
    // No save at all: the original prints nothing and falls through.
    assert!(persist::present_slots(&dir).is_empty());
    let mut none = std::iter::empty();
    assert_eq!(
        persist::choose_slot(&dir, &mut none).unwrap(),
        persist::SlotMenu::NoSaves
    );

    // The shipped corpus is UPPERCASE; the game writes lowercase. Both are
    // the same file on DOS, and `present_slots` has to see either. Order is
    // by name -- a port decision, since FindFirst walks FAT directory order.
    std::fs::write(dir.join("SAVE_R3.SAV"), orig("SAVE_R3.SAV")).unwrap();
    std::fs::write(dir.join("save_r0.sav"), orig("SAVE_R0.SAV")).unwrap();
    assert_eq!(persist::present_slots(&dir), vec!['0', '3']);

    // 1000:6b5e..1000:6b7f accepts 0 and 2..5 and nothing else -- `1` is the
    // key the prompt tells the player to press for a new character, and it
    // is the clearest case of a key that must NOT be a slot.
    for (typed, want) in [
        ("3", persist::SlotMenu::Load('3')),
        ("0", persist::SlotMenu::Load('0')),
        ("1", persist::SlotMenu::NewCharacter),
        ("q", persist::SlotMenu::NewCharacter),
        ("", persist::SlotMenu::NewCharacter),
    ] {
        let mut lines = std::iter::once(Ok(typed.to_string()));
        assert_eq!(
            persist::choose_slot(&dir, &mut lines).unwrap(),
            want,
            "typed {typed:?}"
        );
    }
}

/// The mask `save_r?.sav` matches ANY single character, and the key test is
/// a separate mechanism.
///
/// `1000:6a81`/`1000:6a8a` pass a DOS `?` wildcard to `FindFirst` and
/// `1000:6ada` prints whatever byte came back, so `save_r1.sav` and
/// `save_rx.sav` are both listed and both prompted for -- and neither is a
/// key `1000:6b5e`..`1000:6b7f` accepts, so typing what they show starts a
/// new character. Filtering the SCAN on the key set was the Task 19 review's
/// Important 3: one constant serving two mechanisms.
#[test]
fn the_scan_matches_any_character_while_the_key_test_stays_closed() {
    let dir = scratch("wildcard-scan");
    for name in ["save_r0.sav", "save_r1.sav", "save_r4.sav", "save_rx.sav"] {
        std::fs::write(dir.join(name), orig("SAVE_R4.SAV")).unwrap();
    }
    // Files that do NOT match the mask must be invisible: the `?` is exactly
    // one character wide.
    for name in ["save_r.sav", "save_r10.sav", "save_r4.txt", "notes.txt"] {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    assert_eq!(
        persist::present_slots(&dir),
        vec!['0', '1', '4', 'x'],
        "the mask matches any single character, and only one"
    );

    // ...and all four are offered a menu line.
    let lines = persist::slot_menu_lines(&persist::present_slots(&dir));
    assert!(lines.contains(&"^1Можно начать с 1 района".to_string()));
    assert!(lines.contains(&"^1Можно начать с x района".to_string()));

    // But the key test is unchanged: only `0` and `2`..`5` load. `1` and `x`
    // are listed by the original too and accepted by neither.
    for (typed, want) in [
        ("4", persist::SlotMenu::Load('4')),
        ("0", persist::SlotMenu::Load('0')),
        ("1", persist::SlotMenu::NewCharacter),
        ("x", persist::SlotMenu::NewCharacter),
    ] {
        let mut lines = std::iter::once(Ok(typed.to_string()));
        assert_eq!(
            persist::choose_slot(&dir, &mut lines).unwrap(),
            want,
            "typed {typed:?}"
        );
    }
    assert!(persist::load_slot(&dir, '1', 1).unwrap().is_none());
    assert!(persist::load_slot(&dir, 'x', 1).unwrap().is_none());
}

/// A slot the menu offered but whose file cannot be read is the original's
/// own fall-through (`1000:6bdb` / `1000:6da5`), not an error.
#[test]
fn an_unreadable_slot_falls_through_to_a_new_character() {
    let dir = scratch("bad-slot");
    assert!(persist::load_slot(&dir, '4', 1).unwrap().is_none());
    std::fs::write(dir.join("save_r4.sav"), b"not a record").unwrap();
    assert!(persist::load_slot(&dir, '4', 1).unwrap().is_none());
}

/// Loading slot 2..5 takes the district from the digit (`1000:6bf9`) and
/// never opens `places.sav` (`1000:6c50` gates that on `district == 0`).
#[test]
fn a_district_slot_takes_its_district_from_the_key_and_ignores_places_sav() {
    let dir = scratch("district-slot");
    std::fs::write(dir.join("save_r4.sav"), orig("SAVE_R4.SAV")).unwrap();
    std::fs::write(dir.join("places.sav"), [1u8; 7]).unwrap();

    let g = persist::load_slot(&dir, '4', 1).unwrap().expect("loads");
    assert_eq!(g.district, 4);
    // SAVE_R4 is a Вор (class 6), whose 1000:73bb bonus is Dealers alone.
    // If places.sav had been read, all seven would be found.
    assert!(g.places.is_found(Location::Dealers), "the class bonus");
    assert!(!g.places.is_found(Location::Club));
    assert!(!g.places.is_found(Location::Gym));
    assert!(
        !g.places.is_found(Location::Vet),
        "a load never runs 1000:6dc3, the new-character block's vet flag"
    );
}

/// The mage's paid arm writes both files and prints the original's own
/// confirmation. Driven through `Game::dispatch`'s real handler, not by
/// calling the writer directly.
#[test]
fn the_mage_writes_both_files_when_paid() {
    let dir = scratch("mage");
    let mut g = fresh_game();
    g.save_dir = dir.clone();
    g.district = 3;
    g.player.money = 500;
    g.mage(&mut ["y".to_string()].into_iter().map(Ok)).unwrap();
    // district * 50, 1000:7605 / 1000:761d.
    assert_eq!(g.player.money, 350);
    assert_eq!(
        std::fs::read(dir.join(persist::MAGE_SAVE)).unwrap().len(),
        SIZE
    );
    assert_eq!(
        std::fs::read(dir.join(persist::PLACES_SAVE)).unwrap().len(),
        7
    );
}

/// The district-advance autosave, `1000:ac5e`..`1000:ad12`, driven through
/// `Game::district_advance` -- the port's copy of the whole `1000:ab75` block.
///
/// **The filename digit is the district AFTER `1000:ab92`'s increment**, which
/// is why the shipped corpus is `SAVE_R2`..`SAVE_R5` with no `SAVE_R1`. The
/// check below is not "some file appeared": it names the file the pre-increment
/// district would have produced and requires it to be absent, so an off-by-one
/// in either direction is red.
#[test]
fn the_district_autosave_writes_the_slot_of_the_district_it_just_reached() {
    for (from, to) in [(1u8, 2u8), (2, 3), (3, 4)] {
        let dir = scratch(&format!("district-autosave-{from}"));
        let mut g = fresh_game();
        g.save_dir = dir.clone();
        g.district = from;
        g.player.level = 40; // clears 1000:ab7f for every district here
        g.district_advance(&mut ["y".to_string()].into_iter().map(Ok))
            .unwrap();
        assert_eq!(g.district, to, "1000:ab92");

        let written = dir.join(persist::slot_filename(
            char::from_digit(u32::from(to), 10).unwrap(),
        ));
        assert_eq!(
            std::fs::read(&written).unwrap().len(),
            SIZE,
            "1000:acb9 Rewrite(f, 694) into {}",
            written.display()
        );
        let stale = dir.join(persist::slot_filename(
            char::from_digit(u32::from(from), 10).unwrap(),
        ));
        assert!(
            !stale.exists(),
            "the digit is Str([0x3692]) read AFTER 1000:ab92, not before"
        );
        // 1000:acc8's BlockWrite is followed by 1000:acd5 Close and nothing
        // else: the mage's places.sav pass (1000:766f..1000:7724) is not in
        // this block.
        assert!(
            !dir.join(persist::PLACES_SAVE).exists(),
            "1000:ab75..1000:ad12 has exactly one Rewrite and one BlockWrite"
        );
    }
}

/// `1000:ac59`'s `jz` is the only way to the writer: any other answer jumps
/// to `1000:ad12` and the district still advances.
#[test]
fn the_district_autosave_writes_nothing_when_the_answer_is_not_y() {
    for answer in ["n", "", "yes", "да"] {
        let dir = scratch(&format!("district-autosave-decline-{}", answer.len()));
        let mut g = fresh_game();
        g.save_dir = dir.clone();
        g.district = 2;
        g.player.level = 40;
        g.district_advance(&mut [answer.to_string()].into_iter().map(Ok))
            .unwrap();
        assert_eq!(g.district, 3, "the increment is upstream of the prompt");
        assert!(
            persist::present_slots(&dir).is_empty(),
            "answer {answer:?} must not reach 1000:acb9"
        );
    }
    // ...and the compare itself is case-folded at 1000:ac45, so `Y` writes.
    let dir = scratch("district-autosave-uppercase-y");
    let mut g = fresh_game();
    g.save_dir = dir.clone();
    g.district = 2;
    g.player.level = 40;
    g.district_advance(&mut ["Y".to_string()].into_iter().map(Ok))
        .unwrap();
    assert_eq!(persist::present_slots(&dir), vec!['3'], "0eed:0216");
}

/// The record the autosave writes is the same record the loader reads back,
/// and the district it lands in comes from the DIGIT (`1000:6bf9`), not from
/// anything inside the 694 bytes.
#[test]
fn a_district_autosave_reloads_as_the_district_it_was_named_for() {
    let dir = scratch("district-autosave-reload");
    let mut g = fresh_game();
    g.save_dir = dir.clone();
    g.district = 3;
    g.player.level = 40;
    g.player.money = 1234;
    g.district_advance(&mut ["y".to_string()].into_iter().map(Ok))
        .unwrap();
    assert_eq!(g.district, 4);

    let back = persist::load_slot(&dir, '4', 1).unwrap().expect("loads");
    assert_eq!(back.player.money, 1234);
    assert_eq!(
        back.district, 4,
        "1000:6bf9 takes the district from the key"
    );
}

/// ...and the unpaid arms write nothing at all.
#[test]
fn the_mage_writes_nothing_when_declined_or_broke() {
    for (tag, answer, money) in [("declined", "n", 500), ("broke", "y", 10)] {
        let dir = scratch(&format!("mage-{tag}"));
        let mut g = fresh_game();
        g.save_dir = dir.clone();
        g.district = 3;
        g.player.money = money;
        g.mage(&mut [answer.to_string()].into_iter().map(Ok))
            .unwrap();
        assert_eq!(g.player.money, money, "{tag}: nothing is charged");
        assert!(!dir.join(persist::MAGE_SAVE).exists(), "{tag}");
        assert!(!dir.join(persist::PLACES_SAVE).exists(), "{tag}");
    }
}

// ---------------------------------------------------------------------------
// Guards on the format itself
// ---------------------------------------------------------------------------

/// A record byte the original only ever stores 0 or 1 into is refused when
/// it holds anything else, rather than being silently rewritten as 1 -- see
/// `SaveError::NotBoolean`. Checked at BOTH ends of the record so a check
/// that only covers the first span cannot pass.
#[test]
fn a_flag_byte_that_is_not_a_boolean_is_refused() {
    for off in [0x214usize, 0x21f, 0x226, 0x2ae, 0x2b2] {
        let mut bytes = orig("SAVE_R3.SAV");
        bytes[off] = 2;
        let err = match Save::parse(&bytes) {
            Ok(_) => panic!("0x{off:03x} = 2 must be refused"),
            Err(e) => e,
        };
        assert!(
            matches!(err, gopnik::save::SaveError::NotBoolean { off: o, value: 2 } if o == off),
            "0x{off:03x}: {err:?}"
        );
    }
    // ...and the same byte set to 1 is fine, so the guard is about the value
    // and not about the offset being untouchable.
    let mut bytes = orig("SAVE_R3.SAV");
    bytes[0x214] = 1;
    assert!(Save::parse(&bytes).unwrap().items.broken_jaw);
}

/// `to_bytes` must not carry anything through from the source blob except
/// the two shortstring padding windows. Fed a `Save` whose padding is real
/// but whose every other field is default, the output has to be zero
/// everywhere the fields are zero -- which is what makes the round trip a
/// check on the offsets rather than a copy.
#[test]
fn to_bytes_rebuilds_the_record_instead_of_copying_the_source() {
    let bytes = orig("SAVE_R5.SAV");
    let mut save = Save::parse(&bytes).unwrap();
    save.stats = [0; 8];
    save.hp = 0;
    save.hpmax = 0;
    save.items = Default::default();
    save.buff_countdown = 0;
    save.xp = 0;
    save.threshold = 0;
    save.growth_log = [[0; 3]; 40];
    let out = save.to_bytes().unwrap();
    assert!(
        out[0x200..].iter().all(|&b| b == 0),
        "0x200.. must be rebuilt from the (now zero) fields, not copied"
    );
    // The names still round-trip, padding included, because those two
    // windows ARE carried through.
    assert_eq!(&out[..0x200], &bytes[..0x200]);
}

/// Every field `data/save_layout.json` names must be reachable from the
/// port's `Save`, or the two have drifted. Checked by count and by the
/// tiling property the JSON already asserts, then by writing a distinct
/// value into each named field and requiring the bytes to move.
#[test]
fn every_named_field_is_actually_written_by_to_bytes() {
    let p = root().join("data").join("save_layout.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let fields = json["fields"].as_array().unwrap();

    // Zero every field, then compare against a record that is all zero
    // except the two padding windows: every byte the layout claims must be
    // accounted for.
    let mut save = Save::blank();
    save.magic = String::new();
    let zeroed = save.to_bytes().unwrap();
    assert_eq!(
        zeroed,
        vec![0u8; SIZE],
        "Save::blank must be an empty record"
    );

    // ...and every field, set to 1, must move exactly its own bytes.
    for f in fields {
        let name = f["name"].as_str().unwrap();
        let off = f["off"].as_u64().unwrap() as usize;
        let len = f["len"].as_u64().unwrap() as usize;
        let mut s = Save::blank();
        s.magic = String::new();
        set_one(&mut s, name);
        let out = s.to_bytes().unwrap();
        let moved: Vec<usize> = (0..SIZE).filter(|&i| out[i] != 0).collect();
        assert!(!moved.is_empty(), "{name}: setting it moved no byte at all");
        assert!(
            moved.iter().all(|&i| (off..off + len).contains(&i)),
            "{name} at 0x{off:03x}+{len} moved {moved:?}"
        );
    }
}

/// Write a distinctive value into the named field. Exhaustive over
/// `data/save_layout.json`'s names: an unknown name panics rather than being
/// skipped, so a field added there without a `Save` field fails the test
/// above instead of silently passing it.
fn set_one(s: &mut Save, name: &str) {
    let it = &mut s.items;
    match name {
        "magic" => s.magic = "x".into(),
        "name" => s.name = "x".into(),
        "rank_index" => s.stats[0] = 1,
        "strength" => s.stats[1] = 1,
        "agility" => s.stats[2] = 1,
        "vitality" => s.stats[3] = 1,
        "luck" => s.stats[4] = 1,
        "level" => s.stats[5] = 1,
        "dmg_min" => s.stats[6] = 1,
        "dmg_max" => s.stats[7] = 1,
        "hp" => s.hp = 1,
        "hpmax" => s.hpmax = 1,
        "broken_jaw" => it.broken_jaw = true,
        "broken_leg" => it.broken_leg = true,
        "armour" => it.armour = 1,
        "dark_glasses" => it.dark_glasses = true,
        "suit_abibas" => it.suit_abibas = true,
        "boots" => it.boots = true,
        "jacket" => it.jacket = true,
        "suit_adidas" => it.suit_adidas = true,
        "boots_pontovye" => it.boots_pontovye = true,
        "jacket_krutaya" => it.jacket_krutaya = true,
        "kastet" => it.kastet = true,
        "mobile" => it.mobile = true,
        "prison_tattoo" => it.prison_tattoo = true,
        "krestik" => it.krestik = true,
        "ring_gs" => it.ring_gs = true,
        "ring_pg" => it.ring_pg = true,
        "mega_ring" => it.mega_ring = true,
        "ring_gp" => it.ring_gp = true,
        "nozh" => it.nozh = true,
        "beer_half_litres" => it.beer_half_litres = 1,
        "joints" => it.joints = 1,
        "money" => it.money = 1,
        "junk" => it.junk = 1,
        "street_cred" => it.street_cred = 1,
        "buff_countdown" => s.buff_countdown = 1,
        "xp" => s.xp = 1,
        "threshold" => s.threshold = 1,
        "growth_log" => s.growth_log[0][0] = 1,
        "tooth_guard" => it.tooth_guard = true,
        "dubinka" => it.dubinka = true,
        "tesak" => it.tesak = true,
        "pistol" => it.pistol = true,
        "silencer" => it.silencer = true,
        "cartridges" => it.cartridges = 1,
        "church_stage" => it.church_stage = 1,
        other => panic!("save_layout.json names {other:?}, which src/save.rs has no field for"),
    }
}

/// `docs/re/save-format.md`'s per-byte table and `data/save_layout.json`
/// must not drift: every field in the artifact has to appear in the prose
/// with the same offset and the same DGROUP address.
#[test]
fn the_document_and_the_artifact_name_the_same_bytes() {
    let doc =
        std::fs::read_to_string(root().join("docs").join("re").join("save-format.md")).unwrap();
    let json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("data").join("save_layout.json")).unwrap(),
    )
    .unwrap();
    let mut checked = 0;
    for f in json["fields"].as_array().unwrap() {
        let Some(guest) = f["guest"].as_str() else {
            continue;
        };
        let name = f["name"].as_str().unwrap();
        let off = f["off"].as_u64().unwrap();
        assert!(
            doc.contains(&format!("`{guest}`")),
            "{name}: save-format.md never mentions {guest}"
        );
        assert!(
            doc.contains(&format!("0x{off:03x}")),
            "{name}: save-format.md never mentions offset 0x{off:03x}"
        );
        checked += 1;
    }
    assert_eq!(checked, 35, "every field carrying a DGROUP address");
}

/// The five shipped saves are ground truth and nothing in this suite may
/// write to them. Asserted rather than trusted: `Game::write_save_as` takes
/// a directory, and a default that resolved to `orig/` would be caught here.
#[test]
fn nothing_here_writes_into_the_frozen_corpus() {
    let before: Vec<(String, Vec<u8>)> = SHIPPED
        .iter()
        .map(|n| (n.to_string(), orig(n)))
        .chain(std::iter::once(("PLACES.SAV".into(), orig("PLACES.SAV"))))
        .collect();
    let dir = scratch("corpus-guard");
    let mut g = fresh_game();
    g.save_dir = dir.clone();
    g.write_save_as(&dir, "save_r0.sav").unwrap();
    g.write_places(&dir).unwrap();
    g.mage_save().unwrap();
    for (name, bytes) in before {
        assert_eq!(orig(&name), bytes, "{name} changed");
    }
    assert!(Path::new(&dir).join("save_r0.sav").is_file());
}

/// Every `20ae:` address `src/game.rs`'s field docs name that lies **inside**
/// the 694-byte record (`20ae:369c`..`20ae:3951`) must be cited in
/// `src/persist.rs`, i.e. be a byte `to_save` actually writes.
///
/// This is the guard against the one failure this conversion can have
/// silently: a `Game` field for a record byte that nobody remembered to
/// persist. It re-derives both sets from the sources rather than restating a
/// list, so adding such a field fails here instead of quietly losing the byte
/// on every save.
///
/// Addresses OUTSIDE the record are ignored on purpose and enumerated in
/// `Game::to_save`'s doc: the original does not save them either.
#[test]
fn every_in_record_address_named_in_game_rs_is_persisted() {
    let game_rs = std::fs::read_to_string(root().join("src").join("game.rs")).unwrap();
    // Scoped to `to_save`'s BODY, not to the whole of persist.rs. Two
    // reasons, both of which make the check unfalsifiable if ignored:
    // `src/save.rs` names every record address in its own field docs, and
    // persist.rs's module doc names the record's two BOUNDARY addresses in
    // prose. Either would let a byte `to_save` silently drops still pass.
    let persist_rs = std::fs::read_to_string(root().join("src").join("persist.rs")).unwrap();
    let from = persist_rs.find("pub fn to_save").expect("to_save");
    let to = persist_rs.find("pub fn from_save").expect("from_save");
    assert!(from < to);
    let to_save_body = &persist_rs[from..to];

    let lo = gopnik::save::RECORD_BASE;
    let hi = lo + SIZE;
    let mut in_record: Vec<usize> = Vec::new();
    let mut i = 0;
    let bytes = game_rs.as_bytes();
    while let Some(k) = game_rs[i..].find("20ae:") {
        let at = i + k + 5;
        i = at;
        if at + 4 > bytes.len() {
            break;
        }
        let hex = &game_rs[at..at + 4];
        let Ok(off) = usize::from_str_radix(hex, 16) else {
            continue;
        };
        if (lo..hi).contains(&off) && !in_record.contains(&off) {
            in_record.push(off);
        }
    }
    in_record.sort_unstable();
    // A count floor set at today's value could not fail downwards, so the
    // scan is sanity-checked by a positive AND a negative control instead:
    // an address it must find, and one it must not.
    assert!(
        in_record.contains(&0x394d),
        "the scan missed 20ae:394d, the pistol byte src/game.rs certainly names"
    );
    assert!(
        !in_record.contains(&0x3c83),
        "20ae:3c83 (the rector flag) is ABOVE the record and must be excluded"
    );

    for off in in_record {
        let cite = format!("20ae:{off:04x}");
        assert!(
            to_save_body.contains(&cite),
            "src/game.rs names {cite} (.SAV 0x{:03x}), which is inside the record, \
             but `Game::to_save` never mentions it -- so nothing carries that byte \
             into a save",
            off - lo
        );
    }
}

// ---------------------------------------------------------------------------
// The load path's DECISIONS, as opposed to its data transformations.
//
// `cargo mutants -f src/persist.rs` found ten survivors, all of them here:
// which file gets opened, which slot line gets printed, which arm runs when
// something is missing or malformed, and what district a level maps to. The
// round trip covers the bytes; these cover the branches.
// ---------------------------------------------------------------------------

/// The case fallback is what makes the SHIPPED corpus loadable at all: DOS
/// wrote uppercase names (`SAVE_R3.SAV`) and the game writes lowercase
/// (`save_r3.sav`), which are the same file on FAT and two files here.
#[test]
fn a_slot_loads_under_either_case_and_reports_absence_as_absence() {
    // lowercase only -- the name the game itself writes.
    let dir = scratch("case-lower");
    std::fs::write(dir.join("save_r3.sav"), orig("SAVE_R3.SAV")).unwrap();
    assert_eq!(persist::present_slots(&dir), vec!['3']);
    assert_eq!(
        persist::load_slot(&dir, '3', 1)
            .unwrap()
            .unwrap()
            .player
            .level,
        20
    );

    // uppercase only -- the name the shipped corpus carries.
    let dir = scratch("case-upper");
    std::fs::write(dir.join("SAVE_R3.SAV"), orig("SAVE_R3.SAV")).unwrap();
    assert_eq!(persist::present_slots(&dir), vec!['3']);
    assert_eq!(
        persist::load_slot(&dir, '3', 1)
            .unwrap()
            .unwrap()
            .player
            .level,
        20
    );

    // Neither: not a slot, and loading it falls through rather than erroring.
    let dir = scratch("case-neither");
    assert!(persist::present_slots(&dir).is_empty());
    assert!(persist::load_slot(&dir, '3', 1).unwrap().is_none());

    // The fallback is on ANY failure of the first name, not only NotFound:
    // a DIRECTORY called `save_r3.sav` shadows the lowercase name with a
    // different error, and the uppercase file must still be found. This is
    // what pins the missing case-guard `cargo mutants` reported.
    let dir = scratch("case-shadowed");
    std::fs::create_dir(dir.join("save_r3.sav")).unwrap();
    std::fs::write(dir.join("SAVE_R3.SAV"), orig("SAVE_R3.SAV")).unwrap();
    assert_eq!(
        persist::load_slot(&dir, '3', 1)
            .unwrap()
            .expect("the uppercase file is still readable")
            .player
            .level,
        20
    );
}

/// The menu's text and the separator's placement, `1000:6a99`..`1000:6b0d`.
///
/// `^1или` goes BETWEEN entries -- the original tests a counter of slots
/// printed so far, so it never leads and never trails.
#[test]
fn the_menu_lines_are_the_originals_and_the_separator_goes_between() {
    assert_eq!(persist::slot_menu_lines(&[]), Vec::<String>::new());

    // One slot: no separator at all.
    assert_eq!(
        persist::slot_menu_lines(&['3']),
        vec!["^1Можно начать с 3 района"]
    );

    // Slot 0 gets its own line, and it is not the district one.
    assert_eq!(
        persist::slot_menu_lines(&['0']),
        vec!["^1Можно начать с того места где ты сохранился"]
    );

    // Several slots: exactly one separator between each adjacent pair, and
    // none before the first or after the last.
    assert_eq!(
        persist::slot_menu_lines(&['2', '3', '0']),
        vec![
            "^1Можно начать с 2 района",
            "^1или",
            "^1Можно начать с 3 района",
            "^1или",
            "^1Можно начать с того места где ты сохранился",
        ]
    );
    let four = persist::slot_menu_lines(&['2', '3', '4', '5']);
    assert_eq!(four.len(), 7, "4 entries + 3 separators");
    assert_eq!(four.iter().filter(|l| *l == "^1или").count(), 3);
    assert_ne!(four[0], "^1или", "the separator never leads");
    assert_ne!(four[6], "^1или", "and never trails");

    assert_eq!(
        persist::SLOT_PROMPT,
        "^0Нажми цифру с какого района начать. 1-начать сначала"
    );
}

/// Slot 0's `places.sav`, `1000:6c5a`..`1000:6d73`. Seven one-byte reads, so
/// a file with fewer than seven bytes takes the FAILURE arm (`1000:6d3b`),
/// which clears the flags with three class-keyed exceptions -- and must not
/// panic.
#[test]
fn a_short_places_sav_takes_the_failure_arm_instead_of_panicking() {
    // SAVE_R0 is a class-4 Отморозок, whose 1000:73bb bonus is nothing at
    // all, so the failure arm leaves every flag clear and the success arm
    // with an all-ones file leaves every flag set. The two are
    // distinguishable, which is what makes this a test.
    for (tag, bytes, want_found) in [
        ("empty", vec![], false),
        ("six", vec![1u8; 6], false),
        ("seven", vec![1u8; 7], true),
        ("eight", vec![1u8; 8], true),
    ] {
        let dir = scratch(&format!("places-{tag}"));
        std::fs::write(dir.join("save_r0.sav"), orig("SAVE_R0.SAV")).unwrap();
        std::fs::write(dir.join("places.sav"), &bytes).unwrap();
        let g = persist::load_slot(&dir, '0', 1).unwrap().expect("loads");
        assert_eq!(
            g.places.is_found(Location::Gym),
            want_found,
            "{tag}: {} bytes of places.sav",
            bytes.len()
        );
    }

    // No places.sav at all is the same failure arm.
    let dir = scratch("places-missing");
    std::fs::write(dir.join("save_r0.sav"), orig("SAVE_R0.SAV")).unwrap();
    let g = persist::load_slot(&dir, '0', 1).unwrap().expect("loads");
    assert!(!g.places.is_found(Location::Gym));
}

/// Slot 0's district is `level div 10 + 1` (`1000:6d93`..`1000:6d9d`:
/// `mov ax,[0x38a6]` / `cwd` / `idiv 10` / `inc ax`). Boundaries, not a
/// midpoint: `%` and `*` both survive a single interior sample.
#[test]
fn slot_zero_derives_its_district_from_the_level() {
    let dir = scratch("district-from-level");
    for (level, want) in [
        (0u16, 1u8),
        (9, 1),
        (10, 2),
        (19, 2),
        (20, 3),
        (29, 3),
        (30, 4),
        (39, 4),
        (40, 5),
    ] {
        let mut save = Save::parse(&orig("SAVE_R0.SAV")).unwrap();
        save.stats[5] = level;
        std::fs::write(dir.join("save_r0.sav"), save.to_bytes().unwrap()).unwrap();
        let g = persist::load_slot(&dir, '0', 1).unwrap().expect("loads");
        assert_eq!(g.district, want, "level {level}");
    }
}

/// ...and a district slot takes the digit itself, for every digit the
/// original accepts. A key it does NOT accept never reaches the open
/// (`1000:6b81`) and must not be defaulted into a district.
#[test]
fn a_district_slot_takes_the_digit_and_a_rejected_key_loads_nothing() {
    let dir = scratch("slot-digits");
    for d in ['2', '3', '4', '5'] {
        std::fs::write(dir.join(persist::slot_filename(d)), orig("SAVE_R4.SAV")).unwrap();
        let g = persist::load_slot(&dir, d, 1).unwrap().expect("loads");
        assert_eq!(g.district, d.to_digit(10).unwrap() as u8, "slot {d}");
    }
    // `1` is the key the prompt itself suggests, and it is not a slot; `q`
    // is not even a digit. Neither may produce a game, and in particular
    // neither may be silently defaulted to district 1.
    for bad in ['1', '6', 'q'] {
        std::fs::write(dir.join(format!("save_r{bad}.sav")), orig("SAVE_R4.SAV")).unwrap();
        assert!(
            persist::load_slot(&dir, bad, 1).unwrap().is_none(),
            "slot {bad:?} must be refused even with a readable file present"
        );
    }
}

/// Slot 0's district division is SIGNED, matching `1000:6d93`'s `cwd` /
/// `idiv`, and truncates to the low byte, matching `mov [0x3692],al`.
///
/// Unreachable from play, where the level is 0..40 — but reachable from
/// `tools/savegen.py`, which is the instrument this branch hands the next
/// several tasks, and a citation that holds only for part of its input's
/// range is a citation that does not hold.
#[test]
fn slot_zero_district_matches_the_originals_signed_idiv() {
    let dir = scratch("district-signed");
    // Expected values are written down, not recomputed from the same
    // expression the implementation uses -- that would restate the code
    // rather than check it. `idiv` truncates toward zero.
    for (level, want) in [
        // -1: -1/10 == 0, +1 == 1. An UNSIGNED divide reads 0xFFFF as 65535
        // and gives 6554, whose low byte is 0x9A == 154.
        (0xFFFFu16, 1u8),
        // -10: -10/10 == -1, +1 == 0.
        (0xFFF6, 0),
        // -32768: /10 == -3276, +1 == -3275 == 0xF335; `mov [0x3692],al`
        // keeps 0x35 == 53.
        (0x8000, 53),
    ] {
        let mut save = Save::parse(&orig("SAVE_R0.SAV")).unwrap();
        save.stats[5] = level;
        std::fs::write(dir.join("save_r0.sav"), save.to_bytes().unwrap()).unwrap();
        let g = persist::load_slot(&dir, '0', 1).unwrap().expect("loads");
        assert_eq!(
            g.district, want,
            "level 0x{level:04x} ({} signed)",
            level as i16
        );
    }
    // The two readings must actually disagree, or the cases above would be
    // passing for the wrong reason.
    assert_eq!(
        ((0xFFFFu16 / 10) + 1) as u8,
        154,
        "unsigned would give this"
    );
    assert_ne!(154, 1, "sanity: signed and unsigned disagree on 0xFFFF");
}

/// The three classes of record the port refuses or alters and the original
/// loads verbatim — the `docs/re/gaps.md` entry "What the port REFUSES that
/// the original accepts", made executable.
///
/// All three are reachable with `tools/savegen.py`, which is what makes them
/// worth pinning: the next several tasks force states with it, and a
/// refusal that is only written down gets rediscovered as a bug.
#[test]
fn the_three_records_the_port_refuses_or_alters_are_the_documented_ones() {
    // 1. A flag byte outside {0,1}. The original loads it -- every consumer
    //    tests `<> 0` -- and `data/probes/saveprobe-record-base.json` shows
    //    it doing so with sentinels 0x40.. across both spans.
    let mut bytes = orig("SAVE_R3.SAV");
    bytes[0x2ae] = 2;
    assert!(
        matches!(
            Save::parse(&bytes),
            Err(gopnik::save::SaveError::NotBoolean {
                off: 0x2ae,
                value: 2
            })
        ),
        "the port refuses it"
    );
    // ...and the refusal surfaces as the original's Reset-IOResult arm,
    // which is the part that is a divergence in OUTPUT and not just policy.
    let dir = scratch("refuses-nonboolean");
    std::fs::write(dir.join("save_r3.sav"), &bytes).unwrap();
    assert!(
        persist::load_slot(&dir, '3', 1).unwrap().is_none(),
        "a record the original would load starts a new character here"
    );

    // 2. A negative Integer in one of the three fields `Fighter` holds as
    //    u16. The port clamps to 0 rather than refusing.
    let mut save = Save::parse(&orig("SAVE_R3.SAV")).unwrap();
    save.items.joints = -5;
    save.items.beer_half_litres = -1;
    save.items.junk = -300;
    let g = Game::from_save(&save, Places::from_bytes(&[0u8; 7]), 3, 1);
    assert_eq!(
        (g.player.joints, g.player.beer_dl, g.player.junk),
        (0, 0, 0)
    );
    // The clamp is lossy in one direction only: the record still held them.
    assert_eq!(save.items.junk, -300);

    // 3. A name of exactly 255 CP866 bytes -- the longest the format can
    //    hold -- cannot survive `Game::to_save`, because the `^7 ` prefix
    //    (1000:723a) is re-added on the way out. 255 + 3 = 258.
    let mut g = fresh_game();
    g.player.name = "x".repeat(255);
    let err = g
        .to_save()
        .to_bytes()
        .expect_err("255 + the 3-byte prefix is over the cap");
    assert!(
        matches!(err, gopnik::save::SaveError::TooLong(258)),
        "{err:?}"
    );
    // 252 is the port's real ceiling for a typed name, and it works.
    g.player.name = "x".repeat(252);
    assert_eq!(g.to_save().to_bytes().unwrap().len(), SIZE);
}

/// Task 20's review fix (C2): `1000:7347`..`1000:7364`, inside
/// `FUN_1000_6a0d`, arms `rector_showdown` whenever the game is ENTERED
/// with district already 5 -- a loaded save can have that be true on the
/// very first turn, before `Game::enter_district_5`'s own promotion hook
/// ever gets a chance to run (it only fires on the turn district BECOMES 5
/// during play). Slot 0 derives district from `level / 10 + 1`
/// (`1000:6d93`), so `level = 40` lands a fresh load at district 5 directly.
#[test]
fn loading_a_save_already_at_district_5_arms_the_rector_showdown() {
    let dir = scratch("load-at-district-5-arms-rector-showdown");
    let mut g = fresh_game();
    g.player.level = 40;
    assert!(!g.rector_showdown);
    g.write_save_as(&dir, "save_r0.sav").unwrap();
    g.write_places(&dir).unwrap();

    let back = persist::load_slot(&dir, '0', 1).unwrap().expect("loads");
    assert_eq!(back.district, 5, "level 40 / 10 + 1 = 5");
    assert!(
        back.rector_showdown,
        "1000:7364 must fire on entry when the loaded district is already 5"
    );
}

/// Control: a load that does NOT land at district 5 leaves the flag clear --
/// proves the assertion above can fail, and that `apply_class_bonus`'s new
/// district check does not fire unconditionally.
#[test]
fn loading_a_save_below_district_5_does_not_arm_the_rector_showdown() {
    let dir = scratch("load-below-district-5-no-rector-showdown");
    let mut g = fresh_game();
    g.player.level = 25; // 25 / 10 + 1 = district 3
    g.write_save_as(&dir, "save_r0.sav").unwrap();
    g.write_places(&dir).unwrap();

    let back = persist::load_slot(&dir, '0', 1).unwrap().expect("loads");
    assert_eq!(back.district, 3);
    assert!(!back.rector_showdown);
}
