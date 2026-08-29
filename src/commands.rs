//! Verb parsing for the input-dispatch chain in `orig/g.exe`'s `entry`.
//!
//! ## Authority order, and why this file was rewritten twice
//!
//! This table went through three revisions during Task 11 and the final one
//! is grounded in the disassembly, not in prose:
//!
//! 1. The brief's table came from the project plan's "Reference facts",
//!    already known unreliable elsewhere in this project (it also asserted
//!    the RNG multiplier was absent, later proven false).
//! 2. A revision built from `data/strings.json`'s help-text block
//!    (`"Напиши: <verb> чтобы ..."`, file `0xBFE0`..`0xC30D`) fixed several
//!    brief errors but was itself flagged: **a printed help string is not
//!    proof of what the input parser accepts.** The two can disagree (typos,
//!    dead code, verbs the help text never mentions).
//! 3. This revision is built from the actual dispatcher: `entry`'s
//!    `do`-loop reads one line into `DS:3972` (`1000:ae5a`..`1000:ae63`,
//!    `call far 0f78:06c6`, a Pascal `ReadLn`) and compares it against a
//!    chain of literal tokens with `FUN_1f78_0bd8` (confirmed a Pascal
//!    shortstring `CompareByte`-style equality routine by reading its own
//!    decompilation, `build/decomp/FUN_1f78_0bd8_1f78_0bd8.c`: it walks
//!    `min(len1,len2)` bytes and stops on the first mismatch, leaving the
//!    zero flag set for the caller's `jz`/`jnz`). Each call is
//!    `push DS:3972 / push CS:<token>` -- reproducible with
//!    `python3 tools/re_query.py resolve <citation>`, which converts a
//!    citation of either form (see `docs/re/METHODOLOGY.md`, "Address
//!    convention, and its range of validity") and prints the bytes. On a mismatch
//!    the code falls through to the *next* token's compare; on a match it
//!    runs that token's handler.
//!
//! `docs/re/oracle-captures/command-table-and-combat.md`'s live capture is
//! corroboration only (confirms runtime behaviour: which enemy appears, what
//! `y`/anything-else does at an encounter prompt) -- it is not cited as
//! proof of the verb table itself; see the per-verb table below for exactly
//! what each verb's status is.
//!
//! ## The confirmed dispatch chain
//!
//! Traced with `ndisasm -b16 -o 0xab59` over `entry`'s 17143-byte body
//! (`file_off 0xc429`..`0x10720`) and a script matching `mov di,<token>` /
//! `push ds|cs` / `push di` pairs feeding `call far 0f78:0bd8`, filtered to
//! calls whose *first* operand is `DS:3972` (the just-`ReadLn`'d line; a
//! second, unrelated variable `DS:3a72` is reused for sub-prompts, see
//! `Command::Walk`'s doc). Every row below was independently re-verified by
//! disassembling its own compare instruction and confirming the token string
//! at the file offset the citation resolves to (`tools/re_query.py resolve`):
//!
//! | verb | compare at | token file off | confirmed handler behaviour |
//! |---|---|---|---|
//! | `w` | `1000:ae86` | `0x9D5E` | wander/encounter roll -- see [`Command::Walk`] |
//! | `run` | `1000:ae97`/`1000:aee4` | `0x9D60` | **synonym of `w`** -- same jump target `1000:aea1` |
//! | `mar` | `1000:b94a` | `0xA42C` | market menu, gated on discovery flag `20ae:3694` + a pursuit flag `20ae:3b76` |
//! | `bmar` | `1000:c4be` | `0xAA24` | dealers menu (fallthrough target of `mar`'s mismatch) |
//! | `rep` | `1000:d3a6` | `0xB236` | vet |
//! | `girl` | `1000:d6ed` | `0xB46A` | girlfriend |
//! | `fight` | `1000:d7d8` | `0xB584` | **not a fight command** -- prints `^6Пережитки прошлого жми w чтобы искать врагов` (file `0xB58A`, "vestiges of the past, press w instead"), a deprecated-alias message |
//! | `pr` | `1000:d802` | `0xB5BD` | den |
//! | `kl` | `1000:df06` | `0xB9BA` | club |
//! | `trn` | `1000:e390` | `0xBC23` | gym |
//! | `kos` | `1000:e973` | `0xBEEF` | smoke a joint |
//! | `i` | `1000:ea94` | `0xBFDE` | prints the 13-line command list (`docs/re/oracle-captures/...`'s capture); **not inventory** -- the brief's guess is wrong |
//! | `s` | `1000:ec82` | `0xB855` | stats |
//! | `f` | `1000:ec96` | `0xC31C` | shoot, **traced** in Task 18: `1000:ec9d cmp byte [0x394d],0` / `eca2 jz 0xecbd` gates the refusal `^6Ты чё псих? мигом менты накроют!` (file `0xC31E`) on owning a pistol, and without one the verb is accepted and answered with silence |
//! | `k` | `1000:ecc7` | `0xC341` | handler not traced past its `jz`; corroborated as "fight" the same way, via `^6Чё машешь копытами? Ищи мудака которого будешь пинать!` at `0xC343` (the colour code is `^6`, not `^4`) |
//! | `name` | `1000:ecf1` | `0xC37C` | rename |
//! | `version` | `1000:edab` | `0xC3B9` | **not in the help text at all** -- prints the version banner (its own text is at `0xC3C1`, the same `^4Gopnik: ^7version 1.02 june,sept 2003` the game opens with) |
//! | `help` | `1000:edd5` | `0xC3E9` | dispatched, content not traced (see [`Command::Help`]) |
//! | `exit` | `1000:ede9` | `0xC3EE` | **not in the help text** -- a second spelling of quit |
//! | `e` | `1000:edfa` | `0xB43E` | quit (help text: `"если захочешь выйти"`) |
//!
//! ## `h` and `mh` are dispatched by a subroutine, not by an inline compare
//!
//! They are missing from the table above because `entry` does not compare
//! them itself. At `1000:e966` it pushes the just-read `DS:3972` and calls
//! `FUN_1000_29c4` (`E8 5B 40`, which wraps around 16 bits to `1000:29c4`);
//! that routine compares its own argument against the token `"h"` (file
//! `0x4197`, a length-prefixed `01 68`) at `1000:29f0` and `"mh"` (file
//! `0x4199`, `02 6D 68`) at `1000:2a02`, and returns immediately when the
//! line is neither. Six further `"h"` compares (`1000:2a6a`, `2aa0`, `2af2`,
//! `2b40`, `2b89`) and one further `"mh"` compare (`1000:2bb0`) choose which
//! messages it writes. So both **are** top-level verbs; the earlier revision
//! of this file was right to keep them, and the reviewer's lead that they
//! are not top-level verbs at all does not hold -- `FUN_1000_3d11`'s call at
//! `1000:4b00` is a *second* call site, not the only one.
//!
//! `sv`, `v`, `x`, `wes` are **not** in this `DS:3972` list, and the reason
//! for two of them is now known rather than open.
//!
//! **`sv` and `v` are dispatched -- by `FUN_1000_3d11`, against `DS:3a72`.**
//! The follow-up this paragraph used to ask for was done by the final-review
//! fix wave: combat runs its own nine-token compare chain through the same
//! `0f78:0bd8`, and `sv` is at `1000:4c42` (token file `0x4E71`) and `v` at
//! `1000:4caa` (token file `0x4E96`). Both are **established from flow**; see
//! [`crate::game::Game::run_combat`] for the whole table. So neither is a
//! corroboration-only verb any more -- they are *combat* verbs, which is why
//! they were never going to turn up in `entry`'s list. Both arms were traced
//! in Task 17 (`docs/re/combat-dispatch.md`) and are implemented in
//! [`crate::combat_dispatch`].
//!
//! `x` and `wes` are the dealers' own submenu keys, read at `^0Барыги\\`
//! rather than at the top-level prompt. The speculation this paragraph used
//! to carry -- "they may well have no `DS:3972` compare at all" -- is
//! settled, and the shape is indeed `sv`/`v`'s: they ARE compared, at
//! `1000:ce80` and `1000:ced8` (tokens CS `0x96ce` and CS `0x970a`), against
//! **`DS:3a72`**, the sub-prompt buffer, which is the same variable the fight
//! prompt reads into. Each is compared at exactly one site image-wide.
//!
//! ## Corrections this makes to the brief
//!
//! * `sv` is not `Save` -- see [`Command::Inspect`]'s doc; the owner
//!   confirmed there is no typed save verb.
//! * `f`/`k` were swapped in the brief; `k` fights, `f` shoots (see table).
//! * `x` is not `Quit`; `e`/`exit` are (confirmed dispatcher entries; `x` is
//!   `bmar`-specific junk-selling, corroboration only, see [`Command::SellJunk`]).
//! * `wes` is not `Weapon`; it sells items at `bmar` (corroboration only).
//! * `hp` is not a global health command -- its only occurrence in
//!   `data/strings.json` is inside `pr`'s own submenu; not in this table at
//!   all, so it falls through to `Unknown("hp")` at the top level, which is
//!   correct: there is no evidence it is dispatched here.
//! * `i` is the command list, not inventory (confirmed dispatcher entry,
//!   corroborated by the live capture). The brief's `Inventory` variant is
//!   removed; nothing in this task found a dedicated inventory verb.

