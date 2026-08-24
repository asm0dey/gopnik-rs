# Committed probe outputs

`tools/rngtrace/verbprobe.py` answers, live under qemu+gdb against
`orig/g.exe`, **which typed verbs reach a chosen function and which provably
do not**. Its answers are transcribed into `data/character_sheet.json`'s and
`data/combat_dispatch.json`'s `live_probe` nodes.

Those transcriptions used to cite `build/rngtrace/*.json`, and `/build` is
gitignored wholesale, so the observed half of both artifacts could not be read
from a clean clone at all. Development on this branch stops here, so the raw
outputs are committed:

| file | target | written by | transcribed into |
|---|---|---|---|
| `verbprobe-1000-1a03.json` | `FUN_1000_1a03`, the player's sheet | Task 16 | `data/character_sheet.json` `live_probe` |
| `verbprobe-1000-1348.json` | `FUN_1000_1348`, the enemy's sheet | Task 17 | `data/combat_dispatch.json` `live_probe` |
| `verbprobe-1000-1348-run2.json` | same, second run | Task 17 | the `runs_agree` field |

`verbprobe-1000-1a03.json` predates the `--target` flag, which Task 17 added,
so it carries no `target` key; its breakpoint is `1000:1a03` and its
`markers` node names the three addresses it broke on.

Two runs are kept for the `1000:1348` probe because `runs_agree` is a claim
about two runs producing the same marker stream. It is **provenance, not an
assertion**: `tools/test_combat_dispatch.py` deliberately does not check it,
because an artifact asserting a fact about its own capture is the shape that
file exists to refuse. With both runs committed the claim is now checkable by
hand -- `python3 -c "import json;a,b=(json.load(open(p))['marker_stream'] for
p in ('data/probes/verbprobe-1000-1348.json',
'data/probes/verbprobe-1000-1348-run2.json'));print(a==b, a)"` prints
`True PPPCTCCTCCCP`.

## What is still not reproducible from this tree

Re-**running** the probe needs `build/rngtrace/boot.img`, a 1.4 MB FreeDOS
image the harness builds, which is not committed. So these files make the
observed half **readable and diffable**; they do not make it re-derivable.
That is the same status `docs/re/METHODOLOGY.md` gives any state-tier or
output-tier evidence, and it is why every claim these probes support is also
carried by a flow-tier reading of the compare chain.
