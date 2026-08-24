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
    /// Which of the three crit lines the `Random(3)` at `1000:44e3` /
    /// `1000:4706` picked, `None` when there was no crit. The draw was always
    /// made and always discarded before Task 13 -- it decides only which line
    /// is printed, but it steps the generator, and now it also decides what
    /// the player reads.
    pub taunt: Option<u16>,
    /// The blow broke the defender's jaw or leg. `None` when the break roll
    /// failed. A limb that is *already* broken still reports here -- the
    /// original re-rolls regardless and only suppresses the message.
    pub broke: Option<Break>,
    /// The зубная защита's roll, and only when it happened: `Some(true)` the
    /// guard failed and the jaw broke anyway (`1000:4820`), `Some(false)` the
    /// guard held and the jaw did NOT break (`1000:4827`). `None` means no
    /// `Random(4)` was drawn -- the break was a leg, or the defender does not
    /// own the guard, or the jaw was already broken.
    pub jaw_guard: Option<bool>,
}

/// Which half of the round is swinging, and the one piece of defender state
/// that is not on [`Fighter`].
///
/// The blow code exists TWICE in the original -- `1000:445c`..`1000:4660`
/// with the player swinging and `1000:467f`..`1000:4867` with the enemy --
/// and the two copies are the same instruction sequence with the records
/// swapped. One function covers both, but the `Random` CALL SITES differ, and
/// `data/combat_trace.json` records the site of every draw, so which copy is
/// running has to be said rather than inferred.
///
/// The enemy-swinging copy also has a branch its mirror does not: the
/// зубная защита at `20ae:394a`. It is a player-only item that lives outside
/// the fighter record, so it is carried here rather than on [`Fighter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swing {
    /// `true` for `1000:445c`..`1000:4660`, `false` for the enemy's mirror.
    pub player_attacking: bool,
    /// `20ae:394a` -- only ever true when the PLAYER is the defender, i.e.
    /// when `player_attacking` is false.
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

    fn site(self, player: &'static str, enemy: &'static str) -> &'static str {
        if self.player_attacking {
            player
        } else {
            enemy
        }
    }
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
///
/// **Neither boundary below is observable**, because `0x0a + 0x12 == 0x1c`:
/// at `mine == 10` the guard's two senses agree, and at `mine == 28` one
/// more turn round the loop lands exactly on the collapse. `> 10` and
/// `>= 10`, and `< 28` and `<= 28`, are therefore the same program -- the
/// two skips in `.cargo/mutants.toml` say so with their addresses, the
/// argument is in `docs/re/combat.md`, and
/// `the_blow_budget_boundaries_are_unobservable` reds if the identity
/// breaks. This is the opposite of the `1000:4629` / `1000:48cd` asymmetry
/// in the blow loops, where the two senses genuinely differ.
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
    let o = resolve_blow_nth(rng, attacker, defender, 0, Swing::player());
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
/// Two places where they are NOT the same sequence, both outside this
/// function: the enemy-swinging copy has the зубная защита branch
/// (`1000:47c7`..`1000:4840`, see [`Swing`]), and the two loop TAILS test the
/// defender differently -- `1000:4629` `jg` against `1000:48cd` `jl`, plus a
/// defender check before the player's `ещё раз` line that the enemy's copy
/// does not have. `crate::game::Game::combat_round` writes those out
/// separately; `docs/re/gaps.md`, "Opened by Task 13", has the addresses.
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
/// 7. on a JAW break, and only when the *player* is the defender, owns the
///    зубная защита (`DS:394a`) and does not already have a broken jaw:
///    `Random(4)` at `1000:47fe` decides whether the guard saves the teeth.
///    Enemy-swinging only -- the player-swinging copy has no such branch --
///    and gated by `1000:47c7` `cmp byte [0x38b0],0` / `jnz 0x4840`, so it
///    costs a draw on the FIRST jaw break of a guarded player and never
///    again. The item is not on `Fighter`; it is carried on [`Swing`],
///    because it lives outside the fighter record in the original too.
///    (`1000:47fa` is the `mov ax,4` / `push ax` argument idiom, not the
///    call -- `docs/re/combat.md`, "Player-only branch", records that
///    near-miss.)
///
/// Armour is subtracted and the result floored at zero in between, at
/// `1000:4546` / `1000:4769`; no draw there.
///
/// **UNVERIFIED as behaviour**: step 7 is a transcription. No draw at
/// `1000:47fe` appears in `data/combat_trace.json` -- run C loads the save
/// that ships the guard, but no jaw break landed on the player there -- and
/// `tools/capture_combat_vectors.py` skips the rounds that could have
/// exercised it. What would settle it: a capture in which a guarded player's
/// jaw is broken.
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

    // 1. Hit roll: Random(100) + 1 must be within budget*5 and at most 90.
    let roll = (rng.below_at(swing.site("1000:4460", "1000:4683"), 100) as i16).wrapping_add(1);
    let budget = budget_at(blow_budget(attacker, defender), blow_index);
    if budget.wrapping_mul(5) < roll || roll > ACCURACY_CAP {
        return miss;
    }

    // 2. Damage: dmg_min + Random(dmg_max - dmg_min) + 1, i.e. uniform over
    //    dmg_min+1 ..= dmg_max. The subtraction is a 16-bit `sub` whose
    //    result is passed to Random as a Word (1000:448f / 1000:46b5).
    let span = attacker.dmg_max.wrapping_sub(attacker.dmg_min);
    let rolled = rng.below_at(swing.site("1000:4497", "1000:46ba"), span);
    let mut damage = attacker.dmg_min.wrapping_add(rolled).wrapping_add(1) as i16;

    // 3./4. Crit: Random(100) + 1 < attacker.luck * 3, compared as a signed
    //       32-bit value against the sign-extended product -- luck*3 wraps
    //       in 16 bits, then `cwd` sign-extends it, and the comparison is
    //       Borland's high-word-signed/low-word-unsigned pair
    //       (1000:44cd..1000:44d6 / 1000:46f0..1000:46f9).
    let crit_roll = (rng.below_at(swing.site("1000:44b8", "1000:46db"), 100) as i32) + 1;
    let attacker_luck3 = (attacker.luck.wrapping_mul(3)) as i16 as i32;
    let critical = attacker_luck3 > crit_roll;
    let mut taunt = None;
    if critical {
        damage = damage.wrapping_add(attacker.dmg_max as i16);
        taunt = Some(rng.below_at(swing.site("1000:44e3", "1000:4706"), 3));
    }

    // Armour is a byte in the record, zero-extended before the subtraction,
    // and the result is floored at 0 with a *signed* test
    // (1000:4546..1000:4558 / 1000:4769..1000:477b). The bound and the value
    // stored are both 0, so `< 0` and `<= 0` are the same program -- the
    // third skip in `.cargo/mutants.toml`. `== 0` is NOT: it would let a
    // blow lighter than the armour wrap to 65482 and, at 1000:4560
    // `sub [0x3962],ax`, heal the defender. That one is killed by
    // `armour_heavier_than_the_blow_floors_the_damage_at_zero`.
    damage = damage.wrapping_sub((defender.armor & 0x00ff) as i16);
    if damage < 0 {
        damage = 0;
    }

    // 5./6. Break: Random(defender.luck * 3 + 200) + 1 < attacker.luck * 3,
    //       compared the same way as the crit, then Random(2) picks jaw (0)
    //       or leg (1) (1000:4564..1000:4595 / 1000:4787..1000:47be).
    let break_bound = defender.luck.wrapping_mul(3).wrapping_add(200);
    let break_roll = (rng.below_at(swing.site("1000:4571", "1000:4794"), break_bound) as i32) + 1;
    let mut jaw_guard = None;
    let broke = if attacker_luck3 > break_roll {
        if rng.below_at(swing.site("1000:4595", "1000:47be"), 2) == 0 {
            // 7. The зубная защита, enemy-swinging only. `1000:47c7`
            //    `cmp byte [0x38b0],0` / `jnz 0x4840` skips everything when
            //    the jaw is ALREADY broken -- including the draw -- and
            //    `1000:47f3` `cmp byte [0x394a],0` / `jz 0x4840` skips it
            //    when the guard is not owned. So the extra `Random(4)` at
            //    `1000:47fe` costs a draw only on the first jaw break of a
            //    guarded player, and `0` (`or ax,ax` / `jnz 0x4827`) breaks
            //    it anyway.
            if swing.defender_tooth_guard && !defender.broken_jaw {
                jaw_guard = Some(rng.below_at("1000:47fe", 4) == 0);
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

    /// `data/rng_vectors.json`'s seed-0 `RandSeed` chain.
    ///
    /// It was produced by `tools/gen_rng_vectors.py`, which decodes and
    /// interprets `@Rand`'s own instruction bytes out of `orig/g.exe` -- it
    /// is NOT generated from this port, which is why the draw values below
    /// are an oracle rather than a restatement of `Rng`.
    fn ground_truth_states() -> Vec<u32> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/rng_vectors.json");
        let bytes = std::fs::read(path).expect("read data/rng_vectors.json");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
        let block = &v["seeds"][0];
        assert_eq!(
            block["seed"].as_u64(),
            Some(0),
            "seeds[0] is the seed-0 chain"
        );
        block["next_u32"]
            .as_array()
            .expect("next_u32")
            .iter()
            .map(|x| x.as_u64().expect("u32") as u32)
            .collect()
    }

    /// `Random(n)` given the RandSeed the draw stepped TO: the high half of
    /// the 32x16 widening multiply (`0f78:1152`..`0f78:1163`, listed
    /// instruction by instruction in `docs/re/METHODOLOGY.md`).
    fn random_of(state: u32, n: u16) -> u16 {
        ((state as u64 * n as u64) >> 32) as u16
    }

    fn draws(rng: &mut Rng) -> Vec<crate::rng::Draw> {
        rng.take_log()
    }

    fn want(sites_and_n: &[(&'static str, u16)], states: &[u32]) -> Vec<crate::rng::Draw> {
        sites_and_n
            .iter()
            .zip(states)
            .map(|(&(site, n), &state)| crate::rng::Draw {
                site,
                n,
                r: random_of(state, n),
            })
            .collect()
    }

    /// A brawler who hits, crits and breaks something on every swing.
    ///
    /// `luck * 3` = 900 is above every `Random(100) + 1` (the crit,
    /// `1000:44cd`) and above every `Random(defender.luck * 3 + 200) + 1`
    /// (the break, `1000:47b3`), so both comparisons are decided by the
    /// stats and the DRAW SHAPE is what the seed decides. `agility 20` gives
    /// `blow_budget` 24 against an agility-0 defender, i.e. `24 * 5 = 120`
    /// capped at the `1000:447f` accuracy cap of 90.
    fn brawler() -> Fighter {
        Fighter {
            agility: 20,
            luck: 300,
            dmg_min: 1,
            dmg_max: 3,
            hp: 50,
            hpmax: 50,
            ..Default::default()
        }
    }

    /// The six draws every landed-crit-and-break swing spends, player half
    /// then enemy half. Order and `n` are `resolve_blow_nth`'s doc block,
    /// i.e. `1000:445c`..`1000:4624` and `1000:467f`..`1000:4867`.
    const PLAYER_SWING: [(&str, u16); 6] = [
        ("1000:4460", 100),
        ("1000:4497", 2),
        ("1000:44b8", 100),
        ("1000:44e3", 3),
        ("1000:4571", 200),
        ("1000:4595", 2),
    ];
    /// `dmg_max - dmg_min` for [`brawler`] is 2, which is the `n` at
    /// `1000:4497` / `1000:46ba`.
    const ENEMY_SWING: [(&str, u16); 6] = [
        ("1000:4683", 100),
        ("1000:46ba", 2),
        ("1000:46db", 100),
        ("1000:4706", 3),
        ("1000:4794", 200),
        ("1000:47be", 2),
    ];

    /// The зубная защита's `Random(4)` is spent, and ONLY spent, on the
    /// first jaw break of a guarded player.
    ///
    /// Non-circular by construction: the sites and their `n`s come from the
    /// disassembly (`docs/re/combat.md`, "Player-only branch", and
    /// `1000:47fa`'s `mov ax,4` / `push ax`), and every `r` is computed from
    /// `data/rng_vectors.json`'s seed-0 chain, which an 8086 interpreter
    /// produced from `orig/g.exe`. Nothing here is read back out of
    /// `resolve_blow_nth`.
    #[test]
    fn the_zubnaya_zashchita_spends_one_draw_at_1000_47fe_and_only_the_first_time() {
        let st = ground_truth_states();
        // Starting at chain index 1 the swing hits (roll 4), crits, breaks,
        // and the limb draw is 0 -- a JAW, which is the only limb the guard
        // has anything to do with.
        let seed = st[0];
        let a = brawler();
        let d = Fighter {
            agility: 0,
            luck: 0,
            hp: 50,
            hpmax: 50,
            ..Default::default()
        };

        // The player swinging: no guard branch exists in that copy at all.
        let mut rng = Rng::new(seed);
        rng.start_log();
        let o = resolve_blow_nth(&mut rng, &a, &d, 0, Swing::player());
        assert_eq!(draws(&mut rng), want(&PLAYER_SWING, &st[1..]));
        assert_eq!(o.broke, Some(Break::Jaw));
        assert_eq!(o.jaw_guard, None, "the player's copy has no 1000:47fe");

        // The enemy swinging against an UNGUARDED player: same six draws at
        // the mirror sites.
        let mut rng = Rng::new(seed);
        rng.start_log();
        let o = resolve_blow_nth(&mut rng, &a, &d, 0, Swing::enemy(false));
        assert_eq!(draws(&mut rng), want(&ENEMY_SWING, &st[1..]));
        assert_eq!(o.jaw_guard, None);

        // Guarded: one MORE draw, at 1000:47fe, n = 4, and it is the last.
        let mut rng = Rng::new(seed);
        rng.start_log();
        let o = resolve_blow_nth(&mut rng, &a, &d, 0, Swing::enemy(true));
        let mut sites = ENEMY_SWING.to_vec();
        sites.push(("1000:47fe", 4));
        assert_eq!(draws(&mut rng), want(&sites, &st[1..]));
        // `1000:4803` `or ax,ax` / `jnz 0x4827`: 0 breaks the jaw anyway.
        assert_eq!(o.jaw_guard, Some(random_of(st[7], 4) == 0));
        assert_eq!(o.broke, Some(Break::Jaw));

        // ... and NOT when the jaw is already broken: `1000:47c7`
        // `cmp byte [0x38b0],0` / `jnz 0x4840` jumps past the whole block,
        // the draw included, so the shape falls back to the six.
        let broken = Fighter {
            broken_jaw: true,
            ..d.clone()
        };
        let mut rng = Rng::new(seed);
        rng.start_log();
        let o = resolve_blow_nth(&mut rng, &a, &broken, 0, Swing::enemy(true));
        assert_eq!(draws(&mut rng), want(&ENEMY_SWING, &st[1..]));
        assert_eq!(o.jaw_guard, None);
        assert_eq!(o.broke, Some(Break::Jaw), "the Random(2) is still drawn");
    }

    /// The crit's `Random(3)` picks the line, and the guard's `Random(4)`
    /// picks between the two jaw arms -- both read off the ground-truth
    /// chain, on two seeds that land on DIFFERENT arms.
    #[test]
    fn the_crit_line_and_the_guard_arm_follow_the_draw() {
        let st = ground_truth_states();
        let a = brawler();
        let d = Fighter {
            agility: 0,
            luck: 0,
            hp: 50,
            hpmax: 50,
            ..Default::default()
        };
        // (chain index the swing starts at, taunt index, guard draw)
        for k in [1usize, 2] {
            let mut rng = Rng::new(st[k - 1]);
            rng.start_log();
            let o = resolve_blow_nth(&mut rng, &a, &d, 0, Swing::enemy(true));
            let mut sites = ENEMY_SWING.to_vec();
            sites.push(("1000:47fe", 4));
            assert_eq!(draws(&mut rng), want(&sites, &st[k..]), "chain index {k}");
            assert!(o.critical, "chain index {k}");
            assert_eq!(
                o.taunt,
                Some(random_of(st[k + 3], 3)),
                "chain index {k}: the 1000:4706 line"
            );
            assert_eq!(
                o.jaw_guard,
                Some(random_of(st[k + 6], 4) == 0),
                "chain index {k}: the 1000:47fe arm"
            );
        }
        // The two seeds really do land on different arms, or the loop above
        // would be one case written twice.
        assert_ne!(
            random_of(st[4], 3),
            random_of(st[5], 3),
            "crit line differs"
        );
        assert_ne!(
            random_of(st[7], 4) == 0,
            random_of(st[8], 4) == 0,
            "guard arm differs"
        );
    }

    /// Both boundaries in [`blow_budget`] are UNOBSERVABLE, and this test
    /// says why: the three constants are one arithmetic identity.
    ///
    /// `1000:3fbb` `cmp mine,0x0a` guards the loop, `1000:3fe2`
    /// `mov mine,0x0a` is what the loop collapses to, and `1000:3fc9`
    /// `cmp mine,0x1c` / `1000:3fd4` `sub ax,0x12` sit exactly one step
    /// apart: `0x1c - 0x12 == 0x0a`. So `mine == 10` returns 10 whether or
    /// not the guard lets it in, and `mine == 28` returns 10 whether it
    /// collapses at once or subtracts 18 first. No test can distinguish
    /// `> 10` from `>= 10` at `147:13`, or `< 28` from `<= 28` at
    /// `149:21` -- see `docs/re/combat.md`. This test does not kill those
    /// two mutants; it fails if the identity they rest on is ever broken.
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

    /// Armour heavier than the blow floors the damage at zero -- it does
    /// not wrap, and it does not heal.
    ///
    /// `1000:454b` `sub [bp-0x10c],ax` takes the zero-extended armour byte
    /// off the damage, `1000:454f` `cmp word [bp-0x10c],0x0` / `1000:4554`
    /// `jnl 0x455c` skips the zeroing only when the result is NOT NEGATIVE,
    /// and `1000:4556` `xor ax,ax` is the floor. The test is signed and
    /// strict: `jnl` leaves an exact 0 alone, so the floor is reached only
    /// from below. Without it `1000:4560` `sub [0x3962],ax` would subtract a
    /// negative number from the defender's HP and heal them.
    ///
    /// Armour 60 is `Ректор НГУ`'s (`tests/data_load.rs`, `rektor_ngu_v0`),
    /// so a starting brawler swinging into it is a reachable state, not a
    /// contrived one.
    #[test]
    fn armour_heavier_than_the_blow_floors_the_damage_at_zero() {
        let st = ground_truth_states();
        // [`brawler`] rolls 2..=3 and adds dmg_max on the crit: 6 at most.
        let a = brawler();
        let d = Fighter {
            agility: 0,
            luck: 0,
            armor: 60,
            hp: 666,
            hpmax: 666,
            ..Default::default()
        };
        let mut rng = Rng::new(st[0]);
        let o = resolve_blow_nth(&mut rng, &a, &d, 0, Swing::player());
        assert!(o.hit && o.critical, "the swing lands and crits");
        assert_eq!(
            o.damage, 0,
            "1000:4554 jnl floors the negative result; wrapping it would be 65482"
        );
    }

    /// The break test is STRICT: `luck * 3` exactly equal to
    /// `Random(defender.luck * 3 + 200) + 1` breaks nothing.
    ///
    /// `1000:4571` calls `Random`, `1000:4576` `inc ax` is the `+ 1`, and
    /// the 32-bit compare that follows ends in `1000:458f` `jbe 0x45ea` --
    /// equal takes the branch AWAY from the break. The enemy's copy is the
    /// same shape with the sense flipped: `1000:47b5` `ja 0x47ba` reaches
    /// the break only when strictly above.
    ///
    /// The numbers come from `data/rng_vectors.json`'s seed-0 chain, not
    /// from this port: at chain index 55 the `1000:4571` draw is 59 of 200,
    /// so the `inc` makes it 60, which is exactly `luck 20 * 3`.
    #[test]
    fn the_break_test_is_strict_at_1000_458f() {
        let st = ground_truth_states();
        const K: usize = 55;
        let d = Fighter {
            agility: 0,
            luck: 0,
            hp: 50,
            hpmax: 50,
            ..Default::default()
        };
        assert_eq!(
            random_of(st[K + 4], 200) + 1,
            60,
            "the 1000:4571 draw after the 1000:4576 inc"
        );

        // luck * 3 == 60: not strictly above, so nothing breaks -- and the
        // 1000:4595 limb draw is never spent, leaving five draws, not six.
        let at_bound = Fighter {
            luck: 20,
            ..brawler()
        };
        let mut rng = Rng::new(st[K - 1]);
        rng.start_log();
        let o = resolve_blow_nth(&mut rng, &at_bound, &d, 0, Swing::player());
        assert_eq!(o.broke, None, "60 > 60 is false at 1000:458f");
        assert_eq!(draws(&mut rng), want(&PLAYER_SWING[..5], &st[K..]));

        // One luck step above: 63 > 60 breaks, and spends the limb draw.
        let above = Fighter {
            luck: 21,
            ..brawler()
        };
        let mut rng = Rng::new(st[K - 1]);
        rng.start_log();
        let o = resolve_blow_nth(&mut rng, &above, &d, 0, Swing::player());
        assert!(o.broke.is_some(), "63 > 60 is true");
        assert_eq!(draws(&mut rng), want(&PLAYER_SWING, &st[K..]));
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
