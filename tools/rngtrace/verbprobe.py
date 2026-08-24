#!/usr/bin/env python3
"""Which typed verbs reach `FUN_1000_1a03`, and which provably do not.

    python3 tools/rngtrace/verbprobe.py --boot-img build/rngtrace/boot.img \
        --out build/rngtrace/verbprobe.json

Task 16, Step 1.  `docs/superpowers/RESUME.md` carried an `unverified`
hypothesis that `1000:1a03` is the body behind `stats` from the main loop and
`sv` from combat.  This tool settles it from FLOW: three breakpoints
(`gdbsession.build_verbprobe_script`), a scripted list of verbs, and a per-verb
count of how many times `1000:1a03` was entered in that verb's window.

**The negative is the point.**  A run showing only that two verbs reach the
function cannot tell that apart from "everything reaches it", so the plan types
verbs that are expected NOT to reach it and the report states their stop counts
too -- zero being an observation, not an omission.

Attribution, and what makes it sound.  gdb stops at the ReadLn CALL, before the
line is read, so the guest announces prompt `i` and only then does the driver
type at it; every `T` between prompt stop `i` and prompt stop `i+1` is the work
of the line typed at prompt `i`.  That is only true if the driver's screen
classification agreed with the guest's own breakpoints, so it is checked rather
than assumed: the ordered kinds the driver typed at (`street`/`combat`) must
match the ordered marker kinds (`P`/`C`) position by position, and the marker
stream may run at most one prompt longer (the prompt the guest came back to
after the last line).  A mismatch fails the run -- the same guard shape
`fightrun.py` uses for `lines_the_game_read`.

Nothing here is a frozen oracle: this file is a probe, and its output is a
report, not a replay input.  It writes only where `--out` says.
"""
import argparse
import json
import shutil
import sys
import time
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from rngtrace import (driver, gdbsession, loadbase, run as runmod,
                          seedpatch, tracelog, vm)
else:
    from . import driver, gdbsession, loadbase, seedpatch, tracelog, vm
    from . import run as runmod

REPO = Path(__file__).resolve().parents[2]

# The default plan.  Every entry is a verb the dispatcher really compares
# against, re-derived from the literal each `call 0f78:0bd8` pushes:
#
#   street, buffer DS:3972 -- `s` at 1000:ec82 (literal at CS:9f85), `i` at
#     1000:ea94 (CS:a70e), `w` at 1000:ae86 (CS:848e).
#   combat, buffer DS:3a72 -- `s` at 1000:4c2e (CS:359f), `sv` at 1000:4c42
#     (CS:35a1), `run` at 1000:48e1 (CS:33bb).
#
# `stats` is in the plan precisely because the byte `stats` appears NOWHERE in
# the image, so it is a verb the dispatcher cannot match: it exercises the
# unknown-command fall-through, which is the strongest negative available.
DEFAULT_STREET_PLAN = ["s", "stats", "i", "w", "s", "w"]
DEFAULT_COMBAT_PLAN = ["sv", "sv", "s", "run"]
STREET_FILLER = "w"
COMBAT_FILLER = "run"


class ProbeError(RuntimeError):
    pass


def parse_probe_log(text: str) -> dict:
    """The marker stream, in order, plus everything that must fail a run.

    Deliberately its own parser rather than `tracelog.parse`: that function
    reads the frozen oracles' logs and knows nothing of the `T` tag, and
    widening it would put a Task 16 probe on the path that produced
    `data/rng_trace.json`.  The gdb-noise filters ARE reused, so "what counts
    as noise" stays defined in one place.
    """
    markers = []
    unexpected = []
    aborts = []
    bp_lines = []
    unparsed = []
    ready = None
    pending = None
    for raw in text.splitlines():
        line = tracelog.strip_style(tracelog.strip_gdb_noise(raw))
        if not line:
            pending = None
            continue
        if pending == "abort":
            aborts[-1]["message"] = line
            pending = None
            continue
        if pending == "warning":
            pending = None
            continue
        m = tracelog.RE_READY.match(line)
        if m:
            ready = {"image_base": int(m.group(1), 16)}
            continue
        if line in ("P", "C", "T"):
            markers.append(line)
            continue
        m = tracelog.RE_UNEXPECTED.match(line)
        if m:
            unexpected.append(int(m.group(1), 16))
            markers.append("?")
            continue
        if tracelog.RE_ABORT.search(line):
            aborts.append({"header": line, "message": None,
                           "markers_before": len(markers)})
            pending = "abort"
            continue
        if tracelog.RE_WARNING.match(line):
            pending = "warning"
            continue
        if line.startswith("Breakpoint ") or line.startswith("Num "):
            bp_lines.append(line)
            continue
        if any(p.match(line) for p in tracelog.GDB_NOISE):
            continue
        unparsed.append(line)
    return {"ready": ready, "markers": "".join(markers),
            "unexpected_stops": unexpected, "script_aborts": aborts,
            "breakpoint_lines": bp_lines, "unparsed": unparsed}


