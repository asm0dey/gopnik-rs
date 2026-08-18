//! Drives the real `gopnik` binary as a subprocess with piped (non-tty)
//! stdout and asserts on the raw bytes it writes, under controlled
//! colour-related environment variables. This is the only way to observe
//! `colored`'s destination-aware policy honestly: it depends on whether
//! stdout is a tty, which an in-process unit test cannot fake.
//!
//! Updated by Task 11: `main.rs` now runs character creation and the main
//! loop, not just the banner. With `stdin` null every `read_line` returns
//! immediately at EOF, so the full sequence below (banner, name prompt,
//! class prompt, then the loop's own `\` prompt before it too sees EOF and
//! exits) is exactly what a real "no input available" run produces -- this
//! is not a relaxation of the test, it is the same exact-bytes assertion
//! against the new, larger, real output.

use std::process::{Command, Stdio};

// Must match the literals `main.rs` passes to `term::println`/`term::print`.
const BANNER: &str = "^4Gopnik: ^7version 1.02 june,sept 2003";
const NAME_PROMPT: &str = "^0А зовут тебя: ";
const CLASS_PROMPT: &str = "0-Пацан, 1-Отморозок, 2-Гопник, 3-Вор";
const GAME_PROMPT: &str = "\\";

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
fn expected(f: impl Fn(&str) -> String) -> String {
    format!(
        "{}\n{}{}\n{}",
        f(BANNER),
        f(NAME_PROMPT),
        f(CLASS_PROMPT),
        f(GAME_PROMPT),
    )
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
