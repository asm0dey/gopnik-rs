//! Combat math, transcribed from `FUN_1000_3d11` (`1000:3d11`).
//!
//! Every formula here cites the Ghidra address it came from; the full
//! derivation, with disassembly, is in `docs/re/combat.md`.
//!
//! Two things drive the shape of this module.
//!
//! **The original is 16-bit.** Every intermediate below is an 8086 word, and
//! the original wraps rather than saturating -- `Random(dmg_max - dmg_min)`
//! passes a wrapped `Word` to `System.Random`, `luck * 3` wraps before being
//! sign-extended for comparison, and so on. The arithmetic here wraps at the
//! same places, so a nonsensical `Fighter` produces the same nonsense the
//! original would rather than a Rust panic or a silently different answer.
//!
//! **The draw order is part of the answer.** A blow steps the generator a
//! number of times that depends on what happened, so anything that replays a
//! fight has to consume exactly the draws the original did, including the
//! ones whose only visible effect is which taunt got printed. That is why
//! [`resolve_blow_nth`] reports the crit and the break: a caller that had to
//! re-roll them itself would desynchronise the generator.

use crate::model::Fighter;
use crate::rng::Rng;

/// Agility points consumed per blow. `1000:3fd4`/`1000:3fdb`, where the
/// budget reduction subtracts it, and `1000:4624`, where the blow loop does.
const PER_BLOW: i16 = 0x12;

/// The hit roll is `Random(100) + 1` and a roll above this always misses,
/// whatever the attacker's agility -- `1000:447f`, `cmp [bp-0x112],0x5a`.
/// This is the cap behind the status screen's `Точность 90%` special case
/// (`1000:15a4`).
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
    /// The blow broke the defender's jaw or leg. `None` when the break roll
    /// failed. A limb that is *already* broken still reports here -- the
    /// original re-rolls regardless and only suppresses the message.
    pub broke: Option<Break>,
}

