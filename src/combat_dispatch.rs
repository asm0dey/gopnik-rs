//! The in-combat dispatcher.
//!
//! This module carries arithmetic and state rather than single calls or
//! lines of text. Everything here is testable via numbers.
//!
//! ## The state
//!
//! The backup counter tracks rounds for the reinforcements mechanic.
//! The pistol/silencer/magazine fields track ownership.

use crate::rng::Rng;

/// The player's pistol -- three adjacent bytes that hold the weapon itself,
/// the silencer, and the cartridge count.
///
/// * The pistol itself -- set when the `bmar` menu row is chosen.
///   Read by the fight dispatcher, the entry dispatch, the character sheet,
///   and the dealers' own menu.
/// * The silencer -- set by row 9.
/// * Cartridges -- a word: `+3` with the pistol, `+5` with a box of rounds
///   (the menu says six), `-1` per shot.
///
/// An earlier revision of this port called the first byte `dealer_order_placed`.
/// That was wrong: the price is right and the logic is not. When the menu row
/// is chosen, it sets the flag and hands over three cartridges in the same
/// breath. Later, buying a box of rounds refuses without it with
/// `^6Нету пушки. Сначала купи пистолет`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pistol {
    pub owned: bool,
    pub silencer: bool,
    /// Cartridges. Signed, because the decrement test uses `jle`.
    pub cartridges: i16,
}

/// What one `f` at the fight prompt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    /// Without a pistol, jump straight to the death test. An accepted
    /// verb that prints nothing at all.
    NoPistol,
    /// Neither the district flag nor the silencer is set, so the game
    /// refuses with `^6Тельзя тут стрелять! Менты накроют!` (the game's
    /// own typo).
    NotHere,
    /// No cartridges.
    NoCartridges,
    /// Points to the enemy's class in `Swing`.
    Miss,
    /// Miss.
    Hit { damage: u16 },
}

/// Fire once.
///
/// The flag `harder_encounters` at [`crate::game::Game::harder_encounters`]
/// controls permission. The dealers' row-7 line calls safe places bandit
/// districts (`^0Только помни стреляй в бандитских районах - там менты не накроют`),
/// which is corroboration and not a flow claim, so the parameter is named
/// after permission rather than after a guess.
///
/// **Draws:** exactly two when the shot is taken. A miss spends one. Nothing
/// before the cartridge decrement draws, so a player with no pistol, no
/// permission or no cartridges leaves the RNG stream untouched.
pub fn fire(rng: &mut Rng, pistol: &mut Pistol, harder_encounters: bool, agility: u16) -> Shot {
    if !pistol.owned {
        return Shot::NoPistol;
    }
    // Either the permission flag or the silencer is alone sufficient.
    if !harder_encounters && !pistol.silencer {
        return Shot::NotHere;
    }
    if pistol.cartridges <= 0 {
        return Shot::NoCartridges;
    }
    // Spent before the roll, so a miss still costs a cartridge.
    pistol.cartridges -= 1;
    // The test is a 32-bit comparison with the agility sign-extended and the
    // roll zero-extended; it reproduces the original for every agility the
    // game can reach.
    let roll = rng.below(0x32);
    if i32::from(agility) <= i32::from(roll) {
        return Shot::Miss;
    }
    // 20..=29.
    let damage = rng.below(0xa) + 0x14;
    Shot::Hit { damage }
}

/// The local gopota's countdown -- the whole of what the game tracks
/// about them.
///
/// Zero means nobody has been called; `1..=2` is the wait; `3` is the arrival;
/// `4..=6` is attrition; `7` is the reset. The value is a signed word.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Backup(i16);

/// What the `v` arm itself did. Every arm falls through to the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Called {
    /// The call is placed and the countdown starts. No line of its own;
    /// the status line carries it.
    OnTheWay,
    /// The mobile phone (`мобильник`) short-circuits the wait.
    /// The game prints `^2Подошли пацаны - Ща начнется!.`
    ByPhone,
    /// The district is known but the street cred is short:
    /// `^4Ни кто не хочет за тебя впрягаться.`
    NobodyWillBackYou,
    /// The den flag is clear:
    /// `^6Сначала надо скорешиться с местной гопотой.`
    NoDen,
}

