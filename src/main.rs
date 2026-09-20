//! Entry point: the save-slot menu, then character creation, then the main
//! loop -- the order `FUN_1000_6a0d` runs them in.
//!
//! `orig/g.exe` scans the working directory for `save_r?.sav` before it asks
//! anything (`1000:6a81`/`1000:6a8a`), and with none found jumps straight to
//! the new-character block, printing nothing. So "no save file" is the
//! ordinary new-game case rather than an error, and the menu below is silent
//! for a clean checkout. `crate::persist` holds the whole path with its
//! addresses.

use gopnik::game::Game;
use gopnik::model::Fighter;
use gopnik::opening;
use gopnik::persist;
use gopnik::progress::{self, Progress};
use gopnik::term;
use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Character creation, traced at `1000:6ef4`..`1000:7259`. Every line below
/// is a verbatim string of `orig/g.exe`; nothing here is composed.
///
/// 1. `1000:6f2b` `Выбери кем ты будешь: ` (file `0x7F67`).
/// 2. `1000:6f44`..`1000:6fa8` the five options, each its **own** string:
///    `0-Пацан` (`0x7F7E`), `1-Отморозок` (`0x7F86`), `2-Гопник` (`0x7F92`),
///    `3-Вор` (`0x7F9B`), `4-Чё за батва?` (`0x7FA1`).
/// 3. `1000:6fcf` `ReadLn` into `DS:3b7c`, `1000:6fe8` `Val` into `DS:389c`.
/// 4. `1000:6ff0` answer `4` prints the four class descriptions (files
///    `0x7FB0`, `0x7FE8`, `0x802A`, `0x8059`), then `А теперь выбирай: `
///    (`0x808E`) and the first four options again, and reads once more.
/// 5. `1000:712d` clamps the answer to `0..=3`; `1000:7140`..`1000:71e7`
///    sets the class's stats -- that half is `progress::new_character`.
/// 6. `1000:71ea` writes `^2А зовут тебя:^7 ` (file `0x80A1`) with
///    `Write`, not `WriteLn`, then `1000:7211` reads the name and
///    `1000:7220`/`1000:7227` substitutes `Раз^6дол^4бай` (file `0x80B4`)
///    when the just-read shortstring's **length byte** is zero -- the same
///    idiom `Game::rename` traces at `1000:ed5f`/`1000:ed74`. That test is on
///    length, not on whitespace content: a line of only spaces is kept, not
///    substituted, so only the line terminator is stripped here, never a
///    full trim. That default name carries `^N` markup of its own; it is
///    stored verbatim, exactly as the original stores it.
///
/// **Note the order:** the original asks for the class *first* and the name
/// *second*. The university backstory that precedes it is
/// [`gopnik::opening::backstory`] (`1000:6de6`..`6f2b`), called from
/// [`main`] just above this.
fn create_character(stdin: &mut impl BufRead) -> (Fighter, Progress) {
    const OPTIONS: [&str; 4] = ["0-Пацан", "1-Отморозок", "2-Гопник", "3-Вор"];

    term::println("Выбери кем ты будешь: ");
    for line in OPTIONS {
        term::println(line);
    }
    term::println("4-Чё за батва?");

    let mut answer = read_number(stdin);
    if answer == 4 {
        term::println("^1Пацан - это нормальный тип. (Бонус - Гёлфренд, Клуб).");
        term::println("^1Отморозок - тупой корявый мудак. (Бонус - Самолечение царапин).");
        term::println("^1Гопник - гоп он и есть гоп. (Бонус - Притон)");
        term::println("^1Вор - везучий ублюдок. (Бонус - Воровство, Барыги)");
        term::println("А теперь выбирай: ");
        for line in OPTIONS {
            term::println(line);
        }
        answer = read_number(stdin);
    }
    let answer = if (0..=3).contains(&answer) { answer } else { 0 };

    term::print("^2А зовут тебя:^7 ");
    let name = term::read_line_raw(stdin);
    // 1000:7220 `cmp byte [0x379c],0` tests the just-read shortstring's
    // LENGTH BYTE, not its trimmed content -- see `Game::rename`'s doc for
    // the identical idiom at `1000:ed5f`. `read_line` (unlike
    // `BufRead::lines`) keeps the line terminator, so only that terminator
    // is stripped here, not general whitespace: a line of only spaces must
    // stay nonempty and be kept, exactly like `rename`.
    let name = name.trim_end_matches(['\n', '\r']);
    let name = if name.is_empty() {
        // 1000:7227 -- CS 0x67e4, substituted into 20ae:379c.
        "Раз^6дол^4бай"
    } else {
        name
    };
    // 1000:723a..1000:725d -- AFTER the substitution, 20ae:379c is rebuilt
    // as CS 0x67f2 (`^7 `) + itself, so the default name carries the prefix
    // too. `crate::persist::NAME_PREFIX` is that literal.
    let name = format!("{}{name}", persist::NAME_PREFIX);

    progress::new_character(&name, answer as u16)
}