/// The attacker's agility budget for a round, after the defender's agility
/// has eaten into it.
///
/// `1000:3fa7`..`1000:3fec` computes the *enemy's* budget with the player's
/// agility eating into it; `1000:404a`..`1000:408f` is the same code again
/// with the two records swapped, for the player's budget. Reading the first
/// copy instruction by instruction:
///
/// * `1000:3fa7` `mine := agility + 4`, `1000:3fb1` `theirs := agility + 4`
/// * `1000:3fbb` `cmp mine,0x0a / jng` -- nothing happens at 10 or below
/// * `1000:3fc2` `cmp theirs,0x12 / jng` -- the loop runs while `theirs > 18`
/// * `1000:3fc9` `cmp mine,0x1c / jl 3fe2` -- below 28, jump to the collapse
/// * `1000:3fd4`/`1000:3fdb` -- otherwise both lose 18 and go round again
/// * `1000:3fe2` `mov mine,0x0a` -- the collapse, a flat 10 (mirror at
///   `1000:4085`)
///
/// The messages `Из-за твоей хорошей ловкости враг сможет пнуть тебя раз #
/// вместо #` (`1000:4013`) and its mirror (`1000:40b6`) report this
/// reduction, printing `(budget - 1) div 18 + 1` either side of it
/// (`1000:4018`).
pub fn blow_budget(attacker: &Fighter, defender: &Fighter) -> i16 {
    let mut mine = (attacker.agility as i16).wrapping_add(4);
    let mut theirs = (defender.agility as i16).wrapping_add(4);
    if mine > 10 {
        while theirs > PER_BLOW {
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

/// How many blows the attacker gets in one round.
///
/// The blow loop at `1000:445c`..`1000:4660` is a do-while: it always swings
/// once, subtracts 18 from the budget (`1000:4624`), and swings again while
/// what is left is still positive (`1000:4652`, `cmp [bp-0x10e],0x0 / jng`
/// leaves the loop). So this is `ceil(budget / 18)`, and never less than 1.
/// Live check: `SAVE_R5`, agility 120, printed `- 6 ударов,  Точность 7
/// удара 80%`, i.e. seven blows.
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
/// `1000:446a`..`1000:4476`: the budget left at that point is multiplied by
/// 5 (`shl`, `shl`, `add`) and compared against `Random(100) + 1`; the roll
/// must also be at most 90 (`1000:447f`, `cmp [bp-0x112],0x5a`). So the
/// effective chance is `min(budget * 5, 90)`, clamped at 0. The enemy's copy
/// is `1000:468d`..`1000:46a7`.
pub fn accuracy_pct_nth(attacker: &Fighter, defender: &Fighter, blow_index: u16) -> u16 {
    let budget = budget_at(blow_budget(attacker, defender), blow_index);
    let pct = budget.wrapping_mul(5);
    pct.clamp(0, ACCURACY_CAP) as u16
}

/// Chance in percent that the round's *first* blow lands.
///
/// With the defender's agility left out this is the status screen's
/// `Точность (20 + Ловкость*5)%`, capped at 90: `1000:1574` tests
/// `agility > 14` and prints `agility * 5 + 20` (`1000:157b`) or a flat
/// `Точность 90%` (`1000:15a4`). The in-game help text at `1000:613e` says
/// the same thing in words. `blow_budget` is `agility + 4`, so `budget * 5`
/// and `agility * 5 + 20` are the same number.
pub fn accuracy_pct(attacker: &Fighter, defender: &Fighter) -> u16 {
    accuracy_pct_nth(attacker, defender, 0)
}

/// Chance in percent that the attacker's second blow of a round lands, 0 if
/// there is no second blow.
///
/// This is the status screen's `Второй удар #%` (`1000:15e7`): `1000:1574`
/// tests `agility > 14` -- below that the screen prints plain `Точность #%`
/// and no second blow at all -- and `1000:15c1` subtracts 14 before the
/// print multiplies by 5. `agility - 14` is exactly `blow_budget - 18` for
/// an unopposed attacker, so this agrees with `accuracy_pct_nth(.., 1)`
/// against a defender whose agility is low enough not to eat into the
/// budget. Live check: `SAVE_R2`, agility 15, printed
/// `Точность 90%    Второй удар 5%`.
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
    let o = resolve_blow_nth(rng, attacker, defender, 0);
    Blow {
        hit: o.hit,
        damage: o.damage,
    }
}

/// Resolve blow `blow_index` (0-based) of a round, stepping `rng` exactly as
/// the original does.
///
/// The whole body is `1000:445c`..`1000:4624` (the player swinging) and
/// `1000:467f`..`1000:4867` (the enemy swinging) -- the same instruction
/// sequence twice with the two records' addresses swapped, which is why one
/// function covers both directions. Addresses below are given as
/// player-swinging / enemy-swinging.
///
/// Draw order, and it matters:
///
/// 1. `Random(100)` -- the hit roll, always (`1000:4460` / `1000:4683`).
/// 2. on a hit, `Random(dmg_max - dmg_min)` -- the damage roll, a 16-bit
///    `sub` whose result is passed as a `Word` (`1000:4497` / `1000:46ba`).
/// 3. on a hit, `Random(100)` -- the crit roll (`1000:44b8` / `1000:46db`).
/// 4. on a crit, `Random(3)` -- which of three crit taunts to print
///    (`1000:44e3` / `1000:4706`). Nothing else depends on it, but it steps
///    the seed.
/// 5. on a hit, `Random(defender.luck * 3 + 200)` -- the break roll
///    (`1000:4571` / `1000:4794`).
/// 6. on a break, `Random(2)` -- jaw (0) or leg (1) (`1000:4595` /
///    `1000:47be`). Drawn even when that limb is already broken; only the
///    message is suppressed (`1000:459e` / `1000:47c7`).
///
/// Armour is subtracted and the result floored at zero in between, at
/// `1000:4546` / `1000:4769`; no draw there.
///
/// Not modelled: when the *player* is the defender and owns the зубная
/// защита (`DS:394a`), a jaw break draws one more `Random(4)` (`1000:47fa`)
/// to decide whether the guard saves the teeth. `Fighter` has no field for
/// that item, so a fight where the player is the defender and owns it will
/// desynchronise here. See the open questions in `docs/re/combat.md`.
pub fn resolve_blow_nth(
    rng: &mut Rng,
    attacker: &Fighter,
    defender: &Fighter,
    blow_index: u16,
) -> BlowOutcome {
    let miss = BlowOutcome {
        hit: false,
        damage: 0,
        critical: false,
        broke: None,
    };

    // 1. Hit roll: Random(100) + 1 must be within budget*5 and at most 90.
    let roll = (rng.below(100) as i16).wrapping_add(1);
    let budget = budget_at(blow_budget(attacker, defender), blow_index);
    if budget.wrapping_mul(5) < roll || roll > ACCURACY_CAP {
        return miss;
    }

    // 2. Damage: dmg_min + Random(dmg_max - dmg_min) + 1, i.e. uniform over
    //    dmg_min+1 ..= dmg_max. The subtraction is a 16-bit `sub` whose
    //    result is passed to Random as a Word (1000:448f / 1000:46b5).
    let span = attacker.dmg_max.wrapping_sub(attacker.dmg_min);
    let rolled = rng.below(span);
    let mut damage = attacker.dmg_min.wrapping_add(rolled).wrapping_add(1) as i16;

    // 3./4. Crit: Random(100) + 1 < attacker.luck * 3, compared as a signed
    //       32-bit value against the sign-extended product -- luck*3 wraps
    //       in 16 bits, then `cwd` sign-extends it, and the comparison is
    //       Borland's high-word-signed/low-word-unsigned pair
    //       (1000:44cd..1000:44d6 / 1000:46f0..1000:46f9).
    let crit_roll = (rng.below(100) as i32) + 1;
    let attacker_luck3 = (attacker.luck.wrapping_mul(3)) as i16 as i32;
    let critical = attacker_luck3 > crit_roll;
    if critical {
        damage = damage.wrapping_add(attacker.dmg_max as i16);
        let _taunt = rng.below(3);
    }

    // Armour is a byte in the record, zero-extended before the subtraction,
    // and the result is floored at 0 with a *signed* test
    // (1000:4546..1000:4558 / 1000:4769..1000:477b).
    damage = damage.wrapping_sub((defender.armor & 0x00ff) as i16);
    if damage < 0 {
        damage = 0;
    }

    // 5./6. Break: Random(defender.luck * 3 + 200) + 1 < attacker.luck * 3,
    //       compared the same way as the crit, then Random(2) picks jaw (0)
    //       or leg (1) (1000:4564..1000:4595 / 1000:4787..1000:47be).
    let break_bound = defender.luck.wrapping_mul(3).wrapping_add(200);
    let break_roll = (rng.below(break_bound) as i32) + 1;
    let broke = if attacker_luck3 > break_roll {
        Some(if rng.below(2) == 0 {
            Break::Jaw
        } else {
            Break::Leg
        })
    } else {
        None
    };

    BlowOutcome {
        hit: true,
        damage: damage as u16,
        critical,
        broke,
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
        // 1000:157b prints agility*5 + 20 while agility < 15, else 90.
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
        // 1000:15e7 prints (agility - 14)*5, and nothing at all below 15.
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

    #[test]
    fn a_fast_defender_cuts_the_budget() {
        // Captured live (district 5, docs/re/combat.md): the player's
        // agility 120 against the enemy's 50 printed "ты сможешь пнуть его
        // раз 5 вместо 7", and the enemy's 50 against 120 printed "враг
        // сможет пнуть тебя раз 1 вместо 3".
        let player = f(120);
        let enemy = f(50);
        assert_eq!(blows_per_round(&player, &f(0)), 7);
        assert_eq!(blows_per_round(&player, &enemy), 5);
        assert_eq!(blows_per_round(&enemy, &f(0)), 3);
        assert_eq!(blows_per_round(&enemy, &player), 1);
    }
}
