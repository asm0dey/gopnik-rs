#!/usr/bin/env python3
"""Compare an observed draw trace against the static catalogue in data/wander.json.

Verdicts, per catalogued draw:
  corroborated  -- observed at the catalogued call site with the catalogued `n`
  not observed  -- never fired in these runs (the gate says why)
  contradicted  -- observed at that site, but not as catalogued

A contradiction is a finding, not a failure: it is reported with BOTH readings
and never silently reconciled.  Draws observed at sites the catalogue does not
list are reported separately, split by whether they fall inside the preamble
range the catalogue claims to enumerate completely (1000:ae5a..1000:b3ba) or
downstream of the bucket dispatch, which docs/re/wander.md puts out of scope.
"""
import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools"))

import addr  # noqa: E402  -- the address convention, defined once

PREAMBLE_LO = 0xAE5A
PREAMBLE_HI = 0xB3BA


# The four class growth weights live at DS:(class*4 + 2 .. class*4 + 5); DS is
# Ghidra 20ae.  The arithmetic comes from tools/addr.py, never restated here.
# Read straight out of orig/g.exe rather than taken from another data file.
DS_FILE_OFF = addr.file_off_of_image_off(addr.DATA_SEG_IMAGE_OFF)

# The stored class value is the creation menu answer + 3 (1000:71b8).  Kept in
# step with driver.CLASS_NAME_BY_VALUE by a test; this module is run as a
# script, so it does not import the package.
CLASS_NAME_BY_VALUE = {3: "\u041f\u0430\u0446\u0430\u043d",
                       4: "\u041e\u0442\u043c\u043e\u0440\u043e\u0437\u043e\u043a",
                       5: "\u0413\u043e\u043f\u043d\u0438\u043a",
                       6: "\u0412\u043e\u0440"}


def class_weight_sum(exe: bytes, cls: int) -> int:
    at = DS_FILE_OFF + cls * 4 + 2
    return sum(exe[at:at + 4])


def parse_addr(a):
    """`1000:b353` -> 0xb353.  Only segment 1000 sites are catalogued here."""
    seg, off = a.split(":")
    return seg, int(off, 16)


def catalogue(wander):
    """Every catalogued draw, in ordinal order, flattened."""
    out = []
    for step in wander["steps"]:
        if step.get("kind") != "draw":
            continue
        seg, off = parse_addr(step["at"])
        out.append({
            "draw_ordinal": step["draw_ordinal"],
            "at": step["at"], "segment": seg, "site_offset": off,
            "n": step.get("n"), "n_expr": step.get("n_expr"),
            "gate": (step.get("gate") or {}).get("cond"),
            "where": "preamble",
        })
    church = wander["nested_routines"]["church"]
    for d in church["draws"]:
        seg, off = parse_addr(d["at"])
        out.append({
            "draw_ordinal": d["draw_ordinal"],
            "at": d["at"], "segment": seg, "site_offset": off,
            "n": d.get("n"), "n_expr": d.get("n_expr"),
            "gate": (d.get("gate") or {}).get("cond") if d.get("gate") else None,
            "where": "church",
        })
    out.sort(key=lambda d: d["draw_ordinal"])
    return out


