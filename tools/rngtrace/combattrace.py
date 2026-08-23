#!/usr/bin/env python3
"""Fold per-run fight traces into `data/combat_trace.json`, the fight oracle.

    python3 tools/rngtrace/combattrace.py build/rngtrace/fight-{A,B,C,D}.json \
        --labels A,B,C,D --out data/combat_trace.json

`data/rng_trace.json` scores the draws of runs that never fight -- all five of
them decline or flee, and between them they contain zero `Random` sites inside
`[0x3d11, 0x584c)`.  `data/state_trace.json` scores those same runs' per-turn
state.  **Neither is read, written or regenerated here**, and this tool records
the SHA-256 of both so a reader can check that for themselves; it writes a
third file beside them, exactly as `statetrace.py` wrote the second.

What a run contributes:

  * `draws` -- `{i, turn, site, n, r}` in execution order, the same compact
    shape `data/rng_trace.json` uses, so one replay harness reads both.
  * `lines_the_game_read` -- the ordered input the guest's own `ReadLn`s
    consumed.  A fight capture needs two different answers (`y` at a question,
    `k` or `run` at `Битва\\`), so the port cannot feed one constant string the
    way `tests/wander_sequence.rs` does; it is fed this instead.  The list is
    cross-checked against the guest's 1000:441d stops before it is published
    (`fightrun.py`).
  * `fights` -- one record per `1000:3d11` stop, with the whole enemy record
    the fight was entered with.  That is a second channel on
    `FUN_1000_0d14`: the port must roll the same fighter, not merely spend the
    same draws.
  * `combat_prompts` -- one record per `1000:441d` stop: both fighters' hp and
    all four break flags at that moment.  This is what pins the jaw/leg break
    EFFECT.  Before Task 13 the only `broken_jaw`/`broken_leg` assertion in
    the suite was `tests/data_load.rs`'s check that a FRESH fighter has
    neither, so the break formulas at `1000:4564`..`1000:45ea` and
    `1000:4787`..`1000:4867` were recovered, documented, implemented -- and
    asserted by nothing.
  * `state_samples` / `final_state` -- the per-turn channel, unchanged from
    `run.py`.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools"))

FROZEN = ("data/rng_trace.json", "data/state_trace.json")


def compact_draws(trace):
    """The run's draw stream in `data/rng_trace.json`'s compact shape."""
    return [{"i": d["ordinal"], "turn": d["turn"],
             "site": "1000:%04x" % d["call_site_offset"],
             "n": d["n"], "r": d["result"]} for d in trace["draws"]]


def fight_records(trace, draws):
    """One record per fight, with the draws that fell inside it.

    `draws_before` comes from the parser (the number of draws logged when the
    `1000:3d11` stop happened); a fight's span therefore ends where the next
    one begins, and the last one runs to the end of the stream.  That is the
    per-fight draw count, derived from the markers rather than guessed from
    the sites.
    """
    out = []
    fights = trace["fights"]
    for i, f in enumerate(fights):
        start = f["draws_before"]
        end = fights[i + 1]["draws_before"] if i + 1 < len(fights) else len(draws)
        out.append({
            "index": f["index"],
            "turn": f["turn"],
            "first_draw_index": start,
            "draws_until_next_fight": end - start,
            "combat_prompts": f["prompts"],
            "enemy": f["enemy"],
        })
    return out


def run_record(label, path, trace):
    draws = compact_draws(trace)
    drive = trace["run"]["drive_log"]
    return {
        "label": label,
        "trace_file": path,
        "seed_hex": trace["seed_hex"],
        # The guest's own DS:389c, never the CLI's --class-answer; see
        # driver.class_record for why that distinction is load-bearing.
        "class_value": trace["final_state"]["class_389c"],
        "loaded_save": trace["run"]["creation"].get("loaded_save", False),
        "district_key": trace["run"]["district_key"],
        "saves_copied": trace["run"].get("saves_copied", []),
        "combat_answer": trace["run"]["combat_answer"],
        "walks_requested": trace["run"]["walks_requested"],
        "turns_completed": drive["turns_completed"],
        "guest_left_the_game": drive["guest_left_the_game"],
        "ended_at_turn_marker": trace["run"]["ended_at_turn_marker"],
        "prompt_stops": trace["run"]["prompt_stops"],
        "lines_the_game_read": drive["lines_the_game_read"],
        "load_base": trace["load_base"],
        "verification": trace["verification"],
        "final_state": trace["final_state"],
        "fights": fight_records(trace, draws),
        "combat_prompts": [{"index": r["index"], "turn": r["turn"],
                            "fight": r["fight"],
                            "draws_before": r["draws_before"],
                            **r["values"]} for r in trace["combat_prompts"]],
        "samples": [dict(turn=s["turn"], **s["values"])
                    for s in trace["state_samples"]],
        "draws": draws,
    }


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def build(traces, labels, paths):
    runs = [run_record(lab, p, t) for lab, p, t in zip(labels, paths, traces)]
    first = traces[0]
    return {
        "note": ("Live Random trace of FIGHTS in orig/g.exe under qemu+gdb "
                 "with RandSeed pinned by patching a COPY of the binary.  "
                 "Every number here is read out of the original or out of the "
                 "guest's own memory; nothing comes from src/.  Produced by "
                 "tools/rngtrace/combattrace.py from tools/rngtrace/fightrun.py "
                 "runs; method and addresses: docs/re/rng-trace.md and "
                 "docs/re/combat.md."),
        "harness": "tools/rngtrace (python3 tools/rngtrace/fightrun.py)",
        "source": "orig/g.exe md5 10eb0af07a2d2f5e9da790df7058891c",
        "seed_patch": first["seed_patch"],
        "observation_point": first["observation_point"],
        "turn_marker": first["turn_marker"],
        "fight_marker": first["fight_marker"],
        "round_marker": first["round_marker"],
        "why_a_third_file": (
            "data/rng_trace.json (1387 draws) and data/state_trace.json are "
            "frozen oracles and are never regenerated.  Neither was read, "
            "written or touched to produce this file; their SHA-256 digests "
            "are recorded below so that is checkable rather than asserted."),
        "frozen_oracles": {p: digest(REPO / p) for p in FROZEN},
        "input_policy": (
            "A question is answered `y` -- the ACCEPT arm of the literal-`y` "
            "compare at 1000:b548 / 1000:b696 / 1000:b718 (file 0x9BF3).  The "
            "`Битва\\` prompt is answered with the run's own `combat_answer`: "
            "`k` (1000:4440) fights, `run` (1000:48e1) flees.  Because that is "
            "two different strings, each run records the ordered list of lines "
            "the game's own ReadLns consumed, and a replay is fed exactly that "
            "list rather than one constant string."),
        "granularity_limit": (
            "`combat_prompts` samples at 1000:441d, i.e. once per `Битва\\` "
            "prompt.  It shows what a ROUND left behind, never the order of "
            "changes inside the round: a limb broken and an hp change within "
            "one round arrive together here.  The DRAW stream is the channel "
            "with intra-round order."),
        "draws_total": sum(len(r["draws"]) for r in runs),
        "fights_total": sum(len(r["fights"]) for r in runs),
        "combat_prompts_total": sum(len(r["combat_prompts"]) for r in runs),
        "runs": runs,
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("traces", nargs="+", help="per-run trace JSON from fightrun.py")
    ap.add_argument("--labels", required=True, help="comma-separated run labels")
    ap.add_argument("--out", default=None)
    args = ap.parse_args(argv)

    labels = args.labels.split(",")
    if len(labels) != len(args.traces):
        ap.error("%d labels for %d traces" % (len(labels), len(args.traces)))
    traces = [json.loads(Path(p).read_text()) for p in args.traces]
    for lab, t in zip(labels, traces):
        if not t["fights"]:
            ap.error("run %s has no fight in it" % lab)

    out = build(traces, labels, args.traces)
    text = json.dumps(out, indent=2, ensure_ascii=False) + "\n"
    if args.out:
        Path(args.out).write_text(text)
    else:
        print(text)
    for r in out["runs"]:
        print("run %s: %d draws, %d fights, %d combat prompts, answer %r, "
              "%d/%d turns%s"
              % (r["label"], len(r["draws"]), len(r["fights"]),
                 len(r["combat_prompts"]), r["combat_answer"],
                 r["turns_completed"], r["walks_requested"],
                 " (guest left the game)" if r["guest_left_the_game"] else ""),
              file=sys.stderr)
        for f in r["fights"]:
            print("    fight %d: turn %d, %d draws, %d prompts, enemy class %d "
                  "level %d hp %d"
                  % (f["index"], f["turn"], f["draws_until_next_fight"],
                     f["combat_prompts"], f["enemy"]["e_class_3952"],
                     f["enemy"]["e_level_395c"], f["enemy"]["e_hp_3962"]),
                  file=sys.stderr)
    print("total %d draws, %d fights across %d runs -> %s"
          % (out["draws_total"], out["fights_total"], len(out["runs"]),
             args.out or "stdout"), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
