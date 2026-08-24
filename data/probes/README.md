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

## `saveprobe-record-base.json` — the record-base experiment (Task 19)

`tools/rngtrace/saveprobe.py` answers a different question from `verbprobe`:
**which guest byte does each `.SAV` offset become?** It builds a record with
`tools/savegen.py` carrying a distinct sentinel at every offset of the two
spans Task 19 established, stages it as `SAVE_R3.SAV` in a temp game
directory, boots the real `orig/g.exe`, presses `3` at the slot prompt, and
dumps guest physical memory.

It is a **controlled experiment**, which is what the five shipped saves can
never be: they differ from each other in dozens of bytes at once, so no pair
of them isolates a single offset, while a synthesised record differs from its
base in exactly the bytes the probe chose.

What the committed run says, all three checks green (exit 0):

| field | value |
|---|---|
| `whole_record_matches_the_file` | `true` — all 694 bytes appear verbatim at `20ae:369c` |
| `sentinel_run_lands_at_the_record_base` | `true` — physical `0x36840`, i.e. load base `0x224B0` + DGROUP + `0x369c` + `0x214` |
| every one of the 37 `bytes` rows | `match: true` |
| `screen_tail` | `Загружено из save_r3`, the district-3 intro, and the street prompt |

`sentinel_run_physical_addresses` lists **three** hits, not one. The two
below the load base (`0x2516`, `0x2E20`) are DOS's own sector buffers, still
holding the record at dump time; the probe reports them as such rather than
asserting uniqueness it cannot have.
`sentinel_run_other_copies_inside_the_program` is the assertion that would
actually be a finding, and it is empty.

**Tier.** This is state-tier and the artifact says so in its own `tier`
field: it establishes *where a save byte lands*, never what the code does
with it. Every per-byte meaning in `docs/re/save-format.md` is carried by the
instruction that reads that byte, not by this run. And a synthesised record
can construct states no real playthrough reaches, so behaviour observed after
a probe load is behaviour in a **forced** state — a different claim from
whether a player can get there.

## `saveprobe-fresh-record.json` — what a brand-new character's record holds

`tools/rngtrace/saveprobe.py --fresh` stages **no** save at all, drives
character creation (class answer 0, empty name), and dumps the same 694 bytes
at `20ae:369c`. It answers the question Task 19's brief left as a port
decision — *"if some bytes remain unestablished, decide and document how a
fresh save fills them"* — by observing the original instead of deciding.

`record_hex` is the whole record. What it says:

* `magic` is `^4Gopnik: ^7version 1.02 june,sept 2003`, **padding all zero**.
  It is not a constant that happens to sit there: `1000:6dcd`..`1000:6ddb`
  assigns the CS literal at image `0x6489` (file `0x7D59`) into `DS:369c` in
  the new-character block, three instructions after `district := 1`.
* `name` is `^7 Раз^6дол^4бай`, padding all zero — the `^7 ` prefix
  (`1000:723a`..`1000:725d`, CS `0x67f2`, file `0x80C2`) plus the default
  name the empty-line substitution supplies (`1000:7227`, CS `0x67e4`).
* the eight stat words are `3 3 3 3 3 0 1 3`, `hp` and `hpmax` both 28;
* `0x214`..`0x231` is **all zero**, `0x234` (`threshold`) is 10 — exactly
  `1000:6de0 mov word [0x38d0],0xa` — the growth log is all zero, and
  `0x2ae`..`0x2b5` is all zero.

So a fresh save fills every byte of both former `unk_` spans with **zero**,
and that is an observation about the original rather than a choice this port
made. Two tests in `tests/save_load.rs` assert the port's own fresh record
against these bytes: `a_fresh_record_matches_what_the_original_starts_a_new_
character_with` (line 183) checks the fields, and
`a_fresh_record_is_byte_identical_to_the_probe_dump` (line 219) compares all
694 bytes against `record_hex` directly.

**`tail_all_zero` in the artifact reads `false`, and that is not a
contradiction with the list above.** That flag is computed over
`record[0x214:]`, which includes `threshold` at `0x234` — and `threshold` is
10, not 0 (`1000:6de0`). The spans the list calls all-zero are
`0x214`..`0x231`, the growth log and `0x2ae`..`0x2b5`, and each of those is
zero; the flag simply spans more than they do.

**Tier.** State-tier, and forced only in the weak sense that the driver chose
the class and left the name empty; nothing here is a state a player could not
reach — it is the state every new game starts in.

## What is still not reproducible from this tree

Re-**running** the probe needs `build/rngtrace/boot.img`, a 1.4 MB FreeDOS
image the harness builds, which is not committed. So these files make the
observed half **readable and diffable**; they do not make it re-derivable.
That is the same status `docs/re/METHODOLOGY.md` gives any state-tier or
output-tier evidence, and it is why every claim these probes support is also
carried by a flow-tier reading of the compare chain.
