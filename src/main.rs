//! Entry point: character creation, then the main loop.
//!
//! There is no `.SAV`/`PLACES.SAV` load path here -- verified empirically
//! that `orig/g.exe` runs from itself alone, so "no save file" is the
//! ordinary new-game case, not an error to handle. Loading an existing
//! character is out of this task's scope; see `src/game.rs`'s module doc
//! and task-11-report.md.

use gopnik::game::Game;
use gopnik::model::Fighter;
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
///    `1000:7227` substitutes `Раз^6дол^4бай` (file `0x80B4`) when it is
///    empty. That default name carries `^N` markup of its own; it is stored
///    verbatim, exactly as the original stores it.
///
/// **Note the order:** the original asks for the class *first* and the name
/// *second*. **Not reproduced:** the university backstory the game prints
/// before this (files `0x7D81`..`0x7F1F`).
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
    let mut name = String::new();
    let _ = stdin.read_line(&mut name);
    let name = name.trim();
    let name = if name.is_empty() {
        "Раз^6дол^4бай"
    } else {
        name
    };

    progress::new_character(name, answer as u16)
}

/// Turbo Pascal's `Val` (`1f78:131b`) leaves the target untouched on a bad
/// parse; the caller at `1000:712d` then clamps. Zero is the same outcome.
fn read_number(stdin: &mut impl BufRead) -> i32 {
    let mut buf = String::new();
    let _ = stdin.read_line(&mut buf);
    buf.trim().parse().unwrap_or(0)
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

fn main() -> io::Result<()> {
    term::init();
    term::println("^4Gopnik: ^7version 1.02 june,sept 2003");
    let stdin = io::stdin();
    let (player, progress) = {
        let mut locked = stdin.lock();
        create_character(&mut locked)
    };
    Game::new(player, progress, clock_seed()).run()?;
    io::stdout().flush()
}