def drive(machine, street_plan, combat_plan, street_filler, combat_filler,
          want_combat, max_walks, max_actions):
    """Type the plan, recording what was typed at which kind of prompt.

    Returns `(typed, info)` where `typed` is the ordered
    `[{"kind": "street"|"combat", "line": str}, ...]` -- exactly the lines the
    guest's own ReadLns consumed, and nothing else.  Enter at an any-key page
    is not one of them (no ReadLn of the game's reads it) and `y` at an
    encounter question is not either: the question is read by neither
    `1000:ae63` nor `1000:441d`, so it has no marker to align against and is
    logged separately.
    """
    typed = []
    other = []
    si = ci = 0
    walks = 0
    actions = 0
    exited = False
    while True:
        actions += 1
        if actions > max_actions:
            raise ProbeError("the driver used %d actions and never finished the "
                             "plan; last screen:\n%s"
                             % (actions, machine.screen()))
        screen = driver.settled_screen(machine)
        if driver.game_gone(screen):
            exited = True
            break
        what = driver.classify(screen)
        if what == "street":
            combat_prompts = sum(1 for t in typed if t["kind"] == "combat")
            done = (si >= len(street_plan) and ci >= len(combat_plan)
                    and combat_prompts >= want_combat)
            if done or walks >= max_walks:
                break
            line = street_plan[si] if si < len(street_plan) else street_filler
            si += 1
            if line == "w":
                walks += 1
            machine.type(line + "\n")
            typed.append({"kind": "street", "line": line})
        elif what == "combat":
            line = combat_plan[ci] if ci < len(combat_plan) else combat_filler
            ci += 1
            machine.type(line + "\n")
            typed.append({"kind": "combat", "line": line})
        elif what == "question":
            machine.type(driver.ACCEPT + "\n")
            other.append({"prompt": "question", "typed": driver.ACCEPT})
        else:
            machine.type("\n")
            other.append({"prompt": "other", "typed": "<enter>"})
    return typed, {"actions": actions, "walks": walks,
                   "street_plan_consumed": si, "combat_plan_consumed": ci,
                   "guest_left_the_game": exited,
                   "lines_typed_elsewhere": other}


def align(typed, markers):
    """Attribute every `T` to the line typed at the prompt that precedes it.

    Raises if the driver's classification and the guest's markers disagree,
    which is the one way this attribution could be quietly wrong.
    """
    prompts = [m for m in markers if m in ("P", "C")]
    kinds = ["street" if m == "P" else "combat" for m in prompts]
    typed_kinds = [t["kind"] for t in typed]
    if len(prompts) < len(typed):
        raise ProbeError(
            "the driver typed %d line(s) at prompts but the guest stopped at a "
            "prompt ReadLn only %d time(s): the two streams cannot be aligned, "
            "so no verb may be credited or cleared"
            % (len(typed), len(prompts)))
    if len(prompts) > len(typed) + 1:
        raise ProbeError(
            "the guest stopped at a prompt ReadLn %d time(s) for %d line(s) "
            "typed: more than the one trailing prompt the guest comes back to, "
            "so a prompt was answered by something this driver did not record"
            % (len(prompts), len(typed)))
    for i, (a, b) in enumerate(zip(typed_kinds, kinds)):
        if a != b:
            raise ProbeError(
                "prompt %d: the driver classified the screen as %s and typed "
                "%r, but the guest's own breakpoint says it was the %s prompt"
                % (i + 1, a, typed[i]["line"], b))
    # Walk the marker stream, crediting each T to the open prompt window.
    windows = []
    cur = None
    pre = 0
    for m in markers:
        if m in ("P", "C"):
            if cur is not None:
                windows.append(cur)
            cur = {"index": len(windows) + 1,
                   "kind": "street" if m == "P" else "combat",
                   "sheet_entries": 0}
        elif m == "T":
            if cur is None:
                pre += 1
            else:
                cur["sheet_entries"] += 1
    if cur is not None:
        windows.append(cur)
    for i, t in enumerate(typed):
        windows[i]["line"] = t["line"]
    for w in windows[len(typed):]:
        w["line"] = None
        w["note"] = ("the prompt the guest came back to after the last line "
                     "was typed; nothing was typed at it")
    return {"windows": windows, "sheet_entries_before_first_prompt": pre}


