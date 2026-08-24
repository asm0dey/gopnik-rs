//! The in-combat dispatcher's `v` and `f` arms -- `[1000:4c64, 1000:4f82)`.
//!
//! `docs/re/combat-dispatch.md` is the map of the whole chain; this module is
//! the half of it that carries arithmetic and state rather than a single call
//! or a single line of text. `crate::game::Game::run_combat` walks the chain
//! and prints; everything here is the part a test can pin to a number, the
//! same split `crate::combat` uses for the blow loop.
//!
//! **Established from flow** throughout, re-derived from `orig/g.exe` for this
//! implementation with `tools/dis16.py` from an aligned walk out of
//! `FUN_1000_3d11`'s entry at `1000:3d11` (`docs/re/METHODOLOGY.md`, "Is this
//! address an instruction boundary?"). Every address below decodes to the
//! instruction the comment quotes.
//!
//! ## The three variables
//!
//! | address | here | why it is not in [`crate::model::Fighter`] |
//! |---|---|---|
//! | `20ae:3c80` | [`Backup`] | 17 references image-wide, every one inside `FUN_1000_3d11`, and `1000:5841`/`1000:5843` zero it as the function returns -- so it is a fight-local even though it lives in DGROUP |
//! | `20ae:394d`/`394e`/`394f` | [`Pistol`] | the player's kit, not a combat stat: bought at the dealers and read by `entry`, the character sheet and this chain |
//!
//! ## What is deliberately NOT here
//!
//! `1000:4e2a`'s `^2Подошли пацаны.` (CS `0x36ab`) is **dead code and is not
//! ported**. [`backup_round`] documents the argument at its call site.

use crate::rng::Rng;

/// The player's pistol -- `20ae:394d`, `20ae:394e` and `20ae:394f`.
///
/// The three bytes are adjacent in DGROUP and are written by three adjacent
/// arms of the dealers' menu (`1000:ccd8`, `1000:cd76`, `1000:cdf9`), which is
/// what identifies them:
///
/// * `20ae:394d` -- the pistol itself. `1000:cd05` `c6 06 4d 39 01` sets it in
///   the `bmar` row-7 arm, alongside `1000:cd0a` `83 06 4f 39 03`
///   (`cartridges += 3`). Read at `1000:4eb2` (this chain), `1000:ec9d`
///   (`entry`'s own `f`), `1000:1d38` (the character sheet), and three times
///   more in the dealers' own menu.
/// * `20ae:394e` -- the silencer, set at `1000:ce34` by row 9.
/// * `20ae:394f` -- cartridges, a **word**: `+3` with the pistol
///   (`1000:cd0a`), `+5` with a box of rounds (`1000:cda3`, whose menu line
///   says six), `-1` per shot (`1000:4eed`).
///
/// An earlier revision of this port called `20ae:394d`
/// `dealer_order_placed`, "a 150-rouble order placed with the dealers". The
/// price is right and the reading is not: `1000:cd05`'s arm sets the flag and
/// hands over three cartridges in the same breath, and `1000:cd7b` refuses the
/// box of rounds without it with `^6Нету пушки. Сначала купи пистолет`
/// (CS `0x9666`) -- "no gun, buy a pistol first".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pistol {
    /// `20ae:394d`.
    pub owned: bool,
    /// `20ae:394e`.
    pub silencer: bool,
    /// `20ae:394f`. Signed, because `1000:4ee6` tests it with `jle`.
    pub cartridges: i16,
}

