//! Verb parsing for the top-level ("Street") command loop.
//!
//! ## The command table was re-derived from the binary, not taken from plan
//!
//! Task 11's brief carried a command table sourced from the project plan's
//! "Reference facts", which this project's own history shows is not reliable
//! (the same table asserted the RNG multiplier was absent, later proven
//! false). The owner separately confirmed one entry was wrong (`sv` is not
//! `save` -- saving is checkpoint-only, never a typed verb). Given that, the
//! whole table was re-derived here from `data/strings.json` and
//! `data/string_pointers_audit.tsv`, both extracted straight from
//! `orig/g.exe`.
//!
//! The decisive evidence is the game's own help screen: `data/strings.json`
//! holds an unbroken run of seventeen `^6<verb>^7  чтобы <what it does>`
//! strings at file offsets `0xBFE0`..`0xC30D`, printed by the `help` command
//! itself (`docs/re/tables.md`-style citation: file offset =
//! `0x18D0 + (seg-0x1000)*16 + off`, all these are `CODE_0`/`1000:xxxx`, so
//! file offset **is** `0x18D0 + off`). This is the game documenting its own
//! command set, in order, in Russian, which is stronger evidence than any
//! prose gloss:
//!
//! | verb | file off | text | meaning |
//! |---|---|---|---|
//! | `w` | `0xBFE0` | `чтобы шататься по окрестностям - искать на свою жопу приключения` | wander the neighbourhood looking for trouble |
//! | `mar` | `0xC032` | `чтобы идти на рынок` | go to the market |
//! | `bmar` | `0xC057` | `чтобы идти к барыгам` | go to the dealers |
//! | `rep` | `0xC07D` | `чтобы идти к ветеринару` | go to the vet |
//! | `girl` | `0xC0A6` | `чтобы завалиться к своей девчонке` | go see your girl |
//! | `pr` | `0xC0D9` | `чтобы идти в местный притон гопоты` | go to the local den |
//! | `kl` | `0xC10D` | `чтобы идти в клуб` | go to the club |
//! | `trn` | `0xC130` | `чтобы идти в качалку` | go to the gym |
//! | `s` | `0xC156` | `чтобы посмотреть в лужу на свою уродскую рожу` | look at your own ugly mug (stats) |
//! | `sv` | `0xC195` | `чтобы приглядеться к пинаемому мудаку` | look closer at the guy you're kicking (inspect the enemy) |
//! | `k` | `0xC1CC` | `чтобы гасить мудака который тебе попался на дороге` | beat up the guy you ran into (**fight**) |
//! | `v` | `0xC210` | `чтобы позвать подкрепление` | call for backup |
//! | `kos` | `0xC23C` | `чтобы схавать косяк` | smoke a joint |
//! | `h` | `0xC261` | `чтобы выпить пиво (если не охото к ветеринару)` | drink beer |
//! | `mh` | `0xC2A1` | `чтобы набухаться до чёртиков` | binge drink |
//! | `name` | `0xC2CF` | `чтобы сменить погоняло` | rename yourself |
//! | `e` | `0xC2F7` | `если захочешь выйти` | **quit the game** |
//!
//! Right after that block, at `0xC31C`/`0xC341`, the bare literals `f` and
//! `k` reappear immediately followed by their own "invalid use" response
//! strings (`^6Ты чё псих? мигом менты накроют!` for `f`, `^6Чё машешь
//! копытами? Ищи мудака которого будешь пинать!` for `k`) -- the shape used
//! throughout the binary for "command token, then its handler's replies",
//! confirming `f` (shoot the gun) and `k` (fight) are dispatched, distinct
//! commands, not just help text. `i` (`0xBFDE`) sits immediately before the
//! help block starts and has no counter-evidence; kept per the brief since
//! nothing contradicts it, flagged unverified below.
//!
//! ## Corrections this makes to the brief's table
//!
//! * **`sv` is not `Save`.** It is `Inspect` (look at the current opponent).
//!   Confirmed twice: the help text above, and `docs/re/tables.md` section 4
//!   ("Boss v0"/"Boss v1"), where typing `sv` *during a fight* against the
//!   original prints the enemy's full stat block. There is no typed save
//!   command; see the module doc on [`crate::game`] for the checkpoint-only
//!   design this implies.
//! * **`f` and `k` were swapped in the brief.** The brief mapped `f` to
//!   `Fight`; the help text says `k` is "beat up the guy on the road" (fight)
//!   and `f` is gun-related (`^6Ты чё псих? мигом менты накроют!`, "you crazy?
//!   cops will bust you", the exact refusal for trying to shoot without a gun
//!   or outside a bandit district -- see `docs/re/tables.md`'s `bmar` row 7,
//!   "Можно стрелять в бандитских районах"). So `Fight` is `k`; a new
//!   `Shoot` variant takes `f`.
//! * **`e` is quit, not `x`.** The brief mapped `x` to `Quit`; the help text
//!   is unambiguous that `e` is "если захочешь выйти" (if you want to exit).
//!   `x`'s real meaning is `bmar`-specific: `"Здесь можно толкнуть хлам(x) и
//!   купить кое-что"` (file `0xAA58`) -- selling junk at the dealers, nothing
//!   to do with quitting.
//! * **`wes` is not `Weapon`.** The brief guessed it showed weapon damage.
//!   Its real string is `"Ещё ты можешь продать ненужные вещи - wes"` (file
//!   `0xAA8A`, also `bmar`-specific) -- selling unneeded items. There is no
//!   evidence anywhere of a dedicated "show weapon" verb; damage is part of
//!   the `s` (stats) screen already (`docs/re/combat.md`'s fighter-record
//!   table, `dmg_min`/`dmg_max`).
//! * **`hp` is not a global `Health` command.** The only occurrence of the
//!   literal string `"hp"` in `data/strings.json` (file `0xB852`) sits inside
//!   the `pr` (den) location's own sub-menu: `"Напиши hp чтобы отпинать
//!   мудака который наезжал на пацана"` -- beat up the guy who was hassling
//!   your pal, a den-specific action, not a health readout. Health is already
//!   shown by `s`/`Stats` (see [`crate::model::Fighter`]'s citation of
//!   `1000:1542`, printed in the same block as strength/agility/etc). No
//!   `Health` variant is defined here.
//! * **`v`, `mh`, `Shoot`/`f` are additions.** The brief's table did not have
//!   them; the help text does.
//!
//! ## What stays generic: single letters
//!
//! `a`, `d`, `p`, `r`, `t`, and digits `1`-`9` all appear as real,
//! location-specific single-character commands in the binary (e.g. `r` heals
//! legs at `rep`/vet for 7 rubles but borrows money at `pr`/den; digits `1`-`9`
//! buy a numbered row at `mar`/`bmar`). A flat verb table has no way to give
//! one letter two meanings, so `parse` does not try: any bare ASCII letter or
//! digit not claimed by a verb above falls through to `Command::Key(char)`,
//! and `crate::game`'s dispatch resolves its meaning against `self.location`.
//! This is the brief's own architecture (`Key(char)` plus a location-aware
//! handler); only the verb table feeding it changed.

