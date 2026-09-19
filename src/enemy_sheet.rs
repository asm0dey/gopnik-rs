//! The last opponent's stat block -- `FUN_1000_1348`, `[1000:1348,
//! 1000:165f)`.
//!
//! `sv` at the fight prompt (`1000:4c49 call 0x1348`) is the only caller.
//! The function reads **nothing** in `[20ae:3690, 20ae:3951]` -- the player's
//! record -- so it renders the enemy record at `20ae:3952` and nothing else;
//! that is what settles `sv` against `s`, which calls the player's sheet
//! (`crate::character_sheet`) instead.
//!
//! ## The split, and the sharing
//!
//! Same split as [`crate::character_sheet`]: this module **builds** the
//! lines and [`crate::game::Game::inspect_enemy`] prints them, because
//! `crate::term::println` writes to this process's stdout and a unit test
//! cannot capture it.
//!
//! Two pieces are literally the same program in the two sheets and are
//! **shared, not transcribed twice**:
//!
//! * [`crate::character_sheet::Out`] -- the `Write` / `WriteLn` open-line
//!   model. The accuracy block below opens a line with a `Write`
//!   (`1000:15a4 call 0eed:0000`) that a later `WriteLn` closes, so this
//!   sheet needs it for the same reason the player's does.
//! * [`crate::character_sheet::accuracy_block`] -- `1000:156d`..`1000:1638`
//!   here, `1000:21b0`..`1000:2276` there, identical gate, identical
//!   arithmetic, identical three literals. Read that function's doc for the
//!   correspondence.
//!
//! ## What is NOT modelled
//!
//! The two health-colour thresholds' decimal values. `1000:14da mov cx,0x7f`
//! and `1000:150a mov cx,0x80` are the *same two comparands* the player
//! sheet's `1000:211d` / `1000:214d` load, so
//! [`crate::character_sheet::HEALTH_BROWN_ABOVE`]'s recorded gap covers this
//! copy too and this module reuses
//! [`crate::character_sheet::health_digit`] rather than guessing a second
//! time. `docs/re/gaps.md` carries the entry.
//!
//! ## Where the strings are pinned
//!
//! `tools/test_character_sheet_port.py` decodes every literal below out of
//! `orig/g.exe` and pins each `CS 0x....` citation to the literal beside it,
//! and `tools/difftest.py`'s `enemy_line` / `enemy_gap` / `enemy_fragment`
//! records re-find all fifteen of the span's literals by walking
//! `1000:135c`..`165e` instruction by instruction -- never from a hardcoded
//! offset list -- and compare them against the constants here.

use crate::character_sheet::{accuracy_block, health_digit, Out};
use crate::data;
use crate::model::Fighter;
use crate::text;

// CS `0x1289` -- the header's opener, where the player's sheet has its own.
pub const HEADER_OPEN: &str = "^2Это ";
// CS `0x1290`.
pub const HEADER_LEVEL: &str = " # уровня";
// CS `0x1274` -- what stands in for the крутизна word above the ladder.
pub const NOT_IN_THIS_LIFE: &str = "Не в этой жизни.";
/// What separates the header from the крутизна word -- the CS literal at
/// 0x1285, assigned at `1000:139c`. It carries no `CS` citation because it
/// is not game TEXT by `tools/test_character_sheet_port.py`'s test (no
/// colour markup, no Cyrillic), so that scanner would resolve the citation
/// to the wrong literal; `difftest`'s `enemy_fragment 1` record pins it
/// instead.
pub const KRUTIZNA_SEP: &str = " - ";
// CS `0x129a`.
pub const STATS: &str = "Сл:# Лв:# Жв:# Уд:#";
// CS `0x12ae`.
pub const DAMAGE: &str = "Урон #-#";
// CS `0x12b7` -- two trailing spaces.
pub const BROKEN_JAW: &str = "^4Сломана челюсть  ";
// CS `0x12cb` -- two trailing spaces.
pub const BROKEN_LEG: &str = "^4Сломана нога  ";
/// The one-character CS literal at 0x12dc, assigned at `1000:1523`, that the
/// health line's colour digit is appended to. Uncited for the same reason as
/// [`KRUTIZNA_SEP`], and the player sheet's copy (0x181f) is in the same
/// state; `difftest`'s `enemy_fragment 6` record is what pins it.
pub const COLOUR_PREFIX: &str = "^";
// CS `0x12de` -- two trailing spaces.
pub const HEALTH: &str = "Здоровье #/#  ";
// CS `0x133a` -- four trailing spaces.
pub const ARMOUR: &str = "^2Броня #    ";

