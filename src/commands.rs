//! Verb parsing for the main dispatch chain.
//!
//! The game reads a line and compares it against a chain of literal commands.
//!
//! | verb | behaviour |
//! |---|---|
//! | `w` | wander/encounter roll |
//! | `run` | **synonym of `w`** |
//! | `mar` | market menu |
//! | `bmar` | dealers menu |
//! | `rep` | vet |
//! | `girl` | girlfriend |
//! | `fight` | deprecated alias: `^6Пережитки прошлого жми w чтобы искать врагов` |
//! | `pr` | den |
//! | `kl` | club |
//! | `trn` | gym |
//! | `kos` | smoke a joint |
//! | `i` | prints the command list |
//! | `s` | stats |
//! | `f` | shoot |
//! | `k` | attack |
//! | `name` | rename |
//! | `version` | prints `^4Gopnik: ^7version 1.02 june,sept 2003` |
//! | `help` | help |
//! | `exit` | quit |
//! | `e` | quit |
//! | `h` / `mh` | dispatched by their own routine |
//!
//! Combat verbs `sv` and `v`, and dealers' keys `x` and `wes`, are dispatched
//! separately.

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
    Walk,
    Fight,
    Shoot,
    /// `sv` -- inspect the enemy's stat block mid-fight. Not a save verb.
    Inspect,
    /// `v` -- call reinforcements, gated on discovering the den.
    Backup,
    CommandList,
    Joint,
    Drink,
    BingeDrink,
    Name,
    Help,
    Version,
    Quit,
    LegacyFight,
    /// `x` at the dealers -- sell junk (`"Здесь можно толкнуть хлам(x)"`).
    SellJunk,
    SellItems,
    Unknown(String),
}

pub fn parse(input: &str) -> Command {
    let v = input.to_lowercase();
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
        "h" => Command::Drink,       // 1000:29fa
        "mh" => Command::BingeDrink, // 1000:2a0c
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
    fn is_case_insensitive_but_does_not_trim() {
        assert_eq!(parse("BMAR"), Command::Dealers);
        assert_eq!(parse("Trn"), Command::Gym);
        // The original refuses both of these, and so does this now. The
        // fold is case-only, so `Unknown` carries the lowercased line with
        // its spaces intact.
        assert_eq!(parse("  BMAR "), Command::Unknown("  bmar ".to_string()));
        assert_eq!(parse(" trn"), Command::Unknown(" trn".to_string()));
    }

    #[test]
    fn exact_match_only_no_prefix_matching() {
        assert_eq!(parse("sv"), Command::Inspect);
        assert_eq!(parse("s"), Command::Stats);
    }

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
        match parse("hp") {
            Command::Unknown(s) => assert_eq!(s, "hp"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }
}
