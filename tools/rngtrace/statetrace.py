#!/usr/bin/env python3
"""Fold per-run traces into `data/state_trace.json`, the per-turn state oracle.

    python3 tools/rngtrace/statetrace.py build/rngtrace/state{A,B,C,D,E}.json \
        --labels A,B,C,D,E --out data/state_trace.json

`data/rng_trace.json` scores DRAWS.  This file scores the STATE the guest held
at every turn marker (`1000:ae63`, the top-level prompt's `ReadLn`): one sample
per turn of every variable `run.state_fields()` names, read out of the guest's
own memory while it was stopped there.  The two are separate files ON PURPOSE:
`data/rng_trace.json` is a frozen oracle of 1387 draws and is never
regenerated, so a state capture writes beside it and never over it.

**Alignment is checked, not assumed.**  A state sample is only usable against
the frozen draw stream if the run it came from spent the same draws in the same
order.  Every run folded here is therefore compared, draw for draw, against the
run of the same label in `data/rng_trace.json`; a single difference in count,
site, `n` or result stops the fold.  Without that check the two files could
describe two different histories while looking like one.

Granularity limit, stated here because a consumer can otherwise read more into
the file than it holds: a sample pair shows a turn's NET effect on these
variables.  It does not show the order in which they changed inside the turn,
and a variable that moved and moved back inside one turn is invisible here.
"""
import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools"))

if __package__ in (None, ""):
    from rngtrace import run as runmod
else:
    from . import run as runmod


class AlignmentError(RuntimeError):
    """A run's draws do not match the frozen oracle's, so its state samples
    cannot be keyed to that draw stream."""


def compact_draws(trace):
    """The run's own draw stream in `data/rng_trace.json`'s compact shape."""
    return [{"i": d["ordinal"], "turn": d["turn"],
             "site": "1000:%04x" % d["call_site_offset"],
             "n": d["n"], "r": d["result"]} for d in trace["draws"]]


def check_alignment(label, trace, committed):
    """Every draw of this capture must equal the frozen oracle's, in order.

    Returns the evidence (counts, and that they matched); raises rather than
    recording a mismatch, because a state trace keyed to a draw stream that is
    not the committed one is not a second channel on the same history -- it is
    a different history wearing the same labels.
    """
    got = compact_draws(trace)
    want = committed["draws"]
    if len(got) != len(want):
        raise AlignmentError(
            "run %s: this capture logged %d draws, data/rng_trace.json's run "
            "%s has %d -- the state samples cannot be keyed to it"
            % (label, len(got), label, len(want)))
    for a, b in zip(got, want):
        if (a["site"], a["n"], a["r"], a["turn"]) != (b["site"], b["n"], b["r"], b["turn"]):
            raise AlignmentError(
                "run %s: draw %d differs from data/rng_trace.json (%s n=%s r=%s "
                "turn=%s here, %s n=%s r=%s turn=%s there)"
                % (label, a["i"], a["site"], a["n"], a["r"], a["turn"],
                   b["site"], b["n"], b["r"], b["turn"]))
    return {"draws_compared": len(got),
            "equals_rng_trace_draws": True,
            "rng_trace_run": label}


def run_record(label, path, trace, committed):
    alignment = check_alignment(label, trace, committed)
    samples = trace["state_samples"]
    return {
        "label": label,
        "trace_file": path,
        "seed_hex": trace["seed_hex"],
        # The class is the guest's own DS:389c, never the CLI's --class-answer;
        # see driver.class_record for why that distinction is load-bearing.
        "class_value": trace["final_state"]["class_389c"],
        "loaded_save": trace["run"]["creation"].get("loaded_save", False),
        "district_key": trace["run"]["district_key"],
        "saves_copied": trace["run"].get("saves_copied", []),
        "walks_requested": trace["run"]["walks_requested"],
        "prompt_stops": trace["run"]["prompt_stops"],
        "load_base": trace["load_base"],
        "verification": trace["verification"],
        "alignment_with_rng_trace": alignment,
        "final_state": trace["final_state"],
        "samples": [dict(turn=s["turn"], **s["values"]) for s in samples],
    }


def build(traces, labels, paths, committed_by_label):
    for lab in labels:
        if lab not in committed_by_label:
            raise AlignmentError(
                "data/rng_trace.json has no run %s to align against" % lab)
    runs = [run_record(lab, p, t, committed_by_label[lab])
            for lab, p, t in zip(labels, paths, traces)]
    first = traces[0]
    return {
        "note": ("Per-turn state of orig/g.exe under qemu+gdb with RandSeed "
                 "pinned by patching a COPY of the binary.  Every sample is "
                 "read out of the guest's own memory at the turn marker; "
                 "nothing here comes from src/.  Produced by "
                 "tools/rngtrace/statetrace.py; method and addresses: "
                 "docs/re/rng-trace.md, `The per-turn state channel`."),
        "harness": "tools/rngtrace (python3 tools/rngtrace/run.py)",
        "source": "orig/g.exe md5 10eb0af07a2d2f5e9da790df7058891c",
        "seed_patch": first["seed_patch"],
        "turn_marker": first["turn_marker"],
        "state_channel": first["state_channel"],
        "granularity_limit": (
            "One sample per TURN, taken with the guest stopped at 1000:ae63.  "
            "A pair of samples shows what a turn did to these variables in "
            "net; it never shows the ORDER of changes inside the turn, and a "
            "value that moved and moved back within one turn leaves no trace "
            "here at all."),
        "alignment": (
            "Every run's draw stream was compared draw for draw against the "
            "run of the same label in data/rng_trace.json and found equal, so "
            "a sample's `turn` indexes the same turns that file's draws carry. "
            "data/rng_trace.json itself is untouched by this tool."),
        "samples_total": sum(len(r["samples"]) for r in runs),
        "runs": runs,
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("traces", nargs="+", help="per-run trace JSON from run.py")
    ap.add_argument("--labels", required=True,
                    help="comma-separated run labels, matching data/rng_trace.json")
    ap.add_argument("--rng-trace", default=str(REPO / "data" / "rng_trace.json"),
                    help="the frozen draw oracle to align against (READ ONLY)")
    ap.add_argument("--out", default=None)
    args = ap.parse_args(argv)

    labels = args.labels.split(",")
    if len(labels) != len(args.traces):
        ap.error("%d labels for %d traces" % (len(labels), len(args.traces)))
    traces = [json.loads(Path(p).read_text()) for p in args.traces]
    committed = json.loads(Path(args.rng_trace).read_text())
    by_label = {r["label"]: r for r in committed["runs"]}

    out = build(traces, labels, args.traces, by_label)
    text = json.dumps(out, indent=2, ensure_ascii=False) + "\n"
    if args.out:
        Path(args.out).write_text(text)
    else:
        print(text)
    for r in out["runs"]:
        print("run %s: %d samples over %d walks, %d draws aligned with "
              "data/rng_trace.json"
              % (r["label"], len(r["samples"]), r["walks_requested"],
                 r["alignment_with_rng_trace"]["draws_compared"]),
              file=sys.stderr)
    print("total %d samples across %d runs -> %s"
          % (out["samples_total"], len(out["runs"]), args.out or "stdout"),
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