/// The line written after every `v`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Nothing when nobody has been called.
    Nothing,
    /// The countdown with `3 - counter`.
    KicksToHold(i16),
    /// The arrival message.
    TheyAreHere,
}

impl Backup {
    /// The raw counter, for a caller that has to reproduce a comparison
    /// against it.
    pub fn count(self) -> i16 {
        self.0
    }

    /// The gopota are in the fight (counter >= 3).
    pub fn is_up(self) -> bool {
        self.0 >= 3
    }

    /// Calling the gopota.
    ///
    /// Both gates are `AND`ed: the den flag must be set, and street cred must
    /// be at least `district * 10 + 10`.
    ///
    /// Note the counter is raised to 1 **only from 0**, so calling again while
    /// the gopota are already on their way does not reset the countdown -- but
    /// the phone arm jumps it straight to 3 every time.
    pub fn call(&mut self, den_found: bool, cred: i16, district: u8, has_mobile: bool) -> Called {
        if !den_found {
            // The den flag splits the two refusals.
            return Called::NoDen;
        }
        if i32::from(district) * 10 + 10 > i32::from(cred) {
            return Called::NobodyWillBackYou;
        }
        if self.0 == 0 {
            self.0 = 1;
        }
        if has_mobile {
            self.0 = 3;
            return Called::ByPhone;
        }
        Called::OnTheWay
    }

    /// Reached from every arm of [`Backup::call`].
    ///
    /// With a phone the counter is already 3 and the arrival has just printed,
    /// so `^2Они уже здесь.` would be a second line saying the same thing.
    pub fn status(self, has_mobile: bool) -> Status {
        if self.0 <= 0 {
            return Status::Nothing;
        }
        if self.0 < 3 {
            return Status::KicksToHold(3 - self.0);
        }
        if self.0 == 3 && has_mobile {
            return Status::Nothing;
        }
        Status::TheyAreHere
    }

    /// Returns `true` on the transition to exactly 3, which is when
    /// `^2Подошли пацаны - Ща начнется!` prints. So the gopota arrive on the
    /// third attack after the call.
    pub fn tick_on_attack(&mut self) -> bool {
        self.0 += 1;
        self.0 == 3
    }
}

/// One prompt's worth of backup action -- what [`backup_round`] returns when
/// the block was entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fought {
    /// The rolled damage, floored at zero.
    pub damage: u16,
    /// The enemy's hp after damage. Signed and not clamped.
    pub enemy_hp_after: i32,
    /// `^2Твою подмогу отпинали.` -- backup is exhausted.
    pub beaten: bool,
    /// Street cred is at or below zero (can't afford the backup).
    pub gave_up: bool,
}

