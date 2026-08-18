# Command dispatch (Task 11)

Machine-readable form: `data/command_dispatch.json`. Traced with
`ndisasm -b16 -o 0xab59` over `entry`'s full 17143-byte body (file
`0xC429`..`0x10720`) plus manual disassembly of specific regions.
`file_off = 0x18D0 + off` for every `1000:XXXX` address below, per this
project's standard convention.

## Method

`entry`'s main `do`-loop reads one line per turn into `DS:3972`
(`1000:ae63`, `call far 0f78:06c6`, confirmed a Pascal `ReadLn` by its
position immediately after the loop's per-turn housekeeping and before any
comparison), then compares it against a chain of literal command tokens
using `FUN_1f78_0bd8`. Reading that routine's own decompilation
(`build/decomp/FUN_1f78_0bd8_1f78_0bd8.c`) shows it walks
`min(len1, len2)` bytes of two Pascal shortstrings and stops on the first
mismatch, leaving the zero flag set for the caller's `jz`/`jnz` -- i.e. it
is the compiler's shortstring equality (`=`) operator, not a print routine
(an earlier pass of this task misread it as one).

Each comparison compiles to a fixed four-instruction shape:

```
mov di, DS:3972        ; the line just read
push ds / push di
mov di, <token addr>    ; a literal shortstring embedded in CODE_0
push cs / push di
call far 0f78:0bd8       ; FUN_1f78_0bd8(DS:3972, CS:<token>)
jz <match>
jmp <next token's compare>   ; (or jmp <next arm> variant)
```

A Python pass over the ndisasm output matched this exact instruction
sequence, filtered to calls whose first operand is `DS:3972` (a second,
unrelated variable `DS:3a72` is reused for encounter/sub-prompt answers,
see below), and extracted `(compare address, token file offset)` for every
hit. Each row below was independently re-verified by disassembling its own
compare instruction directly and reading the token bytes at
`0x18D0 + off` out of `orig/g.exe`.

## The confirmed `DS:3972` dispatch chain

| verb | compare at | token file off | notes |
|---|---|---|---|
| `w` | `1000:ae86` | `0x9D5E` | wander/encounter roll |
| `run` | `1000:ae97` / `1000:aee4` | `0x9D60` | confirmed synonym of `w` (same jump target `1000:aea1`) |
| `mar` | `1000:b94a` | `0xA42C` | market; gated on discovery flag `20ae:3694` + pursuit flag `20ae:3b76` |
| `bmar` | `1000:c4be` | `0xAA24` | dealers; fallthrough target of `mar`'s mismatch |
| `rep` | `1000:d3a6` | `0xB236` | vet |
| `girl` | `1000:d6ed` | `0xB46A` | girlfriend |
| `fight` | `1000:d7d8` | `0xB584` | **deprecated alias**, not a fight command -- prints `^6Пережитки прошлого жми w чтобы искать врагов` (file `0xB58A`) |
| `pr` | `1000:d802` | `0xB5BD` | den |
| `kl` | `1000:df06` | `0xB9BA` | club |
| `trn` | `1000:e390` | `0xBC23` | gym |
| `kos` | `1000:e973` | `0xBEEF` | smoke a joint |
| `i` | `1000:ea94` | `0xBFDE` | prints the 13-line command list; **not inventory** |
| `s` | `1000:ec82` | `0xB855` | stats |
| `f` | `1000:ec96` | `0xC31C` | handler not traced past `jz`; corroborated as "shoot" by the adjacent refusal string at `0xC31E` |
| `k` | `1000:ecc7` | `0xC341` | handler not traced past `jz`; corroborated as "fight" by the adjacent refusal string at `0xC343` |
| `name` | `1000:ecf1` | `0xC37C` | rename |
| `version` | `1000:edab` | `0xC3B9` | prints the version banner; **not in the in-game help text** |
| `help` | `1000:edd5` | `0xC3E9` | dispatched; printed content not traced |
| `exit` | `1000:ede9` | `0xC3EE` | quit; **not in the help text** |
| `e` | `1000:edfa` | `0xB43E` | quit |

## Verbs not found in this chain (corroboration only)

`sv`, `v`, `h`, `mh`, `x`, `wes` do not appear among the `DS:3972`-first
`FUN_1f78_0bd8` calls this pass found. They are still implemented in
`src/commands.rs` (not dropped), on the following corroborating evidence
only:

- `sv`: `docs/re/tables.md` section 4's oracle capture (typing `sv`
  mid-fight against the original prints the enemy's stat block) plus the
  help text's own line (file `0xC195`).
- `v`, `h`, `mh`: help text only (files `0xC210`, `0xC261`, `0xC2A1`). `h`
  additionally corroborated by `FUN_1000_29c4` (the beer-healing routine)
  being callable from both `entry` and `FUN_1000_3d11`.
- `x`, `wes`: `bmar`'s own submenu text (files `0xAA58`, `0xAA8A`), not the
  top-level help block at all.