/// The eight CS literals the sheet ASSEMBLES into its two composed lines,
/// in the image's address order -- which is the order `difftest`'s scan of
/// `1000:135c`..`165e` finds them in, so the array has to hold that order for
/// `enemy_fragment` to mean anything. The two composed lines interleave:
/// indices 0..3 build the header, 4..7 the health line.
///
/// The DGROUP halves -- the rank row at `DS:002e` (`1000:13e8 push ds`) and
/// the крутизна row at `DS:0b42` (`1000:136f push ds`) -- are not CS
/// literals and so are not here; [`lines`] interpolates them from
/// [`crate::data::rank_name`] and [`crate::data::krutizna`], the same way
/// `help`'s and the church's composed lines already do.
pub const FRAGMENTS: [&str; 8] = [
    NOT_IN_THIS_LIFE, // 1000:1382
    KRUTIZNA_SEP,     // 1000:139c
    HEADER_OPEN,      // 1000:13d2
    HEADER_LEVEL,     // 1000:13ef
    BROKEN_JAW,       // 1000:146e
    BROKEN_LEG,       // 1000:149f
    COLOUR_PREFIX,    // 1000:1523
    HEALTH,           // 1000:1542
];

/// The seven CS literals the sheet passes STRAIGHT to `Write`/`WriteLn`, in
/// the image's address order, each with whether the call closes the line.
///
/// `false` is `call 0eed:0000` (`Write`) and `true` is `call 0eed:01c2`
/// (`WriteLn`). Only one entry is a `Write`: `1000:15a4`'s `Точность 90% `,
/// which the accuracy block's own `WriteLn` two branches later closes.
///
/// Indices 2..5 are [`crate::character_sheet`]'s four accuracy constants --
/// the shared block's, not a second transcription; see this module's doc.
pub const EMITTED: [(bool, &str); 7] = [
    (true, STATS),                                    // 1000:1419
    (true, DAMAGE),                                   // 1000:1436
    (true, crate::character_sheet::ACCURACY_FLAT),    // 1000:157b
    (false, crate::character_sheet::ACCURACY_CAPPED), // 1000:15a4
    (true, crate::character_sheet::ACCURACY_SECOND),  // 1000:15e7
    (true, crate::character_sheet::ACCURACY_MANY),    // 1000:1611
    (true, ARMOUR),                                   // 1000:163f
];

/// Where the two composed lines fall among [`EMITTED`]'s seven, in the
/// `(index, events)` shape `crate::opening`'s gap tables use: the header's
/// `WriteLn` at `1000:1404` is before [`EMITTED`] index 0, and the health
/// line's at `1000:1568` between index 1 and index 2.
///
/// Neither composed `WriteLn` carries a CS literal of its own -- both print
/// `ss:[bp-...]` -- so nothing that scans for literals can see them, and
/// this is the only place a comparison can put them. `'C'` is the same
/// event code `church_gap` and `wander_gap` already use.
///
/// **The `'B'` and `'K'` codes are absent here because the span has none,
/// and that is compared, not assumed.** `difftest.py`'s `enemy` sweeps
/// `1000:135c`..`165e` for bare `WriteLn`s and `ReadKey`s in the same pass
/// that finds the composed lines, through the same `gaps_of` every other
/// span uses; either one appearing would put a `'B'` or a `'K'` into a
/// record and this table would stop matching. So "`sv` blocks on nothing and
/// prints no blank line" is a claim with a way to fail.
pub const GAPS: [(usize, &str); 2] = [(0, "C"), (2, "C")];

/// The fifteen CS literals of `1000:135c`..`165e`, split eight assembled and
/// seven emitted. `difftest.py`'s walk asserts the same total against the
/// image, so a scan that found fewer raises rather than comparing a short
/// list; this constant is the port's half of that number.
pub const LITERAL_COUNT: usize = FRAGMENTS.len() + EMITTED.len();

