//! The end screen and the victory marquee -- `FUN_1000_074b`
//! (`1000:074b`..`1000:0aca`) and `FUN_1000_0aec`
//! (`1000:0aec`..`1000:0d13`).
//!
//! Every path that ends the game reaches [`end_screen`]: the two deaths in
//! `FUN_1000_3d11` (`1000:4fb4` and `1000:5074`, both `FUN_1000_074b(0)`)
//! and the victory, which gets there the long way -- `1000:5133 call 0xaec`
//! runs [`marquee`] first and `1000:0d0a` calls `FUN_1000_074b(1)` from
//! inside it. `src/game.rs` used to say `1000:5133` *was* `074b(1)`; it is
//! not, and the marquee is the difference.
//!
//! ## Halting is the caller's job
//!
//! `1000:0ac0 call 0f78:0116` is `Halt(0)` -- the RTL restores the interrupt
//! vectors and ends the process, so the `mov sp,bp` / `pop bp` / `ret 2`
//! epilogue at `1000:0ac5` is unreachable. This port has no process-level
//! halt inside the game loop; the convention already used for the `e` verb
//! and for the plain death is `self.running = false`, so these two functions
//! print and return and [`crate::game::Game`] clears `running`.
//!
//! ## `FUN_1000_0acc` is data, not a function
//!
//! The eleven Pascal shortstrings at `1000:0acb`..`1000:0aeb` -- `^`, `Т^`,
//! `Ы ^`, `С^`, `У^`, `П^`, `Е^`, `Р ^`, `Г^`, `О^`, `П` -- are the marquee's
//! literal pool, and `FUN_1000_0aec` references exactly those eleven offsets
//! and no others. Ghidra's `FUN_1000_0acc` is its 16-bit-wrap phantom over
//! them. Interleaved with the rotating digit they spell `ТЫ СУПЕР ГОП`.

use crate::term;

/// `1000:3e8d`'s arm -- the market pickpocket's opener, CS `0x2cfa`,
/// printed at `1000:3ea5`. One line and no `ReadKey`.
pub const OPENER_1: [&str; 1] = ["^4Отдай кошелёк урод!"];

/// `1000:3ead`'s arm -- the first rector fight. CS `0x2d10`, `0x2d31`,
/// `0x2d46`, `0x2d5c`, printed at `1000:3ec5`, `3ee3`, `3f01`, `3f1f`, each
/// followed by a `ReadKey` (`1000:3eca`, `3ee8`, `3f06`, `3f24`).
pub const OPENER_3: [&str; 4] = [
    "^2Ну вот мы и встретились мудак!",
    "^4Чё те нада козёл?!",
    "^2Урыть тебя ублюдок!",
    "^4Ну ты меня достал ща урою!",
];

/// `1000:3f2b`'s arm -- the second. CS `0x2d79`, `0x2d99`, `0x2dc5`,
/// `0x2dda`; `ReadKey`s at `1000:3f48`, `3f66`, `3f84`, `3fa2`.
pub const OPENER_4: [&str; 4] = [
    "^6Тут заходит настоящий ректор.",
    "^4Мудак! ты тупой дебил, думал что я идиот?",
    "^2Я думаю ты сконил!",
    "^4Ну тада сдохни!",
];

/// `1000:5085`'s arm -- the victory ending's five lines. CS `0x3862`,
/// `0x3894`, `0x38bd`, `0x38f6`, `0x3915`. The first four carry a `ReadKey`
/// (`1000:50b3`, `50d1`, `50ef`, `510d`); the fifth is followed by the
/// character sheet instead.
pub const ENDING_4: [&str; 5] = [
    "^1Ты замочил самого ректора!!! ТЫ САМЫЙ КРУТОЙ!!!",
    "^1Вновь сила торжествует над интелектом.",
    "^1После этого сразу началась анархия и полный беспредел.",
    "^1И не стыдно тебе гоп чёртов?",
    "^1А результат:",
];

