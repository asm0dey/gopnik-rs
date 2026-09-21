//! The church's text.
//!
//! A single routine reached from the wander draw. The control flow was already
//! ported; what was missing was the text and every `ReadKey`.
//!
//! | block | what |
//! |---|---|
//! | [`SERMON_2`] | the third-and-later visit's four lines |
//! | [`SERMON_1`] | the second visit's seven lines |
//! | [`SERMON_0`] / [`SERMON_0_FRAGMENTS`] | the first visit: ten lines and one composed line |
//! | [`FORCED_LEVEL`] / [`FORCED_LEVEL_FRAGMENTS`] | before forcing a level: the text and its segments |
//! | [`PARTING`] | the exit choice, two parting lines, and the closing one |
//!
//! ## The composed lines
//!
//! Two of these lines are assembled out of literals AND player-data
//! (player name, rank, krutizna). The literal halves are [`SERMON_0_FRAGMENTS`] and
//! [`FORCED_LEVEL_FRAGMENTS`]; the data halves are the player's name,
//! [`crate::data::rank_name`] and [`crate::data::krutizna`].
//!
//! ## `PARTING` is a straight list of a branch's two arms
//!
//! One branch picks `PARTING[0]` or `PARTING[1]` and never both.

use crate::opening::{play, Gaps};
use crate::{data, model::Fighter};
use std::io;

/// The arm taken once the visit counter has saturated at 2.
pub const SERMON_2: [&str; 4] = [
    // Game string.
    "Бродя по окрестностям с самыми грязными намериниями...",
    // Game string.
    "Ты наткнулся на храм Божий.",
    // Game string.
    "^1Бог: \"А ты опять.\"",
    // Game string.
    "^1Ну ладно насылаю на тебя \"благославление\"",
];

/// One `ReadKey` after the second line and one after the last.
pub const SERMON_2_GAPS: Gaps = &[(2, "K"), (4, "K")];

/// The second visit. Its first two lines are the same two literals, referenced
/// again rather than copied, so they are transcribed twice for the same reason
/// [`crate::opening::ADVANCE_ARRIVAL`] repeats [`crate::opening::START_ARRIVAL`]'s
/// wording: the port must not fold two sites into one constant.
pub const SERMON_1: [&str; 7] = [
    // Game string.
    "Бродя по окрестностям с самыми грязными намериниями...",
    // Game string.
    "Ты наткнулся на храм Божий.",
    // Game string.
    "В прошлый раз Бог сказал чтобы ты не осквернял своей рожей святой храм",
    // Game string.
    "Но надо бы типа помолиться о прощении",
    // Game string.
    "^2\"Господи, Братан, прости грешника опять\".",
    // Game string.
    "^1Бог: \"Да блин, упорный чудак!\"",
    // Game string.
    "^1Ладно насылаю на тебя, типа, \"моё благославление\" снова",
];

/// One `ReadKey` after every line.
pub const SERMON_1_GAPS: Gaps = &[
    (1, "K"),
    (2, "K"),
    (3, "K"),
    (4, "K"),
    (5, "K"),
    (6, "K"),
    (7, "K"),
];

/// The first visit, the longest block. Ten CS literals; the eleventh line is composed.
pub const SERMON_0: [&str; 10] = [
    // Game string.
    "Бродя по окрестностям с самыми грязными намериниями...",
    // Game string.
    "Ты наткнулся на храм Божий.",
    // Game string.
    "Раз такая батва...",
    // Game string (trailing spaces are in the image).
    "Надо типа помолиться Господу Богу...              ",
    // Game string.
    "Как делают новыё русские.",
    // Game string.
    "^2\"Господи, Братан, прости грешника\". - Начал было ты...",
    // Game string.
    "^1Громовой голос: \"Да пошел ты на хрен!\"",
    // Game string.
    "^2 - Эээ.. типа.. а чё?..",
    // Game string.
    "^2Ну..",
    // Game string.
    "^1Ладна насылаю на тебя, типа, \"моё благославление\"",
];

/// One `ReadKey` after every line, plus the composed line at its own `ReadKey`.
pub const SERMON_0_GAPS: Gaps = &[
    (1, "K"),
    (2, "K"),
    (3, "K"),
    (4, "K"),
    (5, "K"),
    (6, "K"),
    (7, "K"),
    (8, "KCK"),
    (9, "K"),
    (10, "K"),
];

/// The CS literals of the first visit's composed line, in address order.
///
/// The whole line is `[0] + name + [1] + rank + [2]`.
pub const SERMON_0_FRAGMENTS: [&str; 3] = ["^1 - \"а чё?\" блин! ", "^1-", " чёртов!"];

/// The zero arm before it forces the level.
pub const FORCED_LEVEL: [&str; 1] = [
    // Game string.
    "^1Да увеличится твоя понтовость!",
];

/// One `ReadKey` and one composed line. The composed line has no `ReadKey` after it.
pub const FORCED_LEVEL_GAPS: Gaps = &[(1, "KC")];

