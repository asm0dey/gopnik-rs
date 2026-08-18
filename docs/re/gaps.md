# Known gaps in the port

The list of things the port does **not** reproduce, and why. It used to live
in `.superpowers/sdd/task-11-report.md`, which is git-ignored, so every source
comment that pointed at it pointed at nothing for anyone outside the session
that wrote it. The substance lives here instead; the source comments cite this
file by section.

Each entry states its evidence tier per `docs/re/METHODOLOGY.md`:
**established from flow** (with an address), **corroborated** (by state or
output, and by what), or **unverified** (and what would settle it). Every
address below was re-derived from `orig/g.exe` — `file_off = 0x18d0 + off` for
a `1000:off` code address, and a `mov di,<n>` / `push cs` string operand names
the string at file offset `0x18d0 + n`.

---

## Discovery: five of the seven flags are still unreachable

*Cited from `src/game.rs`'s `enter_shop`.*

The seven discovery flags are seven contiguous bytes at `20ae:3694..369a`
(`docs/re/command-dispatch.md`, "Discovery gates"). A scan of `orig/g.exe` for
`c6 06 ?? 36 ??` (`mov byte [0x36??],imm8`) finds every store to them.
**Established from flow.** Two of the setters are implemented:

| flag | location | setter | implemented as |
|---|---|---|---|
| `0x3697` | Girl | `1000:b570` (wander bucket 2) | `Game::wander_girl` |
| `0x3699` | Club | `1000:d751` (`girl`'s own reveal) | `Game::visit_girl` |

That makes `w` → girl → club a real, reachable chain. The other five are set
only from paths this port does not model; the ones located so far are below.

### Wander preamble (`1000:af04`..`1000:b2a0`) — not reproduced

*Cited from `src/game.rs`'s `Game::walk` doc.*

A long run of one-shot flavour and discovery events, each gated by its own
`Random()` roll and its own never-repeat flag, running **before** the four-way
bucket roll at `1000:b34d`. Twenty-two `Random` call sites exist between
`1000:ae5a` and `1000:b940` (searching for the `9a 4b 11 78 0f` far call);
`Game::walk` models three of them. **Established from flow** that the
following four are discovery events:

| roll | gate | setter | string |
|---|---|---|---|
| `1000:b186` `Random(10)` | `1000:b18f` `cmp byte [0x3698],0` | `1000:b196` `[0x3698] := 1` (Vet) | file `0x9F8B` |
| `1000:b1b8` `Random(10)` | `1000:b1c1` `cmp byte [0x3694],0` | `1000:b1c8` `[0x3694] := 1` (Market) | file `0x9FB2` |
| `1000:b1ea` `Random(100)` | `1000:b1f3` `cmp byte [0x3699],0` | `1000:b1fa` `[0x3699] := 1` (Club) | file `0x9FC4` |
| `1000:b21c` `Random(100)` | `1000:b225` `cmp byte [0x369a],0` | `1000:b22c` `[0x369a] := 1` (Gym) | file `0x9FFE` |

Each fires when its roll returns `0` and its flag is still clear.

**Why they are not implemented.** They are four *unconditional* draws sitting
inside a preamble whose other eight draws are still uncatalogued. Adding these
four alone would move the port's RNG sequence without bringing it closer to the
original's, and the fidelity rule is the *sequence*, not the individual event.
Implementing them belongs with a pass that catalogues the whole preamble.

Two further stores are outside the wander path and not yet traced to a trigger:
`1000:ae1f` (`[0x3696] := 1`, the Den, immediately after `1000:ae13` sets a
`[0x3c83]` sentinel), and `1000:4aa5` / `1000:52b3` / `1000:73c3` (also the
Den, inside the combat and progression units). `1000:dcf6` / `1000:dcfb` set
BigMarket and Gym together. **Unverified**: what reaches each of them. A
breakpoint on each store during a played session would settle it.

### Wander buckets 1 and 4 — flavour only

**Established from flow** that neither writes a discovery flag (no
`c6 06 ?? 36 ??` store falls between `1000:b3ba` and `1000:b940` except
`1000:b570`).

* Bucket 1 (`1000:b3c4`) toggles `[0x3693]`, then writes one district-keyed
  line from one of two sets (`1000:b3db`.. when the toggle is set,
  `1000:b465`.. when it is clear).
* Bucket 4 (`1000:b836`) branches on the stoned counter `[0x38cd]` and writes
  name-keyed flavour built with `0f78:0ae7` / `0f78:0b66` string calls.

---

## `PLACES.SAV`'s byte order

*Cited from `src/locations.rs`'s `TRACKED`.*

**Unverified.** All five `orig/*.SAV` files and `orig/PLACES.SAV` itself are
`01` in every slot, so the files cannot disambiguate the order, and the routine
that reads `PLACES.SAV` has not been located. The in-memory flag order
(`3694` Market, `3695` BigMarket, `3696` Den, `3697` Girl, `3698` Vet, `3699`
Club, `369a` Gym) is **established from flow** and differs from `TRACKED` at
slots 2 and 4, but that is evidence about the flags, not about the file's
layout, so `TRACKED` is deliberately left as it is. Locating the `BlockRead`
would settle it.

## No `.SAV` load path

*Cited from `src/main.rs`.*

`orig/g.exe` runs from itself alone, so "no save file" is the ordinary
new-game case (**corroborated** by running it). Loading an existing character
is out of scope; `Save::parse` is the only constructor, and `.SAV` offsets
`0x214` (29 bytes) and `0x2ae` (8 bytes) are still unknown, so
`Game::write_save` returns `Unsupported` for every `Game` this code can build.

## No typed save verb, and no "saved" message

*Cited from `src/game.rs`'s `write_save` note.*

**Established from flow** that `sv` is not save (it sizes up the enemy — see
`src/commands.rs`). Saving in the original is checkpoint-only:
`docs/re/tables.md`'s "Other price sources" names `1000:761d` (a paid service,
`district * 50` rubles) and a second path at `0x9bcd`. Neither is a typed verb.
There is no "saved OK" / "save failed" string anywhere in `data/strings.json`,
so a wrapper could only print composed text — which is why there is none.

