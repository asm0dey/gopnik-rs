//! The vet (`rep`).
//!
//! `crate::game` keeps the verb (`Command::Vet` -> `Game::enter_shop`), the
//! discovery gate, the intro line, the healthy-skip and the two
//! [`crate::game::IMM_ROWS`] menu rows; this module is the loop top, the two
//! key arms and the two exits.
//!
//! ## What the two keys actually do
//!
//! Before this, `heal_jaw` charged 3 rubles to clear one break and
//! `heal_leg` charged 7 to clear the other, each independently. Neither key
//! works that way:
//!
//! | key | gate | price | effect |
//! |---|---|---|---|
//! | `r` | either break set | 7 | clears **both** |
//! | `h` | hp < hpmax | 3 | `hp += 5`, clamped to hpmax; touches neither break |
//!
//! So `h` is not a jaw at all -- it is the vet's five-point heal, and its
//! menu row says so: `3^7 рубля тебя залатают` ("they patch you up"), against
//! `r`'s `7^7 рублей починят переломы` ("they fix your fractures"). The old
//! reading also gave `pay_and_heal` a "you are healthy" refusal on the key,
//! which no arm has: both arms fail **silently** when their own
//! precondition is unmet, and the `^0Док: вали отсюда ты здоров.` line
//! belongs to the LOOP TOP instead.
//!
//! ## The loop top is a health test, and it ejects
//!
//! The loop top is the join of the healthy-skip and the back edge, so it
//! runs on entry AND before every prompt. The fall-through is the eject, so
//! the player is thrown out the moment they are whole -- which is also how
//! a successful `h` or `r` ends the visit. [`loop_top`] is that block.
//!
//! **The same predicate is spelled twice, independently.**
//! `Game::print_shop_intro`'s early return decides whether the menu prints;
//! [`loop_top`] branches on it directly to decide the eject. Neither is
//! derived from the other.
//!
//! ## Two exit keys, not one
//!
//! `w` and `e` both exit the vet. `e` at the vet only leaves the vet -- it
//! does not quit the game, because the vet's own input never reaches the
//! street's quit compare. [`exits`] is that pair. The vet is the only
//! location in this port with a second exit key.
//!
//! ## The `h` arm's flavour draw
//!
//! The `h` arm's only randomness is a flavour draw: `Random(3)` picks one
//! of three flavour text blocks and changes nothing else; the
//! `^2Здоровья #/#` line prints on all three paths.

use crate::game::Game;
use crate::term;
use crate::text;

/// Whether the vet's compare chain reaches a compare that `key` matches.
///
/// The chain is `r`, `h`, `w` and `e`, in that order. Neither key arm sits
/// behind a gate that skips its own compare -- both gates stand after the
/// compare and inside the arm, which is why they live in [`run_key`] and
/// not here. The two exits are [`exits`]'.
pub(crate) fn key_dispatches(key: &str) -> bool {
    // This `matches!` is the whole two-key chain: `true` for `h`, and
    // `false` for anything the chain does not match falls through to the
    // exits.
    matches!(key, "r" | "h")
}

/// Whether `key` is one of the vet's two exit tokens.
///
/// `w` is the exit key shared everywhere; `e` is the vet's own second one
/// and has no counterpart at the den, the club or the gym.
pub(crate) fn exits(key: &str) -> bool {
    // A `false` here returns to the loop's back edge.
    key == "w" || key == "e"
}

/// Run the arm `key` selected.
pub(crate) fn run_key(g: &mut Game, key: &str) {
    match key {
        "r" => fix_fractures(g),
        "h" => patch_up(g),
        _ => {}
    }
}

/// The loop top. Returns `true` when the visit is over.
///
/// Reached on entry (the healthy-skip past the menu) and from the back
/// edge, so it runs before every prompt. When hp is at maximum and neither
/// break is set it prints and ejects; otherwise it reaches the prompt.
///
/// **The eject is the fall-through, not a branch.** Writing the condition
/// the other way round -- ejecting when any of the three tests jumps --
/// would throw the player out exactly when they need the vet.
pub(crate) fn loop_top(g: &mut Game) -> bool {
    if g.player.hp < g.player.hpmax || g.player.broken_jaw || g.player.broken_leg {
        return false;
    }
    // Prints `^0Док: вали отсюда ты здоров.`.
    term::println(EMITTED[2].1);
    // Ejecting returns control to the street chain.
    g.leave_shop();
    true
}

