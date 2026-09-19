//! The church's text -- `docs/re/port-gaps.md` rows 3, 18 and 20, plus row
//! 19's share of the `ReadKey`s inside them.
//!
//! `FUN_1000_7c67` (`1000:7c67`..`1000:82af`) is one procedure with one
//! caller (`1000:b3a7`, wander draw 13). Its control flow was already ported
//! in [`crate::game::Game::church`]; what was missing was almost all of its
//! **text** and every one of its `ReadKey`s.
//!
//! | block | original | what |
//! |---|---|---|
//! | [`SERMON_2`] | `1000:7c76`..`7ceb` | the third-and-later visit's four lines |
//! | [`SERMON_1`] | `1000:7ceb`..`7dcb` | the second visit's seven lines |
//! | [`SERMON_0`] / [`SERMON_0_FRAGMENTS`] | `1000:7dcb`..`7f63` | the first visit: ten lines and one composed line |
//! | [`FORCED_LEVEL`] / [`FORCED_LEVEL_FRAGMENTS`] | `1000:7f68`..`7fe4` | draw 15's zero arm, before it forces a level |
//! | [`PARTING`] | `1000:823d`..`82b2` | the convergence `ReadKey`, the two parting lines and the closing one |
//!
//! ## The composed lines are a third gap event
//!
//! Two of these lines are not CS literals at all: they are assembled on a
//! stack local out of literals AND of DGROUP strings (`1000:7ed0`..`7f15`
//! and `1000:7f99`..`7fdf`, `rtl_str_assign` + four/two `rtl_str_append`,
//! then a `WriteLn` of `ss:[bp-0x100]` that carries no CS literal of its
//! own). A [`Gaps`] table records one as a `'C'` event, so
//! `tools/difftest.py` compares WHERE the composed line falls among the
//! plain ones instead of the port asserting it. The literal halves
//! are [`SERMON_0_FRAGMENTS`] and [`FORCED_LEVEL_FRAGMENTS`]; the DGROUP
//! halves are the player's name (`DS:379c`), [`crate::data::rank_name`]
//! (`DS:002e`, stride 0x100, indexed by the class at `DS:389c`) and
//! [`crate::data::krutizna`] (`DS:0b42`, stride 0x100, indexed by the level
//! at `DS:38a6`) -- so they cannot be literals and are interpolated, exactly
//! the way `help`'s two composed lines already are.
//!
//! ## `PARTING` is a straight list of a branch's two arms
//!
//! `1000:8247`'s `cmp byte [0x3951],0x2` picks `PARTING[0]` or `PARTING[1]`
//! and never both, so this array is NOT played through [`play`] -- it is a
//! table [`crate::game::Game::church`] indexes. [`PARTING_GAPS`] still
//! describes the image's straight-line layout, because that is what
//! `difftest.py` reads out of it: the bare `WriteLn` at `1000:828c` sits
//! after whichever arm ran and before `PARTING[2]`, which in the image is
//! between the second arm's literal and the last one's.

use crate::opening::{play, Gaps};
use crate::{data, model::Fighter};
use std::io;

/// `1000:7c76`..`7ceb` -- the arm taken once `20ae:3951` has saturated at 2.
/// The only arm that was already ported before this batch; its two
/// `ReadKey`s were not.
pub const SERMON_2: [&str; 4] = [
    // 1000:7c91 cs 0x777c
    "Бродя по окрестностям с самыми грязными намериниями...",
    // 1000:7caa cs 0x77b3
    "Ты наткнулся на храм Божий.",
    // 1000:7cc8 cs 0x77cf
    "^1Бог: \"А ты опять.\"",
    // 1000:7ce1 cs 0x77e4
    "^1Ну ладно насылаю на тебя \"благославление\"",
];

/// `1000:7caf` and `1000:7ce6` -- one `ReadKey` after the second line and
/// one after the last. See [`Gaps`].
pub const SERMON_2_GAPS: Gaps = &[(2, "K"), (4, "K")];

