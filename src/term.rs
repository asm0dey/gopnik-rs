//! Cross-platform colour output for the game's `^N`-markup text.
//!
//! This is the ONLY way the game writes user-visible text; calling
//! `println!` on a `text::render`/`text::strip` result is a bug after this
//! task. `colored` is used purely as a policy oracle here (its
//! `SHOULD_COLORIZE` destination check) and for the Windows VT FFI call it
//! makes possible — never as a styling API. The `^N` markup, already parsed
//! by `text::parse`, is what chooses colours; this module never does.

use std::io::{self, IsTerminal, Read, Write};
use std::process::{Command, Stdio};

use crate::text;

/// Wait for ONE keystroke and throw it away.
///
/// The original returns on a single key. This port used to consume a whole
/// LINE at every one of these sites, so the splash's own
/// `Нажми какую-нибудь кнопку` was a lie -- pressing a key did nothing until
/// you also pressed Enter.
///
/// On a terminal this now reads a single byte with the tty in raw mode,
/// which is what the banner promises. `stty` does the mode switch so the
/// port stays dependency-free; if it is missing or fails, the line read is
/// the fallback rather than a hang.
///
/// Off a terminal -- any piped run -- it keeps consuming one line. There is
/// no raw mode to set on a pipe, and a one-byte read would split scripted
/// input mid-line and desynchronise every `ReadLn` after it. That is why
/// this takes `lines` at all.
///
/// `None` at EOF is treated as any other keystroke, as before.
pub fn read_key(lines: &mut dyn Iterator<Item = io::Result<String>>) {
    if !io::stdin().is_terminal() {
        let _ = lines.next();
        return;
    }
    let Some(saved) = stty(&["-g"]) else {
        let _ = lines.next();
        return;
    };
    if stty(&["raw", "-echo"]).is_none() {
        let _ = lines.next();
        return;
    }
    // Through `io::stdin()` deliberately: it is the same buffer `lines`
    // reads from, so a byte cannot be stranded in one of two buffers. The
    // lock is reentrant, so holding `StdinLock` in `main` is not a deadlock.
    let mut byte = [0u8; 1];
    // Direct on fd 0, NOT through `io::stdin()`. Measured, not assumed:
    // with the tty in raw mode, `io::stdin().lock().read(&mut [0u8; 1])`
    // does NOT return on a single keypress -- a pty test sent one byte and
    // the read never came back -- while the same read on fd 0 returns
    // `Ok(1)` immediately. `io::stdin()` is a `BufReader`, so the cause is
    // somewhere in its fill path; the exact mechanism is not established
    // here and this comment does not claim one.
    // `ManuallyDrop` keeps fd 0 open when the handle goes away.
    let mut f = std::mem::ManuallyDrop::new(unsafe {
        <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(0)
    });
    let _ = f.read(&mut byte);
    stty(&[&saved]);
}

/// How many consecutive end-of-inputs on a TERMINAL are treated as Ctrl+D
/// before this port gives up and lets the caller exit.
///
/// ponytail: a plain counter, not tty liveness detection. On a live
/// terminal a read BLOCKS, so the count never advances and the ceiling is
/// never approached; it exists only so a terminal that goes away (the
/// window closes, the pty is torn down) ends the process instead of
/// spinning on instant `None`s forever. Raise it or replace it with a
/// `poll()` on fd 0 if a real case ever reaches it.
const EOF_RETRIES_ON_A_TTY: u32 = 1024;

/// Read one line, ignoring a terminal's end-of-input.
///
/// **Ctrl+D is a Unix key the original cannot see.** DOS has no such
/// keystroke, so not even DOS's own Ctrl+Z ends input there. Nothing the
/// player types can stop the original this way.
///
/// (Ctrl+C is the opposite case and is deliberately left alone: the
/// original aborts on Ctrl+C, and this port keeps that.)
///
/// On a terminal an end-of-input is therefore not an end of anything: it
/// is retried, which reads as "Ctrl+D did nothing". Off a terminal -- any
/// piped run -- `None` still means the input is genuinely exhausted and is
/// passed straight through, since that is what every caller's `else` arm
/// is written to end on.
pub fn read_line<I>(lines: &mut I) -> Option<io::Result<String>>
where
    I: Iterator<Item = io::Result<String>> + ?Sized,
{
    if !io::stdin().is_terminal() {
        return lines.next();
    }
    for _ in 0..EOF_RETRIES_ON_A_TTY {
        if let Some(line) = lines.next() {
            return Some(line);
        }
    }
    None
}

/// [`read_line`]'s policy for a caller holding a `BufRead` rather than the
/// line iterator -- `main`'s character creation. Returns the line with its
/// terminator intact, as `BufRead::read_line` does, because
/// `main::create_character` inspects that terminator.
pub fn read_line_raw(stdin: &mut impl io::BufRead) -> String {
    let mut buf = String::new();
    if !io::stdin().is_terminal() {
        let _ = stdin.read_line(&mut buf);
        return buf;
    }
    for _ in 0..EOF_RETRIES_ON_A_TTY {
        match stdin.read_line(&mut buf) {
            Ok(0) => continue, // Ctrl+D: not an end, on a terminal
            _ => return buf,
        }
    }
    buf
}

/// Run `stty` against this process's own terminal, returning its trimmed
/// stdout on success. `stdin` is inherited so it acts on the real tty;
/// only `stdout` is captured.
fn stty(args: &[&str]) -> Option<String> {
    // `output()` would give the child a NULL stdin, and `stty` acts on its
    // OWN stdin -- without this inherit it configures nothing at all.
    let out = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

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
