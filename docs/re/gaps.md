# Known gaps in the port

The list of things the port does **not** reproduce, and why. Source comments
cite this file by section.

Each entry states its evidence tier per `docs/re/METHODOLOGY.md`:
**established from flow** (with an address), **corroborated** (by state or
output, and by what), or **unverified** (and what would settle it). Every
address below was re-derived from `orig/g.exe` — `file_off = 0x18d0 + off` for
a `1000:off` code address, and a `mov di,<n>` / `push cs` string operand names
the string at file offset `0x18d0 + n`.

---

## Discovery flags: the complete store inventory

*Cited from `src/game.rs`'s `enter_shop` and `Game::new`.*

The seven discovery flags are seven contiguous bytes at `20ae:3694..369a`
(`docs/re/command-dispatch.md`, "Discovery gates"). Scanning `orig/g.exe` for
`c6 06 [94-9a] 36 imm8` (`mov byte [0x36??],imm8`) yields **31** stores:
**14 clears** and **17 set-to-1**. The clears are the two block resets —
`1000:6d3b`..`1000:6d6e` (the `places.sav` load-failure arm) and
`1000:ab96`..`1000:abc9` (`reset_for_new_district`). All seventeen setters are
below; **established from flow** (the scan is byte-exact and the encoding is
fixed-length, so it cannot miss a store of this form).

An earlier revision of this section claimed the same scan "finds every store to
them", then listed twelve of the seventeen and said "**Two** further stores"
while naming five addresses. That is the "evidence that proves less than it
claims" failure `docs/re/METHODOLOGY.md` exists to stop; the count and the
inventory are now stated together.

| setter | flag | location | trigger | tier | in the port? |
|---|---|---|---|---|---|
| `1000:6dc3` | `0x3698` | Vet | character creation, `1000:6dbe` | flow | **yes** — `Game::new` |
| `1000:6dc8` | `0x3694` | Market | character creation, `1000:6dbe` | flow | **yes** — `Game::new` |
| `1000:b196` | `0x3698` | Vet | wander preamble, `Random(10)` at `1000:b186` | flow | no |
| `1000:b1c8` | `0x3694` | Market | wander preamble, `Random(10)` at `1000:b1b8` | flow | no |
| `1000:b1fa` | `0x3699` | Club | wander preamble, `Random(100)` at `1000:b1ea` | flow | no |
| `1000:b22c` | `0x369a` | Gym | wander preamble, `Random(100)` at `1000:b21c` | flow | no |
| `1000:b570` | `0x3697` | Girl | wander bucket 2 | flow | **yes** — `Game::wander_girl` |
| `1000:d751` | `0x3699` | Club | `girl`'s own reveal | flow | **yes** — `Game::visit_girl` |
| `1000:73c3` | `0x3696` | Den | `[0x389c] == 5` at `1000:73bb` | flow | no |
| `1000:73cf` | `0x3697` | Girl | `[0x389c] == 3` at `1000:73bb` | flow | no |
| `1000:73d4` | `0x3699` | Club | `[0x389c] == 3` at `1000:73bb` | flow | no |
| `1000:73e0` | `0x3695` | BigMarket | `[0x389c] == 6` at `1000:73bb` | flow | no |
| `1000:dcf6` | `0x3695` | BigMarket | the `a` token at `1000:dcef` | flow | no |
| `1000:dcfb` | `0x369a` | Gym | the `a` token at `1000:dcef` | flow | no |
| `1000:ae1f` | `0x3696` | Den | the chapter-5 endgame arm at `1000:adbf` | flow | no |
| `1000:4aa5` | `0x3696` | Den | the de-level (flee) penalty, `1000:4a87`/`1000:4aa0` | flow | no |
| `1000:52b3` | `0x3696` | Den | the post-kill block, `1000:5295`/`1000:52b1` | flow | no |

All three Den triggers were **closed by Task 11b** — see `docs/re/wander.md`,
"The three Den setters". `1000:4aa5`'s store and the line it prints contradict
each other in the original; that is recorded there, not resolved.

Four of the seven flags are reachable in this port: Market and Vet from
character creation, Girl from the wander bucket, Club from `girl`. BigMarket,
Den and Gym are not reachable at all.

### Character creation grants Vet and Market — `1000:6dbe`

