//! Combat math, transcribed from the original.
//!
//! Every formula here is from disassembly.
//!
//! Two things drive the shape of this module.
//!
//! **The original is 16-bit.** Every intermediate is a 16-bit word, and
//! the original wraps rather than saturating -- random draws wrap before
//! comparison, luck calculations wrap, and so on. The arithmetic here wraps
//! at the same places.
//!
//! **The draw order is part of the answer.** A blow steps the generator a
//! number of times that depends on what happened, so replaying a fight
//! requires consuming exactly the draws the original did, including the
//! ones whose only visible effect is which taunt was printed. That is why
//! [`resolve_blow_nth`] reports the crit and the break: a caller that
//! had to re-roll them would desynchronise the generator.

use crate::model::Fighter;
use crate::rng::Rng;

/// Agility points consumed per blow.
const PER_BLOW: i16 = 0x12;

/// The hit roll is `Random(100) + 1` and a roll above this always misses,
/// whatever the attacker's agility -- this is the cap behind the status
/// screen's `Точность 90%` special case.
const ACCURACY_CAP: i16 = 90;

/// What one blow did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blow {
    pub hit: bool,
    pub damage: u16,
}

/// Which limb a blow broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Break {
    Jaw,
    Leg,
}

/// One blow, including the results the caller needs in order to apply it
/// without drawing from the generator again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlowOutcome {
    pub hit: bool,
    pub damage: u16,
    /// The `Точный удар!!!` / `Двойной урон!!!` roll landed: `dmg_max` was
    /// added to the damage.
    pub critical: bool,
    /// Which of the three crit lines was printed, `None` when there was no
    /// crit. The draw was always made and always stepped the generator,
    /// and now it also decides what the player reads.
    pub taunt: Option<u16>,
    /// The blow broke the defender's jaw or leg. `None` when the break roll
    /// failed. A limb that is *already* broken still reports here -- the
    /// original re-rolls regardless and only suppresses the message.
    pub broke: Option<Break>,
    /// The зубная защита's roll, and only when it happened: `Some(true)` the
    /// guard failed and the jaw broke anyway, `Some(false)` the guard held
    /// and the jaw did NOT break. `None` means no guard roll was drawn --
    /// the break was a leg, or the defender does not own the guard, or the
    /// jaw was already broken.
    pub jaw_guard: Option<bool>,
}

/// Which half of the round is swinging, and the one piece of defender state
/// that is not on [`Fighter`].
///
/// The blow code exists TWICE in the original -- once with the player
/// swinging and once with the enemy -- and the two copies are the same
/// instruction sequence with the records swapped. One function covers both,
/// but the `Random` CALL SITES differ. The enemy-swinging copy also has a
/// branch its mirror does not: the зубная защита at a player-only item that
/// lives outside the fighter record, so it is carried here rather than on
/// [`Fighter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swing {
    /// `true` for the player-swinging version, `false` for the enemy's.
    pub player_attacking: bool,
    /// Only ever true when the PLAYER is the defender.
    pub defender_tooth_guard: bool,
}

impl Swing {
    /// The player's half of the round.
    pub fn player() -> Swing {
        Swing {
            player_attacking: true,
            defender_tooth_guard: false,
        }
    }

    /// The enemy's half, with the player's зубная защита as it stands.
    pub fn enemy(defender_tooth_guard: bool) -> Swing {
        Swing {
            player_attacking: false,
            defender_tooth_guard,
        }
    }
}