/// One parsed player command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Dealers,
    Market,
    Vet,
    Girl,
    Den,
    Club,
    Gym,
    Stats,
    /// `w` or `run` (confirmed synonyms, same jump target `1000:aea1`).
    /// Rolls for a random encounter; `crate::game::Game::walk` is the
    /// reconstruction and documents exactly what is proven vs. simplified.
    Walk,
    /// `k`. Dispatcher entry confirmed at `1000:ecc7`; "fight" is
    /// corroboration (adjacent refusal string), not a traced handler body.
    Fight,
    /// `f`. Dispatcher entry confirmed at `1000:ec96`, and "shoot" is no
    /// longer corroboration: `1000:ec9d cmp byte [0x394d],0` gates the
    /// street refusal on the pistol flag, and the fight prompt's own arm at
    /// `1000:4eb2` fires it ([`crate::combat_dispatch::fire`]).
    Shoot,
    /// `sv`. Not in `entry`'s `DS:3972` chain, because it is a **combat**
    /// verb: `FUN_1000_3d11` compares it at `1000:4c42` against its own
    /// `DS:3a72` buffer, token file `0x4E71`. **Established from flow.** The
    /// oracle capture in `docs/re/tables.md` section 4 (typing `sv` mid-fight
    /// printed the enemy's stat block) and the help text's `"приглядеться к
    /// пинаемому мудаку"` were the only evidence when this variant was
    /// written; they now corroborate a traced dispatch instead of standing
    /// alone. Definitely not `Save` -- the owner confirmed no typed save verb
    /// exists.
    Inspect,
    /// `v`. Also a combat verb, compared at `1000:4caa`, token file `0x4E96`
    /// -- **established from flow**, and at exactly that one site in the
    /// whole image, so the street prompt does not dispatch it at all
    /// (`Game::call_backup` carries that scan). Its arm at `1000:4cb4` opens
    /// with `cmp byte [0x3696],1`, the den's discovery flag, which fits the
    /// help text's `"чтобы позвать подкрепление"` (file `0xC210`); the whole
    /// arm is traced in `docs/re/combat-dispatch.md` and implemented in
    /// [`crate::combat_dispatch::Backup`].
    Backup,
    /// `i`. Confirmed at `1000:ea94`: prints the 13-line command list.
    CommandList,
    /// `kos`. Confirmed at `1000:e973`.
    Joint,
    /// `h`, drink one half-litre. **Confirmed**: `entry` passes the typed
    /// line to `FUN_1000_29c4` at `1000:e966`, which compares it against the
    /// token at file `0x4197` at `1000:29f0`. See the module doc.
    Drink,
    /// `mh`, drink until full. **Confirmed** the same way: token file
    /// `0x4199`, compared at `1000:2a02`.
    BingeDrink,
    /// `name`. Confirmed at `1000:ecf1`.
    Name,
    /// `help`. Confirmed dispatched at `1000:edd5`; its printed content was
    /// not traced (unlike `i`, whose content the live capture happened to
    /// show).
    Help,
    /// `version`. Confirmed at `1000:edab`; not in the help text at all.
    Version,
    /// `e` (confirmed `1000:edfa`) or `exit` (confirmed `1000:ede9`, not in
    /// the help text) -- both quit.
    Quit,
    /// `fight`. Confirmed dispatcher entry at `1000:d7d8` whose handler
    /// prints a deprecation message pointing at `w`, not a fight action.
    LegacyFight,
    /// `x` at the dealers (sell junk). Not found in the dispatch chain;
    /// corroborated only by `bmar`'s own submenu text (`data/strings.json`
    /// file `0xAA58`: `"Здесь можно толкнуть хлам(x)"`).
    SellJunk,
    /// `wes` at the dealers (sell items). Same corroboration level as
    /// `SellJunk` (file `0xAA8A`).
    SellItems,
    /// Any line the dispatcher's compare chain does not match. The original
    /// writes nothing for these (`1000:ee01` `jmp 0xab75`, straight back to
    /// the prompt), and it draws no distinction between a stray single
    /// character and a stray word -- so neither does this.
    Unknown(String),
}