/// The whole block, in the original's order.
///
/// Five lines, or six when the enemy wears armour:
///
/// ```text
/// 1000:1404   ^2Это <rank> # уровня[ - <крутизна>]
/// 1000:1419   Сл:# Лв:# Жв:# Уд:#
/// 1000:1436   Урон #-#
/// 1000:1568   ^<digit>Здоровье #/#  [<injuries>]
/// 1000:157b.. Точность ...
/// 1000:1656   ^2Броня #          (only when the armour byte is non-zero)
/// ```
pub fn lines(e: &Fighter) -> Vec<String> {
    let mut o = Out::default();
    header(&mut o, e);
    o.writeln(&text::fill(
        STATS,
        &[
            i64::from(e.strength),
            i64::from(e.agility),
            i64::from(e.vitality),
            i64::from(e.luck),
        ],
    ));
    o.writeln(&text::fill(
        DAMAGE,
        &[i64::from(e.dmg_min), i64::from(e.dmg_max)],
    ));
    health_line(&mut o, e);
    accuracy_block(&mut o, e);
    armour_line(&mut o, e);
    o.finish()
}

/// `1000:135c`..`1000:1414` -- the composed header.
///
/// Three branches, in this order:
///
/// ```text
/// 135c  cmp word [0x395c],0x28 / 1361 jnle 0x1382   ; level > 40 -> the fallback
/// 1363  di = [0x395c] << 8 + 0xb42                  ; крутизна[level]
/// 139c  the suffix := ' - ' + that
/// 13c0  cmp word [0x3952],0x8  / 13c5 jl 0x13cc     ; class >= 8 -> no suffix
/// 13c7  mov byte [bp-0x200],0x0                     ; ... by emptying it
/// 13dc  di = [0x3952] << 8 + 0x2e                   ; ranks[class]
/// ```
///
/// **The name is not in this line.** `1000:13dc` indexes the rank table by
/// the enemy record's CLASS word, exactly as the player sheet's `1000:1a36`
/// indexes it by the player's; the enemy record has no name field at all
/// (`crate::model`'s table stops at `+0x16`). This port used to put
/// `enemy.name` here, which happened to print the same text --
/// `data/enemies.json`'s eleven names ARE the eleven rank rows -- so the
/// correction is structural, not visible: a hand-built [`Fighter`] whose
/// `name` and `class` disagree is the only place the two differ, and the
/// test below is exactly that.
///
/// **`data::krutizna` has 43 rows and the guard passes 41 of them.** Levels
/// 41 and 42 take the fallback although a row exists, which is the guard's
/// behaviour and not a bound check -- reproduced as written. The compare is
/// signed (`jnle`), but `level` cannot be negative in this port.
fn header(o: &mut Out, e: &Fighter) {
    let suffix = if e.class > 7 {
        // 1000:13c0: classes 8, 9 and 10 print no крутизна at all.
        String::new()
    } else if e.level < 0x29 {
        format!("{KRUTIZNA_SEP}{}", data::krutizna(e.level))
    } else {
        format!("{KRUTIZNA_SEP}{NOT_IN_THIS_LIFE}")
    };
    o.writeln(&text::fill(
        &format!(
            "{HEADER_OPEN}{}{HEADER_LEVEL}{suffix}",
            data::rank_name(e.class)
        ),
        &[i64::from(e.level)],
    ));
}

/// `1000:1451`..`1000:1568` -- the health line and the two injuries.
///
/// The injuries are **not** separate lines. `1000:1451` empties the
/// shortstring at `[bp-0x100]`, each flag appends its label to it, and
/// `1000:154c` appends the whole accumulator onto the health line after
/// `Здоровье #/#  `; only `1000:1568` closes it. Same shape as the player
/// sheet's four conditions.
///
/// Both guards are `cmp byte [...],0x1` / `jnz` -- an EQUALITY, not the
/// `> 0` the item flags elsewhere use: `1000:1456` for the jaw
/// (`20ae:3966`) and `1000:1487` for the leg (`20ae:3967`). A `bool` holds
/// only 0 and 1, so the port cannot tell the two senses apart here; the
/// equality is what the record's writers (`1000:45be`, `1000:45e5`) store.
fn health_line(o: &mut Out, e: &Fighter) {
    let mut cond = String::new();
    if e.broken_jaw {
        cond.push_str(BROKEN_JAW);
    }
    if e.broken_leg {
        cond.push_str(BROKEN_LEG);
    }
    o.writeln(&text::fill(
        &format!(
            "{COLOUR_PREFIX}{}{HEALTH}{cond}",
            health_digit(e.hp, e.hpmax)
        ),
        &[i64::from(e.hp), i64::from(e.hpmax)],
    ));
}