/// What one `f` at the fight prompt did -- `[1000:4eb2, 1000:4f82)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    /// `1000:4eb2` `cmp byte [0x394d],0` / `1000:4eb7 jnz 0x4ebc`: without a
    /// pistol `1000:4eb9` jumps straight to the death test. **An accepted
    /// verb that prints nothing at all** -- the case
    /// `docs/re/METHODOLOGY.md` means by "absence of a visible response is not
    /// absence of dispatch".
    NoPistol,
    /// `1000:4ebc` / `1000:4ec3`: neither `20ae:3693` nor the silencer is set,
    /// so `1000:4eca` writes `^6Тельзя тут стрелять! Менты накроют!`
    /// (CS `0x3716`, the game's own typo).
    NotHere,
    /// `1000:4ee6` `cmp word [0x394f],0` / `jle 0x4f69` -> CS `0x37a5`.
    NoCartridges,
    /// `1000:4f04`..`1000:4f0c` went the other way -> CS `0x3789`.
    Miss,
    /// `1000:4f18`'s `Random(10) + 0x14`, subtracted from the enemy's hp at
    /// `1000:4f28` with **no armour term** -- the only damage site in
    /// `FUN_1000_3d11` that has none.
    Hit { damage: u16 },
}

/// Fire once -- `[1000:4eb2, 1000:4f82)`.
///
/// `flag_3693` is `20ae:3693`, [`crate::game::Game::flag_3693`]. What the flag
/// *means* is still not established (`docs/re/gaps.md` has it as a wander
/// toggle flipped in bucket 1, and `docs/re/combat-dispatch.md` records
/// `1000:4ebc` as a **third** reader where that entry claimed two); the
/// dealers' own row-7 line calls the safe places bandit districts
/// (`^0Только помни стреляй в бандитских районах - там менты не накроют`,
/// CS `0x95db`), which is corroboration and not a flow claim, so the parameter
/// is named after the address rather than after a guess.
///
/// **Draws:** none unless the shot is actually taken, and then exactly two --
/// `1000:4ef5` `Random(0x32)` and, on a hit, `1000:4f18` `Random(0xa)`. A miss
/// spends one. Nothing before `1000:4eed` draws, so a player with no pistol,
/// no permission or no cartridges leaves the RNG stream untouched.
pub fn fire(rng: &mut Rng, pistol: &mut Pistol, flag_3693: bool, agility: u16) -> Shot {
    if !pistol.owned {
        return Shot::NoPistol;
    }
    // 1000:4ebc `cmp byte [0x3693],0` / `jnz 0x4ee6`, then 1000:4ec3
    // `cmp byte [0x394e],0` / `jnz 0x4ee6` -- either one alone is enough.
    if !flag_3693 && !pistol.silencer {
        return Shot::NotHere;
    }
    if pistol.cartridges <= 0 {
        return Shot::NoCartridges;
    }
    // 1000:4eed `ff 0e 4f 39` -- spent before the roll, so a miss still costs
    // a cartridge.
    pistol.cartridges -= 1;
    // 1000:4ef1 `mov ax,0x32`. The test is Borland's 32-bit pair with the roll
    // zero-extended (1000:4efa `xor dx,dx`) and the agility sign-extended
    // (1000:4f03 `cwd`); widening both with `i32::from(u16)` here reproduces
    // it for every agility the game can reach, exactly as `Game::claim_spoils`
    // does for the two luck comparisons.
    let roll = rng.below_at("1000:4ef5", 0x32);
    if i32::from(agility) <= i32::from(roll) {
        return Shot::Miss;
    }
    // 1000:4f14 `mov ax,0xa`, 1000:4f1d `add ax,0x14`: 20..=29.
    let damage = rng.below_at("1000:4f18", 0xa) + 0x14;
    Shot::Hit { damage }
}

/// `20ae:3c80` -- the local gopota's countdown, and the whole of what the
/// original tracks about them.
///
/// Zero means nobody has been called; `1..=2` is the wait; `3` is the arrival;
/// `4..=6` is attrition; `7` is the reset at `1000:4e43`. The value is a
/// signed word (`1000:4c64` uses `jl`, `1000:4d43` `jle`, `1000:4d4a` `jnl`),
/// so it is an `i16` here rather than a `u16`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Backup(i16);