## `help`'s printed content

*Cited from `src/game.rs`'s `show_help`.*

**Established from flow** that `help` is dispatched at `1000:edd5`. Its handler
body was not traced, so nothing is printed rather than inventing a line: the
game has no "not implemented" string to quote. Disassembling the handler
settles it.

## `rename`'s prompts

*Cited from `src/game.rs`'s `rename`.*

`^2Звали тебя:^7 ` and `^2А теперь будут:^7 ` are **this port's own wording**
and are the one place the code knowingly departs from the byte-verbatim rule.
`1000:ecf1`'s handler body was not traced, so the real prompts are unknown.

## The vet's charged amounts

*Cited from `src/game.rs`'s `heal_jaw` / `heal_leg`.*

**Established from flow** that the menu prints `3` and `7` (files `0xB2B2`,
`0xB2D9`) and that the affordability colour compares money against the same
literals (`cmp word [0x38c7],0x3` at `1000:d410`, `cmp word [0x38c7],0x7` at
`1000:d465`). That the *debit* is also 3 and 7 is an **inference** — the vet's
own submenu handler was not traced.

## The in-combat verb set

*Cited from `src/game.rs`'s `run_combat`.*

**Corroborated** modal by the live capture (`mar` and `i` typed at `^0Битва\`
were ignored, reprinting the prompt). `sv` (inspect) is corroborated by
`docs/re/tables.md`'s oracle capture; `h`/`mh` (beer) are **established from
flow** via `FUN_1000_3d11`'s call into `FUN_1000_29c4` at `1000:4b00`. `k`
(attack) is **this port's own choice** — consistent with `k` being the fight
verb everywhere else, but not independently confirmed. `FUN_1000_3d11`'s own
input loop was not disassembled.

## Other unreproduced behaviour

* **`kl` / `trn` priced rows** — prices are not in `data/shops.json`.
* **The class-keyed combat-opener table** (`1000:3d32`..`1000:3e8a`, files
  `0x452E`, `0x453B`, `0x4548`, `0x4565`, `0x457A`, …).
* **The rector death branch and the hospital rescue** (`1000:4f8c`,
  `1000:4fce`) — need fields `crate::model::Fighter` does not have.
* **`sv`, `v`, `x`, `wes` token compare sites** — not located; those four
  verbs are corroboration-only, not dispatch-confirmed.
* **The quit message** (files `0xC3F3`, `0xC41A`, written at `1000:ee04`) and
  the university backstory (`0x7D81`..`0x7F1F`) — real strings, not wired up.
* **Shop purchase effects** — `data/shops.json` rows deduct `price` and print
  their text, but never change `strength` / `armor` / etc.: most rows have no
  representable target on `Fighter`.
* **The joint (`kos`) heal formula** reuses beer's `FUN_1000_29c4` by analogy;
  the joint's own handler was not traced.
* **The decline branch after a fight encounter.** `1000:b721`'s `Random(2)`
  evade-vs-detected split is **established from flow**, but a second,
  similarly-shaped path at `1000:b691` has no roll on decline at all. Which one
  a real encounter reaches depends on `1000:b5fc`, untraced. The port always
  takes the `Random(2)` branch.
* **Shop modality** — `Mode::Shop`'s "accept a few keys, `w` to leave, ignore
  the rest" shape is **established from flow** only as far as each location
  writing its own prompt and `ReadLn`-ing into `DS:3a72` (`1000:bd08` /
  `1000:bd21` for `mar`); the submenu dispatch chain itself was not traced.