/// The attacker's agility budget for a round, after the defender's agility
/// has eaten into it.
///
/// A loop runs while both budgets are above a threshold: the mine-budget
/// guard at 10, the theirs-budget guard at 18. When one falls to or below
/// its threshold, the loop collapses to a flat 10.
///
/// The messages `Из-за твоей хорошей ловкости враг сможет пнуть тебя раз #
/// вместо #` report this reduction, printing `(budget - 1) div 18 + 1` on
/// each side.
///
/// Neither boundary is observable in the game, because `10 + 18 == 28`:
/// at `mine == 10` the loop guard agrees with the collapse, and at `mine == 28`
/// one more iteration lands exactly on the collapse.
pub fn blow_budget(attacker: &Fighter, defender: &Fighter) -> i16 {
    let mut mine = (attacker.agility as i16).wrapping_add(4);
    let mut theirs = (defender.agility as i16).wrapping_add(4);
    // Budget at 10 or below.
    if mine > 10 {
        // Theirs over 18.
        while theirs > PER_BLOW {
            // Below 28, loop again.
            if mine < 28 {
                mine = 10;
                break;
            }
            mine = mine.wrapping_sub(PER_BLOW);
            theirs = theirs.wrapping_sub(PER_BLOW);
        }
    }
    mine
}

/// The two numbers the agility-reduction report line prints, or `None` when
/// the original prints nothing.
///
/// The two gates do not use the same arithmetic as the two printed numbers:
/// the gate compares plain `budget / 18`, while each number is
/// `(budget - 1) / 18 + 1`. Both truncate toward zero.
///
/// The reduced count is printed first, confirmed by captures like agility 120
/// against 50 printing `ты сможешь пнуть его раз 5 вместо 7`.
pub fn budget_report(attacker: &Fighter, defender: &Fighter) -> Option<(u16, u16)> {
    // Gate: above 18 to print.
    let unreduced = (attacker.agility as i16).wrapping_add(4);
    if unreduced <= PER_BLOW {
        return None;
    }
    // Plain `div 18` on both sides, not the `(x - 1) div 18 + 1` the two `#`s use.
    let reduced = blow_budget(attacker, defender);
    if reduced / PER_BLOW >= unreduced / PER_BLOW {
        return None;
    }
    Some((
        // Pushed first.
        (reduced.wrapping_sub(1) / PER_BLOW + 1) as u16,
        // Pushed second.
        (unreduced.wrapping_sub(1) / PER_BLOW + 1) as u16,
    ))
}

/// How many blows the attacker gets in one round.
///
/// The blow loop is a do-while: always swings once, subtracts 18 from the
/// budget, and swings again while what is left is still positive. So this is
/// `ceil(budget / 18)`, never less than 1. Confirmed: agility 120 gives
/// `- 6 ударов,  Точность 7 удара 80%`, i.e. seven blows.
pub fn blows_per_round(attacker: &Fighter, defender: &Fighter) -> u16 {
    let mut left = blow_budget(attacker, defender);
    let mut blows = 1u16;
    loop {
        left = left.wrapping_sub(PER_BLOW);
        if left < 1 {
            return blows;
        }
        blows += 1;
    }
}

/// Chance in percent that `blow_index` (0-based, within one round) lands.
///
/// The budget left at that point is multiplied by 5 and compared against
/// `Random(100) + 1`; the roll must also be at most 90. So the effective
/// chance is `min(budget * 5, 90)`.
pub fn accuracy_pct_nth(attacker: &Fighter, defender: &Fighter, blow_index: u16) -> u16 {
    let budget = budget_at(blow_budget(attacker, defender), blow_index);
    let pct = budget.wrapping_mul(5);
    pct.clamp(0, ACCURACY_CAP) as u16
}

/// Chance in percent that the round's *first* blow lands.
///
/// With the defender's agility left out this is the status screen's
/// `Точность (20 + Ловкость*5)%`, capped at 90: above 14 agility the screen
/// prints `agility * 5 + 20` or a flat `Точность 90%`. `blow_budget` is
/// `agility + 4`, so `budget * 5` and `agility * 5 + 20` are the same.
pub fn accuracy_pct(attacker: &Fighter, defender: &Fighter) -> u16 {
    accuracy_pct_nth(attacker, defender, 0)
}

