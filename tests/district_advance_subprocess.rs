//! Drives the real `gopnik` binary as a subprocess to exercise the
//! district-advance autosave *through `Game::run`'s loop*, which is the only
//! place the Task 21 wiring actually lives.
//!
//! `src/game.rs`'s in-process tests call `Game::district_advance` directly.
//! That checks the block's own behaviour and says nothing about WHERE it is
//! called from -- and the placement is the whole of this task: `1000:ab75`
//! runs at the top of every turn, before the street prompt at `1000:ae3c` /
//! `1000:ae55`, reached by the `1000:ee01 jmp 0xab75` back edge. `Game::run`
//! reads from `io::stdin()` directly, so nothing in-process can drive it;
//! this is the same harness `tests/den_reveal_subprocess.rs` and
//! `tests/term_output.rs` already use for exactly that reason.
//!
//! The state is synthesized on disk rather than played into: a district slot
//! (`save_r2.sav`, loaded with the key `2`) takes its district from the DIGIT
//! at `1000:6bf9`, so a level-40 record in slot 2 lands at district 2 with
//! `1000:ab7f`'s gate (`district * 10 <= level`) already satisfied on turn
//! one. That needs no RNG, which `main.rs` seeds from the wall clock with no
//! override.

use gopnik::game::Game;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// file `0x9B83`, decimal 39811 -- `1000:abec`.
const ADVANCE_LINE: &str =
    "Ты доказал, что ты самый крутой в этом районе - отправляйся в следующий";
/// file `0x9BCD`, decimal 39885 -- `1000:ac05`.
const SAVE_PROMPT: &str = "Хочешь сохранить свои достижения?";
/// file `0x9C01`, decimal 39937 -- `1000:ad0d`, with the digit and `.sav`
/// appended from `DS:3b7c` and file `0x9BFC`.
const SAVED_PREFIX: &str = "Сохранено в save_r";
/// `1000:6c1e`, file `0x7CAC`, printed by the loader before `run()` starts.
const LOADED: &str = "Загружено из save_r2";

fn scratch(tag: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("district-advance-subprocess-tests")
        .join(tag);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

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

/// A record whose level clears `1000:ab7f` for districts 2, 3 and 4, written
/// into slot 2 so `1000:6bf9` puts it at district 2.
fn slot_2_at_level_40(dir: &Path) {
    let (player, progress) = gopnik::progress::new_character("Тест", 2);
    let mut g = Game::new(player, progress, 12345);
    g.player.level = 40;
    g.player.money = 1234;
    g.write_save_as(dir, "save_r2.sav").unwrap();
}

/// The whole block, through the real loop: it fires on turn one, before the
/// street prompt, and `y` writes the slot named for the district it just
/// reached.
#[test]
fn the_advance_runs_at_the_top_of_the_turn_and_y_writes_the_new_slot() {
    let dir = scratch("advance-and-save");
    slot_2_at_level_40(&dir);

    // `2` picks the slot; `y` answers 1000:ac31's ReadLn; `e` (1000:edfa)
    // quits before the next turn's advance can fire again.
    let out = run_in(&dir, "2\ny\ne\n");

    assert!(
        out.contains(LOADED),
        "the save must load -- stdout: {out:?}"
    );
    assert!(out.contains(ADVANCE_LINE), "1000:abec -- stdout: {out:?}");
    assert!(out.contains(SAVE_PROMPT), "1000:ac05 -- stdout: {out:?}");

    // **The ordering assertion, and the reason this test is a subprocess.**
    // `main.rs` prints the load line and then hands straight to `Game::run`.
    // If the advance were called after `self.prompt()`, the `\` of
    // 1000:ae3c's street prompt would sit between them. It must not: the
    // announcement is the very next thing written.
    let after_load = &out[out.find(LOADED).unwrap() + LOADED.len()..];
    // `chars().take(80)`, NOT `&s[..80]`. Everything printed here is
    // Cyrillic, so a byte-index slice lands mid-codepoint and panics with
    // `not a char boundary` INSTEAD of the message naming the claim -- which
    // is the one thing `docs/re/METHODOLOGY.md`'s mutate rule requires a
    // failing assertion to print. The first cut of this file did exactly
    // that and the review caught it.
    assert!(
        after_load
            .trim_start_matches('\n')
            .starts_with(ADVANCE_LINE),
        "1000:ab75 runs BEFORE 1000:ae3c's prompt; got {:?}",
        after_load.chars().take(80).collect::<String>()
    );

    // 1000:acb9 Rewrite(f, 694) into the post-increment digit.
    let written = dir.join("save_r3.sav");
    assert_eq!(
        std::fs::read(&written).unwrap().len(),
        gopnik::save::SIZE,
        "1000:ac73's Str([0x3692]) reads the district AFTER 1000:ab92"
    );
    assert!(
        out.contains(&format!("{SAVED_PREFIX}3.sav")),
        "1000:ad0d names the file it wrote -- stdout: {out:?}"
    );
    // One turn, one promotion: 1000:ab75..1000:ad12 has no back edge.
    assert_eq!(
        out.matches(ADVANCE_LINE).count(),
        1,
        "one 1000:ab92 per turn -- stdout: {out:?}"
    );
    assert!(
        !dir.join("save_r4.sav").exists(),
        "a second promotion inside one turn would have written slot 4"
    );
    // 1000:acd5 Close, then straight to 1000:ad0d: no places.sav here.
    assert!(
        !dir.join("places.sav").exists(),
        "the mage's arm, not this one"
    );
}

/// The control: same run, `n` at the prompt. The district still advances --
/// `1000:ab92` is upstream of `1000:ac31` -- but nothing reaches
/// `1000:acb9`. Without this the assertions above could pass on a build that
/// wrote a file on every answer.
#[test]
fn declining_still_advances_the_district_but_writes_nothing() {
    let dir = scratch("advance-declined");
    slot_2_at_level_40(&dir);

    // Two turns: `n`, then `n` again. The second announcement proves the
    // first turn really did increment even though it wrote no file.
    let out = run_in(&dir, "2\nn\nn\ne\n");

    assert_eq!(
        out.matches(ADVANCE_LINE).count(),
        2,
        "one promotion per turn, two turns -- stdout: {out:?}"
    );
    assert!(
        !out.contains(SAVED_PREFIX),
        "1000:ac59's jz is the only way to 1000:acb9 -- stdout: {out:?}"
    );
    for slot in ['3', '4', '5'] {
        assert!(
            !dir.join(format!("save_r{slot}.sav")).exists(),
            "slot {slot} must not exist"
        );
    }
}