/// `1000:1638`..`1000:1656` -- the armour line, and its gate.
///
/// `1000:1638 cmp byte [0x3968],0x0` / `1000:163d jbe 0x165b`: an UNSIGNED
/// test on a byte, so the line prints for any non-zero armour and is skipped
/// entirely at zero. This port used to print it unconditionally, which gave
/// every unarmoured opponent a spurious `^2Броня 0` line.
///
/// Unlike the player sheet's `1000:228a`, which is a `Write` that the
/// clothing rows continue, this one is a `WriteLn` (`1000:1656`) and nothing
/// follows it: `1000:165b` is the epilogue.
fn armour_line(o: &mut Out, e: &Fighter) {
    if e.armor != 0 {
        // 1000:1644 `mov al,[0x3968]` / `xor ah,ah` -- a byte, zero-extended.
        o.writeln(&text::fill(ARMOUR, &[i64::from(e.armor)]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plausible opponent: class 0, level 3, and every number distinct so
    /// a line that fills the wrong field is visible.
    fn enemy() -> Fighter {
        Fighter {
            name: "не важно".into(),
            class: 0,
            level: 3,
            strength: 11,
            agility: 12,
            vitality: 13,
            luck: 14,
            hp: 40,
            hpmax: 50,
            dmg_min: 5,
            dmg_max: 9,
            armor: 0,
            ..Fighter::default()
        }
    }

    #[test]
    fn the_header_names_the_class_rank_and_not_the_name() {
        let mut e = enemy();
        e.name = "Вася".into();
        let l = lines(&e);
        assert!(l[0].contains(data::rank_name(0)), "{:?}", l[0]);
        assert!(!l[0].contains("Вася"), "{:?}", l[0]);
        assert!(l[0].starts_with(HEADER_OPEN), "{:?}", l[0]);
    }

    #[test]
    fn the_header_appends_the_krutizna_word_for_this_level() {
        let mut e = enemy();
        e.level = 7;
        let l = lines(&e);
        assert!(
            l[0].ends_with(&format!("{KRUTIZNA_SEP}{}", data::krutizna(7))),
            "{:?}",
            l[0]
        );
        assert!(l[0].contains("7 уровня"), "{:?}", l[0]);
        // The two tables are different tables, indexed differently. A port
        // that took `docs/re/port-gaps.md`'s old mislabel would print the
        // rank name twice.
        assert_ne!(data::krutizna(7), data::rank_name(7));
    }

    #[test]
    fn a_class_above_seven_gets_no_krutizna_suffix() {
        for class in 0..11u16 {
            let mut e = enemy();
            e.class = class;
            let head = lines(&e).remove(0);
            let want = format!("{HEADER_OPEN}{}{HEADER_LEVEL}", data::rank_name(class));
            let want = text::fill(&want, &[i64::from(e.level)]);
            if class > 7 {
                assert_eq!(head, want, "class {class} should print no suffix");
            } else {
                assert!(head.starts_with(&want), "class {class}: {head:?}");
                assert!(head.len() > want.len(), "class {class}: {head:?}");
            }
        }
    }

    /// `1000:135c` passes 41 of the ladder's 43 rows. 41 and 42 exist and
    /// are still unreachable.
    #[test]
    fn level_forty_one_falls_back_although_a_krutizna_row_exists() {
        let mut e = enemy();
        e.level = 40;
        assert!(lines(&e)[0].ends_with(data::krutizna(40)));
        for level in [41u16, 42] {
            e.level = level;
            assert!(!data::krutizna(level).is_empty(), "row {level} is real");
            assert!(
                lines(&e)[0].ends_with(NOT_IN_THIS_LIFE),
                "level {level} should fall back"
            );
        }
    }

    #[test]
    fn the_stat_and_damage_lines_fill_in_record_order() {
        let l = lines(&enemy());
        assert_eq!(l[1], "Сл:11 Лв:12 Жв:13 Уд:14");
        assert_eq!(l[2], "Урон 5-9");
    }

    #[test]
    fn each_injury_appends_to_the_health_line_and_both_appear_in_order() {
        let mut e = enemy();
        let plain = lines(&e).remove(3);
        assert!(plain.ends_with("Здоровье 40/50  "), "{plain:?}");

        e.broken_jaw = true;
        assert_eq!(lines(&e)[3], format!("{plain}{BROKEN_JAW}"));

        e.broken_jaw = false;
        e.broken_leg = true;
        assert_eq!(lines(&e)[3], format!("{plain}{BROKEN_LEG}"));

        e.broken_jaw = true;
        assert_eq!(lines(&e)[3], format!("{plain}{BROKEN_JAW}{BROKEN_LEG}"));
    }

    /// The injuries are part of the health line, not lines of their own --
    /// so the block's length does not change when they are set.
    #[test]
    fn the_injuries_add_no_lines() {
        let mut e = enemy();
        let n = lines(&e).len();
        e.broken_jaw = true;
        e.broken_leg = true;
        assert_eq!(lines(&e).len(), n);
    }

    #[test]
    fn the_health_colour_walks_four_six_two_with_the_ratio() {
        let mut e = enemy();
        e.hpmax = 100;
        let digit = |hp: u16, e: &mut Fighter| {
            e.hp = hp;
            lines(e)[3].chars().nth(1).unwrap()
        };
        assert_eq!(digit(5, &mut e), '4');
        assert_eq!(digit(40, &mut e), '6');
        assert_eq!(digit(90, &mut e), '2');
    }

    /// `1000:1638`'s gate. The line used to print unconditionally, so an
    /// unarmoured opponent got a `^2Броня 0` line the original never shows.
    #[test]
    fn the_armour_line_is_gated_on_a_non_zero_byte() {
        let mut e = enemy();
        e.armor = 0;
        let bare = lines(&e);
        assert!(!bare.iter().any(|l| l.contains("Броня")), "{bare:?}");
        e.armor = 3;
        let armoured = lines(&e);
        assert_eq!(armoured.len(), bare.len() + 1);
        assert_eq!(armoured.last().unwrap(), "^2Броня 3    ");
    }

    /// The three shapes of `1000:156d`..`1638`. The middle two are the
    /// oracle-confirmed captures quoted in `crate::combat`'s docs:
    /// `SAVE_R2` (agility 15) and `SAVE_R5` (agility 120).
    #[test]
    fn the_accuracy_block_has_three_shapes() {
        let mut e = enemy();

        e.agility = 12;
        assert_eq!(lines(&e)[4], "Точность 80%");

        e.agility = 15;
        assert_eq!(lines(&e)[4], "Точность 90%    Второй удар 5%");

        e.agility = 120;
        assert_eq!(
            lines(&e)[4],
            "Точность 90% - 6 ударов,  Точность 7 удара 80%"
        );
    }

    /// The `> 14` gate itself: 14 takes the flat line, 15 the capped one.
    #[test]
    fn fourteen_is_flat_and_fifteen_is_capped() {
        let mut e = enemy();
        e.agility = 14;
        assert_eq!(lines(&e)[4], "Точность 90%");
        e.agility = 15;
        assert!(lines(&e)[4].starts_with("Точность 90% "));
    }

    /// Every constant the difftest records compare is reachable from the
    /// rendered block, so the two tables cannot drift from the code.
    #[test]
    fn every_emitted_literal_is_one_the_block_can_print() {
        assert_eq!(LITERAL_COUNT, 15);
        let mut e = enemy();
        e.armor = 2;
        e.broken_jaw = true;
        e.broken_leg = true;
        e.agility = 120;
        let out = lines(&e).join("\n");
        for (_, lit) in EMITTED {
            if lit == crate::character_sheet::ACCURACY_FLAT
                || lit == crate::character_sheet::ACCURACY_SECOND
            {
                continue; // the other two accuracy branches
            }
            let stem: String = lit.chars().take_while(|c| *c != '#').collect();
            assert!(out.contains(&stem), "{lit:?} never appears in {out:?}");
        }
        for frag in [HEADER_OPEN, BROKEN_JAW, BROKEN_LEG, HEALTH] {
            let stem: String = frag.chars().take_while(|c| *c != '#').collect();
            assert!(out.contains(&stem), "{frag:?} never appears in {out:?}");
        }
    }

    /// The gap table has to index inside [`EMITTED`], or `difftest` compares
    /// a record no scan of the image can produce.
    #[test]
    fn every_gap_index_is_inside_the_emitted_table() {
        for (at, events) in GAPS {
            assert!(at <= EMITTED.len(), "gap {at} is past the table");
            assert!(!events.is_empty());
        }
    }
}