def compare(cat, observed, context):
    """context: what the runs actually held, for evaluating computed `n`."""
    by_site = {}
    for d in observed:
        by_site.setdefault(d["call_site_offset"], []).append(d)

    shared = {}
    for c in cat:
        shared.setdefault(c["site_offset"], []).append(c["draw_ordinal"])

    results = []
    for c in cat:
        hits = by_site.get(c["site_offset"], [])
        ns = sorted({h["n"] for h in hits})
        entry = {
            "draw_ordinal": c["draw_ordinal"], "at": c["at"],
            "catalogued_n": c["n"] if c["n"] is not None else c["n_expr"],
            "gate": c["gate"], "observed_count": len(hits),
            "observed_n": ns,
        }
        others = [o for o in shared[c["site_offset"]] if o != c["draw_ordinal"]]
        if others:
            # Two catalogued draws at ONE address are ONE set of stops.  Without
            # this, a reader of the machine-readable artifact alone sees draw 17
            # and draw 18 each reporting `observed_count: 2` and totals four
            # independent observations, when there were two stops in all.
            entry["shared_call_site"] = {
                "with_draws": others,
                "note": ("draw(s) %s fire at this SAME address and cannot be "
                         "told apart by it.  `observed_count` here is that one "
                         "set of stops, reported once per catalogued ordinal -- "
                         "it must not be added across these entries as if they "
                         "were independent observations.  What the trace "
                         "corroborates is HOW MANY draws the arm spends at this "
                         "address, not which ordinal each stop is."
                         % ", ".join(str(o) for o in others)),
            }
        if not hits:
            entry["verdict"] = "not observed"
            entry["why"] = ("gate never satisfied in these runs: %s" % c["gate"]
                            if c["gate"] else "site never reached in these runs")
        elif c["n"] is not None:
            if ns == [c["n"]]:
                entry["verdict"] = "corroborated"
            else:
                entry["verdict"] = "contradicted"
                entry["detail"] = ("catalogued n=%s, observed n=%s"
                                   % (c["n"], ns))
        else:
            expected = context["n_expr_values"].get(str(c["draw_ordinal"]))
            if expected is not None and ns == [expected]:
                entry["verdict"] = "corroborated"
                entry["detail"] = ("computed n: `%s` = %d in this run's state (%s)"
                                   % (c["n_expr"], expected, context["state_note"]))
            elif expected is None:
                # Three verdicts exist, and only three.  A computed-`n` draw
                # with no value for this run's state cannot be judged, so it is
                # an error in the comparison itself rather than a fourth verdict
                # nobody downstream knows how to read.
                raise ValueError(
                    "draw %d has a computed `n` (%s) but this run's context "
                    "supplies no expected value for it: the comparison cannot "
                    "judge it, and must not invent a verdict"
                    % (c["draw_ordinal"], c["n_expr"]))
            else:
                entry["verdict"] = "contradicted"
                entry["detail"] = ("computed n `%s`; expected %s here, observed %s"
                                   % (c["n_expr"], expected, ns))
        results.append(entry)

    catalogued_sites = {c["site_offset"] for c in cat}
    extra = {}
    for site, hits in sorted(by_site.items()):
        if site in catalogued_sites:
            continue
        extra["1000:%04x" % site] = {
            "count": len(hits),
            "n_values": sorted({h["n"] for h in hits}),
            "inside_preamble_range": PREAMBLE_LO <= site <= PREAMBLE_HI,
        }
    return results, extra


def preamble_ordinals(cat):
    """{call-site offset: catalogued draw ordinal} for the preamble draws."""
    return {c["site_offset"]: c["draw_ordinal"]
            for c in cat if c["where"] == "preamble"}


def check_order(cat, observed, label=None):
    """Each turn's preamble draws must appear in catalogue ORDINAL order.

    compare() above matches on call site and `n` alone, so a re-run whose order
    had drifted would still be reported "corroborated" -- while
    docs/re/wander.md and docs/re/gaps.md claim the catalogued ORDER, not just
    the catalogued set.  This turns that claim into something the tool asserts
    rather than something a human read off the turn signatures.

    Only preamble draws are checked: the church's four (15..18) fire nested
    inside another routine, so their position in the turn is not the preamble's
    ordinal sequence.  A site seen twice in one turn is a violation too -- a
    subsequence visits each ordinal at most once.
    """
    order = preamble_ordinals(cat)
    turns = {}
    for d in observed:
        turns.setdefault(d["turn"], []).append(d)
    violations = []
    checked = 0
    for t in sorted(turns):
        seen = [(order[d["call_site_offset"]], d["call_site_offset"])
                for d in turns[t] if d["call_site_offset"] in order]
        if not seen:
            continue
        checked += 1
        ords = [o for o, _ in seen]
        if any(b <= a for a, b in zip(ords, ords[1:])):
            violations.append({
                "run": label, "turn": t,
                "ordinals": ords,
                "sites": ["1000:%04x" % site for _, site in seen],
            })
    return {"turns_checked": checked, "violations": violations,
            "in_catalogue_order": not violations}


def turn_site_sequences(observed):
    """Per turn (top-level prompt read), the ordered list of call sites."""
    turns = {}
    for d in observed:
        turns.setdefault(d["turn"], []).append("%04x" % d["call_site_offset"])
    return turns


def summarise_turns(observed):
    seqs = turn_site_sequences(observed)
    hist = {}
    for t, sites in seqs.items():
        hist[" ".join(sites)] = hist.get(" ".join(sites), 0) + 1
    return sorted(hist.items(), key=lambda kv: -kv[1])


def compact_draws(run):
    """The observed stream, small enough to read: ordinal, turn, site, n, result."""
    return [{"i": d["ordinal"], "turn": d["turn"],
             "site": "1000:%04x" % d["call_site_offset"],
             "n": d["n"], "r": d["result"]} for d in run["draws"]]