/// Parse one line of typed input into a [`Command`].
///
/// Matches the dispatcher's own behaviour as traced: exact whole-line
/// comparison (`FUN_1f78_0bd8` compares the two shortstrings' full content,
/// not a prefix), case handled by lowercasing here (the original's own
/// case-fold is `FUN_1eed_0216`, called on the input right after `ReadLn` at
/// `1000:ae72`, before any comparison -- this reproduces its effect without
/// having decompiled that routine's internals).
pub fn parse(input: &str) -> Command {
    let v = input.trim().to_lowercase();
    match v.as_str() {
        "w" | "run" => Command::Walk,
        "mar" => Command::Market,
        "bmar" => Command::Dealers,
        "rep" => Command::Vet,
        "girl" => Command::Girl,
        "fight" => Command::LegacyFight,
        "pr" => Command::Den,
        "kl" => Command::Club,
        "trn" => Command::Gym,
        "kos" => Command::Joint,
        "i" => Command::CommandList,
        "s" => Command::Stats,
        "f" => Command::Shoot,
        "k" => Command::Fight,
        "name" => Command::Name,
        "version" => Command::Version,
        "help" => Command::Help,
        "exit" | "e" => Command::Quit,
        "sv" => Command::Inspect,
        "v" => Command::Backup,
        "h" => Command::Drink,
        "mh" => Command::BingeDrink,
        "x" => Command::SellJunk,
        "wes" => Command::SellItems,
        _ => Command::Unknown(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_confirmed_dispatcher_verbs() {
        assert_eq!(parse("mar"), Command::Market);
        assert_eq!(parse("bmar"), Command::Dealers);
        assert_eq!(parse("rep"), Command::Vet);
        assert_eq!(parse("girl"), Command::Girl);
        assert_eq!(parse("pr"), Command::Den);
        assert_eq!(parse("kl"), Command::Club);
        assert_eq!(parse("trn"), Command::Gym);
        assert_eq!(parse("kos"), Command::Joint);
        assert_eq!(parse("i"), Command::CommandList);
        assert_eq!(parse("s"), Command::Stats);
        assert_eq!(parse("f"), Command::Shoot);
        assert_eq!(parse("k"), Command::Fight);
        assert_eq!(parse("name"), Command::Name);
        assert_eq!(parse("version"), Command::Version);
        assert_eq!(parse("help"), Command::Help);
        assert_eq!(parse("exit"), Command::Quit);
        assert_eq!(parse("e"), Command::Quit);
        assert_eq!(parse("fight"), Command::LegacyFight);
    }

    #[test]
    fn w_and_run_are_synonyms() {
        assert_eq!(parse("w"), Command::Walk);
        assert_eq!(parse("run"), Command::Walk);
    }

    #[test]
    fn i_is_the_command_list_not_inventory() {
        // Corrects both the brief (which guessed Inventory) and an earlier
        // revision of this file (which trusted the help text alone before
        // the dispatcher itself was traced).
        assert_eq!(parse("i"), Command::CommandList);
    }

    #[test]
    fn sv_inspects_rather_than_saves() {
        assert_eq!(parse("sv"), Command::Inspect);
    }

    #[test]
    fn corroborated_only_verbs_still_parse() {
        assert_eq!(parse("v"), Command::Backup);
        assert_eq!(parse("h"), Command::Drink);
        assert_eq!(parse("mh"), Command::BingeDrink);
        assert_eq!(parse("x"), Command::SellJunk);
        assert_eq!(parse("wes"), Command::SellItems);
    }

    #[test]
    fn is_case_insensitive_and_trims() {
        assert_eq!(parse("  BMAR "), Command::Dealers);
        assert_eq!(parse("Trn"), Command::Gym);
    }

    #[test]
    fn exact_match_only_no_prefix_matching() {
        assert_eq!(parse("sv"), Command::Inspect);
        assert_eq!(parse("s"), Command::Stats);
    }

    /// A single character the table does not claim is no more dispatched
    /// than a stray word is. Location submenu keys (`h`/`r` at the vet,
    /// the digits at `mar`) never reach `parse`: `Game::shop_turn` matches
    /// them on the raw line first, because the original reads them through
    /// its own `ReadLn DS:3a72` that never enters `entry`'s dispatch chain.
    #[test]
    fn unclaimed_single_letters_are_unknown_like_any_other_line() {
        for c in ['a', 'd', 'p', 'r', 't', '7'] {
            assert_eq!(parse(&c.to_string()), Command::Unknown(c.to_string()));
        }
    }

    #[test]
    fn unknown_input_is_preserved() {
        match parse("zzz") {
            Command::Unknown(s) => assert_eq!(s, "zzz"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn hp_is_unknown_not_a_command() {
        // hp's only occurrence anywhere in data/strings.json is inside pr's
        // own submenu text, not this dispatch chain.
        match parse("hp") {
            Command::Unknown(s) => assert_eq!(s, "hp"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }
}
