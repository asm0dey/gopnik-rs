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
/// NOTE: this branch compiles, links and runs — `scripts/check-windows.sh`
/// cross-builds it for `x86_64-pc-windows-gnu` and exercises it under wine,
/// where the colour-policy decisions come out byte-identical to the native
/// build. What is still UNVERIFIED is whether the VT call has its intended
/// effect: `ENABLE_VIRTUAL_TERMINAL_PROCESSING` changes how a console
/// *renders* bytes, not which bytes we write, so the same escapes reach a
/// pipe whether or not `SetConsoleMode` succeeded. No byte-capture test can
/// settle it — on wine or on real Windows. It needs a human looking at a
/// `cmd.exe` window on a Windows build that does not enable VT by default
/// (Windows Terminal does, so a clean run there proves nothing).
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
    #[cfg(test)]
    if capture::push(src, true) {
        return;
    }
    write_out(&rendered(src));
    write_out("\n");
}

/// Same, without the trailing newline (for prompts). Flushes.
pub fn print(src: &str) {
    #[cfg(test)]
    if capture::push(src, false) {
        return;
    }
    write_out(&rendered(src));
    let _ = io::stdout().flush();
}

/// Test-only output capture.
///
/// `println`/`print` write straight to `io::stdout()`, which is why
/// `tests/den_reveal_subprocess.rs` had to spawn the real binary to assert
/// on a *printed line* rather than on game state. That is the right harness
/// for "what does the shipped binary do", but it cannot reach a branch whose
/// precondition needs a specific RNG outcome -- and the den's `d` arm
/// (`1000:dd32`..`1000:decd`) has three such branches. This seam exists so a
/// unit test can assert the exact lines an arm emits.
///
/// It captures the **source** string, before `rendered` applies the colour
/// policy, so an assertion compares against the literal quoted from
/// `data/strings.json` and does not change meaning with `NO_COLOR`,
/// `CLICOLOR_FORCE` or whether stdout is a tty.
#[cfg(test)]
pub mod capture {
    use std::cell::RefCell;

    thread_local! {
        static SINK: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// Appends to the active sink and reports whether one was active. When
    /// none is, the caller falls through to the real stdout write.
    pub(super) fn push(src: &str, newline: bool) -> bool {
        SINK.with(|s| match s.borrow_mut().as_mut() {
            Some(buf) => {
                buf.push_str(src);
                if newline {
                    buf.push('\n');
                }
                true
            }
            None => false,
        })
    }

    /// Clears the sink on drop, so a panicking assertion inside `lines`
    /// cannot leave capture armed for the next test on this thread.
    struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            SINK.with(|s| *s.borrow_mut() = None);
        }
    }

    /// Runs `f` with output captured and returns the lines it wrote, split
    /// on `\n`. A trailing `print` with no newline still yields its own
    /// entry; a run that wrote nothing yields an empty `Vec`.
    pub fn lines(f: impl FnOnce()) -> Vec<String> {
        SINK.with(|s| *s.borrow_mut() = Some(String::new()));
        let _guard = Guard;
        f();
        let text = SINK.with(|s| s.borrow().clone()).unwrap_or_default();
        if text.is_empty() {
            return Vec::new();
        }
        text.strip_suffix('\n')
            .unwrap_or(&text)
            .split('\n')
            .map(str::to_string)
            .collect()
    }
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

    /// The capture seam every den-arm line assertion in `crate::game` rests
    /// on. Three properties it has to have for those to mean what they say:
    /// a `print` and the `println` after it are ONE line (which is how
    /// `1000:dc1c`/`1000:dc39`'s two-`print`-then-`println` announcement is
    /// asserted), an empty `println` is a line and not nothing, and the
    /// markup survives -- capture takes the SOURCE, so the assertions
    /// compare against `data/strings.json`'s own bytes whatever the ambient
    /// colour policy is.
    #[test]
    fn capture_joins_a_print_into_the_line_that_follows_it() {
        let out = capture::lines(|| {
            println("^2first");
            print("^6a ");
            print("b ");
            println("c");
            println("");
        });
        assert_eq!(out, vec!["^2first", "^6a b c", ""]);
        assert!(capture::lines(|| {}).is_empty());
    }

    /// A panic inside the captured closure must not leave the sink armed
    /// for the next test on this thread -- otherwise one failing assertion
    /// would silently swallow another test's output.
    #[test]
    fn capture_disarms_itself_when_the_closure_panics() {
        let panicked = std::panic::catch_unwind(|| {
            capture::lines(|| {
                println("swallowed");
                panic!("deliberate");
            })
        });
        assert!(panicked.is_err());
        // If the guard had not fired, this would capture nothing at all.
        let out = capture::lines(|| println("visible"));
        assert_eq!(out, vec!["visible"]);
    }
}