/// The gopota's own attack.
///
/// This block runs on every prompt once the enemy is still up and the
/// backup is active ([`Backup::is_up`]).
///
/// ```text
/// dmg := district*3 + Random(district*4)
/// dmg := dmg - enemy.armour div 3
/// if dmg < 0 then dmg := 0
/// enemy.hp := enemy.hp - dmg
/// ```
///
/// Draws exactly two dice rolls whenever the block runs, and none when
/// either gate is shut.
///
/// `cred` is debited `district * 5` every round the block runs.
///
/// The line `^2Подошли пацаны.` can never actually print: by the time
/// this state is reached the counter has already been pushed past the
/// value the check requires.
///
/// The counter can also jump straight from 6 to 8 in the same prompt
/// through two separate increments, while the backup-ending check tests
/// for exactly 7 -- deliberately `== 7`, not `>= 7`, since above 7 only
/// running out of cred can end the backup.
pub fn backup_round(
    rng: &mut Rng,
    backup: &mut Backup,
    district: u8,
    enemy_armor: u8,
    enemy_hp: i32,
    cred: &mut i16,
) -> Option<Fought> {
    if enemy_hp <= 0 || !backup.is_up() {
        return None;
    }
    // District shifted left twice to multiply by 4.
    let roll = i32::from(rng.below(u16::from(district) * 4));
    let district = i32::from(district);
    // Armour divided by 3, truncating, subtracted AFTER the roll.
    let mut damage = district * 3 + roll - i32::from(enemy_armor) / 3;
    if damage < 0 {
        damage = 0;
    }
    let enemy_hp_after = enemy_hp - damage;
    // The roll: 0 advances the counter, 1 does not.
    let mut beaten = false;
    if rng.below(2) == 0 {
        backup.0 += 1;
    }
    if backup.0 == 7 {
        backup.0 = 0;
        beaten = true;
    }
    // Street cred is a word subtract, so it wraps.
    *cred = cred.wrapping_sub((district * 5) as i16);
    let mut gave_up = false;
    // Cred at or below zero.
    if *cred <= 0 {
        backup.0 = 0;
        gave_up = true;
    }
    Some(Fought {
        damage: damage as u16,
        enemy_hp_after,
        beaten,
        gave_up,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every arm of the v-call chain, and what each leaves the counter at.
    #[test]
    fn the_v_arm_gates_on_the_den_flag_and_on_the_street_cred() {
        // No den flag.
        let mut b = Backup::default();
        assert_eq!(b.call(false, 10_000, 1, false), Called::NoDen);
        assert_eq!(b.count(), 0);

        // Den known, cred one short of `district*10 + 10`.
        let mut b = Backup::default();
        assert_eq!(b.call(true, 19, 1, false), Called::NobodyWillBackYou);
        assert_eq!(b.count(), 0);

        // Exactly on the boundary: `jnle` is strict, so 20 passes at
        // district 1.
        let mut b = Backup::default();
        assert_eq!(b.call(true, 20, 1, false), Called::OnTheWay);
        assert_eq!(b.count(), 1);

        // District 2 needs 30, so the cred that just worked now fails.
        let mut b = Backup::default();
        assert_eq!(b.call(true, 20, 2, false), Called::NobodyWillBackYou);
        let mut b = Backup::default();
        assert_eq!(b.call(true, 30, 2, false), Called::OnTheWay);

        // The phone jumps straight to 3.
        let mut b = Backup::default();
        assert_eq!(b.call(true, 20, 1, true), Called::ByPhone);
        assert_eq!(b.count(), 3);
    }

    /// A second `v` while the countdown is running must NOT reset it to 1.
    #[test]
    fn a_second_call_does_not_restart_the_countdown() {
        let mut b = Backup::default();
        b.call(true, 20, 1, false);
        b.tick_on_attack();
        assert_eq!(b.count(), 2);
        assert_eq!(b.call(true, 20, 1, false), Called::OnTheWay);
        assert_eq!(b.count(), 2, "1000:4cd5 is guarded by `counter == 0`");
    }

    /// The arrival line fires on the transition to 3 and on no other value.
    #[test]
    fn the_countdown_announces_arrival_only_on_the_third_tick() {
        let mut b = Backup::default();
        b.call(true, 20, 1, false);
        assert_eq!(b.count(), 1);
        assert!(!b.tick_on_attack(), "2 is not the arrival");
        assert!(b.tick_on_attack(), "3 is");
        assert!(!b.tick_on_attack(), "4 is past it");
        assert_eq!(b.count(), 4);
    }

    /// Entry and exit at every counter value on the way up.
    #[test]
    fn the_status_line_counts_down_and_is_suppressed_once_by_the_phone() {
        let mut b = Backup::default();
        assert_eq!(b.status(false), Status::Nothing, "never called");
        b.call(true, 20, 1, false);
        assert_eq!(b.status(false), Status::KicksToHold(2));
        b.tick_on_attack();
        assert_eq!(b.status(false), Status::KicksToHold(1));
        b.tick_on_attack();
        assert_eq!(b.status(false), Status::TheyAreHere);
        // Suppressed at EXACTLY 3, and only then.
        assert_eq!(b.status(true), Status::Nothing);
        b.tick_on_attack();
        assert_eq!(b.count(), 4);
        assert_eq!(
            b.status(true),
            Status::TheyAreHere,
            "the suppression is `counter == 3`, not `counter >= 3`"
        );
    }

    fn backup_at(n: i16) -> Backup {
        let mut b = Backup::default();
        while b.count() < n {
            b.tick_on_attack();
        }
        b
    }

    /// The damage range is pinned by the district alone.
    #[test]
    fn the_backup_damage_spans_district_times_three_to_seven() {
        for district in 1u8..=5 {
            let lo = i32::from(district) * 3;
            let hi = i32::from(district) * 7 - 1; // Random(4d) tops out at 4d-1
            let mut seen_lo = false;
            let mut seen_hi = false;
            for seed in 0..600u32 {
                let mut rng = Rng::new(seed);
                let mut b = backup_at(3);
                let mut cred = 500;
                let f = backup_round(&mut rng, &mut b, district, 0, 500, &mut cred).unwrap();
                let d = i32::from(f.damage);
                assert!(
                    (lo..=hi).contains(&d),
                    "district {district}: {d} outside {lo}..={hi}"
                );
                seen_lo |= d == lo;
                seen_hi |= d == hi;
            }
            assert!(seen_lo, "district {district}: the floor {lo} is reachable");
            assert!(
                seen_hi,
                "district {district}: the ceiling {hi} is reachable"
            );
        }
    }

    /// `armour div 3`, truncating, subtracted AFTER the roll.
    #[test]
    fn the_backup_damage_loses_the_enemy_armour_divided_by_three() {
        let base = {
            let mut rng = Rng::new(11);
            let mut b = backup_at(3);
            let mut cred = 500;
            i32::from(
                backup_round(&mut rng, &mut b, 5, 0, 500, &mut cred)
                    .unwrap()
                    .damage,
            )
        };
        // 0..=8 covers all three remainders twice over; `div` truncates, so
        // armour 2 costs nothing and armour 3 costs one.
        for armor in 0u8..=8 {
            let mut rng = Rng::new(11);
            let mut b = backup_at(3);
            let mut cred = 500;
            let f = backup_round(&mut rng, &mut b, 5, armor, 500, &mut cred).unwrap();
            assert_eq!(
                i32::from(f.damage),
                base - i32::from(armor) / 3,
                "armour {armor}"
            );
        }
    }

    /// The clamp to zero. District 1 rolls 3..=6 and armour 60 takes 20.
    #[test]
    fn the_backup_damage_clamps_at_zero_instead_of_healing_the_enemy() {
        for seed in 0..200u32 {
            let mut rng = Rng::new(seed);
            let mut b = backup_at(3);
            let mut cred = 500;
            let f = backup_round(&mut rng, &mut b, 1, 60, 50, &mut cred).unwrap();
            assert_eq!(f.damage, 0, "seed {seed}");
            assert_eq!(f.enemy_hp_after, 50, "seed {seed}: hp must not go UP");
        }
    }

    /// The attrition tick, the reset at exactly 7, and the fact that the
    /// counter can pass 7 without resetting.
    #[test]
    fn the_attrition_resets_the_counter_at_exactly_seven() {
        // Find a seed whose tick roll is 0.
        let ticks = |seed: u32, start: i16| {
            let mut rng = Rng::new(seed);
            let mut b = backup_at(start);
            let mut cred = 5_000;
            let f = backup_round(&mut rng, &mut b, 1, 0, 500, &mut cred).unwrap();
            (b.count(), f.beaten)
        };
        let tick_seed = (0..500u32)
            .find(|&s| ticks(s, 3).0 == 4)
            .expect("some seed rolls 0 at 1000:4e16");
        let quiet_seed = (0..500u32)
            .find(|&s| ticks(s, 3).0 == 3)
            .expect("some seed rolls 1 at 1000:4e16");

        assert_eq!(ticks(quiet_seed, 6), (6, false), "a 1 leaves 6 alone");
        assert_eq!(ticks(tick_seed, 6), (0, true), "6 -> 7 resets and reports");
        // Started at 7, the tick makes it 8 and the test for 7 misses it.
        assert_eq!(ticks(quiet_seed, 7), (0, true), "7 stays 7 and resets");
        assert_eq!(
            ticks(tick_seed, 7),
            (8, false),
            "7 -> 8 skips the reset: above 7 only the cred can end it"
        );
        assert_eq!(ticks(tick_seed, 8), (9, false));
    }

    /// `district * 5` off the cred every round, and the backup gives up
    /// the moment the cred is not positive.
    #[test]
    fn the_backup_eats_street_cred_and_leaves_when_it_runs_out() {
        let mut rng = Rng::new(3);
        let mut b = backup_at(3);
        let mut cred = 26;
        let f = backup_round(&mut rng, &mut b, 5, 0, 500, &mut cred).unwrap();
        assert_eq!(cred, 1, "district 5 costs 25");
        assert!(!f.gave_up, "1000:4e79 is `<= 0`, and 1 is above it");
        assert!(b.is_up());

        let mut rng = Rng::new(3);
        let mut b = backup_at(3);
        let mut cred = 25;
        let f = backup_round(&mut rng, &mut b, 5, 0, 500, &mut cred).unwrap();
        assert_eq!(cred, 0);
        assert!(f.gave_up, "exactly 0 is not positive");
        assert_eq!(b.count(), 0);
    }

    /// Either the flag or the silencer opens the shot.
    #[test]
    fn either_the_flag_or_the_silencer_permits_the_shot() {
        for (harder_encounters, silencer, permitted) in [
            (false, false, false),
            (true, false, true),
            (false, true, true),
            (true, true, true),
        ] {
            let mut rng = Rng::new(1);
            let mut p = Pistol {
                owned: true,
                silencer,
                cartridges: 9,
            };
            let got = fire(&mut rng, &mut p, harder_encounters, 50);
            assert_eq!(
                got != Shot::NotHere,
                permitted,
                "flag {harder_encounters}, silencer {silencer}"
            );
        }
    }

    /// **hit iff `agility > Random(50)`** on both sides of the comparison.
    #[test]
    fn the_hit_test_is_strictly_agility_above_the_roll() {
        let mut checked_hit = 0;
        let mut checked_miss = 0;
        let mut checked_equal = 0;
        for seed in 0..400u32 {
            // What Random(0x32) yields for this seed, read once.
            let roll = Rng::new(seed).below(0x32);
            for agility in [roll.saturating_sub(1), roll, roll + 1] {
                let mut rng = Rng::new(seed);
                let mut p = Pistol {
                    owned: true,
                    silencer: true,
                    cartridges: 9,
                };
                let hit = matches!(fire(&mut rng, &mut p, false, agility), Shot::Hit { .. });
                assert_eq!(hit, agility > roll, "seed {seed}, agility {agility}");
                assert_eq!(p.cartridges, 8, "a miss spends one too (1000:4eed)");
                match agility.cmp(&roll) {
                    std::cmp::Ordering::Greater => checked_hit += 1,
                    std::cmp::Ordering::Equal => checked_equal += 1,
                    std::cmp::Ordering::Less => checked_miss += 1,
                }
            }
        }
        // A test that only ever saw one side of the comparison would pass
        // vacuously; refuse that.
        assert!(checked_hit > 0 && checked_miss > 0 && checked_equal > 0);
    }

    /// `Random(10) + 20` -- 20..=29 range, no armour term.
    #[test]
    fn the_pistol_damage_is_twenty_to_twenty_nine_whatever_the_armour() {
        let mut seen = [false; 10];
        for seed in 0..800u32 {
            let mut rng = Rng::new(seed);
            let mut p = Pistol {
                owned: true,
                silencer: true,
                cartridges: 9,
            };
            // Agility 50 beats every Random(0x32), which tops out at 49.
            let Shot::Hit { damage } = fire(&mut rng, &mut p, false, 50) else {
                panic!("agility 50 must always beat Random(0x32)");
            };
            assert!((20..=29).contains(&damage), "seed {seed}: {damage}");
            seen[usize::from(damage) - 20] = true;
        }
        assert!(seen.iter().all(|s| *s), "every value 20..=29 must occur");
    }
}
