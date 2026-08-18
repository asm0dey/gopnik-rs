#!/usr/bin/env python3
"""The oracle must reproduce the original's own screens, twice the same way.

Asserting that dosbox-x exited cleanly would prove nothing: the emulator
exits 0 whether or not a single frame was captured. Every assertion here is
therefore on captured content -- text the original printed, cross-checked
against the independently extracted data/strings.json -- plus the two
properties later tasks lean on: the same script captures the same bytes, and
running the game never touches the read-only corpus in orig/.
"""
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import unittest.mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import capture  # noqa: E402

ROOT = capture.ROOT
OUT = pathlib.Path("/tmp/gopnik_oracle_smoke")

# A whole game, start to quit: the intro walk, then the `e` command and the
# any-key it waits on before returning to DOS.
SCRIPT = capture.INTRO_KEYS + "e\n\n"

# The result screen the original prints on the way out. Fixed values for a
# freshly rolled "Пацан" -- character generation is not random, so these are
# invariants of the original, not a sample.
RESULT_LINES = [
    "Блин не быть тебе нормальным пацаном",
    "А результат:",
    "Ты Подтсан 0 уровня - Опущеный",
    "А зовут тебя:  Раздолбай",
    "Сейчас у тебя 0 опыта, А для прокачки надо 10",
    "Сл:3 Лв:3 Жв:3 Уд:3",
    "Урон 1-3",
    "Здоровье 28/28",
    "Точность 35%",
]


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def test_capture():
    if OUT.exists():
        shutil.rmtree(OUT)

    corpus_before = {p.name: md5(p) for p in sorted(ROOT.joinpath("orig").iterdir())}
    frames = capture.run(keys=SCRIPT, out_dir=OUT, timeout=90)

    # One frame per key the game asked for. The script feeds 14 keys and the
    # game asks a 15th time (the any-key before it quits), so the count is a
    # fixed property of this script, not a floor.
    assert len(frames) == 15, f"expected 15 frames, got {len(frames)}"

    assert "Версия 1.02" in frames[0], "frame 0 is not the title screen"
    assert "Нажми какую-нибудь кнопку" in frames[0]

    last = frames[-1]
    for line in RESULT_LINES:
        assert line in last, f"missing from the final screen: {line!r}"

    # The capture decodes CP866 the same way tools/extract_strings.py does:
    # these lines have to match the corpus extracted straight out of g.exe.
    corpus = {
        e["plain"].strip()
        for e in json.loads(
            ROOT.joinpath("data", "strings.json").read_text(encoding="utf-8")
        )
    }
    for line in ["Нажми какую-нибудь кнопку", "Блин не быть тебе нормальным пацаном"]:
        assert line in corpus, f"{line!r} is not in data/strings.json"

    work = OUT / "work"
    assert (work / "g.exe").exists(), "g.exe missing from the oracle workdir"
    assert (work / "SAVE_R0.SAV").stat().st_size == 694

    corpus_after = {p.name: md5(p) for p in sorted(ROOT.joinpath("orig").iterdir())}
    assert corpus_after == corpus_before, "a run modified the read-only corpus in orig/"
    assert corpus_before["g.exe"] == "10eb0af07a2d2f5e9da790df7058891c"

    first = (work / "SCREEN.BIN").read_bytes()

    # Determinism: same script, same bytes. The whole oracle is worthless if
    # two runs disagree, and nothing downstream would notice the drift.
    again = OUT / "again"
    if again.exists():
        shutil.rmtree(again)
    capture.run(keys=SCRIPT, out_dir=again, timeout=90)
    second = (again / "work" / "SCREEN.BIN").read_bytes()
    assert second == first, "two runs of the same script captured different screens"

    print(f"OK {len(frames)} frames captured, byte-identical across two runs")


