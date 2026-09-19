//! Text for `run`'s own extra line, wander bucket 1's district lines, and
//! wander bucket 4's flavour turn -- `docs/re/port-gaps.md` rows 22, 12 and
//! 11, plus row 19's share of the `ReadKey`s inside them (row 24's phone
//! gag, which has no new text, is `crate::game::Game::wander_preamble`'s
//! own doc, not here).
//!
//! | block | original | what |
//! |---|---|---|
//! | [`RAN`] | `1000:aee4`..`aeff` | `run`'s own extra line |
//! | [`BUCKET1`] | `1000:b3db`..`b4e8` | bucket 1's eight district-keyed lines |
//! | [`BUCKET4`] / [`BUCKET4_FRAGMENTS`] | `1000:b82f`..`b94a` | bucket 4's flavour turn, plus the outer dispatch's mismatch arm ("bucket 0") that shares its address range |
//!
//! ## `run`'s extra line is inside the shared preamble, not the dispatch
//!
//! `1000:ae86` compares the just-read line against `w` and falls straight
//! into `1000:aea1`; `1000:ae97` does the same for `run`, via `goto
//! LAB_1000_aea1`. Both spellings reach the SAME code, and `1000:aee4`
//! re-reads the SAME line and compares it against `run` a SECOND time,
//! inside that shared body -- not as a second dispatch. `Game::run` answers
//! this the way the fight prompt's own `run` compare already does
//! (`Game::run_combat`'s doc, "combat's own `run` compare, ahead of
//! everything `parse` knows about"): by matching the raw line text rather
//! than `Command`, since `crate::commands::parse` folds `w` and `run` into
//! one `Command::Walk` and cannot tell them apart on its own.
//!
//! ## Bucket 0's line was mis-stated before this batch
//!
//! `Game::walk`'s doc used to say bucket 0 "ends the turn with nothing". It
//! does not: `1000:b92a`, the outer dispatch's mismatch arm (any
//! `20ae:3970` value outside 1..4, which is exactly what a church-cancelled
//! turn leaves it at), prints `BUCKET4[3]` -- the same `Ничё не
//! происходит.` text bucket 4 itself prints at `1000:b90f` on two of its
//! own paths. Both sites sit inside this module's `1000:b82f`..`b94a` span,
//! which is why [`BUCKET4`] carries the string twice: two separate CS
//! references in the image, the same reason [`crate::opening`]'s two
//! district-line groups are transcribed twice rather than shared.
//!
//! ## [`BUCKET4`] is kept in the image's ADDRESS order, not execution order
//!
//! Two mutually exclusive arms are interleaved on the page (the stoned
//! path's two lines sit either side of the composed line; the not-stoned
//! line sits after both), exactly the way [`crate::church::PARTING`] mixes
//! two arms of a branch into one array. `tools/difftest.py` reads the image
//! left to right, so the array has to match that order for the comparison
//! to mean anything -- `Game::wander_flavor` indexes it by name, not by
//! walking it in order.
//!
//! ## What's still not modelled
//!
//! `1000:b8e2`..`b8ec`'s `ReadLn` into `DS:3a72` is consumed
//! (`lines.next()`) but its text is never compared against anything
//! downstream in the original either -- it is a pure pacing read, the same
//! shape a `ReadKey` gets elsewhere in this port.

/// `1000:aee4`'s `run`-only compare; the line it prints at `1000:aeff`.
pub const RAN: &str = "^6Забегал мудак.";

/// `1000:b3db`..`b4e8` -- bucket 1's eight lines: districts 1..4 with the
/// flag `20ae:3693` freshly SET (index 0..3), then districts 1..4 with it
/// freshly CLEAR (index 4..7). There is no district-5 line in either half;
/// `crate::game::Game::walk_verb` does not invent one.
pub const BUCKET1: [&str; 8] = [
    // 1000:b3e5 cs 0x87e5
    "^6Ты зашел на тропинку где бродит искитимская гопота.",
    // 1000:b405 cs 0x881b
    "^6Ты зашел в какие-то дебри подваротен.",
    // 1000:b425 cs 0x8843
    "^6Ты зашел на планы.",
    // 1000:b445 cs 0x8858
    "^6Ты зашел чёрте куда.",
    // 1000:b46f cs 0x886f
    "^6Ты вышел с тропинки.",
    // 1000:b48f cs 0x8886
    "^6Ты вышел из подваротен.",
    // 1000:b4af cs 0x88a0
    "^6Ты вышел с планов.",
    // 1000:b4cf cs 0x88b5
    "^6Ты вышел чёрте откуда.",
];

/// `1000:b82f`..`b94a`, in the image's address order -- see the module doc.
pub const BUCKET4: [&str; 4] = [
    // 1000:b84d cs 0x8aed -- stoned, first Random(7) == 0.
    "Все куда-то плывёт - совсем башню заклинило",
    // 1000:b8f4 cs 0x8b19 -- stoned, second Random(7) == 0, printed after
    // the composed line and the ReadLn.
    "^6Чё за ботва? ни кого нет.. глюки какие-то...",
    // 1000:b90f cs 0x8b48 -- `LAB_1000_b90c`: not stoned, OR stoned with
    // the second Random(7) != 0.
    "Ничё не происходит.",
    // 1000:b92a cs 0x8b48 -- the SAME text, a second site: the outer
    // dispatch's mismatch arm ("bucket 0").
    "Ничё не происходит.",
];

/// `1000:b886`..`b8cf`'s composed line: `[0] + rank_name(roll) + [1]`,
/// filled with `district * 10 + 1` -- see
/// `crate::game::Game::wander_flavor`.
pub const BUCKET4_FRAGMENTS: [&str; 2] = [
    // 1000:b888 cs 0x8997
    "^6Идет ",
    // 1000:b8ac cs 0x899f
    " # уровня. Хочешь наехать?",
];