/// Chance in percent that the attacker's second blow of a round lands, 0 if
/// there is no second blow.
///
/// This is the status screen's `Второй удар #%`. Below 15 agility the screen
/// prints plain `Точность #%` and no second blow at all. The calculation
/// `agility - 14` is exactly `blow_budget - 18` for an unopposed attacker.
pub fn second_blow_pct(attacker: &Fighter) -> u16 {
    if attacker.agility < 15 {
        return 0;
    }
    let budget = (attacker.agility as i16).wrapping_sub(14);
    budget.wrapping_mul(5).clamp(0, ACCURACY_CAP) as u16
}

fn budget_at(budget: i16, blow_index: u16) -> i16 {
    budget.wrapping_sub(PER_BLOW.wrapping_mul(blow_index as i16))
}

/// Resolve the round's first blow. See [`resolve_blow_nth`] for later ones.
pub fn resolve_blow(rng: &mut Rng, attacker: &Fighter, defender: &Fighter) -> Blow {
    let o = resolve_blow_nth(rng, attacker, defender, 0, Swing::player());
    Blow {
        hit: o.hit,
        damage: o.damage,
    }
}

/// Resolve one blow of a round, stepping the RNG exactly as the original does.
///
/// Draw order, and it matters:
///
/// 1. `Random(100)` -- the hit roll, always.
/// 2. on a hit, `Random(dmg_max - dmg_min)` -- the damage roll.
/// 3. on a hit, `Random(100)` -- the crit roll.
/// 4. on a crit, `Random(3)` -- which of three crit taunts. Steps the seed.
/// 5. on a hit, `Random(defender.luck * 3 + 200)` -- the break roll.
/// 6. on a break, `Random(2)` -- jaw (0) or leg (1). Drawn even when
///    already broken; only the message is suppressed.
/// 7. on a JAW break when the player is the defender, owns the зубная защита,
///    and does not already have a broken jaw: `Random(4)` decides whether the
///    guard saves the teeth. Enemy-swinging only. Costs a draw on the FIRST
///    jaw break of a guarded player and never again.
pub fn resolve_blow_nth(
    rng: &mut Rng,
    attacker: &Fighter,
    defender: &Fighter,
    blow_index: u16,
    swing: Swing,
) -> BlowOutcome {
    let miss = BlowOutcome {
        hit: false,
        damage: 0,
        critical: false,
        taunt: None,
        broke: None,
        jaw_guard: None,
    };

    // 1. Hit roll within budget and at most 90.
    //    The cap applies everywhere in both directions.
    let roll = (rng.below(100) as i16).wrapping_add(1);
    let budget = budget_at(blow_budget(attacker, defender), blow_index);
    if budget.wrapping_mul(5) < roll || roll > ACCURACY_CAP {
        return miss;
    }

    // 2. Damage: dmg_min + Random(dmg_max - dmg_min) + 1, i.e. uniform over
    //    dmg_min+1 ..= dmg_max.
    let span = attacker.dmg_max.wrapping_sub(attacker.dmg_min);
    let rolled = rng.below(span);
    let mut damage = attacker.dmg_min.wrapping_add(rolled).wrapping_add(1) as i16;

    // 3./4. Crit: Random(100) + 1 < attacker.luck * 3, compared as a signed
    //       32-bit value.
    let crit_roll = (rng.below(100) as i32) + 1;
    let attacker_luck3 = (attacker.luck.wrapping_mul(3)) as i16 as i32;
    //       The high-word test is TWO branches, not one. All four combinations
    //       across the two swinger directions are the same comparison.
    let critical = attacker_luck3 > crit_roll;
    let mut taunt = None;
    if critical {
        damage = damage.wrapping_add(attacker.dmg_max as i16);
        taunt = Some(rng.below(3));
    }

    // Armour is subtracted from the blow and the result is floored at 0.
    // A blow lighter than the armour does not wrap or heal the defender.
    damage = damage.wrapping_sub(i16::from(defender.armor));
    if damage < 0 {
        damage = 0;
    }

    // 5./6. Break: Random(defender.luck * 3 + 200) + 1 < attacker.luck * 3,
    //       then Random(2) picks jaw (0) or leg (1).
    let break_bound = defender.luck.wrapping_mul(3).wrapping_add(200);
    let break_roll = (rng.below(break_bound) as i32) + 1;
    let mut jaw_guard = None;
    //       The break's high-word test is the same two-branch shape as the crit's.
    let broke = if attacker_luck3 > break_roll {
        // A non-zero draw is the LEG. Zero is the jaw.
        if rng.below(2) == 0 {
            // 7. The зубная защита, enemy-swinging only. When the jaw is
            //    already broken or the guard is not owned, the draw is skipped.
            //    So the extra `Random(4)` costs a draw only on the first jaw
            //    break of a guarded player, and `0` (`or ax,ax` / `jnz`) breaks
            //    it anyway.
            if swing.defender_tooth_guard && !defender.broken_jaw {
                jaw_guard = Some(rng.below(4) == 0);
            }
            Some(Break::Jaw)
        } else {
            Some(Break::Leg)
        }
    } else {
        None
    };

    BlowOutcome {
        hit: true,
        damage: damage as u16,
        critical,
        taunt,
        broke,
        jaw_guard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(agility: u16) -> Fighter {
        Fighter {
            agility,
            ..Default::default()
        }
    }

    #[test]
    fn accuracy_matches_the_status_screen_formula() {
        // Agility < 15: `agility * 5 + 20`. Otherwise 90.
        let weak = f(0);
        for agility in 0..15u16 {
            assert_eq!(
                accuracy_pct(&f(agility), &weak),
                agility * 5 + 20,
                "agility {agility}"
            );
        }
        for agility in 15..40u16 {
            assert_eq!(accuracy_pct(&f(agility), &weak), 90, "agility {agility}");
        }
    }

    #[test]
    fn second_blow_matches_the_status_screen_formula() {
        // `(agility - 14) * 5`, and nothing below 15.
        assert_eq!(second_blow_pct(&f(14)), 0);
        assert_eq!(second_blow_pct(&f(15)), 5);
        assert_eq!(second_blow_pct(&f(20)), 30);
        // SAVE_R2: the game printed "Точность 90%    Второй удар 5%".
        assert_eq!(second_blow_pct(&f(15)), 5);
    }

    #[test]
    fn blow_count_matches_the_status_screen() {
        // SAVE_R5, agility 120, printed "Точность 90% - 6 ударов, Точность
        // 7 удара 80%" against no opponent worth the name.
        let weak = f(0);
        assert_eq!(blows_per_round(&f(120), &weak), 7);
        assert_eq!(accuracy_pct_nth(&f(120), &weak, 6), 80);
        // The boundary: budget 18 is one blow, 19 is two.
        assert_eq!(blows_per_round(&f(14), &weak), 1);
        assert_eq!(blows_per_round(&f(15), &weak), 2);
    }

    /// Both boundaries in [`blow_budget`] are UNOBSERVABLE, and this test says why:
    /// the three constants are one arithmetic identity. The three collapse
    /// at the same point: the boundaries cannot be separated by any test.
    #[test]
    fn the_blow_budget_boundaries_are_unobservable() {
        assert_eq!(
            28 - PER_BLOW,
            10,
            "1000:3fc9's 0x1c less 1000:3fd4's 0x12 is 1000:3fe2's 0x0a"
        );
        for d in 0..=255u16 {
            // mine == 10, the guard's own bound: entering the loop either
            // leaves at once or collapses to the same 10.
            assert_eq!(blow_budget(&f(6), &f(d)), 10, "agility 6 against {d}");
            // mine == 28, the collapse bound: one subtraction lands ON the
            // collapse value, so collapsing early changes nothing.
            let want = if (d as i16 + 4) > PER_BLOW { 10 } else { 28 };
            assert_eq!(blow_budget(&f(24), &f(d)), want, "agility 24 against {d}");
        }
    }

    /// The report line's two gates and its two numbers.
    ///
    /// The numbers come from the live capture quoted in
    /// `a_fast_defender_cuts_the_budget`, not from `budget_report` itself:
    /// the game printed `раз 5 вместо 7` for the player's line and
    /// `раз 1 вместо 3` for the enemy's, in that district-5 fight.
    #[test]
    fn the_agility_report_prints_the_captured_pair() {
        let player = f(120);
        let enemy = f(50);
        // Opponent's report: "1 вместо 3".
        assert_eq!(budget_report(&player, &enemy), Some((5, 7)));
        // Player's report: "5 вместо 7".
        assert_eq!(budget_report(&enemy, &player), Some((1, 3)));
        // ... and each `#` is `blows_per_round` of the budget either side of
        // the reduction, which is what makes the pair readable as blows.
        assert_eq!(
            budget_report(&player, &enemy),
            Some((
                blows_per_round(&player, &enemy),
                blows_per_round(&player, &f(0))
            ))
        );
    }

    /// Gate 1 alone -- `unreduced <= 18` -- against the ONE input where
    /// nothing else can refuse for it.
    ///
    /// Gate 2 returns `None` when the defender is too slow, so gate 1 is
    /// the only test that can refuse when both attacker and defender are
    /// fast. Without gate 1 the game prints `раз 1 вместо 1` at attacker
    /// agility 14 against a fast enough defender -- a visible wrong line
    /// in a reachable fight.
    #[test]
    fn gate_1_is_the_only_thing_refusing_the_report_at_agility_14() {
        // A defender fast enough that the collapse always runs, so gate 2
        // would PASS for every attacker whose unreduced budget reaches 18.
        let fast = f(200);
        for agility in 0..=255u16 {
            let got = budget_report(&f(agility), &fast);
            assert_eq!(
                got.is_none(),
                agility <= 14,
                "agility {agility} against a fast defender: 1000:3ff5 refuses \
                 at or below an unreduced budget of 18 and nowhere else, got {got:?}"
            );
        }
        // The boundary, spelled out: 14 is silent only because of gate 1 --
        // delete it and this becomes `Some((1, 1))`, `раз 1 вместо 1`.
        assert_eq!(budget_report(&f(14), &f(15)), None, "budget 18, not above");
        assert_eq!(budget_report(&f(15), &f(15)), Some((1, 2)), "budget 19");
    }

    /// Gate 2 alone -- `reduced < unreduced` -- returns `None` when the
    /// defender is too slow to eat into the budget.
    #[test]
    fn gate_2_is_silent_when_the_reduction_costs_no_blow() {
        let weak = f(0);
        for agility in 0..=255u16 {
            assert_eq!(
                budget_report(&f(agility), &weak),
                None,
                "an unopposed attacker of agility {agility} has nothing to report"
            );
        }
        // The gate is on `div 18`, not on the budget moving at all: agility
        // 24 against 15 drops the budget from 28 to 10, and 28 div 18 is 1
        // just as 10 div 18 is 0 -- so this one DOES print, while agility 22
        // (26 -> 10, 1 -> 0) is the same shape. The pair that must NOT print
        // is one whose collapse leaves both divisions equal.
        assert_eq!(budget_report(&f(24), &f(15)), Some((1, 2)));
        // `mine == 10` never enters the loop, so there is no reduction
        // to report.
        assert_eq!(
            budget_report(&f(6), &f(200)),
            None,
            "budget 10: 1000:3fc0 skips the collapse"
        );
    }

    #[test]
    fn a_fast_defender_cuts_the_budget() {
        // Live capture: agility 120 vs 50 printed "ты сможешь пнуть его раз 5
        // вместо 7", and 50 vs 120 printed "враг сможет пнуть тебя раз 1 вместо 3".
        let player = f(120);
        let enemy = f(50);
        assert_eq!(blows_per_round(&player, &f(0)), 7);
        assert_eq!(blows_per_round(&player, &enemy), 5);
        assert_eq!(blows_per_round(&enemy, &f(0)), 3);
        assert_eq!(blows_per_round(&enemy, &player), 1);
    }
}