/// `1000:7ceb`..`7dcb` -- the second visit. Its first two lines are the same
/// two CS literals [`SERMON_2`] opens with (`0x777c`, `0x77b3`), referenced
/// again rather than copied, so they are transcribed twice here for the same
/// reason [`crate::opening::ADVANCE_ARRIVAL`] repeats
/// [`crate::opening::START_ARRIVAL`]'s wording: the port must not fold two
/// sites into one constant `difftest.py` then compares twice.
pub const SERMON_1: [&str; 7] = [
    // 1000:7d09 cs 0x777c
    "Бродя по окрестностям с самыми грязными намериниями...",
    // 1000:7d27 cs 0x77b3
    "Ты наткнулся на храм Божий.",
    // 1000:7d45 cs 0x7810
    "В прошлый раз Бог сказал чтобы ты не осквернял своей рожей святой храм",
    // 1000:7d63 cs 0x7857
    "Но надо бы типа помолиться о прощении",
    // 1000:7d81 cs 0x787d
    "^2\"Господи, Братан, прости грешника опять\".",
    // 1000:7d9f cs 0x78a9
    "^1Бог: \"Да блин, упорный чудак!\"",
    // 1000:7dbd cs 0x78ca
    "^1Ладно насылаю на тебя, типа, \"моё благославление\" снова",
];

/// `1000:7d0e`, `7d2c`, `7d4a`, `7d68`, `7d86`, `7da4`, `7dc2` -- a
/// `ReadKey` after every line, including the last.
pub const SERMON_1_GAPS: Gaps = &[
    (1, "K"),
    (2, "K"),
    (3, "K"),
    (4, "K"),
    (5, "K"),
    (6, "K"),
    (7, "K"),
];

/// `1000:7dcb`..`7f63` -- the first visit, the longest block in the routine.
/// Ten CS literals; the eleventh line, between index 7 and index 8, is
/// composed (see [`SERMON_0_FRAGMENTS`] and [`SERMON_0_GAPS`]'s `'C'`).
pub const SERMON_0: [&str; 10] = [
    // 1000:7de9 cs 0x777c
    "Бродя по окрестностям с самыми грязными намериниями...",
    // 1000:7e07 cs 0x77b3
    "Ты наткнулся на храм Божий.",
    // 1000:7e25 cs 0x7904
    "Раз такая батва...",
    // 1000:7e43 cs 0x7917 -- the trailing spaces are in the image.
    "Надо типа помолиться Господу Богу...              ",
    // 1000:7e61 cs 0x794a
    "Как делают новыё русские.",
    // 1000:7e7f cs 0x7964
    "^2\"Господи, Братан, прости грешника\". - Начал было ты...",
    // 1000:7e9d cs 0x799d
    "^1Громовой голос: \"Да пошел ты на хрен!\"",
    // 1000:7ebb cs 0x79c6
    "^2 - Эээ.. типа.. а чё?..",
    // 1000:7f33 cs 0x7a01
    "^2Ну..",
    // 1000:7f51 cs 0x7a08
    "^1Ладна насылаю на тебя, типа, \"моё благославление\"",
];

/// A `ReadKey` after every one of the ten lines (`1000:7dee` ..
/// `1000:7f56`), plus the composed line at `1000:7ec5` and its own `ReadKey`
/// at `1000:7f1a` -- which is why gap 8 is `"KCK"` and not `"K"`.
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

/// The CS literals of `1000:7ed0`..`7f01`'s composed line, in address order.
///
/// The whole line is `[0] + name + [1] + rank + [2]`: the name is appended
/// from `DS:379c` (`1000:7eda`, `push ds`) and the rank from
/// `DS:([0x389c] * 0x100 + 0x2e)` (`1000:7ee9`..`7ef7`), so neither is a
/// literal and neither appears here. [`sermon_0_composed`] joins them.
pub const SERMON_0_FRAGMENTS: [&str; 3] = ["^1 - \"а чё?\" блин! ", "^1-", " чёртов!"];

