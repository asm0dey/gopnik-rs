//! Entry point: the save-slot menu, then character creation, then the main
//! loop, in that order.
//!
//! The game scans the working directory for save files before asking
//! anything, and with none found jumps straight to the new-character block,
//! printing nothing. So "no save file" is the ordinary new-game case rather
//! than an error, and the menu below is silent for a clean checkout.

use gopnik::game::Game;
use gopnik::model::Fighter;
use gopnik::opening;
use gopnik::persist;
use gopnik::progress::{self, Progress};
use gopnik::term;
use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Character creation. Every line below is a verbatim string of the game;
/// nothing here is composed.
///
/// 1. `Выбери кем ты будешь: `.
/// 2. The five options, each its **own** string:
///    `0-Пацан`, `1-Отморозок`, `2-Гопник`, `3-Вор`, `4-Чё за батва?`.
/// 3. Reads the answer.
/// 4. Answer `4` prints the four class descriptions, then
///    `А теперь выбирай: ` and the first four options again, and reads
///    once more.
/// 5. Clamps the answer to `0..=3` and sets the class's stats -- that half
///    is `progress::new_character`.
/// 6. Writes `^2А зовут тебя:^7 ` without a trailing newline, then reads
///    the name and substitutes `Раз^6дол^4бай` when the just-read line's
///    **length** is zero -- the same idiom `Game::rename` uses. The test
///    is on length, not on whitespace content: a line of only spaces is
///    kept, not substituted, so only the line terminator is stripped
///    here, never a full trim. That default name carries `^N` markup of
///    its own; it is stored verbatim.
///
/// **Note the order:** the game asks for the class *first* and the name
/// *second.* The university backstory that precedes it is
/// [`gopnik::opening::backstory`], called from [`main`] just above this.
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
    // Tests the just-read line's LENGTH, not its trimmed content -- see
    // `Game::rename`'s doc for the identical idiom. `read_line` (unlike
    // `BufRead::lines`) keeps the line terminator, so only that terminator
    // is stripped here, not general whitespace: a line of only spaces must
    // stay nonempty and be kept, exactly like `rename`.
    let name = name.trim_end_matches(['\n', '\r']);
    let name = if name.is_empty() {
        "Раз^6дол^4бай"
    } else {
        name
    };
    // AFTER the substitution, the default name is rebuilt with the prefix
    // `^7 ` + itself, so the default name carries the prefix too.
    // `crate::persist::NAME_PREFIX` is that literal.
    let name = format!("{}{name}", persist::NAME_PREFIX);

    progress::new_character(&name, answer as u16)
}

/// A bad parse leaves the answer untouched; the caller then clamps it.
/// Zero is the same outcome.
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

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => {}
        _ => {
            eprintln!("usage: gopnik");
            std::process::exit(2);
        }
    }

    // The game prints no version text at start-up -- the screen it opens
    // with is the ASCII-art splash (`opening::splash`). The string
    // `^4Gopnik: ^7version 1.02 june,sept 2003` appears twice elsewhere:
    // once copied into the save record's `magic` slot (`save::MAGIC` /
    // `Save::blank()`, never printed), and once as the `version` verb's
    // own copy, printed only when the player types `version` at the
    // street prompt (`Game::banner`). Neither runs at process start, so
    // this must not print it there either.
    term::init();
    let stdin = io::stdin();
    let seed = clock_seed();
    let here = std::env::current_dir()?;

    // Falls through to the new-character block rather than failing: no
    // save file at all, a key that is none of `0`/`2`..`5`, or a save file
    // that fails to open.
    let loaded = {
        let mut locked = stdin.lock();
        let mut lines = (&mut locked).lines();
        // The splash runs before the save path is resolved and the RNG is
        // seeded, so it is the first thing on screen whether or not a save
        // exists.
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
                // The new-character block's cold open reads through the
                // same lock `create_character` then takes: `Lines` borrows
                // the `StdinLock` rather than owning a buffer of its own,
                // so nothing is lost between the two.
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
    // The last thing done before returning into the turn loop, once per
    // process on both paths above -- `Game::apply_class_bonus` runs twice
    // on a load, so this text cannot live there. See its doc.
    game.announce_district();
    game.run()?;
    io::stdout().flush()
}