/// Turbo Pascal's `Val` (`1f78:131b`) leaves the target untouched on a bad
/// parse; the caller at `1000:712d` then clamps. Zero is the same outcome.
fn read_number(stdin: &mut impl BufRead) -> i32 {
    term::read_line_raw(stdin).trim().parse().unwrap_or(0)
}

/// The original seeds `RandSeed` from the DOS clock (`Randomize`,
/// `1f78:11e0`) unless pinned for reproducibility -- `src/rng.rs`'s own doc
/// says that host-clock policy is deliberately left to the caller. This is
/// that choice: a seed drawn from the wall clock, not part of the game's
/// verified logic.
fn clock_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
}

/// `--trace-deterministic` writes `gopnik::trace`'s record stream to stdout
/// and exits without starting a game: no terminal setup, no colour, no RNG,
/// no input. `tools/difftest.py` is its only consumer.
///
/// An unrecognised argument is refused rather than ignored, so a typo in the
/// flag cannot silently launch an interactive session that a harness then
/// reads as an empty trace.
fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => {}
        ["--trace-deterministic"] => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            gopnik::trace::emit(&mut out)?;
            return out.flush();
        }
        _ => {
            eprintln!("usage: gopnik [--trace-deterministic]");
            std::process::exit(2);
        }
    }

    // `docs/re/port-gaps.md`'s "Known defects not yet filed as rows" flagged
    // an unconditional print here of `^4Gopnik: ^7version 1.02 june,sept
    // 2003`. **Fixed by the `FUN_1000_6a0d` survey.** The original prints no
    // version text at start-up at all -- the screen it opens with is
    // `FUN_1000_02c2`'s ASCII-art splash (`opening::splash`, called from
    // `1000:6a5a`), ported below. That exact string appears twice in the
    // image, and NEITHER site is a start-up print:
    // * `1000:6dcd`..`1000:6ddb` copies it into `DS:369c`, the save record's
    //   `magic` slot (`docs/re/save-format.md`), a plain memory move with no
    //   console call -- ported as `save::MAGIC` / `Save::blank()`.
    // * `1000:edb2`..`1000:edd0`, inside `entry`, is the `version` verb's
    //   OWN separate copy (file `0xC3C1`), read only when the player types
    //   `version` at the street prompt -- ported as `Game::banner`.
    // Neither runs at process start, so this port must not print it there
    // either.
    term::init();
    let stdin = io::stdin();
    let seed = clock_seed();
    let here = std::env::current_dir()?;

    // 1000:6a62..1000:6b81, then 1000:6b84..1000:6d9d. Both fall through to
    // the new-character block (1000:6dbe) rather than failing: no save file
    // at all, a key that is none of `0`/`2`..`5`, or a Reset whose IOResult
    // is non-zero.
    let loaded = {
        let mut locked = stdin.lock();
        let mut lines = (&mut locked).lines();
        // 1000:6a5a `call 0x2c2` -- the splash runs between the `GetDir`
        // that builds the save path and the `Randomize` at 1000:6a5d, so it
        // is the first thing on screen whether or not a save exists.
        opening::splash(&mut lines);
        match persist::choose_slot(&here, &mut lines)? {
            persist::SlotMenu::Load(slot) => persist::load_slot(&here, slot, seed)?,
            persist::SlotMenu::NoSaves | persist::SlotMenu::NewCharacter => None,
        }
    };

    let mut game = match loaded {
        Some(g) => g,
        None => {
            let (player, progress) = {
                let mut locked = stdin.lock();
                // 1000:6de6..1000:6f2b, the new-character block's cold open.
                // It reads through the same lock `create_character` then
                // takes: `Lines` borrows the `StdinLock` rather than owning a
                // buffer of its own, so nothing is lost between the two.
                {
                    let mut lines = (&mut locked).lines();
                    opening::backstory(&mut lines);
                }
                create_character(&mut locked)
            };
            Game::new(player, progress, seed)
        }
    };
    game.save_dir = here;
    // 1000:7262..1000:73bb, the last thing `FUN_1000_6a0d` does before it
    // returns into the turn loop. Once per process, on BOTH paths above --
    // `Game::apply_class_bonus` runs twice on a load, so the text cannot
    // live there. See its doc.
    game.announce_district();
    game.run()?;
    io::stdout().flush()
}