/// `1000:7f68`..`7fe4` -- draw 15's zero arm, up to (but not including) the
/// `xp := threshold` / `call 0x2526` that forces the level.
pub const FORCED_LEVEL: [&str; 1] = [
    // 1000:7f84 cs 0x7a3c
    "^1Да увеличится твоя понтовость!",
];

/// `1000:7f89`'s `ReadKey` and `1000:7f8e`'s composed line, in that order.
/// The composed line has **no** `ReadKey` after it: `1000:7fe4` is the
/// `mov ax,[0x38d0]` that starts the forced level-up.
pub const FORCED_LEVEL_GAPS: Gaps = &[(1, "KC")];

/// The CS literals of `1000:7f99`..`7fb6`'s composed line.
///
/// The whole line is `[0] + krutizna(level) + [1] + krutizna(level + 1)`.
/// Both appends read `DS:([0x38a6] * 0x100 + 0xb42)` (`1000:7f9e`..`7fac`
/// and `1000:7fbb`..`7fcb`, the second on `level + 1`) -- the крутизна
/// ladder, [`crate::data::krutizna`], NOT the eleven-row rank table
/// `1000:7ee9` uses. `docs/re/port-gaps.md` records dispatch-2's gap 6
/// calling `DS:0b42` the "rank" table as a defect for exactly this reason.
///
/// Both reads happen BEFORE `1000:7fea`'s `call 0x2526`, so the level they
/// see is the OLD one and the "а стал" half is `level + 1` -- not the level
/// the call ends up granting, which at the cap (`1000:2580`) does not move
/// at all.
pub const FORCED_LEVEL_FRAGMENTS: [&str; 2] = ["^1Был ты ", " а стал "];

/// `1000:823d`..`82b2` -- everything after the draw-15 chain converges.
///
/// `[0]` and `[1]` are the two arms of `1000:8247`'s branch and only one of
/// them is ever printed; `[2]` is the line that closes the routine. The span
/// starts at `1000:823d`, the `call 0eed:01c2` that ends draw 15's last arm,
/// so the convergence `ReadKey` at `1000:8242` falls strictly INSIDE the
/// first gap rather than exactly on the span's lower bound.
pub const PARTING: [&str; 3] = [
    // 1000:8262 cs 0x7c08 -- the `[0x3951] < 2` arm.
    "^1А теперь вали отсюда и никогда здесь не появляйся!",
    // 1000:827d cs 0x7c3d -- the other one.
    "^1А теперь проваливай!",
    // 1000:82aa cs 0x7c54
    "Ты идещь дальше...",
];

/// `1000:8242`'s `ReadKey` before the branch, and `1000:828c`'s bare
/// `WriteLn` after it. The blank line is emitted once, after whichever arm
/// ran, which in the image's straight-line layout puts it between
/// `PARTING[1]` and `PARTING[2]`.
pub const PARTING_GAPS: Gaps = &[(0, "K"), (2, "B")];

/// `1000:7ed0`..`7f01` -- the first visit's composed line, built.
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

/// `1000:7f99`..`7fb6` -- the forced level-up's composed line, built from the
/// level as it stands BEFORE the level is granted.
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
        // 1000:7dcb `cmp byte [0x3951],0x0`.
        0 => play(
            lines,
            &SERMON_0,
            SERMON_0_GAPS,
            Some(&sermon_0_composed(player)),
        ),
        // 1000:7ceb `cmp byte [0x3951],0x1`.
        1 => play(lines, &SERMON_1, SERMON_1_GAPS, None),
        // 1000:7c76 `cmp byte [0x3951],0x2`.
        2 => play(lines, &SERMON_2, SERMON_2_GAPS, None),
        // All three compares are `==`, so a stage above 2 prints nothing
        // and reads no key. `20ae:3951` cannot reach that in play -- only
        // `1000:7dc7` and `1000:7f5b` write it, each from a `== 1` / `== 0`
        // arm -- but `Save::parse` takes the byte from the file, so a
        // hand-edited save can. Falling through to the `== 2` arm here
        // would be a divergence with nothing behind it.
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

    /// `1000:7fbb`'s `inc ax` -- the second half is the NEXT rung of the
    /// ladder, and the two halves must differ.
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
