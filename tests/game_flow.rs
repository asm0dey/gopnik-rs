use gopnik::locations::Places;
use std::path::Path;

/// The keystrokes the opening consumes before typed input reaches the game.
///
/// `gopnik::opening`'s gap tables are the port's own record of where the
/// original's `ReadKey`s fall, and `tools/difftest.py` re-derives them from
/// `orig/g.exe`; counting them here rather than writing a literal number of
/// newlines means this script cannot drift away from the code it drives.
fn keys(gaps: gopnik::opening::Gaps) -> String {
    let n = gaps
        .iter()
        .flat_map(|(_, events)| events.chars())
        .filter(|c| *c == 'K')
        .count();
    "\n".repeat(n)
}

#[test]
fn places_round_trips_the_real_file() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("orig")
        .join("PLACES.SAV");
    let bytes = std::fs::read(p).unwrap();
    assert_eq!(bytes.len(), 7);

    let places = Places::from_bytes(&bytes);
    assert_eq!(places.to_bytes().to_vec(), bytes);
}

#[test]
fn new_district_hides_all_places() {
    let mut places = Places::from_bytes(&[1u8; 7]);
    places.reset_for_new_district();
    assert_eq!(places.to_bytes(), [0u8; 7]);
}

// ---------------------------------------------------------------------------
// Task 18: what the STREET prompt does with the two verbs `Game::call_backup`
// and `Game::shoot` answer.
//
// Both used to print an invented refusal. Neither line is observable from a
// unit test -- `crate::term` writes straight to this process's stdout -- so
// this drives the real binary the way `tests/term_output.rs` does and reads
// the bytes back.
// ---------------------------------------------------------------------------

use std::io::Write;
use std::process::{Command, Stdio};

/// Run the shipped binary with `script` on stdin and colour disabled, and
/// return its stdout.
fn transcript(script: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gopnik"))
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to run gopnik binary");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(script.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "binary exited non-zero");
    String::from_utf8(out.stdout).expect("stdout was not valid UTF-8")
}

/// `v` and `f` at the street prompt, on a fresh character who owns no
/// pistol.
///
/// * `v` is compared at exactly ONE site in the whole image, `1000:4caa`,
///   and that site pushes the FIGHT prompt's buffer `20ae:3a72`. `entry`'s
///   chain never compares it, so the street writes nothing.
/// * `f` IS an `entry` verb (`1000:ec96`), but `1000:ec9d`
///   `cmp byte [0x394d],0` / `1000:eca2 jz 0xecbd` means the refusal is only
///   for someone actually carrying a pistol.
///
/// `k` is the control: it is a street verb with an unconditional line
/// (`1000:ecce`), so it proves this harness can see a handler's output at
/// all. Without it, "nothing was printed" would be indistinguishable from
/// "the transcript was never read".
#[test]
fn v_and_f_write_nothing_at_the_street_prompt_without_a_pistol() {
    // 0 = Пацан, then the name, then the three verbs.
    let out = transcript(&format!(
        "{}{}0\n^7 test\nv\nf\nk\n",
        keys(gopnik::opening::SPLASH_GAPS),
        keys(gopnik::opening::BACKSTORY_GAPS)
    ));

    const K_LINE: &str = "Чё машешь копытами? Ищи мудака которого будешь пинать!";
    const OLD_V_LINE: &str = "Ни кто не хочет за тебя впрягаться."; // CS 0x35e9
    const OLD_F_LINE: &str = "Ты чё псих? мигом менты накроют!"; // CS 0xaa4e

    assert!(
        out.contains(K_LINE),
        "the control line is missing, so this transcript proves nothing:\n{out}"
    );
    assert!(
        !out.contains(OLD_V_LINE),
        "`v` is not an `entry` verb and must write nothing:\n{out}"
    );
    assert!(
        !out.contains(OLD_F_LINE),
        "1000:eca2 skips the refusal without a pistol:\n{out}"
    );

    // And the shape: three prompts before the `k` line and one after, with
    // nothing between them -- so `v` and `f` did not print something else
    // either.
    // Anchored past the opening, NOT on the first `\` in the whole
    // transcript: `crate::opening::SPLASH`'s banner rows are ASCII art with
    // backslashes in them, so `out.find('\\')` lands inside the title screen.
    // `1000:73a2`'s tutorial line is the last thing printed before the turn
    // loop's first prompt.
    let last_opening_line = gopnik::text::strip(gopnik::opening::TUTORIAL[2]);
    let after_opening =
        &out[out.find(&last_opening_line).expect("1000:73a2") + last_opening_line.len()..];
    let tail = &after_opening[after_opening.find('\\').expect("a street prompt")..];
    assert!(
        tail.starts_with(&format!("\\\\\\{K_LINE}\n\\")),
        "unexpected street transcript: {tail:?}"
    );
}