**Established from flow.** `1000:6dbe` writes `[0x3692] := 1` (district),
`1000:6dc3` writes Vet and `1000:6dc8` writes Market, three consecutive
five-byte stores. Three paths reach the block and all three write all three
bytes: `1000:6b3a` (the `save_r?.sav` scan at `1000:6a62`..`1000:6ab9` found
nothing — **the path a fresh run with no `.SAV` files takes**, and it prints
nothing), `1000:6b81` (the slot prompt at `1000:6b51` read a key that is none
of `'0'`,`'2'`..`'5'`, i.e. "начать сначала"), and `1000:6bdd` (`IOResult`
non-zero at `1000:6bd4`, via `1000:6da5`, which prints file `0x7D21`).

The `places.sav` reader's own failure arm (`1000:6d3b`) does **not** reach
`1000:6dbe`; it clears flags and leaves at `1000:6da0`.

### The `[0x389c]` progression reveals — `1000:73bb`..`1000:73e0`

**Established from flow**, contrary to an earlier "not yet traced to a trigger
/ unverified" tiering. `1000:73bb` `a1 9c 38` `mov ax,[0x389c]`, then:

```text
73be  cmp ax,5   / 73c1 jnz 0x73ca / 73c3  [0x3696] := 1   (Den)
73ca  cmp ax,3   / 73cd jnz 0x73db / 73cf  [0x3697] := 1   (Girl)
                                    73d4  [0x3699] := 1   (Club)
73db  cmp ax,6   / 73de jnz 0x73e5 / 73e0  [0x3695] := 1   (BigMarket)
73e5  mov byte [0x3e35],5
```

**Closed by Task 11b.** `[0x389c]` is the character class, written only at
`1000:6fed`, `1000:6ffc`, `1000:712a`, `1000:713d` and `1000:71b8` (plus the
694-byte record `BlockRead` at `1000:6c01`), and these four stores are the
class bonuses the creation menu advertises. `1000:73bb` is reached on **every**
entry into the game — new character and loaded save alike, both converging on
`1000:7262` — so the bonuses are re-applied each time. Full derivation and the
complete write inventory: `docs/re/wander.md`, "`[20ae:389c]` is the character
class".

### The `a` token — `1000:dce5`..`1000:dcfb`

**Established from flow.** Not an untraceable path: it is a typed word.

```text
dcba  cmp byte [0x3695],0 / 74 07  ; already-have check: BigMarket and
dcc1  cmp byte [0x369a],0 / 75 6a  ;   Gym both set -> skip to 0xdd32
dcc8..dcdc                         ; ax := ([0x38a6] - ([0x3692]-1)*10)*2 + [0x38cb]
dce0  cmp ax,0x28 / 7c 4d          ; < 40 -> skip
dce5  push ds:0x3a72               ; the line just typed
dcea  mov di,0x9fc9 / push cs      ; file 0xB899 = the single character 'a'
dcef  call 0f78:0bd8 / 75 3c       ; string compare; not equal -> skip
dcf6  [0x3695] := 1                ; BigMarket
dcfb  [0x369a] := 1                ; Gym
```