/// `1000:5139`'s arm -- the fake-out. CS `0x3924` and `0x3965`, `ReadKey`s
/// at `1000:5164` and `5182`.
pub const ENDING_3: [&str; 2] = [
    "^1Ты замочил самого ректора!!!^6 о чёрт! да это ж не ректор был.",
    "^6Это был проректор СУНЦа!",
];

/// `1000:57ce`'s two reward lines -- CS `0x3c99` filled with `district * 20`
/// (`1000:57e7`) and CS `0x3ce9` filled with `district * 10` (`1000:5808`).
pub const ERRAND_AWARDS: [(i32, &str); 2] = [
    (
        20,
        "^2Ты отпинал этого мудака - пацаны этого незабудут. Понтовость улутшилась на #.",
    ),
    (10, "^6Ты получаешь # качков опыта за помощь"),
];

/// The eight rows of the block-drawing banner, CS `0x052d`, `0x056a`,
/// `0x05a7`, `0x05e4`, `0x0621`, `0x065e`, `0x069b`, `0x06d8` -- each a
/// 60-byte shortstring, printed at `1000:0831`, `0873`, `08b5`, `08f7`,
/// `0939`, `097b`, `09bd`, `09ff`.
///
/// Each row is written as `CS 0x0521` (ten spaces and a bare `^`) plus the
/// verdict colour digit plus the row itself, assembled at
/// `1000:07ff`..`1000:081d` and the seven repeats of that shape below it.
pub const BANNER: [&str; 8] = [
    "│    │ ┌────┐ ┌────┐  ┌────┐     │    │ ┌────┐ │    │ ┌────┐",
    "│    │ │      │    │  │          │    │ │    │ │    │ │    │",
    "│    │ │      │    │  │           \\  /  │    │ │    │ │    │",
    "│   /│ │      │    │  │            \\/   │    │ │    │ │    │",
    "│  / │ │      ├────┘  ├────        /\\   │    │ ├────┤ │    │",
    "│ /  │ │      │       │           /  \\  ├────┤ │    │ ├────┤",
    "│/   │ │      │       │          │    │ │    │ │    │ │    │",
    "│    │ │      │       └────┘     │    │ │    │ │    │ │    │",
];

/// The banner's indent-and-colour prefix, CS `0x0521`, without its colour
/// digit -- that is `1000:0765`'s `0x34` or `1000:076b`'s `0x32`.
pub const BANNER_INDENT: &str = "          ^";

/// CS `0x04be`, printed at `1000:07a7` when `param_1 == 0`.
pub const DEATH_LINE: &str = "                                      ^4Ты сдох.";

/// CS `0x04ef`, printed at `1000:07c2` when `param_1 != 0`.
pub const VICTORY_LINE: &str = "                                    ^2Ты победил.";

/// CS `0x0715`, printed at `1000:0a63`.
pub const ANY_KEY: &str = "                          ^6Нажми какую-нибудь кнопку";