A follow-up should grep each token's file offset in
`data/string_pointers_audit.tsv` and disassemble the referencing
instruction the way every row above was, to either confirm a `DS:3972`
(or other) compare site or establish that none exists (e.g. because the
token is compared inline as a raw byte pair rather than through
`FUN_1f78_0bd8`, as turned out to be true of `w`/`run` never showing up in
`data/strings.json`'s own extraction despite being real length-1
shortstrings at `0x9D5E`/`0x9D60` -- confirmed directly by `xxd`, not by
the extractor, which apparently drops length-1 candidates).

## The prompt

File `0x9BF1`: a one-byte Pascal shortstring `01 5C` (length 1, `\`),
printed repeatedly through `entry` as the ordinary prompt. The live
capture (`docs/re/oracle-captures/command-table-and-combat.md`) confirms
`Битва\` during a fight.

## Wander → encounter → combat trace

`1000:ae5a`..`1000:b82c`, reproduced in `src/game.rs`'s `Game::walk` doc
comment with the same addresses; summarised here for the machine-readable
side:

1. `1000:ae63`: `ReadLn` into `DS:3972` -- this is the loop's own per-turn
   input read, shared by every verb.
2. `1000:ae86`/`1000:ae97`: compare against `"w"`/`"run"`; both jump to
   `1000:aea1`.
3. `1000:aea1`..`1000:af04`: decays a "stoned" counter (`DS:38cd`); on it
   hitting zero, applies `^4Глюки прошли. Сила -2.` (file `0x9D64`). Not
   reproduced in `src/game.rs` (no countdown field on `Fighter`, only
   `stoned: bool`).
4. `1000:af04` onward: a long run of one-shot flavour/discovery events
   (phone calls, finding the market sign, the silencer's 25-wander counter
   `docs/re/tables.md` already documents at `20ae:3e32`), each gated by its
   own `Random()` roll and a never-repeat flag. **Not catalogued** -- too
   many for this task's remaining budget.
5. `1000:b358` (within the *district-transition* preamble, structurally
   identical branch shape) rolls `Random(25)+1`, bucketed 1 / 2-4 / 5-9 /
   10-25 into `DS:3970`. The regular-turn path (`1000:b4e8`..`1000:b5ae`)
   branches on the same variable via a `cmp al,N` chain (N = 2,3,4,5),
   strongly suggesting reuse of the same roll -- **the specific `Random`
   call feeding the regular-turn branch was not found**, so `src/game.rs`
   reuses the district-transition roll's bucketing as an assumption, not a
   confirmed fact.
6. `1000:b5ae`: `cmp al,3` -- bucket 3 leads to `1000:b5b8`, `call
   FUN_1000_0d14` (the encounter generator, `docs/re/tables.md` section 3).
7. `1000:b5c0`: `cmp word[0x3952],8` -- if the rolled enemy's class is 8
   (Мент/cop), branches to a separate stealth flow at `1000:b76a`. Not
   modelled in `src/game.rs` (Fighter has no "spotted by a cop" state).
8. `1000:b660`..`1000:b691`: prints `"Идет <rank> <крутизна> уровня..."`,
   then a **second** `ReadLn`, into `DS:3a72` (confirmed different from the
   line-level `DS:3972`), compared against the literal `"y"` (file
   `0x9BF3`: length-prefixed `01 79`). On match, sets accept flag `DS:3b72`.
9. `1000:b721`: on any other answer, `Random(2)` picks between
   `^X Ты смылся.` and `^X Он тебя заметил.` + a taunt. Confirmed for this
   one code path; a second, similarly-shaped block at `1000:b691` shows no
   visible random roll on decline at all (reached for a different enemy
   class range via the luck-vs-roll branch at `1000:b5fc`). Which path a
   real encounter takes depends on the enemy's rolled class; `src/game.rs`
   always uses the `Random(2)` 50/50, a real branch of the original but not
   proven to be the only one.
10. `1000:b81f`/`1000:b826`: if the accept flag is set, `call FUN_1000_3d11`
    (combat) with `param_1 = 0`.

## Combat modality

The live capture (`docs/re/oracle-captures/command-table-and-combat.md`)
shows `mar` and `i` typed at the `Битва\` prompt being ignored (the prompt
just reprints). This task did not disassemble `FUN_1000_3d11`'s own input
loop to enumerate its accepted verbs; `src/game.rs`'s `run_combat` accepts
only `sv` (inspect, corroborated) and `k` (attack, this port's own choice,
not confirmed as the in-combat key -- the capture's three `w` presses in
combat produced no visible output, consistent with `w` either doing
nothing there or doing something silent).

## Shop modality

Not independently disassembled. `src/game.rs`'s `Mode::Shop` (accept a
location's own keys plus `w` to leave, reject everything else) is inferred
by symmetry with the confirmed combat modality, plus every location's own
intro text naming `w` as the only way out (e.g. `mar`'s `"напиши w чтобы
уйти"`, file `0xA430`) and never mentioning another verb. Flagged as an
inference, not a confirmed fact, in task-11-report.md.
