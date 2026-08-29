//! Drives the real `gopnik` binary as a subprocess to reach the den's `a`
//! reveal (`1000:dcba`..`1000:dd32`) with Dealers and Gym already found --
//! the "both already set" skip (`1000:dcbf`/`1000:dcc6`) that
//! `Game::den_reveal`'s in-process unit tests cannot observe, because its
//! only effect besides two idempotent flag stores is two `WriteLn`s, and
//! `src/term.rs` writes straight to `io::stdout()` with no in-process
//! capture.
//!
//! This is exactly the harness `tests/term_output.rs` already established
//! for that class of assertion: the real binary, piped stdout, exact-byte
//! (well, exact-substring, since this run's preceding output is not fully
//! pinned) checks. Reaching the state through normal typed play would need
//! real combat to raise level/street cred past the reveal's threshold,
//! which needs the RNG, which `main.rs` seeds from the wall clock with no
//! override -- so this test instead synthesizes `save_r0.sav`/`places.sav`
//! on disk with `Game::write_save_as`/`write_places` (the same helpers
//! `tests/save_load.rs` uses) and loads them, which is deterministic and
//! touches no RNG at all.

use gopnik::game::Game;
use gopnik::locations::Location;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const REVEAL_LINE_1: &str = "Тут у нас есть пара мест куда тебе стоит сходить"; // file 0xB89B
const REVEAL_LINE_2: &str = "Ты узнал где находится качалка и где находятся барыги"; // file 0xB8CE

fn scratch(tag: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("den-reveal-subprocess-tests")
        .join(tag);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Runs the real binary with `dir` as its working directory (so
/// `persist::choose_slot` finds the synthesized save there) and `script`
/// fed to stdin. `NO_COLOR` is forced so the assertions below match plain
/// text regardless of the ambient environment, matching
/// `tests/term_output.rs`'s own colour-policy handling.
fn run_in(dir: &Path, script: &str) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gopnik"));
    cmd.current_dir(dir)
        .env_remove("CLICOLOR_FORCE")
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().expect("failed to spawn gopnik binary");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(script.as_bytes())
        .expect("write script to stdin");
    let output = child
        .wait_with_output()
        .expect("failed to run gopnik binary");
    assert!(output.status.success(), "binary exited non-zero");
    String::from_utf8(output.stdout).expect("stdout was not valid UTF-8")
}

/// `player.level = 20`, `pontovost_street = 100`, class Гопник (so Den is
/// already found via the class bonus, `1000:73c3`) -- comfortably above the
/// den reveal's `(level - (district-1)*10)*2 + street_cred >= 0x28`
/// threshold under any district slot 0's `level/10 + 1` derives.
fn base_game() -> Game {
    let (player, progress) = gopnik::progress::new_character("Тест", 2); // 2 = Гопник
    let mut g = Game::new(player, progress, 12345);
    g.player.level = 20;
    g.pontovost_street = 100;
    g
}

#[test]
fn the_both_already_set_skip_prints_neither_reveal_line() {
    let mut g = base_game();
    g.places.mark_found(Location::Dealers);
    g.places.mark_found(Location::Gym);
    assert!(g.places.is_found(Location::Den), "Гопник's class bonus");

    let dir = scratch("both-already-set");
    g.write_save_as(&dir, "save_r0.sav").unwrap();
    g.write_places(&dir).unwrap();

    // "0" loads save_r0.sav, "pr" enters the den, "a" is the reveal token.
    let stdout = run_in(&dir, "0\npr\na\n");

    assert!(
        stdout.contains("Загружено из save_r0"),
        "the save must actually load -- stdout: {stdout:?}"
    );
    assert!(
        !stdout.contains(REVEAL_LINE_1),
        "1000:dd00's line must NOT print when Dealers and Gym are already found -- stdout: {stdout:?}"
    );
    assert!(
        !stdout.contains(REVEAL_LINE_2),
        "1000:dd19's line must NOT print when Dealers and Gym are already found -- stdout: {stdout:?}"
    );
}

/// Control: identical setup, but Dealers/Gym are NOT pre-found, so the two
/// reveal lines DO print. Without this, the assertions above could not be
/// shown to fail -- this is what makes them checks and not tautologies.
#[test]
fn the_reveal_does_print_when_not_yet_both_found() {
    let g = base_game();
    assert!(!g.places.is_found(Location::Dealers));
    assert!(!g.places.is_found(Location::Gym));

    let dir = scratch("not-yet-both-found");
    g.write_save_as(&dir, "save_r0.sav").unwrap();
    g.write_places(&dir).unwrap();

    let stdout = run_in(&dir, "0\npr\na\n");

    assert!(stdout.contains(REVEAL_LINE_1), "stdout: {stdout:?}");
    assert!(stdout.contains(REVEAL_LINE_2), "stdout: {stdout:?}");
}