/// `FUN_1000_074b` -- the end screen both endings reach.
///
/// `victory` is `param_1 != 0`. It picks the verdict line (`1000:078d`) and
/// the colour digit the eight banner rows carry (`1000:075f`), and nothing
/// else in the function reads it.
///
/// `1000:0aa4` `TextColor(0)` and `1000:0ab1` `TextColor(15)` are dropped:
/// this port carries no persistent text attribute (`crate::term` renders the
/// `^N` markup of one line and resets), so the pair has no state to move.
/// `1000:075a` and `1000:0ab9` `ClrScr` are dropped for the same reason --
/// there is no screen to clear in a line-based port. The `1000:0aac`
/// `ReadKey` between them is NOT dropped: it consumes a line, the same
/// stand-in `Game::enter_district_5` and `persist::choose_slot` use.
pub fn end_screen(victory: bool, lines: &mut dyn Iterator<Item = std::io::Result<String>>) {
    // 1000:0774 and 1000:0783 -- two bare WriteLns before the verdict.
    term::println("");
    term::println("");
    // 1000:07a7 / 1000:07c2.
    term::println(if victory { VICTORY_LINE } else { DEATH_LINE });
    // 1000:07cc, 07db, 07ea.
    for _ in 0..3 {
        term::println("");
    }
    // 1000:0765 `mov ah,0x34` / 1000:076b `mov ah,0x32` -- '4' on death,
    // '2' on victory.
    let digit = if victory { '2' } else { '4' };
    for row in BANNER {
        term::println(&format!("{BANNER_INDENT}{digit}{row}"));
    }
    // 1000:0a09, 0a18, 0a27, 0a36, 0a45.
    for _ in 0..5 {
        term::println("");
    }
    term::println(ANY_KEY);
    // 1000:0a6d, 0a7c, 0a8b, 0a9a.
    for _ in 0..4 {
        term::println("");
    }
    // 1000:0aac -- ReadKey, value discarded.
    let _ = lines.next();
}

/// `1000:0b8f cmp byte [bp-0xb],0x20` -- the marquee's indent, written one
/// space at a time by `1000:0b80` with no newline behind it.
pub const MARQUEE_INDENT: usize = 32;

/// The number of distinct phases `1000:0ce5`..`1000:0cf0` cycles the digit
/// string through: `if e < 8 then e := e + 1 else e := 0`, so `e` takes 0..8
/// before wrapping. `e = 8` and `e = 0` draw the same frame, because the
/// digit is `(i + e - 1) mod 8`.
pub const MARQUEE_PHASES: u16 = 9;

/// One frame of the marquee: the eleven literals of the `FUN_1000_0acc` pool
/// with the ten rotating digits interleaved, assembled at
/// `1000:0ba0`..`1000:0ccc` and written at `1000:0ce0`.
///
/// `1000:0b36`..`1000:0b5c` is the Pascal `for i := 1 to 10 do
/// s[i] := chr((i + e - 1) mod 8 + 48)` that fills the digit string; the
/// `1000:0b07` loop above it is the same thing with `e` implicitly 0 and is
/// overwritten before it is read.
pub fn marquee_frame(phase: u16) -> String {
    let d = |i: u16| char::from(b'0' + ((i + phase - 1) % 8) as u8);
    format!(
        "^{}Т^{}Ы ^{}С^{}У^{}П^{}Е^{}Р ^{}Г^{}О^{}П",
        d(1),
        d(2),
        d(3),
        d(4),
        d(5),
        d(6),
        d(7),
        d(8),
        d(9),
        d(10),
    )
}

