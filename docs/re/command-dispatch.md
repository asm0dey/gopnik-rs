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
| `k` | `1000:ecc7` | `0xC341` | handler not traced past `jz`; corroborated as "fight" by the adjacent refusal string at `0xC343`, `^6Чё машешь копытами? Ищи мудака которого будешь пинать!` (colour code `^6`, **not** `^4`) |
| `name` | `1000:ecf1` | `0xC37C` | rename |
| `version` | `1000:edab` | `0xC3B9` | prints the version banner; **not in the in-game help text** |
| `help` | `1000:edd5` | `0xC3E9` | dispatched; printed content not traced |
| `exit` | `1000:ede9` | `0xC3EE` | quit; **not in the help text** |
| `e` | `1000:edfa` | `0xB43E` | quit |

## Verbs not found in this chain (corroboration only)

`sv`, `v`, `x`, `wes` do not appear among the `DS:3972`-first
`FUN_1f78_0bd8` calls this pass found. They are still implemented in
`src/commands.rs` (not dropped), on the following corroborating evidence
only:

- `sv`: `docs/re/tables.md` section 4's oracle capture (typing `sv`
  mid-fight against the original prints the enemy's stat block) plus the
  help text's own line (file `0xC195`). **`0xC195` is that whole help line,
  not a token string** -- no `sv` token was located anywhere.
- `v`: help text only (file `0xC210`, again a whole help line, not a token).
- `x`, `wes`: `bmar`'s own submenu text (files `0xAA58`, `0xAA8A`), not the
  top-level help block at all. Their token strings at `0xAF9E`/`0xAFDA`
  **are** real tokens; they are most likely compared inside `bmar`'s own
  `^0Барыги\` submenu loop rather than in `entry`.

### `h` and `mh` are dispatched, by a subroutine (resolved)

An earlier revision listed these two here as corroboration-only. They are
**confirmed**, and they are top-level verbs. `entry` does not compare them
itself; at `1000:e966` it pushes the just-read line `DS:3972` and calls
`FUN_1000_29c4` (`E8 5B 40`, whose 16-bit-wrapping target is `1000:29c4` --
a naive 32-bit relative-target calculation lands on `0x12FC4` and misses the
call, which is why an earlier byte scan for callers found only the one from
`FUN_1000_3d11`). That routine holds the compares:

| token | file off | bytes | compared at |
|---|---|---|---|
| `h` | `0x4197` | `01 68` | `1000:29f0`, and again at `2a6a`, `2aa0`, `2af2`, `2b40`, `2b89` |
| `mh` | `0x4199` | `02 6D 68` | `1000:2a02`, and again at `2bb0` |

The first two compares gate entry to the routine (`1000:2a0e` returns when
the line is neither); the rest choose which messages get written and whether
the drink loop repeats. `FUN_1000_3d11` calls the same routine at
`1000:4b00` with its own `DS:3a72`, which is why beer also works in a fight.

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

## Shop modality (was inferred, now confirmed)

An earlier revision of this document flagged `Mode::Shop` as an inference by
symmetry with combat. It is now **disassembled and confirmed**: each location
handler ends by *writing* its own prompt string and then `ReadLn`-ing into
`DS:3a72` -- the same second input variable combat uses, never the top-level
`DS:3972`. Traced end-to-end for `mar`: `1000:bd08` writes `^0Базар\` (file
`0xA691`) with `0eed:0000` (`Write`, no newline) and `1000:bd21`..`1000:bd2f`
is the `ReadLn`.

| location | prompt string | file off |
|---|---|---|
| `mar` | `^0Базар\` | `0xA691` |
| `bmar` | `^0Барыги\` | `0xAC4B` |
| `bmar`'s sell-items submenu | `^0Продать вещи\` | `0xB00C` |
| `rep` | `^0Ветеренар\` | `0xB313` |
| `pr` | `^0Притон\` | `0xB787` |
| `kl` | `^0Клуб\` | `0xBAB2` |
| `trn` | `^0Качалка\` | `0xBD43` |

`girl` has **no** prompt string and no `ReadLn`: `1000:d701`..`1000:d798`
runs to completion in a single turn, so it is not modal and `src/game.rs`
does not put it in `Mode::Shop`.

This also explains why the vet's `h` (heal a broken jaw) and the street's
`h` (drink a beer) can share a letter: they are read by two different
`ReadLn`s and never reach the same compare chain.

## Discovery gates

Every gated location is `cmp byte [<flag>],1` immediately after its token
matches; the not-equal arm writes exactly one refusal string. The seven
flags are contiguous.

| verb | flag | gate at | refusal at | refusal string (file off) |
|---|---|---|---|---|
| `mar` | `20ae:3694` | `1000:b954` | `1000:c49b` | `0xA9F8` |
| `bmar` | `20ae:3695` | `1000:c4c8` | `1000:d383` | `0xB1CC` |
| `pr` | `20ae:3696` | `1000:d80c` | `1000:dee3` | `0xB980` |
| `girl` | `20ae:3697` | `1000:d6f7` | `1000:d7b5` | `0xB568` |
| `rep` | `20ae:3698` | `1000:d3b0` | `1000:d6ca` | `0xB440` |
| `kl` | `20ae:3699` | `1000:df10` | `1000:e36d` | `0xBBF6` |
| `trn` | `20ae:369a` | `1000:e39a` | `1000:e948` | `0xBEC2` |

Nothing sets a flag from a *failed* entry. Two setters are implemented in the
port: `1000:d751` (`girl` reveals the club) and `1000:b570` (the wander
bucket-2 branch reveals the girl, after its own `Random(2)`).

Four more are located but not implemented, all in the wander preamble that
runs before the bucket roll, each firing on `Random(n) == 0` with its flag
still clear: `1000:b196` (`0x3698`, Vet), `1000:b1c8` (`0x3694`, Market),
`1000:b1fa` (`0x3699`, Club) and `1000:b22c` (`0x369a`, Gym). See
`docs/re/gaps.md`, "Wander preamble", for the rolls, gates and strings, and
for why they are not wired up.

## The encounter decline branch (corrected)

`1000:b718` is `jnz 0xb721`, i.e. the answer was **not** `y`. `1000:b725`
calls `Random(2)`; `1000:b72a`..`1000:b72c` is `or ax,ax` / `jnz 0xb74e`.

- **`ax == 0`** falls through to `1000:b72e`, writes `^4Он тебя заметил.`
  (file `0xA2BB`) and sets the accept flag with
  `mov byte [0x3b72],1` at `1000:b747`. **The fight happens.**
- **`ax != 0`** jumps to `1000:b74e`, writes `^2Ты смылся.` (file `0xA2CE`)
  and leaves the flag clear. **Escaped.**

`1000:b81f` then tests the flag and calls `FUN_1000_3d11(0)` at `1000:b829`.
Nothing else is written on either arm; `^4Эй мудак?!` (file `0x457A`) is
`FUN_1000_3d11`'s class-7 opener at `1000:3dc7`, not part of this branch.

The answer is case-folded before the compare (`call 0eed:0216` at
`1000:b704`), so `Y` is accepted as well as `y`.