def run_record(label, path, run):
    segs = sorted({"%04x" % d["return_segment"] for d in run["draws"]})
    return {
        "label": label,
        "trace_file": path,
        "seed_hex": run["seed_hex"],
        # The class is the guest's own DS:389c, never the CLI's --class-answer:
        # a run that LOADED a save never reached the class prompt, so the answer
        # describes nothing (run E: answer 0 -> class 3, guest holds class 6).
        "class_value": run["final_state"]["class_389c"],
        "class_name": CLASS_NAME_BY_VALUE.get(run["final_state"]["class_389c"]),
        "class_source": "guest DS:389c, read at the end of the run",
        "class_answer": (None if run["run"]["creation"].get("loaded_save")
                         else run["run"].get("class_answer")),
        "loaded_save": run["run"]["creation"].get("loaded_save", False),
        "district_key": run["run"]["district_key"],
        "saves_copied": run["run"].get("saves_copied", []),
        "walks_requested": run["run"]["walks_requested"],
        "prompt_stops": run["run"]["prompt_stops"],
        "load_base": run["load_base"],
        "runtime_checks": run["runtime_checks"],
        "verification": run["verification"],
        "final_state": run.get("final_state"),
        "return_segments_seen": segs,
        "turn_signatures": summarise_turns(run["draws"]),
        "draws": compact_draws(run),
    }


VERDICT_RANK = {"contradicted": 3, "corroborated": 2, "not observed": 0}


