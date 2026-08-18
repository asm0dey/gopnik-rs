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
use gopnik::rng::Rng;
use gopnik::term;
use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// `^0А зовут тебя:` / the class prompt (`0-Пацан, 1-Отморозок, 2-Гопник,
/// 3-Вор`), both at character creation (`1000:7140`..`1000:71e8`,
/// `docs/re/progression.md`'s `new_character` doc). The exact prompt wording
/// beyond the name cue was not extracted by this task; the flow (ask a name,
/// ask a 0-3 answer) is what `progress::new_character`'s own signature
/// requires, not fabricated menu text.
fn create_character() -> (Fighter, Progress) {
    term::print("^0А зовут тебя: ");
    let mut name = String::new();
    let _ = io::stdin().lock().read_line(&mut name);
    let name = name.trim();
    let name = if name.is_empty() { "Пацан" } else { name };

    term::println("0-Пацан, 1-Отморозок, 2-Гопник, 3-Вор");
    let mut answer = String::new();
    let _ = io::stdin().lock().read_line(&mut answer);
    let answer: u16 = answer.trim().parse().unwrap_or(0);

    progress::new_character(name, answer)
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
    let (player, progress) = create_character();
    let mut rng = Rng::new(clock_seed());
    // Burn one draw so a name/answer combination that happens to require no
    // RNG interaction still starts from a stepped state, matching the
    // original always stepping RandSeed at least once during startup
    // (unverified precisely, but harmless: Rng has no "first draw" special
    // case to preserve).
    let _ = rng.next_u32();
    let seed = rng.state();
    Game::new(player, progress, seed).run()?;
    io::stdout().flush()
}