`DS:3a72` is the same submenu input buffer `mar` reads into (`1000:bd21`).
**Closed by Task 11b**: the read that leaves the token there is the den's own
`ReadLn` at `1000:db00`..`1000:db09` — the only `0f78:06c6` call between
`1000:d802` and `1000:dd48` — so `a` is typed at the `^0Притон\` prompt, not at
the top level. `[0x38cb]` is a street-cred counter distinct from the level
(`1000:5291` grows it per kill, `1000:db9b` spends it, `1000:dc79` prints it).
See `docs/re/wander.md`. Still not implemented here.

### Wander preamble (`1000:af04`..`1000:b34d`) — not reproduced

*Cited from `src/game.rs`'s `Game::walk` doc.*

A long run of one-shot flavour and discovery events, each gated by its own
`Random()` roll and its own never-repeat flag, running **before** the four-way
bucket roll at `1000:b34d`. Twenty-two `Random` call sites exist between
`1000:ae5a` and `1000:b940` (searching for the `9a 4b 11 78 0f` far call);
`Game::walk` models three of them: the bucket roll `1000:b353`, bucket 2's
girl roll `1000:b54e`, and the decline roll `1000:b725`.

Counting the preamble exactly, because an earlier revision put it at "eight
other draws" and no reading yields eight:

* In `1000:af04`..`1000:b2a0` there are **nine** sites: `af68`, `afc7`, `b030`,
  `b0dc`, `b186`, `b1b8`, `b1ea`, `b21c`, `b272`. Four are the discovery rolls
  in the table below, so **five** others.
* Extending the range to the bucket roll adds `b2fa` and `b321` — **eleven**
  sites, **seven** others. (Those two are inside the `[0x389c] == 6` arm and
  are skipped by `1000:b2ed` `jnz 0xb34d`, so they are conditional, not
  per-walk.)

**Established from flow** that these four are discovery events:

| roll | gate | setter | string |
|---|---|---|---|
| `1000:b186` `Random(10)` | `1000:b18f` `cmp byte [0x3698],0` | `1000:b196` `[0x3698] := 1` (Vet) | file `0x9F8B` |
| `1000:b1b8` `Random(10)` | `1000:b1c1` `cmp byte [0x3694],0` | `1000:b1c8` `[0x3694] := 1` (Market) | file `0x9FB2` |
| `1000:b1ea` `Random(100)` | `1000:b1f3` `cmp byte [0x3699],0` | `1000:b1fa` `[0x3699] := 1` (Club) | file `0x9FC4` |
| `1000:b21c` `Random(100)` | `1000:b225` `cmp byte [0x369a],0` | `1000:b22c` `[0x369a] := 1` (Gym) | file `0x9FFE` |

Each fires when its roll returns `0` and its flag is still clear.

**The preamble is now catalogued.** Task 11b recovered all fourteen draws in
`1000:ae5a`..`1000:b3ba` as one ordered sequence, plus the **four** the church
can spend — one inside `1000:7c67` itself, one on its `== 1` arm, and two more
inside the level-up routine `1000:2526` that its `== 0` arm calls —
`docs/re/wander.md` and `data/wander.json`. The site list is byte-scan
complete. What follows is why the *port* still does not spend them.

**Why they are not implemented.** This is a **scope** call, not a fidelity
blocker. They are four unconditional draws inside a preamble whose other seven
draws had not been catalogued when Task 11 shipped (no `n`, no gate, no
effect), so adding these four alone would move the port's RNG sequence without
bringing it closer to the original's. The port's sequence is *already* wrong relative to the original —
see the next section — and stays wrong either way; implementing these belongs
with a pass that catalogues the whole preamble, so the sequence is wired at
once rather than in fragments. Nothing about the deferral says the events
themselves are uncertain: all four are established from flow.

The same standard applies in the other direction. `Game::wander_girl` and
`Game::visit_girl` *are* implemented, and each spends its draw where the
original spends one (`1000:b54e`, `1000:d728`); implementing a store that costs
no draw at all — `Game::new`'s `1000:6dc3`/`1000:6dc8` — is likewise free of
this argument, which is why it was not deferred.

### Two unconditional draws after the bucket roll — `1000:b39e`, `1000:b3ae`

**Established from flow**, and **the port spends neither**:

```text
b39a  b8 c8 00        mov ax,0xc8      ; 200
b39d  50              push ax
b39e  9a 4b 11 78 0f  call Random
b3a3  09 c0           or ax,ax
b3a5  75 03           jnz 0xb3aa
b3a7  e8 bd c8        call 0x7c67
b3aa  b8 64 00        mov ax,0x64      ; 100
b3ad  50              push ax
b3ae  9a 4b 11 78 0f  call Random
b3b3  09 c0           or ax,ax
b3b5  75 03           jnz 0xb3ba
b3b7  e8 7e c1        call 0x7538
```

They sit between the last bucket store (`1000:b395`, reached from the `== 1`
compare at `1000:b38e`) and the bucket dispatch (`1000:b3ba` `mov al,[0x3970]`), on the
fall-through path, so **every** walk executes both calls; only the
`call 0x7c67` / `call 0x7538` payloads are gated on a `0`. **Both callees were
disassembled by Task 11b.** `1000:7c67` is the church: it spends a further
`Random(5)` unconditionally; a `Random(4)` when that returns `1`; and **two**
`Random(class-weight-sum)` draws when it returns `0`, because that arm sets
`xp := threshold` (`1000:7fe4`/`1000:7fe7`) and calls the level-up routine at
`1000:7fed`, whose two-iteration inner loop (`1000:287d`) always draws twice.
It ends by clearing `[0x3970]` at `1000:8282`, so a church turn produces no
encounter at all. `1000:7538` is the wandering mage's paid save: no draws, but
a blocking `ReadLn` into a stack local, and it charges `chapter*50` while
printing `chapter*25`. An earlier revision enumerated `b34d → b359 → b35c..b393 →
b3ba` and walked straight past this block.

**Consequence for draw-sequence fidelity.** In the original, bucket 2 reaches
`1000:b54e`'s `Random(2)` as the *fourth* draw since the bucket roll (`b39e`,
`b3ae`, then bucket 2's own); in the port it is the *first*. The port's wander
draw sequence is therefore already out of step with the original's, before any
of the preamble is counted. Not implemented in this wave for the same scope
reason as the preamble: half-wiring the sequence is worse than a documented
gap.

### Wander buckets 1 and 4 — flavour only

**Established from flow** that neither writes a discovery flag (no
`c6 06 [94-9a] 36 imm8` store falls between `1000:b3ba` and `1000:b940` except
`1000:b570`).

* Bucket 1 (`1000:b3c4`) toggles `[0x3693]`, then writes one district-keyed
  line from one of two sets (`1000:b3db`.. when the toggle is set,
  `1000:b465`.. when it is clear).
* Bucket 4 (`1000:b836`) branches on the stoned counter `[0x38cd]` and writes
  name-keyed flavour built with `0f78:0ae7` / `0f78:0b66` string calls.

---

## `PLACES.SAV`'s byte order — settled

*Cited from `src/locations.rs`'s `TRACKED`.*

**Established from flow.** The reader is at `1000:6c5a` and uses `Read`, not
`BlockRead` — seven one-byte reads, each naming its destination flag:

```text
6c5a  push ds:0x3e36                 ; the file variable
6c6a  call 0f78:0ae7                 ; copy DS:3d32 (the directory) into a temp
6c74  call 0f78:0b66                 ; append cs:0x63f2 = file 0x7CC2, 'places.sav'
6c79  call 0f78:072e                 ; Assign
6c87  call 0f78:0769                 ; Reset(f, 1)  -- record size 1
6c8c  call 0f78:028a                 ; IOResult; non-zero -> 1000:6d3b
6ca2  call 0f78:081e -> DS:0x3694    ; Read #1  Market
6cb4  call 0f78:081e -> DS:0x3695    ; Read #2  BigMarket
6cc6  call 0f78:081e -> DS:0x3696    ; Read #3  Den
6cd8  call 0f78:081e -> DS:0x3697    ; Read #4  Girl
6cea  call 0f78:081e -> DS:0x3698    ; Read #5  Vet
6cfc  call 0f78:081e -> DS:0x3699    ; Read #6  Club
6d0e  call 0f78:081e -> DS:0x369a    ; Read #7  Gym
6d1b  call 0f78:07ea                 ; Close
6d20  writes '^0Загружено из places' (file 0x7CCD)
```

File order therefore equals flag-address order: **Market, BigMarket, Den, Girl,
Vet, Club, Gym**. `TRACKED` carried Vet and Den swapped at slots 2 and 4 and
has been corrected; the file's own bytes still cannot arbitrate (`orig/*.SAV`
and `orig/PLACES.SAV` are `01` in every slot), but they no longer need to.

Earlier revisions of this section and of `src/locations.rs` said the read "has
not been located" and that "locating the `BlockRead` would settle it". Both
claims were wrong: the routine exists and there is no `BlockRead`.

The failure arm at `1000:6d3b` is a **conditional** reset. It clears Vet
(`6d3b`), Market (`6d40`), Club (`6d4c`), Gym (`6d51`), Girl (`6d5d`),
BigMarket (`6d62`) and Den (`6d6e`), except that `1000:6d45`
(`cmp word [0x389c],3` / `jz 0x6d51`) skips the Club clear, `1000:6d56`
(same compare, `jz 0x6d62`) skips the Girl clear, and `1000:6d67`
(`cmp word [0x389c],5` / `jz 0x6d73`) skips the Den clear — one flag each,
not pairs. It then writes `^6Чё-то глюкануло - немогу прoгрузить Places:Ресет ту Default` (file `0x7CE3`) and
leaves via `1000:6d8c`/`1000:6da0`, never reaching `1000:6dbe`.
`[0x389c]` is the character class (Task 11b) — the skips keep the class
bonuses and clear only what was discovered. The port has no `.SAV` load path at
all, so none of this is reproduced.

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
* **The decline branch after a fight encounter.** The evade-vs-detected split
  on the `Random(2)` at `1000:b725` (`1000:b721` is its `mov ax,2`,
  `1000:b724` the `push`) is **established from flow**, but a second,
  similarly-shaped path at `1000:b691` has no roll on decline at all. Which one
  a real encounter reaches depends on `1000:b5fc`, untraced. The port always
  takes the `Random(2)` branch.
* **Shop modality** — `Mode::Shop`'s "accept a few keys, `w` to leave, ignore
  the rest" shape is **established from flow** only as far as each location
  writing its own prompt and `ReadLn`-ing into `DS:3a72` (`1000:bd08` /
  `1000:bd21` for `mar`); the submenu dispatch chain itself was not traced.

---

## Opened by Task 11b (the wander catalogue)

*Cited from `docs/re/wander.md` and `data/wander.json`.*

The wander preamble is now fully catalogued as one ordered sequence, so the
port's divergence there is a known quantity rather than an unknown one. These
are the questions that pass left open, and the ones it created.

* **The whole sequence is static-only.** Every one of the eighteen draws is
  **established from flow** from the disassembly; **none** has been corroborated
  by a live breakpoint. A `tools/qemu` run on a pinned seed that logs the
  fourteen in-range sites in order — and the church's two when it fires — would
  raise the whole catalogue a tier, and is the natural first step of Task 12.
* **`unk_38b2`.** `1000:81e9` increments this byte under
  `^1Накладываю на тебя защиту!` (file `0x9476`). No consumer was located.
  The name in `data/wander.json` stays `unk_38b2`.
* **The item at `DS:394d`.** Bought from the dealers for 150 roubles at
  `1000:cd05` (price byte `DS:0b3e`), and it arms the 25-walk delivery counter
  `DS:3e32` that `1000:af1d` drives. `docs/re/tables.md` calls that counter
  "the silencer"; the purchase's own name string was not traced, so
  `data/wander.json` keeps the neutral `dealer_order_placed`.
* **`1000:4aa5` sets the Den flag while printing a refusal.** The byte is
  `c6 06 96 36 01` (verified) and the line is
  `^4Такого конявого непустят в местный притон!` (file `0x4D42`); the den gate
  at `1000:d80c` reads nothing but that flag. Whether a clear was intended is
  **unverified** and cannot be settled from the binary.
* **Does the chapter-5 block re-run every turn?** `1000:ae18` is at the top of
  every iteration (back-edge `1000:ee01` `jmp 0xab75`) and nothing clears
  `[0x3c83]` — its only writes are `1000:7364` and `1000:ae13`. So on the face
  of the flow, once chapter 5 is reached the rector fight and the endgame fight
  run every turn. Whether `FUN_1000_3d11(4)` returns at all was not traced.
* ~~**The mage's printed price disagrees with the charged price.**~~
  **Folded back in fix wave 1.** `docs/re/tables.md`'s "Other price sources"
  now records both halves — printed `chapter*25` at `1000:758d`, checked and
  charged `chapter*50` at `1000:7605`/`1000:7618`.
* ~~**`data/command_dispatch.json` still records the three Den setters as
  trigger-UNVERIFIED.**~~ **Folded back in fix wave 1.** All three
  `setters_found` entries now carry the trigger established from flow;
  `1000:4aa5` keeps its unresolved set-while-refusing note (above).
* ~~**`docs/re/command-dispatch.md` step 5 is wrong.**~~ **Folded back in fix
  wave 1.** Step 5 now names `1000:b353` as the regular-turn bucket roll, says
  there is one wander path, and points at `docs/re/wander.md`. Step 4's "not
  catalogued" was corrected at the same time.
* ~~**`docs/re/progression.md` lists `DS:38c1` as "text only".**~~ **Folded
  back in fix wave 1.** The one-shot table now names it the ring "Господи
  помилуй" with its per-walk regen, and records the church's second grant site
  for all three gift flags.
