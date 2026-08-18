//! Cross-platform colour output for the game's `^N`-markup text.
//!
//! This is the ONLY way the game writes user-visible text; calling
//! `println!` on a `text::render`/`text::strip` result is a bug after this
//! task. `colored` is used purely as a policy oracle here (its
//! `SHOULD_COLORIZE` destination check) and for the Windows VT FFI call it
//! makes possible — never as a styling API. The `^N` markup, already parsed
//! by `text::parse`, is what chooses colours; this module never does.

use std::io::{self, Write};

use crate::text;

/// Enable Windows VT processing. Call once at startup, before any output.
///
/// `colored` does not do this internally (verified by reading the crate
/// source: nothing calls `set_virtual_terminal` on our behalf), and the
/// function itself is `#[cfg(windows)]`-gated, so this needs its own cfg
/// block and is a no-op everywhere else. The `Result` is ignored
/// deliberately: a failure here means the console isn't a VT-capable one
/// (e.g. a legacy `cmd.exe`), which `SHOULD_COLORIZE`'s own tty/CLICOLOR
/// checks already degrade for by falling back to plain text — there is
/// nothing else actionable to do with the error.
///
/// NOTE: the Windows branch is untested on this Linux host. Windows VT
/// behaviour is delegated to `colored` and has not been verified against a
/// real Windows console; this is a known gap, not a verified claim.
#[cfg(windows)]
pub fn init() {
    let _ = colored::control::set_virtual_terminal(true);
}

#[cfg(not(windows))]
pub fn init() {}

fn rendered(src: &str) -> String {
    if colored::control::SHOULD_COLORIZE.should_colorize() {
        text::render(src)
    } else {
        text::strip(src)
    }
}

/// A broken pipe (e.g. `gopnik | head`) is not a bug — it's a normal way for
/// output to stop being consumed, so it is swallowed. `Cargo.toml` sets
/// `panic = "abort"` for release builds, so panicking here would hard-kill the
/// process instead of letting it exit normally, and a line-based game has no
/// retry story for a broken stdout anyway.
///
/// Any OTHER write failure — a full disk on redirected output, say — is a real
/// problem the player should hear about, so it goes to stderr rather than
/// vanishing with the broken-pipe case. Output continues either way: one failed
/// line is not a reason to take the game down.
fn write_out(s: &str) {
    if let Err(e) = io::stdout().write_all(s.as_bytes()) {
        if e.kind() != io::ErrorKind::BrokenPipe {
            let _ = writeln!(io::stderr(), "gopnik: cannot write to stdout: {e}");
        }
    }
}

/// Write one line of game text to stdout, with `^N` markup rendered as
/// colour when the destination can display it and stripped when it cannot.
pub fn println(src: &str) {
    write_out(&rendered(src));
    write_out("\n");
}

/// Same, without the trailing newline (for prompts). Flushes.
pub fn print(src: &str) {
    write_out(&rendered(src));
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `colored::control::set_override` is process-global state, and `cargo
    // test` runs tests from this file on multiple threads within one
    // process by default. Without serializing, two of these tests can
    // interleave their set_override/unset_override calls and flake. The
    // brief's starting-point test sketch does not mention this; fixing it
    // here.
    static COLOR_LOCK: Mutex<()> = Mutex::new(());

    /// RAII guard: sets the override on construction, always unsets it on
    /// drop (including during a test panic/unwind), so a failing assertion
    /// inside a test can never leave the global override stuck for the
    /// next test that acquires COLOR_LOCK.
    struct OverrideGuard;

    impl OverrideGuard {
        fn new(value: bool) -> Self {
            colored::control::set_override(value);
            OverrideGuard
        }
    }

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            colored::control::unset_override();
        }
    }

    #[test]
    fn renders_ansi_when_colorize_forced_on() {
        let _lock = COLOR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = OverrideGuard::new(true);
        assert_eq!(rendered("^4x"), text::render("^4x"));
    }

    #[test]
    fn strips_markup_when_colorize_forced_off() {
        let _lock = COLOR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = OverrideGuard::new(false);
        assert_eq!(rendered("^4x"), text::strip("^4x"));
    }
}