/// `FUN_1000_0aec` -- the victory marquee, then the end screen.
///
/// **The loop bound is a port decision.** The original is
/// `repeat ... until KeyPressed` (`1000:0cf4 call 0xf16:0x308`), redrawing
/// one frame per pass behind `1000:0b62 Delay(5000)` and `1000:0b67 ClrScr`.
/// This port reads whole lines and has no non-blocking key poll, so a
/// faithful `KeyPressed` is not available: it draws exactly one full phase
/// cycle ([`MARQUEE_PHASES`] frames) and stops. `Delay` and `ClrScr` are
/// dropped with the rest of the screen control, so the frames scroll rather
/// than replace each other. Recorded in `docs/re/gaps.md`.
///
/// `1000:0b6c`..`1000:0b93` writes 32 spaces with no newline before each
/// frame -- `rtl_text_write_char(Output, ' ')` thirty-two times, the loop
/// bound being `1000:0b8f cmp byte [bp-0xb],0x20`. It is the marquee's
/// indent, so it is kept.
pub fn marquee(lines: &mut dyn Iterator<Item = std::io::Result<String>>) {
    for phase in 0..MARQUEE_PHASES {
        // 1000:0b6c..0b93 -- the 32-space indent, then the frame.
        term::print(&" ".repeat(MARQUEE_INDENT));
        term::println(&marquee_frame(phase));
    }
    // 1000:0d00 and 1000:0d05 -- two ReadKeys.
    let _ = lines.next();
    let _ = lines.next();
    // 1000:0d0a -- FUN_1000_074b(1).
    end_screen(true, lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_lines() -> std::vec::IntoIter<std::io::Result<String>> {
        Vec::new().into_iter()
    }

    /// Phase 0 is the string `1000:0b07`'s prologue loop builds, and the
    /// digits are `(i - 1) mod 8` for `i` in 1..=10 -- so the ninth and
    /// tenth wrap back to `0` and `1`.
    #[test]
    fn phase_zero_restarts_the_digit_run_after_eight() {
        assert_eq!(marquee_frame(0), "^0Т^1Ы ^2С^3У^4П^5Е^6Р ^7Г^0О^1П");
    }

    /// `e = 8` and `e = 0` are the same frame: `(i + 8 - 1) mod 8` is
    /// `(i - 1) mod 8`. The cycle is nine passes long and eight frames wide,
    /// and that is the original's, not a rounding of it.
    #[test]
    fn the_last_phase_of_the_cycle_repeats_the_first() {
        assert_eq!(marquee_frame(8), marquee_frame(0));
        assert_eq!(MARQUEE_PHASES, 9);
    }

    /// Every frame spells `ТЫ СУПЕР ГОП` once the markup is stripped -- the
    /// eleven literals of the `1000:0acb` pool, in order.
    #[test]
    fn every_frame_spells_the_same_words() {
        for phase in 0..MARQUEE_PHASES {
            assert_eq!(crate::text::strip(&marquee_frame(phase)), "ТЫ СУПЕР ГОП");
        }
    }

    /// The verdict line and the banner's colour digit are the only two
    /// things `param_1` changes, and they move together.
    #[test]
    fn the_verdict_picks_both_the_line_and_the_banner_colour() {
        let dead = term::capture::lines(|| end_screen(false, &mut no_lines()));
        let won = term::capture::lines(|| marquee(&mut no_lines()));
        assert!(dead.contains(&DEATH_LINE.to_string()));
        assert!(won.contains(&VICTORY_LINE.to_string()));
        assert!(dead.iter().any(|l| l.starts_with("          ^4│")));
        assert!(won.iter().any(|l| l.starts_with("          ^2│")));
        // Eight banner rows on each, and the same eight.
        for (side, colour) in [(&dead, '4'), (&won, '2')] {
            let rows: Vec<&String> = side
                .iter()
                .filter(|l| l.starts_with(&format!("{BANNER_INDENT}{colour}")))
                .collect();
            assert_eq!(rows.len(), BANNER.len());
            for (got, want) in rows.iter().zip(BANNER) {
                assert!(got.ends_with(want), "{got}");
            }
        }
    }

    /// The marquee runs the end screen itself -- `1000:0d0a`. A victory that
    /// printed the banner without the marquee would mean `1000:5133` had
    /// been read as `FUN_1000_074b(1)` again.
    #[test]
    fn the_marquee_ends_with_the_end_screen() {
        let out = term::capture::lines(|| marquee(&mut no_lines()));
        let frames = out
            .iter()
            .filter(|l| l.contains("Т^") && l.contains("О^"))
            .count();
        assert_eq!(frames, usize::from(MARQUEE_PHASES));
        assert!(out.contains(&ANY_KEY.to_string()));
        let first_banner = out
            .iter()
            .position(|l| l.starts_with(BANNER_INDENT))
            .expect("the banner is drawn");
        let last_frame = out
            .iter()
            .rposition(|l| l.contains("Т^"))
            .expect("a frame is drawn");
        assert!(last_frame < first_banner, "the marquee runs first");
    }
}