/// `r` -- `починят переломы`, 7 rubles.
///
/// **One price clears both breaks**, and the gate is an OR: a broken jaw
/// goes straight to the money test, and only a clear jaw is checked against
/// the leg. So a player with two breaks pays 7 once, and a player with none
/// is answered with **silence** -- there is no "you are healthy" literal on
/// this path; that line is [`loop_top`]'s.
///
/// **Two lines print on success, not one.** `^0Ого! да тебя не иначе как
/// грузовик откатал!` precedes `^2Твои переломы залечены.`
fn fix_fractures(g: &mut Game) {
    // An OR of the two break flags; silent when neither holds.
    if !(g.player.broken_jaw || g.player.broken_leg) {
        return;
    }
    // 6 refuses and 7 buys.
    if g.player.money < 7 {
        // Prints `^4Блин халявщик, медицина не бесплатная`.
        term::println(EMITTED[6].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(7_i16);
    g.player.broken_jaw = false;
    g.player.broken_leg = false;

    // Prints `^0Ого! да тебя не иначе как грузовик откатал!`.
    term::println(EMITTED[4].1);
    // Prints `^2Твои переломы залечены.`.
    term::println(EMITTED[5].1);
}

/// `h` -- `тебя залатают`, 3 rubles for +5 hp.
///
/// **Neither break is touched by this arm.** Both breaks are cleared only
/// by the `r` arm.
///
/// **The `Random(3)` is flavour only**: all three outcomes converge on the
/// same result and none of them touches a global. The `0` outcome is the
/// only one with two lines. The draw still moves the RNG stream, which is
/// why it goes through [`crate::rng::Rng::below_at`] rather than being
/// elided.
///
/// **The clamp uses `<=`, not `<`.** At exactly hpmax the clamp store is
/// skipped, which is the same value either way -- but it is a genuine
/// branch of the original and the port records it.
fn patch_up(g: &mut Game) {
    // Silent, not refused.
    if g.player.hp >= g.player.hpmax {
        return;
    }
    // 2 refuses and 3 buys.
    if g.player.money < 3 {
        // Prints `^4Блин халявщик, медицина не бесплатная` -- the same
        // literal the `r` arm refuses with.
        term::println(EMITTED[12].1);
        return;
    }
    g.player.money = g.player.money.wrapping_sub(3_i16);
    g.player.hp += 5;

    if g.player.hp > g.player.hpmax {
        g.player.hp = g.player.hpmax;
    }

    // The draw is `Random(3)`.
    match g.rng.below(3) {
        1 => {
            // Prints `^0Щас гайки подтянем и будешь как новый!`.
            term::println(EMITTED[7].1);
        }
        2 => {
            // Prints `^0Так чё тут у нас? Ага, пара швов и всё будет в
            // порядке.`.
            term::println(EMITTED[8].1);
        }
        // The fall-through of both compares, i.e. a draw of 0, and the
        // only arm with two lines.
        _ => {
            // Prints `^6Эй, Док а зачем тебе паяльник?`.
            term::println(EMITTED[9].1);
            // Prints `^0Док: Молчи животное!`.
            term::println(EMITTED[10].1);
        }
    }
    // Prints `^2Здоровья #/#`, whose two `#` are hp and hpmax, in that
    // order.
    term::println(&text::fill(
        EMITTED[11].1,
        &[i64::from(g.player.hp), i64::from(g.player.hpmax)],
    ));
}

/// `(closes, text)` -- `closes` is true when the line ends here, false
/// when the next literal continues it.
pub(crate) const EMITTED: [(bool, &str); 14] = [
    (
        true,
        "Ты пришел на ремот, к ветеринару напиши  ^6w^7  чтобы уйти",
    ),
    (true, "^0Док: не волнуйся всё зарастёт как на собаке"),
    (true, "^0Док: вали отсюда ты здоров."),
    (false, "^0Ветеренар\\"),
    (true, "^0Ого! да тебя не иначе как грузовик откатал!"),
    (true, "^2Твои переломы залечены."),
    (true, "^4Блин халявщик, медицина не бесплатная"),
    (true, "^0Щас гайки подтянем и будешь как новый!"),
    (
        true,
        "^0Так чё тут у нас? Ага, пара швов и всё будет в порядке.",
    ),
    (true, "^6Эй, Док а зачем тебе паяльник?"),
    (true, "^0Док: Молчи животное!"),
    (true, "^2Здоровья #/#"),
    (true, "^4Блин халявщик, медицина не бесплатная"),
    (true, "^6Сначала найди где находтся эта больница"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::Location;

    /// The location this module owns, named once for the tests below.
    const LOCATION: Location = Location::Vet;
    use crate::model::Fighter;
    use crate::progress::Progress;
    use crate::term::capture;
    use std::io;

    fn player() -> Fighter {
        Fighter {
            name: "Тест".to_string(),
            hp: 20,
            hpmax: 20,
            strength: 5,
            agility: 5,
            vitality: 5,
            luck: 5,
            dmg_min: 1,
            dmg_max: 3,
            ..Fighter::default()
        }
    }

    /// A vet-ready game. `Game::new` already marks the vet found, so only
    /// the standing-in-it half is set here.
    fn vet(money: i16) -> Game {
        let mut g = Game::new(player(), Progress::new(), 12345);
        g.player.money = money;
        assert!(g.places.is_found(LOCATION), "1000:6dc3");
        g.location = LOCATION;
        g
    }

    fn no_input() -> std::iter::Empty<io::Result<String>> {
        std::iter::empty()
    }

    /// One turn at the vet prompt, through the real dispatch in
    /// `Game::shop_turn`, so the wiring -- including the loop top that runs
    /// after the arm -- is under test too.
    fn turn(g: &mut Game, key: &str) -> Vec<String> {
        capture::lines(|| {
            g.shop_turn(LOCATION, key, &mut no_input()).unwrap();
        })
    }

    // -- arm `r` ---------------------------------------------------------

    /// Both breaks clear behind ONE price test. A port that healed one
    /// break per 7 rubles leaves the other set here.
    #[test]
    fn arm_r_clears_both_breaks_for_one_price() {
        let mut g = vet(7);
        g.player.broken_jaw = true;
        g.player.broken_leg = true;
        g.player.hp = 1; // keeps the loop top from ejecting
        assert_eq!(
            turn(&mut g, "r"),
            vec![
                "^0Ого! да тебя не иначе как грузовик откатал!",
                "^2Твои переломы залечены.",
            ],
            "1000:d576 then 1000:d58f"
        );
        assert!(!g.player.broken_jaw, "1000:d558");
        assert!(!g.player.broken_leg, "1000:d55d");
        assert_eq!(g.player.money, 0, "1000:d553 sub 0x7, once");
    }

    /// The jaw case goes straight to the money test, so a leg alone must
    /// reach it through the same gate and heal just the same.
    #[test]
    fn arm_r_runs_from_either_break_alone() {
        for (jaw, leg) in [(true, false), (false, true)] {
            let mut g = vet(7);
            g.player.broken_jaw = jaw;
            g.player.broken_leg = leg;
            g.player.hp = 1;
            assert_eq!(turn(&mut g, "r").len(), 2, "jaw {jaw} leg {leg}");
            assert_eq!(g.player.money, 0);
        }
    }

    /// **No break, no line** -- the `^0Док: вали отсюда ты здоров.` an
    /// earlier port printed here is the LOOP TOP's, and the loop top does
    /// not fire while hp is short either.
    #[test]
    fn arm_r_is_silent_with_no_break_at_all() {
        let mut g = vet(100);
        g.player.hp = 1;
        assert!(turn(&mut g, "r").is_empty(), "1000:d54a");
        assert_eq!(g.player.money, 100, "and nothing is charged");
    }

    /// 6 refuses and 7 buys.
    #[test]
    fn arm_r_refuses_at_six_and_buys_at_seven() {
        let mut g = vet(6);
        g.player.broken_leg = true;
        g.player.hp = 1;
        assert_eq!(
            turn(&mut g, "r"),
            vec!["^4Блин халявщик, медицина не бесплатная"],
            "1000:d5aa"
        );
        assert!(g.player.broken_leg, "no heal on the refusal");
        assert_eq!(g.player.money, 6);
    }

    // -- arm `h` ---------------------------------------------------------

    /// **`h` is not a jaw.** `h` only adds 5 hp; it touches neither break.
    /// A port that cleared a break here fails on the break.
    #[test]
    fn arm_h_adds_five_health_and_touches_neither_break() {
        let mut g = vet(3);
        g.player.hp = 4;
        g.player.broken_jaw = true;
        let out = turn(&mut g, "h");
        assert_eq!(g.player.hp, 9, "1000:d5de add 0x5");
        assert!(g.player.broken_jaw, "1000:d558 is the `r` arm's, not this");
        assert_eq!(g.player.money, 0, "1000:d5d9 sub 0x3");
        assert_eq!(
            out.last().unwrap(),
            "^2Здоровья 9/20",
            "1000:d683, the # pair is 1000:d672 then 1000:d676"
        );
    }

    /// The heal clamps to hpmax, so the last +5 cannot overshoot and the
    /// reported pair is `hpmax/hpmax`.
    #[test]
    fn arm_h_clamps_the_overshoot_to_hpmax() {
        let mut g = vet(3);
        g.player.hp = 18; // 18 + 5 = 23 > 20
        let out = turn(&mut g, "h");
        assert_eq!(g.player.hp, 20, "1000:d5ef");
        assert!(
            out.iter().any(|l| l == "^2Здоровья 20/20"),
            "1000:d683 after the clamp"
        );
    }

    /// At full health the arm does nothing. Nothing is printed by the
    /// ARM; the eject line that follows is the loop top's.
    #[test]
    fn arm_h_is_silent_at_full_health_with_a_break_standing() {
        let mut g = vet(100);
        g.player.broken_leg = true; // keeps the loop top from ejecting
        assert!(turn(&mut g, "h").is_empty(), "1000:d5cc");
        assert_eq!(g.player.money, 100);
    }

    /// 2 refuses and 3 buys -- with the same literal the `r` arm uses at
    /// its own refusal.
    #[test]
    fn arm_h_refuses_at_two_and_buys_at_three() {
        let mut g = vet(2);
        g.player.hp = 4;
        assert_eq!(
            turn(&mut g, "h"),
            vec!["^4Блин халявщик, медицина не бесплатная"],
            "1000:d69e"
        );
        assert_eq!(g.player.hp, 4, "no heal on the refusal");
    }

    /// `Random(3)` splits three ways and all three converge on the same
    /// result. The `0` outcome is the only one with two lines, and every
    /// outcome ends with `^2Здоровья #/#`.
    #[test]
    fn arm_h_has_three_flavour_arms_and_always_reports_the_health() {
        let one = "^0Щас гайки подтянем и будешь как новый!";
        let two = "^0Так чё тут у нас? Ага, пара швов и всё будет в порядке.";
        let zero = "^6Эй, Док а зачем тебе паяльник?";
        let mut seen: Vec<&str> = Vec::new();
        for seed in 1u32..400 {
            let mut g = Game::new(player(), Progress::new(), seed);
            g.player.money = 3;
            g.player.hp = 4;
            g.location = LOCATION;
            let out = turn(&mut g, "h");
            assert_eq!(
                out.last().unwrap(),
                "^2Здоровья 9/20",
                "1000:d66d is on every path"
            );
            let head = out[0].clone();
            if head == zero {
                assert_eq!(out.len(), 3, "1000:d63b then 1000:d654 then 1000:d66d");
                assert_eq!(out[1], "^0Док: Молчи животное!");
            } else {
                assert_eq!(out.len(), 2, "1000:d619 / 1000:d639 skip to 1000:d66d");
            }
            for &l in &[one, two, zero] {
                if head == l && !seen.contains(&l) {
                    seen.push(l);
                }
            }
            if seen.len() == 3 {
                break;
            }
        }
        assert_eq!(seen.len(), 3, "all three arms of 1000:d5fb/1000:d61e");
    }

    // -- the loop top ----------------------------------------------------

    /// The loop top's fall-through is the eject. Healing the last point of
    /// health with `h` must therefore print the doctor's line and put the
    /// player back on the street in the SAME turn -- the back edge returns
    /// to the loop top, not to the prompt.
    #[test]
    fn the_loop_top_ejects_as_soon_as_the_player_is_whole() {
        let mut g = vet(3);
        g.player.hp = 16; // 16 + 5 clamps to 20
        let out = turn(&mut g, "h");
        assert_eq!(
            out.last().unwrap(),
            "^0Док: вали отсюда ты здоров.",
            "1000:d4e5, after 1000:d683"
        );
        assert_eq!(g.location, Location::Street, "1000:d4ea -> 1000:d6c8");
    }

    /// And a standing break holds the player in even at full hp: all three
    /// tests must pass for the fall-through to be reached.
    #[test]
    fn the_loop_top_keeps_a_broken_player_in_at_full_health() {
        for (jaw, leg) in [(true, false), (false, true)] {
            let mut g = vet(0);
            g.player.broken_jaw = jaw;
            g.player.broken_leg = leg;
            assert!(turn(&mut g, "zzz").is_empty(), "1000:d4c8 / 1000:d4cf");
            assert_eq!(g.location, LOCATION);
        }
    }

    /// Entering the vet whole prints the intro, then the doctor's line, and
    /// leaves -- the healthy-skip skips the menu, and the eject is there, not
    /// in the menu block.
    #[test]
    fn entering_whole_prints_the_intro_then_ejects() {
        let mut g = vet(100);
        g.location = Location::Street;
        let out = capture::lines(|| {
            g.enter_vet_for_test();
        });
        assert_eq!(
            out,
            vec![
                "Ты пришел на ремот, к ветеринару напиши  ^6w^7  чтобы уйти",
                "^0Док: вали отсюда ты здоров.",
            ],
            "1000:d3ce then 1000:d4e5; 1000:d3f4 skipped the menu"
        );
        assert_eq!(g.location, Location::Street);
    }

    // -- the exits -------------------------------------------------------

    /// **Two exit keys.** `w` and `e` both exit. `e` is the street's quit
    /// key, but the vet reads its own buffer, so here it only leaves the
    /// vet.
    #[test]
    fn both_w_and_e_leave_the_vet_and_e_does_not_quit() {
        for key in ["w", "e"] {
            let mut g = vet(0);
            g.player.hp = 1;
            assert!(turn(&mut g, key).is_empty(), "{key}");
            assert_eq!(g.location, Location::Street, "{key}: 1000:d6c8");
            // Quitting is `Command::Quit`'s, reached only from the
            // street's own compare on the street buffer; the vet reads its
            // own buffer and can never get there.
            assert!(g.mode_is_street(), "{key} left the shop, it did not quit");
        }
        assert!(exits("w") && exits("e"));
        assert!(!exits("r") && !exits("h"));
    }

    /// **There is no unknown-key line.** The back edge targets the loop
    /// top, not the menu, so a bad key has no refusal literal to print.
    #[test]
    fn an_unrecognised_key_prints_nothing_and_stays_in_the_vet() {
        for key in ["1", "2", "x", "", "hp"] {
            let mut g = vet(100);
            g.player.hp = 1;
            let before = g.player.clone();
            assert!(turn(&mut g, key).is_empty(), "{key:?} must print nothing");
            assert_eq!(g.player, before);
            assert_eq!(g.location, LOCATION, "1000:d6c5 -> 1000:d4ba");
        }
    }

    /// The vet's own input lowercases and does not trim, and
    /// `Game::shop_turn` no longer does either -- but the fold IS case, so
    /// `H` hits and ` H` misses.
    #[test]
    fn the_vet_prompt_is_case_insensitive_but_does_not_trim() {
        let mut g = vet(3);
        g.player.hp = 4;
        assert!(!turn(&mut g, "H").is_empty(), "0eed:0216 lowercases");
        assert_eq!(g.player.hp, 9);
        let mut g = vet(3);
        g.player.hp = 4;
        assert!(turn(&mut g, " H").is_empty(), "a miss here and there");
        assert_eq!(g.player.hp, 4, "and nothing was healed");
    }
}