def test_expect_frames_guard():
    """expect_frames must fail loud on a mismatch, not silently accept a
    truncated capture. Reuses the SCREEN.BIN test_capture already captured
    -- no extra emulator run, per the cost constraint on this test file.
    """
    screen = OUT / "work" / "SCREEN.BIN"
    frames = capture.decode_frames(screen.read_bytes())
    assert len(frames) == 15, f"expected 15 frames from test_capture, got {len(frames)}"

    log = OUT / "dosbox.log"
    try:
        capture._check_frame_count(frames, expect_frames=len(frames) + 1, log=log)
    except capture.OracleError as e:
        assert "expected" in str(e)
    else:
        raise AssertionError(
            "_check_frame_count did not raise OracleError on a frame-count mismatch"
        )

    # A correct count must not raise.
    capture._check_frame_count(frames, expect_frames=len(frames), log=log)
    capture._check_frame_count(frames, expect_frames=None, log=log)
    print("OK expect_frames mismatch raises OracleError")


def test_cli_threads_timeout_and_expect_frames():
    """main() must forward --timeout and --expect-frames to run(), not just
    parse and discard them (that was the bug fixed in fix wave 1: --timeout
    was parsed but never passed to run()). run() is stubbed out, so this
    never touches the emulator.
    """
    calls = []

    def fake_run(keys, out_dir, timeout=120, settle=3.0, expect_frames=None):
        calls.append(
            {"keys": keys, "out_dir": out_dir, "timeout": timeout, "expect_frames": expect_frames}
        )
        return ["stub frame"]

    argv = [
        "capture.py",
        "--keys", r"\n",
        "--out", "/tmp/gopnik_oracle_cli_stub",
        "--timeout", "7",
        "--expect-frames", "3",
    ]
    with unittest.mock.patch.object(capture, "run", fake_run), \
         unittest.mock.patch.object(sys, "argv", argv):
        rc = capture.main()

    assert rc == 0
    assert len(calls) == 1
    assert calls[0]["timeout"] == 7, f"--timeout not threaded into run(): {calls[0]}"
    assert calls[0]["expect_frames"] == 3, f"--expect-frames not threaded into run(): {calls[0]}"
    print("OK CLI threads --timeout and --expect-frames into run()")


def test_cli_reports_oracle_error_cleanly():
    """A run() failure must come back as a clean message and a nonzero exit,
    not a raw traceback out of main(). run() is stubbed, no emulator.
    """
    def raiser(*args, **kwargs):
        raise capture.OracleError("boom")

    argv = ["capture.py", "--keys", r"\n", "--out", "/tmp/gopnik_oracle_cli_stub2"]
    with unittest.mock.patch.object(capture, "run", raiser), \
         unittest.mock.patch.object(sys, "argv", argv):
        rc = capture.main()  # must not raise

    assert rc != 0, "main() should report failure via return code, not raise"
    print("OK CLI reports OracleError without a raw traceback")


def test_oracle_prompts_json_matches_source():
    """data/oracle_prompts.json must be exactly what capture.py's own
    INTRO_KEY_PROMPTS/COMMANDS constants generate -- it is a generated
    artifact, not a hand-maintained duplicate.
    """
    committed = json.loads(
        ROOT.joinpath("data", "oracle_prompts.json").read_text(encoding="utf-8")
    )
    assert committed["intro_keys"] == capture.INTRO_KEYS
    assert committed["intro_key_prompts"] == capture.INTRO_KEY_PROMPTS
    assert committed["commands"] == capture.COMMANDS
    print("OK data/oracle_prompts.json matches capture.py's INTRO_KEY_PROMPTS/COMMANDS")


def test_scrhook_matches_source():
    """The committed scrhook.com must be what scrhook.asm assembles to."""
    if shutil.which("nasm") is None:
        print("SKIP scrhook.com vs scrhook.asm: nasm not installed")
        return
    here = pathlib.Path(__file__).resolve().parent
    built = OUT / "scrhook.com"
    built.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["nasm", "-f", "bin", str(here / "scrhook.asm"), "-o", str(built)], check=True
    )
    assert built.read_bytes() == (here / "scrhook.com").read_bytes(), (
        "scrhook.com does not match scrhook.asm -- reassemble it"
    )
    print("OK scrhook.com matches scrhook.asm")


if __name__ == "__main__":
    test_capture()
    test_expect_frames_guard()
    test_cli_threads_timeout_and_expect_frames()
    test_cli_reports_oracle_error_cleanly()
    test_oracle_prompts_json_matches_source()
    test_scrhook_matches_source()
