use crate::character_sheet::{accuracy_block, health_digit, Out};
use crate::data;
use crate::model::Fighter;
use crate::text;

pub const HEADER_OPEN: &str = "^2Это ";
pub const HEADER_LEVEL: &str = " # уровня";
pub const NOT_IN_THIS_LIFE: &str = "Не в этой жизни.";
/// A separator string that appears between the header and крутизна.
pub const KRUTIZNA_SEP: &str = " - ";
pub const STATS: &str = "Сл:# Лв:# Жв:# Уд:#";
pub const DAMAGE: &str = "Урон #-#";
pub const BROKEN_JAW: &str = "^4Сломана челюсть  ";
pub const BROKEN_LEG: &str = "^4Сломана нога  ";
pub const COLOUR_PREFIX: &str = "^";
pub const HEALTH: &str = "Здоровье #/#  ";
pub const ARMOUR: &str = "^2Броня #    ";

/// Eight literal strings the sheet assembles into its two composed lines.
/// Indices 0..3 build the header, 4..7 the health line.
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

pub const EMITTED: [(bool, &str); 7] = [
    (true, STATS),                                    // 1000:1419
    (true, DAMAGE),                                   // 1000:1436
    (true, crate::character_sheet::ACCURACY_FLAT),    // 1000:157b
    (false, crate::character_sheet::ACCURACY_CAPPED), // 1000:15a4
    (true, crate::character_sheet::ACCURACY_SECOND),  // 1000:15e7
    (true, crate::character_sheet::ACCURACY_MANY),    // 1000:1611
    (true, ARMOUR),                                   // 1000:163f
];

pub const GAPS: [(usize, &str); 2] = [(0, "C"), (2, "C")];

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

/// The composed header computation.
///
/// If level > 40, use a fallback. Otherwise append крутизна[level] to the
/// rank name. If class >= 8, omit the крутизна.
///
/// The name indexes the rank table by CLASS word; the enemy record has no
/// name field. Levels 41 and 42 take the fallback although a row exists.
fn header(o: &mut Out, e: &Fighter) {
    let suffix = if e.class > 7 {
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

fn armour_line(o: &mut Out, e: &Fighter) {
    if e.armor != 0 {
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