/// What the `v` arm itself did -- `[1000:4cb4, 1000:4d3e)`. Every arm falls
/// through to the status line ([`Backup::status`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Called {
    /// `1000:4cd5 mov word [0x3c80],1` -- the call is placed and the countdown
    /// starts. No line of its own; the status line carries it.
    OnTheWay,
    /// `1000:4ce2 mov word [0x3c80],3` -- the mobile phone (`20ae:38bb`)
    /// short-circuits the wait, with `^2Подошли пацаны - Ща начнется!.`
    /// (CS `0x35c8`, the copy WITH the trailing dot; `1000:4c87`'s is
    /// CS `0x35a6`, without).
    ByPhone,
    /// `1000:4d0a` -- the den is known but the street cred is short:
    /// `^4Ни кто не хочет за тебя впрягаться.` (CS `0x35e9`).
    NobodyWillBackYou,
    /// `1000:4d25` -- the den flag is clear:
    /// `^6Сначала надо скорешиться с местной гопотой.` (CS `0x360f`).
    NoDen,
}

/// The line `[1000:4d3e, 1000:4d93)` writes after every `v`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// `1000:4d3e` `cmp word [0x3c80],0` / `jle 0x4d93` -- nothing at all.
    Nothing,
    /// `1000:4d4c`, CS `0x363d`, with `1000:4d51`/`1000:4d54`'s `3 - counter`.
    KicksToHold(i16),
    /// `1000:4d7a`, CS `0x3670`.
    TheyAreHere,
}

impl Backup {
    /// The raw counter, for a caller that has to reproduce a comparison
    /// against it.
    pub fn count(self) -> i16 {
        self.0
    }

    /// `1000:4d9d cmp word [0x3c80],3` / `jnl 0x4da7` -- the gopota are in the
    /// fight.
    pub fn is_up(self) -> bool {
        self.0 >= 3
    }

