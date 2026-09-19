//! Drives the real `gopnik` binary as a subprocess with piped (non-tty)
//! stdout and asserts on the raw bytes it writes, under controlled
//! colour-related environment variables. This is the only way to observe
//! `colored`'s destination-aware policy honestly: it depends on whether
//! stdout is a tty, which an in-process unit test cannot fake.
//!
//! Updated by Task 11: `main.rs` now runs character creation and the main
//! loop, not just the banner. With `stdin` null every `read_line` returns
//! immediately at EOF, so the full sequence below is exactly what a real "no
//! input available" run produces -- this is not a relaxation of the test, it
//! is the same exact-bytes assertion against the new, larger, real output.
//!
//! Updated again by `docs/re/port-gaps.md` rows 6, 8 and 10: that sequence is
//! now the splash, the backstory, the class menu, the name prompt, the entry
//! district announcement and the tutorial, then the loop's own `\` prompt
//! before it too sees EOF and exits.
//!
//! The constants below are transcribed from `orig/g.exe` via
//! `data/strings.json`, at the file offsets named on each line -- **not**
//! copied out of `main.rs`. That is the point: if `main.rs` drifts away from
//! the original's bytes, this test fails. To convert a Ghidra `1000:XXXX`
//! address to a file offset, run `python3 tools/re_query.py resolve 1000:XXXX`
//! (`docs/re/METHODOLOGY.md` is the authority for the rule).

use std::process::{Command, Stdio};

const CHOOSE: &str = "Выбери кем ты будешь: "; // file 0x7F67, written at 1000:6f2b
const OPT0: &str = "0-Пацан"; // file 0x7F7E
const OPT1: &str = "1-Отморозок"; // file 0x7F86
const OPT2: &str = "2-Гопник"; // file 0x7F92
const OPT3: &str = "3-Вор"; // file 0x7F9B
const OPT4: &str = "4-Чё за батва?"; // file 0x7FA1
const NAME_PROMPT: &str = "^2А зовут тебя:^7 "; // file 0x80A1, Write at 1000:71ea
const GAME_PROMPT: &str = "\\"; // file 0x9BF1, the one-byte shortstring "\"

fn run(env_remove: &[&str], env_set: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gopnik"));
    cmd.stdout(Stdio::piped()).stdin(Stdio::null());
    for var in env_remove {
        cmd.env_remove(var);
    }
    for (k, v) in env_set {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("failed to run gopnik binary");
    assert!(output.status.success(), "binary exited non-zero");
    String::from_utf8(output.stdout).expect("stdout was not valid UTF-8")
}

const COLOR_ENV_VARS: &[&str] = &["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE"];

/// Build the expected full-run transcript, applying `f` (either
/// `gopnik::text::render` or `gopnik::text::strip`) to each piece exactly as
/// `term::println`/`term::print` would.
///
/// **What comes from where, and why the two halves differ.** The constants
/// above are transcribed from `orig/g.exe` by hand, so `main.rs` drifting
/// from the original's bytes fails this test. The opening blocks pulled in
/// from `gopnik::opening` below are NOT re-transcribed here, because
/// `tools/difftest.py` re-decodes all 71 of those lines and all 13 of their
/// blank/`ReadKey` gaps straight out of the image and compares them to the
/// same constants -- a second hand copy in this file would add a second
/// place to get them wrong and no independent reading. What this test is
/// for, and what nothing else covers, is the ESCAPE-SEQUENCE POLICY applied
/// to every one of those lines, plus the order the blocks run in.
fn expected(f: impl Fn(&str) -> String) -> String {
    use gopnik::opening;

    let mut out = String::new();
    let mut line = |s: &str| {
        out.push_str(&f(s));
        out.push('\n');
    };
    // 1000:02c2's splash, then -- with stdin at EOF and no save file --
    // straight into 1000:6de6's backstory. Both blocks' blank lines and
    // `ReadKey`s come from their gap tables; a `ReadKey` at EOF writes
    // nothing, so only the `'B'` events reach the transcript.
    for (text, gaps) in [
        (&opening::SPLASH[..], opening::SPLASH_GAPS),
        (&opening::BACKSTORY[..], opening::BACKSTORY_GAPS),
    ] {
        for i in 0..=text.len() {
            if let Some((_, events)) = gaps.iter().find(|(at, _)| *at == i) {
                for _ in events.chars().filter(|c| *c == 'B') {
                    line("");
                }
            }
            if let Some(s) = text.get(i) {
                line(s);
            }
        }
    }
    for s in [CHOOSE, OPT0, OPT1, OPT2, OPT3, OPT4] {
        line(s);
    }
    out.push_str(&f(NAME_PROMPT));
    // `Game::announce_district` at district 1: 1000:7262's two lines and
    // 1000:7369's three.
    for s in opening::START_ARRIVAL[..2].iter().chain(&opening::TUTORIAL) {
        out.push_str(&f(s));
        out.push('\n');
    }
    out.push_str(&f(GAME_PROMPT));
    out
}

#[test]
fn no_color_env_and_piped_stdout_yields_plain_text() {
    let stdout = run(COLOR_ENV_VARS, &[]);
    assert!(
        !stdout.contains("\x1b["),
        "expected no ANSI escapes in piped output, got: {stdout:?}"
    );
    assert_eq!(stdout, expected(gopnik::text::strip));
}

#[test]
fn clicolor_force_yields_ansi_even_when_piped() {
    let stdout = run(COLOR_ENV_VARS, &[("CLICOLOR_FORCE", "1")]);
    assert!(
        stdout.contains("\x1b["),
        "expected an ANSI escape with CLICOLOR_FORCE=1, got: {stdout:?}"
    );
    assert_eq!(stdout, expected(gopnik::text::render));
}

#[test]
fn clicolor_force_wins_over_no_color() {
    // `colored` documents CLICOLOR_FORCE as taking precedence over NO_COLOR.
    // A strict reading of https://no-color.org says NO_COLOR should always
    // suppress colour; we deliberately follow the crate's own precedence
    // (verified in colored 3.1.1, control.rs:100-115) instead of fighting it,
    // per the brief's design decision to treat `colored` as the policy
    // oracle rather than reimplementing its logic here.
    let stdout = run(
        COLOR_ENV_VARS,
        &[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")],
    );
    assert!(
        stdout.contains("\x1b["),
        "expected CLICOLOR_FORCE to win over NO_COLOR, got: {stdout:?}"
    );
    assert_eq!(stdout, expected(gopnik::text::render));
}
