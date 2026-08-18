#!/usr/bin/env python3
"""The oracle must reproduce the original's own screens, twice the same way.

Asserting that dosbox-x exited cleanly would prove nothing: the emulator
exits 0 whether or not a single frame was captured. Every assertion here is
therefore on captured content -- text the original printed, cross-checked
against the independently extracted data/strings.json -- plus the two
properties later tasks lean on: the same script captures the same bytes, and
running the game never touches the read-only corpus in orig/.

These tests run in a fixed sequence via the `__main__` block at the bottom
of this file, not through pytest autodiscovery: test_expect_frames_guard
reads the SCREEN.BIN that test_capture writes, so running them out of order
(or in isolation, e.g. under pytest) will misbehave.
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


def test_run_wires_frame_count_guard():
    """run() must actually call _check_frame_count on the frames it decodes
    before returning, not just parse expect_frames and let it fall on the
    floor. test_expect_frames_guard above only tests the helper in
    isolation -- it would stay green even if run() stopped calling it. This
    exercises run() itself with Popen, the wait loop and decode_frames
    stubbed out, so it never launches the emulator.
    """
    out = pathlib.Path("/tmp/gopnik_oracle_run_stub")
    if out.exists():
        shutil.rmtree(out)

    class FakeProc:
        def poll(self):
            return 0

        def kill(self):
            pass

        def wait(self, timeout=None):
            pass

    def fake_popen(cmd, cwd=None, env=None, stdout=None, stderr=None):
        # run() only checks that SCREEN.BIN exists; its content is
        # irrelevant because decode_frames is stubbed below.
        (pathlib.Path(cwd) / "SCREEN.BIN").write_bytes(b"")
        return FakeProc()

    def fake_wait(proc, screen, deadline, settle):
        return None

    def fake_decode_frames(blob):
        return ["frame 0", "frame 1"]

    with unittest.mock.patch.object(capture.subprocess, "Popen", fake_popen), \
         unittest.mock.patch.object(capture, "_wait", fake_wait), \
         unittest.mock.patch.object(capture, "decode_frames", fake_decode_frames):
        try:
            capture.run(keys="\n", out_dir=out, expect_frames=3)
        except capture.OracleError as e:
            assert "expected 3" in str(e), f"unexpected message: {e}"
        else:
            raise AssertionError(
                "run() did not raise OracleError on a frame-count mismatch"
            )

        # A matching expect_frames must not raise, and still returns the
        # decoded frames.
        frames = capture.run(keys="\n", out_dir=out, expect_frames=2)
        assert frames == ["frame 0", "frame 1"]

    print("OK run() propagates OracleError from the frame-count guard")


def test_run_oracle_sh_forwards_extra_args():
    """run_oracle.sh must forward flags past the two positionals (`"$@"` in
    the exec line) to capture.py -- the fix from fix wave 2. Runs the real
    shell script, copied into a scratch dir next to a stub capture.py that
    just records sys.argv, so this never launches the emulator.

    run_oracle.sh resolves capture.py as "$(dirname "$0")/capture.py", so
    the stub has to live next to the *copy* of the script for this to
    actually exercise that resolution, not just call the real capture.py.
    """
    work = pathlib.Path("/tmp/gopnik_oracle_sh_stub")
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)

    shutil.copy2(capture.HERE / "run_oracle.sh", work / "run_oracle.sh")
    argv_dump = work / "argv.json"
    (work / "capture.py").write_text(
        "import json, sys, pathlib\n"
        f"pathlib.Path({str(argv_dump)!r}).write_text(json.dumps(sys.argv))\n"
    )

    subprocess.run(
        [
            "sh", str(work / "run_oracle.sh"),
            r"\n", "/tmp/gopnik_oracle_sh_stub_out",
            "--timeout", "7", "--expect-frames", "3",
        ],
        check=True,
    )

    argv = json.loads(argv_dump.read_text())
    assert "--timeout" in argv, f"--timeout not forwarded to capture.py: {argv}"
    assert argv[argv.index("--timeout") + 1] == "7", argv
    assert "--expect-frames" in argv, f"--expect-frames not forwarded: {argv}"
    assert argv[argv.index("--expect-frames") + 1] == "3", argv
    print("OK run_oracle.sh forwards extra args past keys/out_dir to capture.py")


def test_cli_threads_timeout_and_expect_frames():
    """main() must forward --timeout, --expect-frames and --seed to run(),
    not just parse and discard them (that was the bug fixed in fix wave 1:
    --timeout was parsed but never passed to run()). run() is stubbed out, so
    this never touches the emulator.
    """
    calls = []

    def fake_run(keys, out_dir, timeout=120, settle=3.0, expect_frames=None, seed=None):
        calls.append(
            {
                "keys": keys,
                "out_dir": out_dir,
                "timeout": timeout,
                "expect_frames": expect_frames,
                "seed": seed,
            }
        )
        return ["stub frame"]

    argv = [
        "capture.py",
        "--keys", r"\n",
        "--out", "/tmp/gopnik_oracle_cli_stub",
        "--timeout", "7",
        "--expect-frames", "3",
        "--seed", "0x1234",
    ]
    with unittest.mock.patch.object(capture, "run", fake_run), \
         unittest.mock.patch.object(sys, "argv", argv):
        rc = capture.main()

    assert rc == 0
    assert len(calls) == 1
    assert calls[0]["timeout"] == 7, f"--timeout not threaded into run(): {calls[0]}"
    assert calls[0]["expect_frames"] == 3, f"--expect-frames not threaded into run(): {calls[0]}"
    assert calls[0]["seed"] == 0x1234, f"--seed not threaded into run(): {calls[0]}"
    print("OK CLI threads --timeout, --expect-frames and --seed into run()")


def test_seed_pin_is_exact_and_refuses_a_wrong_binary():
    """The pin must be the same length as System.Randomize's body, must
    reproduce the two stores it replaces, and must refuse to patch anything
    that does not have that body where it expects it -- otherwise a silently
    misplaced patch would corrupt the game instead of pinning it.
    """
    patch = capture.pin_seed_patch(0xDEADBEEF)
    assert len(patch) == len(capture.RANDOMIZE_ORIGINAL), (
        f"pin is {len(patch)} bytes, Randomize's body is "
        f"{len(capture.RANDOMIZE_ORIGINAL)}; a different length would shift "
        "everything after it"
    )
    assert patch == bytes.fromhex("c7067e36efbec7068036addecb"), patch.hex()

    # orig/g.exe really does have that body where capture.py says it does.
    orig = ROOT.joinpath("orig", "g.exe").read_bytes()
    at = capture.RANDOMIZE_FILE_OFF
    assert orig[at : at + len(capture.RANDOMIZE_ORIGINAL)] == capture.RANDOMIZE_ORIGINAL

    scratch = OUT / "pin_seed_probe.exe"
    scratch.parent.mkdir(parents=True, exist_ok=True)
    scratch.write_bytes(orig)
    capture.pin_seed(scratch, 0)
    patched = scratch.read_bytes()
    assert len(patched) == len(orig), "the pin must not resize the image"
    assert patched[at : at + 13] == capture.pin_seed_patch(0)
    assert patched[:at] == orig[:at] and patched[at + 13 :] == orig[at + 13 :], (
        "the pin touched bytes outside Randomize's body"
    )
    # orig/ itself is never written to.
    assert ROOT.joinpath("orig", "g.exe").read_bytes() == orig

    scratch.write_bytes(b"\x00" * (at + 64))
    try:
        capture.pin_seed(scratch, 0)
    except capture.OracleError:
        pass
    else:
        raise AssertionError("pin_seed patched a binary that is not g.exe")
    print("OK seed pin is byte-exact, in place, and refuses a foreign binary")


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
    test_run_wires_frame_count_guard()
    test_run_oracle_sh_forwards_extra_args()
    test_cli_threads_timeout_and_expect_frames()
    test_cli_reports_oracle_error_cleanly()
    test_oracle_prompts_json_matches_source()
    test_scrhook_matches_source()
    test_seed_pin_is_exact_and_refuses_a_wrong_binary()
