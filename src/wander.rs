//! Text for `run`'s own extra line, wander bucket 1's district lines, and
//! wander bucket 4's flavour turn.
//!
//! | block | what |
//! |---|---|
//! | [`RAN`] | `run`'s own extra line |
//! | [`BUCKET1`] | bucket 1's eight district-keyed lines |
//! | [`BUCKET4`] / [`BUCKET4_FRAGMENTS`] | bucket 4's flavour turn, plus the outer dispatch's mismatch arm ("bucket 0") |
//!
//! ## `run`'s extra line is inside the shared preamble, not the dispatch
//!
//! Typing `w` or `run` both reach the same code, which re-reads the same
//! line and compares it against `run` a SECOND time inside that shared
//! body -- not as a second dispatch. `Game::run` answers this the way the
//! fight prompt's own `run` compare does: by matching the raw line text
//! rather than `Command`, since `crate::commands::parse` folds `w` and
//! `run` into one `Command::Walk` and cannot tell them apart on its own.
//!
//! ## Bucket 0 does not end the turn with nothing
//!
//! The outer dispatch's mismatch arm -- any value outside 1..4, which is
//! exactly what a church-cancelled turn leaves it at -- prints `Ничё не
//! происходит.`, the same text bucket 4 itself prints on two of its own
//! paths. That is why [`BUCKET4`] carries the string twice.
//!
//! ## [`BUCKET4`]'s entries are not in execution order
//!
//! Two mutually exclusive arms are interleaved in the array (the stoned
//! path's two lines sit either side of the composed line; the not-stoned
//! line sits after both), the same way [`crate::church::PARTING`] mixes two
//! arms of a branch into one array. `Game::wander_flavor` indexes it by
//! name, not by walking it in order.
//!
//! ## What's still not modelled
//!
//! One `ReadLn` here is consumed but its text is never compared against
//! anything downstream either -- it is a pure pacing read, the same shape
//! a `ReadKey` gets elsewhere in this port.

/// The `run`-only compare's own line.
pub const RAN: &str = "^6Забегал мудак.";

/// Bucket 1's eight lines: districts 1..4 with the flag freshly SET (index
/// 0..3), then districts 1..4 with it freshly CLEAR (index 4..7). There is
/// no district-5 line in either half; `crate::game::Game::walk_verb` does
/// not invent one.
pub const BUCKET1: [&str; 8] = [
    "^6Ты зашел на тропинку где бродит искитимская гопота.",
    "^6Ты зашел в какие-то дебри подваротен.",
    "^6Ты зашел на планы.",
    "^6Ты зашел чёрте куда.",
    "^6Ты вышел с тропинки.",
    "^6Ты вышел из подваротен.",
    "^6Ты вышел с планов.",
    "^6Ты вышел чёрте откуда.",
];

pub const BUCKET4: [&str; 4] = [
    // Stoned, first Random(7) == 0.
    "Все куда-то плывёт - совсем башню заклинило",
    // Stoned, second Random(7) == 0, printed after the composed line and
    // the ReadLn.
    "^6Чё за ботва? ни кого нет.. глюки какие-то...",
    // Not stoned, OR stoned with the second Random(7) != 0.
    "Ничё не происходит.",
    // The same text again, for the outer dispatch's mismatch arm
    // ("bucket 0").
    "Ничё не происходит.",
];

/// The composed line `[0] + rank_name(roll) + [1]`, filled with
/// `district * 10 + 1` -- see `crate::game::Game::wander_flavor`.
pub const BUCKET4_FRAGMENTS: [&str; 2] = ["^6Идет ", " # уровня. Хочешь наехать?"];