/// One parsed player command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    BigMarket,
    Market,
    Vet,
    Girl,
    Den,
    Club,
    Gym,
    Stats,
    /// `w` -- wander (at the street) or leave (at a shop); see the location
    /// dispatch in `crate::game`.
    Leave,
    /// `k` -- corrected from the brief's `f`; see the module doc.
    Fight,
    /// `f` -- shoot the pistol; corrected from the brief's `Fight`.
    Shoot,
    /// `sv` -- inspect the current opponent; corrected from the brief's
    /// `Save`, which the owner has stated does not exist as a typed verb.
    Inspect,
    /// `v` -- call for backup. Not in the brief's table.
    Backup,
    Inventory,
    Joint,
    /// `mh` -- binge drink. Not in the brief's table.
    BingeDrink,
    Name,
    Help,
    /// `e` -- corrected from the brief's `x`; see the module doc.
    Quit,
    /// `x` -- sell junk at the dealers. Corrected from the brief's `Quit`.
    SellJunk,
    /// `wes` -- sell unneeded items at the dealers. Corrected from the
    /// brief's `Weapon`.
    SellItems,
    Key(char),
    Unknown(String),
}

/// Parse one line of typed input into a [`Command`].
///
/// The original dispatches on exact whole-word matches (see the "longest
/// match" test below: `s` and `sv` are distinct commands, and a prefix match
/// would confuse them), so this does the same: trim and lowercase, then
/// match the whole string against the verb table, falling back to a single
/// `Key(char)` for one bare ASCII character and `Unknown` for anything else.
pub fn parse(input: &str) -> Command {
    let v = input.trim().to_lowercase();
    match v.as_str() {
        "bmar" => Command::BigMarket,
        "mar" => Command::Market,
        "rep" => Command::Vet,
        "girl" => Command::Girl,
        "pr" => Command::Den,
        "kl" => Command::Club,
        "trn" => Command::Gym,
        "s" => Command::Stats,
        "w" => Command::Leave,
        "k" => Command::Fight,
        "f" => Command::Shoot,
        "sv" => Command::Inspect,
        "v" => Command::Backup,
        "i" => Command::Inventory,
        "kos" => Command::Joint,
        "mh" => Command::BingeDrink,
        "name" => Command::Name,
        "help" => Command::Help,
        "e" => Command::Quit,
        "x" => Command::SellJunk,
        "wes" => Command::SellItems,
        _ => {
            let mut chars = v.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Command::Key(c),
                _ => Command::Unknown(v),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_letter_verbs() {
        assert!(matches!(parse("bmar"), Command::BigMarket));
        assert!(matches!(parse("mar"), Command::Market));
        assert!(matches!(parse("rep"), Command::Vet));
        assert!(matches!(parse("girl"), Command::Girl));
        assert!(matches!(parse("pr"), Command::Den));
        assert!(matches!(parse("kl"), Command::Club));
        assert!(matches!(parse("trn"), Command::Gym));
        assert!(matches!(parse("kos"), Command::Joint));
        assert!(matches!(parse("mh"), Command::BingeDrink));
        assert!(matches!(parse("name"), Command::Name));
        assert!(matches!(parse("wes"), Command::SellItems));
        assert!(matches!(parse("help"), Command::Help));
    }

    #[test]
    fn parses_single_letter_verbs() {
        assert!(matches!(parse("s"), Command::Stats));
        assert!(matches!(parse("w"), Command::Leave));
        // Corrected from the brief: k fights, f shoots (see module doc).
        assert!(matches!(parse("k"), Command::Fight));
        assert!(matches!(parse("f"), Command::Shoot));
        assert!(matches!(parse("i"), Command::Inventory));
        assert!(matches!(parse("v"), Command::Backup));
        // Corrected from the brief: e quits, not x.
        assert!(matches!(parse("e"), Command::Quit));
        assert!(matches!(parse("x"), Command::SellJunk));
    }

    #[test]
    fn sv_inspects_rather_than_saves() {
        // The brief asserted `parse("sv") == Command::Save`. The owner
        // stated there is no typed save command, and the help text at file
        // 0xC195 ("приглядеться к пинаемому мудаку") confirms sv looks at
        // the opponent instead. This replaces the brief's test.
        assert!(matches!(parse("sv"), Command::Inspect));
    }

    #[test]
    fn is_case_insensitive_and_trims() {
        assert!(matches!(parse("  BMAR "), Command::BigMarket));
        assert!(matches!(parse("Trn"), Command::Gym));
    }

    #[test]
    fn exact_match_only_no_prefix_matching() {
        // "s" is Stats but "sv" is Inspect -- the parser must not
        // prefix-match one onto the other.
        assert!(matches!(parse("sv"), Command::Inspect));
        assert!(matches!(parse("s"), Command::Stats));
    }

    #[test]
    fn unclaimed_single_letters_fall_back_to_key() {
        // a, d, p, r, t are real location-specific commands in the binary
        // (e.g. r heals legs at the vet but borrows money at the den) that
        // a flat verb table cannot disambiguate; they fall through to Key
        // for crate::game's location-aware dispatch to resolve.
        for c in ['a', 'd', 'p', 'r', 't'] {
            assert_eq!(parse(&c.to_string()), Command::Key(c));
        }
        // Shop-row digits fall through the same way.
        assert_eq!(parse("7"), Command::Key('7'));
    }

    #[test]
    fn unknown_input_is_preserved() {
        match parse("zzz") {
            Command::Unknown(s) => assert_eq!(s, "zzz"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }
}