/// The text of a composed line.
///
/// The whole line is `[0] + krutizna(level) + [1] + krutizna(level + 1)`.
/// Both lookups read the крутизна ladder, [`crate::data::krutizna`], NOT the
/// rank table.
///
/// Both reads happen BEFORE the level-up call, so the level they see is the
/// OLD level and the "а стал" half is `level + 1` -- not the level the
/// call ends up granting, which at the cap does not move.
pub const FORCED_LEVEL_FRAGMENTS: [&str; 2] = ["^1Был ты ", " а стал "];

/// The convergence point: everything after draw 15's chain converges.
///
/// `[0]` and `[1]` are the two arms of one branch and only one of them is
/// printed; `[2]` is the line that closes the routine.
pub const PARTING: [&str; 3] = [
    // The visit count < 2 arm.
    "^1А теперь вали отсюда и никогда здесь не появляйся!",
    // The other branch.
    "^1А теперь проваливай!",
    // Game string.
    "Ты идещь дальше...",
];

/// One `ReadKey` before the branch, and one bare `WriteLn` after it.
/// The blank line is emitted once, after whichever arm ran.
pub const PARTING_GAPS: Gaps = &[(0, "K"), (2, "B")];

/// The first visit's composed line, built.
pub fn sermon_0_composed(player: &Fighter) -> String {
    format!(
        "{}{}{}{}{}",
        SERMON_0_FRAGMENTS[0],
        player.name,
        SERMON_0_FRAGMENTS[1],
        data::rank_name(player.class),
        SERMON_0_FRAGMENTS[2],
    )
}

/// The forced level-up's composed line, built from the level BEFORE the level is granted.
pub fn forced_level_composed(level: u16) -> String {
    format!(
        "{}{}{}{}",
        FORCED_LEVEL_FRAGMENTS[0],
        data::krutizna(level),
        FORCED_LEVEL_FRAGMENTS[1],
        data::krutizna(level + 1),
    )
}

/// Play one sermon: its lines, its `ReadKey`s and -- for the first visit --
/// its composed line, wherever [`Gaps`] says the `'C'` falls.
pub fn sermon(lines: &mut dyn Iterator<Item = io::Result<String>>, stage: u8, player: &Fighter) {
    match stage {
        // First visit.
        0 => play(
            lines,
            &SERMON_0,
            SERMON_0_GAPS,
            Some(&sermon_0_composed(player)),
        ),
        // Second visit.
        1 => play(lines, &SERMON_1, SERMON_1_GAPS, None),
        // Third and later visits.
        2 => play(lines, &SERMON_2, SERMON_2_GAPS, None),
        // All three compares are `==`, so a stage above 2 prints nothing
        // and reads no key. The file can have any byte, so falling through to
        // the third arm here would be a divergence.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Fighter {
        Fighter {
            name: "Вася".to_string(),
            class: 5,
            ..Fighter::default()
        }
    }

    /// The composed line interpolates, so a port that printed a constant
    /// would pass every `difftest` record. Two different classes must give
    /// two different lines.
    #[test]
    fn the_first_sermon_names_the_player_and_the_class() {
        let mut p = player();
        let a = sermon_0_composed(&p);
        p.class = 6;
        let b = sermon_0_composed(&p);
        assert_ne!(a, b, "the rank is interpolated, not baked in");
        assert!(a.contains("Вася"), "{a:?} must carry the name");
        assert!(a.contains(data::rank_name(5)), "{a:?} must carry the rank");
        assert!(a.ends_with(SERMON_0_FRAGMENTS[2]));
    }

    /// The second half is the NEXT rung of the ladder, and the two must differ.
    #[test]
    fn the_forced_level_line_names_this_rung_and_the_next() {
        let line = forced_level_composed(3);
        assert!(line.contains(data::krutizna(3)));
        assert!(line.ends_with(data::krutizna(4)));
        assert_ne!(data::krutizna(3), data::krutizna(4));
        // The krutizna ladder, not the rank table: `data::rank_name(3)` is
        // Подтсан and has no business here.
        assert_ne!(data::krutizna(3), data::rank_name(3));
    }

    /// Every gap index must be reachable: `play` walks `0..=len`, so an
    /// index past that is a table the port silently ignores.
    #[test]
    fn every_gap_index_is_inside_its_block() {
        for (n, gaps) in [
            (SERMON_2.len(), SERMON_2_GAPS),
            (SERMON_1.len(), SERMON_1_GAPS),
            (SERMON_0.len(), SERMON_0_GAPS),
            (FORCED_LEVEL.len(), FORCED_LEVEL_GAPS),
            (PARTING.len(), PARTING_GAPS),
        ] {
            for (at, events) in gaps {
                assert!(*at <= n, "gap {at} is past the block's {n} lines");
                assert!(
                    events.chars().all(|c| matches!(c, 'B' | 'K' | 'C')),
                    "gap {at} holds {events:?}"
                );
            }
        }
    }

    /// The first sermon eats eleven keystrokes: ten lines plus the composed
    /// one. A count is what the wander tests' stdin budget depends on.
    #[test]
    fn the_first_sermon_eats_eleven_keystrokes() {
        let n: usize = SERMON_0_GAPS
            .iter()
            .map(|(_, e)| e.chars().filter(|c| *c == 'K').count())
            .sum();
        assert_eq!(n, 11);
    }
}