def fold(results_by_run):
    """Merge each run's per-draw results into one verdict per draw ordinal.

    Extracted from `main` and made testable because this is where a per-run
    field survived into a folded verdict once already: nine `comparison`
    entries shipped carrying a non-observing run's `why` beside a
    `corroborated` verdict, because a later run fired the draw and the fold
    kept the earlier explanation.  `observed_count`, `observed_n`, `detail`
    and `per_run` are all merged by this same loop, so it gets a direct test
    rather than only being exercised through a full comparison run.

    `results_by_run` is an ordered sequence of `(run_label, results)`.
    """
    folded = {}
    for lab, res in results_by_run:
        for entry in res:
            key = entry["draw_ordinal"]
            entry = dict(entry, per_run={lab: entry["observed_n"]})
            prev = folded.get(key)
            if prev is None:
                folded[key] = entry
                continue
            prev["observed_count"] += entry["observed_count"]
            prev["observed_n"] = sorted(set(prev["observed_n"]) |
                                        set(entry["observed_n"]))
            prev["per_run"].update(entry["per_run"])
            if VERDICT_RANK[entry["verdict"]] > VERDICT_RANK[prev["verdict"]]:
                prev["verdict"] = entry["verdict"]
                prev["detail"] = entry.get("detail", "")
            elif entry.get("detail") and entry["verdict"] == prev["verdict"]:
                have = prev.get("detail", "")
                if entry["detail"] not in have:
                    prev["detail"] = (have + "; " if have else "") + entry["detail"]
    for entry in folded.values():
        entry["per_run"] = {k: v for k, v in entry["per_run"].items() if v}
        if entry["verdict"] != "not observed":
            # `why` explains a NON-observation; a run that did not fire the draw
            # contributes it, and a later run that DID must not keep it.
            entry.pop("why", None)
        if not entry.get("detail"):
            # An empty string is noise in a ground-truth artifact.
            entry.pop("detail", None)
    return [folded[k] for k in sorted(folded)]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("traces", nargs="+", help="trace JSON files from run.py")
    ap.add_argument("--labels", default=None,
                    help="comma-separated labels, one per trace")
    ap.add_argument("--wander", default=str(REPO / "data" / "wander.json"))
    ap.add_argument("--out", default=None)
    args = ap.parse_args(argv)

    wander = json.loads(Path(args.wander).read_text())
    cat = catalogue(wander)
    runs = [json.loads(Path(t).read_text()) for t in args.traces]
    labels = (args.labels.split(",") if args.labels
              else [chr(ord("A") + i) for i in range(len(runs))])
    if len(labels) != len(runs):
        ap.error("need one label per trace")

    merged = []
    for i, r in enumerate(runs):
        for d in r["draws"]:
            e = dict(d)
            e["run"] = labels[i]
            merged.append(e)

    # Draws 10 and 11 push `chapter * 20` / `chapter * 5`, and draws 17/18 push
    # the sum of the four class growth weights.  Neither is assumed: the
    # district and the class are read out of the guest's own DS:3692 / DS:389c
    # at the end of each run, and the weights are read out of orig/g.exe.
    exe = (REPO / "orig" / "g.exe").read_bytes()
    results_by_run = []
    extra = {}
    contexts = {}
    order_checks = []
    for run, lab in zip(runs, labels):
        district = run["final_state"]["district_3692"]
        cls = run["final_state"]["class_389c"]
        wsum = class_weight_sum(exe, cls)
        ctx = {
            "n_expr_values": {"10": 20 * district, "11": 5 * district,
                              "17": wsum, "18": wsum},
            "state_note": ("district %d and class %d read from the guest's own "
                           "DS:3692 / DS:389c at the end of run %s; the class "
                           "growth weights are read from orig/g.exe"
                           % (district, cls, lab)),
        }
        contexts[lab] = {"district": district, "class": cls,
                         "class_weight_sum": wsum,
                         "n_expr_values": ctx["n_expr_values"]}
        obs = [d for d in merged if d["run"] == lab]
        res, ext = compare(cat, obs, ctx)
        order_checks.append(dict(check_order(cat, obs, label=lab), run=lab))
        results_by_run.append((lab, res))
        for site, info in ext.items():
            if site in extra:
                extra[site]["count"] += info["count"]
                extra[site]["n_values"] = sorted(set(extra[site]["n_values"]) |
                                                 set(info["n_values"]))
            else:
                extra[site] = dict(info)
    results = fold(results_by_run)
    context = {
        "note": ("What each run's computed `n` should be, and where every "
                 "operand came from.  Nothing here is assumed."),
        "per_run": contexts,
    }

    out = {
        "note": ("Live Random trace of orig/g.exe under qemu+gdb with RandSeed "
                 "pinned by patching a COPY of the binary, and its draw-by-draw "
                 "comparison against the static catalogue in data/wander.json.  "
                 "Ground truth is the original only; nothing here comes from "
                 "src/.  Verdicts are corroborated / not observed / "
                 "contradicted; a contradiction is reported with both readings, "
                 "never reconciled away.  Method and addresses: "
                 "docs/re/rng-trace.md."),
        "harness": "tools/rngtrace (python3 tools/rngtrace/run.py)",
        "source": "orig/g.exe md5 10eb0af07a2d2f5e9da790df7058891c",
        "seed_patch": runs[0]["seed_patch"],
        "observation_point": runs[0]["observation_point"],
        "turn_marker": runs[0]["turn_marker"],
        "context": context,
        "draws_observed_total": len(merged),
        "order_check": {
            "note": ("Asserted, not eyeballed: within every turn, the preamble "
                     "draws observed must appear in catalogued ordinal order "
                     "(1..14), each at most once.  compare() matches on call "
                     "site and `n` only, so without this a re-run whose order "
                     "had drifted would still read as corroborated.  The "
                     "church's draws 15..18 fire nested inside another routine "
                     "and are outside this check."),
            "turns_checked": sum(o["turns_checked"] for o in order_checks),
            "violations": [v for o in order_checks for v in o["violations"]],
            "in_catalogue_order": all(o["in_catalogue_order"] for o in order_checks),
            "per_run": {o["run"]: {"turns_checked": o["turns_checked"],
                                   "in_catalogue_order": o["in_catalogue_order"]}
                        for o in order_checks},
        },
        "comparison": results,
        "sites_not_in_catalogue": extra,
        "runs": [run_record(lab, t, r) for lab, t, r in zip(labels, args.traces, runs)],
    }
    text = json.dumps(out, indent=2, ensure_ascii=False) + "\n"
    if args.out:
        Path(args.out).write_text(text)
    else:
        print(text)
    oc = out["order_check"]
    print("ORDER: %d turns checked, %d violation(s), in catalogue order: %s"
          % (oc["turns_checked"], len(oc["violations"]), oc["in_catalogue_order"]),
          file=sys.stderr)
    for v in oc["violations"]:
        print("  ORDER VIOLATION run %s turn %s: %s"
              % (v["run"], v["turn"], " ".join(v["sites"])), file=sys.stderr)
    for r in results:
        print("draw %2d %-11s n=%-6s %-13s observed=%d %s"
              % (r["draw_ordinal"], r["at"],
                 str(r["catalogued_n"])[:24], r["verdict"],
                 r["observed_count"], str(r.get("detail", ""))[:70]), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