    /// `v` -- `[1000:4cb4, 1000:4d3e)`.
    ///
    /// Both gates are `AND`ed: `1000:4cb4 cmp byte [0x3696],1` / `jnz 0x4d03`
    /// is the den flag, and `1000:4cbb`..`1000:4ccc` computes
    /// `district * 10 + 10` and compares it against the street cred
    /// `20ae:38cb` with `cmp ax,[0x38cb]` / `jnle 0x4d03`, so the call needs
    /// `cred >= district * 10 + 10`.
    ///
    /// **Draws:** none. There is no `9a 4b 11 78 0f` in
    /// `[1000:4cb4, 1000:4d93)`.
    ///
    /// Note the counter is raised to 1 **only from 0** (`1000:4cce`
    /// `cmp word [0x3c80],0` / `jnz 0x4cdb`), so calling again while the
    /// gopota are already on their way does not reset the countdown -- but
    /// the phone arm at `1000:4ce2` is unconditional and *does* jump it
    /// straight to 3 every time.
    pub fn call(&mut self, den_found: bool, cred: i32, district: u8, has_mobile: bool) -> Called {
        if !den_found {
            // 1000:4d03 `cmp byte [0x3696],0` / `jz 0x4d25` splits the two
            // refusals: the den flag is the one that picks between them.
            return Called::NoDen;
        }
        if i32::from(district) * 10 + 10 > cred {
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

    /// `[1000:4d3e, 1000:4d93)` -- reached from every arm of [`Backup::call`].
    ///
    /// The suppression at 3 is the pair `1000:4d6c cmp word [0x3c80],3` /
    /// `jnz 0x4d7a` and `1000:4d73 cmp byte [0x38bb],0` / `jnz 0x4d93`: with a
    /// phone the counter is *already* 3 and `1000:4ce8` has just printed the
    /// arrival, so `^2Они уже здесь.` would be a second line saying the same
    /// thing.
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

    /// The second `k` compare's arm -- `1000:4c7c inc [0x3c80]`, guarded by
    /// `1000:4c64 cmp word [0x3c80],1` / `jl 0x4ca0`.
    ///
    /// Returns `true` on the transition to exactly 3
    /// (`1000:4c80` / `1000:4c85`), which is when `^2Подошли пацаны - Ща
    /// начнется!` (CS `0x35a6`) prints. So the gopota arrive on the third
    /// attack after the call -- which is what [`Status::KicksToHold`] counts
    /// down.
    ///
    /// The guard is the caller's, not this method's, because it is a compare
    /// against the counter that sits *before* the `k` token compare in the
    /// chain: with the counter at 0 the whole compare is skipped and the
    /// typed line is never looked at.
    pub fn tick_on_attack(&mut self) -> bool {
        self.0 += 1;
        self.0 == 3
    }
}

/// One prompt's worth of backup action -- what [`backup_round`] returns when
/// the block was entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fought {
    /// `1000:4dbe`..`1000:4de7`, clamped at zero.
    pub damage: u16,
    /// The enemy's hp after `1000:4def`. Signed: `20ae:3962` is a signed word
    /// and this block does not clamp it.
    pub enemy_hp_after: i32,
    /// `1000:4e43 cmp word [0x3c80],7` fired -- CS `0x36bd`,
    /// `^2Твою подмогу отпинали.`
    pub beaten: bool,
    /// `1000:4e79` found the street cred at or below zero -- CS `0x36d6`.
    pub gave_up: bool,
}

/// `[1000:4d93, 1000:4e9e)` -- the gopota's own attack.
///
/// **This block is not part of the `v` arm.** It sits between the `v` arm and
/// the `f` compare on the dispatcher's straight line, so it runs on **every**
/// prompt once the two gates open: `1000:4d93 cmp word [0x3962],0` / `jnle
/// 0x4d9d` (the enemy must still be up) and `1000:4d9d cmp word [0x3c80],3` /
/// `jnl 0x4da7` ([`Backup::is_up`]). `enemy_hp` is passed as an `i32` for the
/// same reason `Game::combat_round` keeps one: `Fighter::hp` saturates at 0
/// and the original's word does not.
///
/// ```text
/// dmg := district*3 + Random(district*4)   ; 1000:4db7 / 1000:4dbe..4dc9
/// dmg := dmg - enemy.armour div 3          ; 1000:4dcf..4dda, [0x3968]
/// if dmg < 0 then dmg := 0                 ; 1000:4dde / 1000:4de5
/// enemy.hp := enemy.hp - dmg               ; 1000:4def
/// ```
///
/// **Draws:** exactly two whenever the block is entered -- `1000:4db7`
/// `Random(district * 4)` and `1000:4e16` `Random(2)` -- and none when either
/// gate is shut. Both are unconditional inside the block, so the count does
/// not depend on the roll.
///
/// `cred` is `20ae:38cb`, debited `district * 5` at `1000:4e68`..`1000:4e75`
/// every round the block runs.
///
/// **`1000:4e2a` is not ported.** `^2Подошли пацаны.` (CS `0x36ab`) is
/// unreachable: the block is entered only with the counter at 3 or more,
/// `1000:4e1f` raises it to 4 or more, and `1000:4e23 cmp word [0x3c80],3` /
/// `jnz 0x4e43` can then never be equal. A scan of every branch target in
/// `FUN_1000_3d11` finds no jump into `[1000:4e12, 1000:4e43)`, so
/// fall-through from `1000:4e0d` is the only way in
/// (`docs/re/combat-dispatch.md`). The literal is in the image and never
/// printed.
///
/// A second consequence of there being two increment sites, and this one IS
/// reachable: `1000:4c7c` can raise the counter from 6 to 7 on a `k` and
/// `1000:4e1f` can raise it to 8 in the same prompt, while `1000:4e43` tests
/// for **exactly** 7. Above 7 only the cred exhaustion can end the backup, so
/// the reset is deliberately `== 7` here and not `>= 7`.
pub fn backup_round(
    rng: &mut Rng,
    backup: &mut Backup,
    district: u8,
    enemy_armor: u16,
    enemy_hp: i32,
    cred: &mut i32,
) -> Option<Fought> {
    if enemy_hp <= 0 || !backup.is_up() {
        return None;
    }
    // 1000:4dad `mov al,[0x3692]` / `xor ah,ah` / two `shl ax,1`.
    let roll = i32::from(rng.below_at("1000:4db7", u16::from(district) * 4));
    let district = i32::from(district);
    // 1000:4dcf..4dda: `idiv cx` with cx = 3, a SIGNED divide of the
    // zero-extended armour byte -- so for every armour the record can hold
    // this is a truncating `armour / 3`.
    let mut damage = district * 3 + roll - i32::from(enemy_armor) / 3;
    if damage < 0 {
        damage = 0;
    }
    let enemy_hp_after = enemy_hp - damage;
    // 1000:4e12 `mov ax,0x2`: 0 advances the counter, 1 does not.
    let mut beaten = false;
    if rng.below_at("1000:4e16", 2) == 0 {
        backup.0 += 1;
    }
    if backup.0 == 7 {
        backup.0 = 0;
        beaten = true;
    }
    *cred -= district * 5;
    let mut gave_up = false;
    // 1000:4e79 `cmp word [0x38cb],0` / `jnle 0x4e9e`.
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

    /// Every arm of `[1000:4cb4, 1000:4d3e)`, and what each leaves the
    /// counter at. The counter is the point: the two refusals must not start
    /// a countdown, and the phone arm must jump it to 3 rather than to 1.
    #[test]
    fn the_v_arm_gates_on_the_den_flag_and_on_the_street_cred() {
        // No den flag (1000:4cb4): 1000:4d03's `jz 0x4d25` arm.
        let mut b = Backup::default();
        assert_eq!(b.call(false, 10_000, 1, false), Called::NoDen);
        assert_eq!(b.count(), 0);

        // Den known, cred one short of `district*10 + 10` (1000:4cc8).
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

        // The phone (1000:4cdb) jumps straight to 3.
        let mut b = Backup::default();
        assert_eq!(b.call(true, 20, 1, true), Called::ByPhone);
        assert_eq!(b.count(), 3);
    }

    /// `1000:4cce`'s `jnz 0x4cdb`: a second `v` while the countdown is
    /// running must NOT reset it to 1.
    #[test]
    fn a_second_call_does_not_restart_the_countdown() {
        let mut b = Backup::default();
        b.call(true, 20, 1, false);
        b.tick_on_attack();
        assert_eq!(b.count(), 2);
        assert_eq!(b.call(true, 20, 1, false), Called::OnTheWay);
        assert_eq!(b.count(), 2, "1000:4cd5 is guarded by `counter == 0`");
    }

    /// `1000:4c7c`..`1000:4c85`, and the fact that the arrival line fires on
    /// the transition to 3 and on no other value.
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

    /// `[1000:4d3e, 1000:4d93)` at every value the counter can hold on the
    /// way up, with and without the phone.
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
        // 1000:4d6c / 1000:4d73: suppressed at EXACTLY 3, and only then.
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

    /// Both gates of `1000:4d93`/`1000:4d9d`, asserted on the DRAW COUNT:
    /// a shut gate must leave the RNG stream where it found it, because a
    /// spurious pair of draws here is exactly the desynchronisation
    /// `data/combat_trace.json` exists to catch.
    #[test]
    fn the_backup_block_draws_nothing_while_either_gate_is_shut() {
        for (hp, counter) in [(50, 0), (50, 2), (0, 5), (-3, 5)] {
            let mut rng = Rng::new(7);
            rng.start_log();
            let mut b = backup_at(counter);
            let mut cred = 500;
            let out = backup_round(&mut rng, &mut b, 3, 0, hp, &mut cred);
            assert_eq!(out, None, "hp {hp}, counter {counter}");
            assert!(
                rng.take_log().is_empty(),
                "hp {hp}, counter {counter}: no draw may be spent"
            );
            assert_eq!(cred, 500, "hp {hp}, counter {counter}: cred untouched");
        }

        // ... and open, it spends exactly the two sites the scan of
        // `[0x4900, 0x5080)` found in this block, in order.
        let mut rng = Rng::new(7);
        rng.start_log();
        let mut b = backup_at(3);
        let mut cred = 500;
        assert!(backup_round(&mut rng, &mut b, 3, 0, 50, &mut cred).is_some());
        let sites: Vec<&str> = rng.take_log().iter().map(|d| d.site).collect();
        assert_eq!(sites, vec!["1000:4db7", "1000:4e16"]);
    }

    /// `1000:4db7`'s `n` is `district * 4` and the damage floor is
    /// `district * 3`, so the whole reachable range is pinned by the district
    /// alone. Asserted over every district the game has, against bounds
    /// derived from the formula rather than from a run of this code.
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

    /// `1000:4dcf`..`1000:4dda` -- `armour div 3`, truncating, subtracted
    /// AFTER the roll. Asserted by holding the roll fixed (same seed) and
    /// moving only the armour, so the difference is the armour term and
    /// nothing else.
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
        for armor in 0u16..=8 {
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

    /// `1000:4dde` / `1000:4de5` -- the clamp, and that it clamps to zero
    /// rather than wrapping. District 1 rolls 3..=6 and armour 60 takes 20,
    /// so every roll is deep underwater.
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
    /// counter can pass 7 without resetting -- `1000:4e43`'s `jnz`.
    #[test]
    fn the_attrition_resets_the_counter_at_exactly_seven() {
        // Find a seed whose 1000:4e16 roll is 0 (the tick fires).
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
        // Started at 7 -- which `1000:4c7c` can do on a `k` -- the tick makes
        // it 8 and `cmp word [0x3c80],7` / `jnz 0x4e68` misses it entirely,
        // while a quiet round leaves the 7 for the test to find.
        assert_eq!(ticks(quiet_seed, 7), (0, true), "7 stays 7 and resets");
        assert_eq!(
            ticks(tick_seed, 7),
            (8, false),
            "7 -> 8 skips the reset: above 7 only the cred can end it"
        );
        assert_eq!(ticks(tick_seed, 8), (9, false));
    }

    /// `1000:4e68`..`1000:4e82`: `district * 5` off the cred every round, and
    /// the backup gives up the moment the cred is not positive.
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

    /// The three refusals of `[1000:4eb2, 1000:4ee6)`, each asserted to spend
    /// no draw and no cartridge.
    #[test]
    fn the_pistol_refusals_cost_neither_a_draw_nor_a_cartridge() {
        let cases = [
            (
                Pistol {
                    owned: false,
                    silencer: true,
                    cartridges: 9,
                },
                true,
                Shot::NoPistol,
            ),
            (
                Pistol {
                    owned: true,
                    silencer: false,
                    cartridges: 9,
                },
                false,
                Shot::NotHere,
            ),
            (
                Pistol {
                    owned: true,
                    silencer: false,
                    cartridges: 0,
                },
                true,
                Shot::NoCartridges,
            ),
        ];
        for (start, flag_3693, want) in cases {
            let mut rng = Rng::new(1);
            rng.start_log();
            let mut p = start;
            assert_eq!(fire(&mut rng, &mut p, flag_3693, 50), want);
            assert_eq!(p, start, "{want:?}: no state may move");
            assert!(rng.take_log().is_empty(), "{want:?}: no draw");
        }
    }

    /// `1000:4ebc` / `1000:4ec3` is an OR: either the flag or the silencer
    /// opens the shot, and the silencer is what makes it work with the flag
    /// clear.
    #[test]
    fn either_the_flag_or_the_silencer_permits_the_shot() {
        for (flag_3693, silencer, permitted) in [
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
            let got = fire(&mut rng, &mut p, flag_3693, 50);
            assert_eq!(
                got != Shot::NotHere,
                permitted,
                "flag {flag_3693}, silencer {silencer}"
            );
        }
    }

    /// `1000:4f04`..`1000:4f0c` -- **hit iff `agility > Random(50)`** -- on
    /// both sides of the comparison, using the roll the RNG actually
    /// produced rather than a bound chosen to match.
    #[test]
    fn the_hit_test_is_strictly_agility_above_the_roll() {
        let mut checked_hit = 0;
        let mut checked_miss = 0;
        let mut checked_equal = 0;
        for seed in 0..400u32 {
            // What Random(0x32) yields for this seed, read once.
            let roll = Rng::new(seed).below_at("1000:4ef5", 0x32);
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

    /// `1000:4f18`'s `Random(10) + 0x14` -- 20..=29 and no armour term. Both
    /// ends of the range have to actually occur.
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