def tally(windows):
    """Per `(kind, verb)`: how many prompts typed it, and how many entries."""
    out = {}
    for w in windows:
        if w.get("line") is None:
            continue
        key = "%s:%s" % (w["kind"], w["line"])
        rec = out.setdefault(key, {"prompt_kind": w["kind"], "line": w["line"],
                                   "prompts": 0, "sheet_entries": 0})
        rec["prompts"] += 1
        rec["sheet_entries"] += w["sheet_entries"]
    for rec in out.values():
        rec["reaches_1000_1a03"] = rec["sheet_entries"] > 0
    return dict(sorted(out.items()))


def check_log(parsed, min_prompts=1):
    if parsed["ready"] is None:
        raise ProbeError("gdb never reported READY: the breakpoints were never "
                         "installed, so nothing here is evidence")
    accepted = [l for l in parsed["breakpoint_lines"] if l.startswith("Breakpoint ")]
    if len(accepted) < 3:
        raise ProbeError("expected 3 breakpoints to be accepted, log shows %d: %r"
                         % (len(accepted), parsed["breakpoint_lines"]))
    # The harness's own shutdown aborts the sourced script on purpose (the VM
    # is killed first so gdb's pending `continue` fails), so the discriminator
    # is the MESSAGE and the position, exactly as `tracelog.check_script_abort`
    # has it -- a command error inside the loop leaves the guest stopped at a
    # breakpoint and every other guard still passing.
    aborts = parsed["script_aborts"]
    if len(aborts) > 1:
        raise ProbeError("the gdb script aborted %d times: %r"
                         % (len(aborts), aborts))
    if aborts:
        a = aborts[0]
        if a["message"] != tracelog.EXPECTED_ABORT_MESSAGE:
            raise ProbeError(
                "the gdb probe loop aborted with %r, not the deliberate "
                "shutdown (%r): the marker stream stops there, so a missing T "
                "means nothing"
                % (a["message"], tracelog.EXPECTED_ABORT_MESSAGE))
        if parsed["markers"][a["markers_before"]:]:
            raise ProbeError(
                "markers logged after the gdb script aborted (%r): the abort "
                "is supposed to be the last thing that happens"
                % parsed["markers"][a["markers_before"]:])
    if parsed["unexpected_stops"]:
        raise ProbeError("the guest stopped at an address no breakpoint was set "
                         "at: %r" % [hex(x) for x in parsed["unexpected_stops"]])
    if parsed["unparsed"]:
        raise ProbeError("unparsed gdb output -- a line the reader did not "
                         "recognise may be a dropped marker: %r"
                         % parsed["unparsed"][:5])
    if parsed["markers"].count("P") + parsed["markers"].count("C") < min_prompts:
        raise ProbeError("only %d prompt stop(s) in the whole run"
                         % (parsed["markers"].count("P")
                            + parsed["markers"].count("C")))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--boot-img", required=True)
    ap.add_argument("--exe", default=str(REPO / "orig" / "g.exe"))
    ap.add_argument("--workdir", default=str(REPO / "build" / "rngtrace" / "verbprobe"))
    ap.add_argument("--seed", default=hex(runmod.DEFAULT_SEED))
    ap.add_argument("--class-answer", type=int, default=0, choices=[0, 1, 2, 3])
    ap.add_argument("--district", default="1")
    ap.add_argument("--street-plan", default=",".join(DEFAULT_STREET_PLAN))
    ap.add_argument("--combat-plan", default=",".join(DEFAULT_COMBAT_PLAN))
    ap.add_argument("--want-combat-prompts", type=int, default=len(DEFAULT_COMBAT_PLAN))
    ap.add_argument("--max-walks", type=int, default=60)
    ap.add_argument("--max-actions", type=int, default=1200)
    ap.add_argument("--gdb-port", type=int, default=1234)
    ap.add_argument("--sock-dir", default="/tmp")
    ap.add_argument("--out", default=None)
    args = ap.parse_args(argv)

    street_plan = [v for v in args.street_plan.split(",") if v]
    combat_plan = [v for v in args.combat_plan.split(",") if v]
    seed = int(args.seed, 0)
    work = Path(args.workdir)
    if work.exists():
        shutil.rmtree(work)
    gamedir = work / "gamedir"
    gamedir.mkdir(parents=True)
    patch = seedpatch.write_patched_copy(args.exe, gamedir / "G.EXE", seed)
    exe = (gamedir / "G.EXE").read_bytes()

    machine = vm.Vm(args.boot_img, gamedir, work, sock_dir=args.sock_dir,
                    gdb_port=args.gdb_port)
    gdb = None
    try:
        machine.start()
        driver.boot_to_dos(machine)
        driver.launch_game(machine)

        mem = machine.dump_memory()
        base_info = loadbase.derive(mem, exe)
        base = base_info["image_base"]
        checks, randseed_at_attach = loadbase.verify_guest_code(
            mem, exe, base, seed, seedpatch.build_patch(seed))

        script = work / "probe.gdb"
        log = work / "probe.gdb.log"
        script.write_text(gdbsession.build_verbprobe_script(base, args.gdb_port))
        gdb = gdbsession.GdbSession(script, log).start()
        gdb.wait_ready()

        creation = driver.create_character(machine, args.class_answer,
                                           district=args.district)
        log_before = log.stat().st_size
        screen_before = machine.screen()
        typed, info = drive(machine, street_plan, combat_plan, STREET_FILLER,
                            COMBAT_FILLER, args.want_combat_prompts,
                            args.max_walks, args.max_actions)
        time.sleep(1.5)
        screen_after = machine.screen()
        if not gdb.alive():
            raise ProbeError("gdb exited during the run; the marker stream is "
                             "truncated:\n%s"
                             % log.read_text(errors="replace")[-2000:])
        if log.stat().st_size <= log_before:
            raise ProbeError("no marker output while driving -- the tracer "
                             "stopped, so a missing T proves nothing")
        # The guest is idle in a ReadLn here (running, not stopped), so a
        # second dump is safe.  It is read for two reasons: RandSeed is the
        # second half of the progress guard, and `final_state` records the
        # state the sheet was rendered against -- which is what makes the
        # per-verb result readable as "this character, these flags".
        dump = machine.dump_memory()
        final_state = runmod.read_state(dump, base)
    finally:
        machine.kill()
        if gdb is not None:
            gdb.stop()

    parsed = parse_probe_log(log.read_text(errors="replace"))
    if parsed["ready"] is not None and parsed["ready"]["image_base"] != base:
        raise ProbeError("gdb attached at a different base than derived")
    check_log(parsed)
    progress = tracelog.check_guest_progressed(
        screen_before, screen_after, randseed_at_attach,
        final_state["randseed_367e"])
    aligned = align(typed, parsed["markers"])
    result = {
        "note": ("Task 16 Step 1: which typed verbs enter FUN_1000_1a03.  "
                 "Live, under qemu+gdb, against orig/g.exe with RandSeed "
                 "pinned by patching a COPY.  Nothing here comes from src/, "
                 "and this file is a report, not a replay oracle."),
        "harness": "tools/rngtrace/verbprobe.py",
        "seed": seed,
        "seed_hex": "0x%08X" % seed,
        "seed_patch": patch,
        "load_base": base_info,
        "runtime_checks": checks,
        "guest_progressed": progress,
        "final_state": final_state,
        "markers": {
            "P": {"ghidra": "1000:ae63",
                  "what": "the top-level prompt's ReadLn (9a c6 06 78 0f)"},
            "C": {"ghidra": "1000:441d",
                  "what": "the `Битва\\` prompt's ReadLn (9a c6 06 78 0f)"},
            "T": {"ghidra": "1000:1a03",
                  "what": "FUN_1000_1a03's prologue (55 89 e5)"},
        },
        "run": {
            # The class comes from the guest's own DS:389c, never from the CLI
            # answer -- `driver.class_record` is the one place that rule lives.
            **driver.class_record(args.class_answer,
                                  bool(creation.get("loaded_save")),
                                  final_state["class_389c"]),
            "district_key": args.district,
            "creation": creation,
            "street_plan": street_plan,
            "combat_plan": combat_plan,
            "street_filler": STREET_FILLER,
            "combat_filler": COMBAT_FILLER,
            **info,
        },
        "marker_stream": parsed["markers"],
        "prompt_windows": aligned["windows"],
        "sheet_entries_before_first_prompt":
            aligned["sheet_entries_before_first_prompt"],
        "per_verb": tally(aligned["windows"]),
        "totals": {
            "prompt_stops_street": parsed["markers"].count("P"),
            "prompt_stops_combat": parsed["markers"].count("C"),
            "sheet_entries": parsed["markers"].count("T"),
            "lines_typed": len(typed),
        },
    }
    out = Path(args.out) if args.out else work / "verbprobe.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    for key, rec in result["per_verb"].items():
        print("%-14s prompts=%-3d entries_1a03=%-3d %s"
              % (key, rec["prompts"], rec["sheet_entries"],
                 "REACHES" if rec["reaches_1000_1a03"] else "does NOT reach"))
    print("-> %s" % out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
