#!/usr/bin/env bash
# Cross-compile for Windows and check the colour-policy layer under wine.
#
# WHAT THIS PROVES
#   * the `#[cfg(windows)]` branch in src/term.rs compiles and links at all --
#     nothing else in this repo ever builds it, so a typo there is otherwise
#     invisible until a player on Windows hits it;
#   * the Windows binary runs, and its colour POLICY decisions (strip when
#     piped, colour when forced, obey NO_COLOR) match the Linux build's
#     byte-for-byte.
#
# WHAT THIS CANNOT PROVE
#   * that ENABLE_VIRTUAL_TERMINAL_PROCESSING works. VT processing changes how
#     a console RENDERS bytes, not which bytes the program writes -- the same
#     escapes reach the pipe whether or not SetConsoleMode succeeded. That is
#     equally true on real Windows: no byte-capture test can verify it. It
#     needs a human looking at a cmd.exe window on a Windows build that does
#     not enable VT by default (Windows Terminal does, so it proves nothing).
#
# Requires: rustup target x86_64-pc-windows-gnu, mingw-w64 gcc, wine.
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET=x86_64-pc-windows-gnu
EXE=target/$TARGET/release/gopnik.exe
NATIVE=target/release/gopnik
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for tool in wine x86_64-w64-mingw32-gcc; do
    command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }
done

echo "building $TARGET..."
cargo build --release --target "$TARGET"
echo "building native..."
cargo build --release

# The two policy paths whose output is byte-comparable across targets.
for env_desc in "CLICOLOR_FORCE=1:forced colour" "NO_COLOR=1:no colour"; do
    var=${env_desc%%:*}
    desc=${env_desc#*:}
    env "$var" "$NATIVE"                       >"$tmp/native.out" 2>/dev/null
    env "$var" WINEDEBUG=-all wine "$EXE"      >"$tmp/win.out"    2>/dev/null
    if cmp -s "$tmp/native.out" "$tmp/win.out"; then
        echo "OK   $desc: Windows output is byte-identical to native"
    else
        echo "FAIL $desc: Windows and native output differ" >&2
        diff <(cat -A "$tmp/native.out") <(cat -A "$tmp/win.out") >&2 || true
        exit 1
    fi
done

# Piped output must carry no escape bytes at all.
if WINEDEBUG=-all wine "$EXE" 2>/dev/null | grep -q $'\x1b'; then
    echo "FAIL piped: escape bytes reached a non-tty destination" >&2
    exit 1
fi
echo "OK   piped: no escape bytes when stdout is not a terminal"
echo
echo "NOTE: the Windows VT path is still unverified -- see the header comment."
