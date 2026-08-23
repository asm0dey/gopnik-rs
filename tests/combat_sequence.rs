//! Task 13: differential replay of whole FIGHTS against
//! `data/combat_trace.json`.
//!
//! ## Why a second capture file exists at all
//!
//! `data/rng_trace.json`'s five runs decline or flee every encounter, so
//! between them they contain **zero** `Random` sites inside
//! `[0x3d11, 0x584c)` -- the whole of `FUN_1000_3d11`. `data/combat_vectors.json`
//! does cover the blow arithmetic (295 seed-pinned cases from the original,
//! asserting per-blow `hit` and `damage`), but a per-blow vector set cannot
//! cover a fight as a control flow: which draws a whole fight spends, in what
//! order, and what the victory and death blocks do after the last blow. That
//! is what this file's oracle adds.
//!
//! `data/combat_trace.json` is four live runs of `orig/g.exe` under
//! `tools/rngtrace/fightrun.py` -- qemu + gdb, `RandSeed` pinned by patching a
//! COPY of the binary -- with the `Битва\` prompt answered `k` (fight) or
//! `run` (flee): **1900 draws, 15 fights**. The two older oracles were not
//! read, written or regenerated to produce it; the file records their SHA-256
//! so that is checkable.
//!
//! ## Four channels, and none of them is derivable from another
//!
//! * `draws` -- site, `n`, `r`, in order, for the whole run. Compared exactly,
//!   including the count, by [`replay`].
//! * `lines_the_game_read` -- the ordered input the guest's own `ReadLn`s
//!   consumed. `tests/wander_sequence.rs` can feed one constant string because
//!   `run` answers every prompt in a declining run the way the capture driver
//!   did; a FIGHT capture needs `y` at a question and `k` at `Битва\`, so this
//!   port is fed the recorded list instead. The capture cross-checks that list
//!   against the guest's own `1000:441d` stops before publishing it, and the
//!   test below asserts the port consumed exactly as many lines as it holds --
//!   so a port that stopped early cannot look like one that finished.
//! * `fights` -- the whole enemy record at each `1000:3d11` stop. A second
//!   channel on `FUN_1000_0d14`: the port must roll the same fighter, not
//!   merely spend the same draws.
//! * `combat_prompts` -- both fighters' hp and all four break flags at each
//!   `1000:441d` stop. Before this file, the only `broken_jaw`/`broken_leg`
//!   assertion in the whole suite was `tests/data_load.rs`'s check that a
//!   FRESH fighter has neither: the break rolls at `1000:4564`..`1000:45ea`
//!   and `1000:4787`..`1000:4867` were recovered, documented and implemented,
//!   and asserted by nothing. Runs A and B both contain jaw breaks, on the
//!   player and on the enemy respectively.
//!
//! ## The final-state channel, and where it does NOT apply
//!
//! `final_state` is read out of a whole-memory dump taken after the drive. For
//! a run that ended at the top-level prompt that is the state at a turn
//! boundary and the port can be compared against all 35 variables. For a run
//! whose player DIED (`1000:5053` -> `FUN_1000_074b` -> the RTL's `int 0x21`),
//! the dump is of a guest that left the game mid-turn, and two of those
//! variables are then not comparable at all:
//!
//! * `hp_38ac` is **negative** there -- the original's hp is a signed word and
//!   the killing blow drives it below zero (run A ends at `-2`), while
//!   `Fighter::hp` is a `u16` this port saturates at 0. That is a real and
//!   still-open divergence, registered in `docs/re/gaps.md`; it costs no draw,
//!   because every test that reads hp is `<= 0` (`1000:4f82`) or `< 1`
//!   (`1000:507b`).
//! * the three `enemy_*` loot words hold the NEXT roll in the original's
//!   memory, not the dead fight's.
//!
//! So the whole-state assertion runs on the runs where its premise holds, the
//! capture itself records which those are (`ended_at_turn_marker`), and
//! [`at_least_one_run_reaches_the_final_state_channel`] refuses a state of
//! affairs where none of them does.

use gopnik::game::{FightLog, Game};
use gopnik::locations::Location;
use gopnik::model::Fighter;
use gopnik::progress::{new_character, Progress};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Draw {
    site: String,
    n: u16,
    r: u16,
}

/// One `1000:3d11` stop: the enemy record the fight was entered with, read
/// out of the guest at `20ae:3952`..`20ae:396f`.
#[derive(Deserialize)]
struct EnemyRecord {
    e_class_3952: u16,
    e_strength_3954: u16,
    e_agility_3956: u16,
    e_vitality_3958: u16,
    e_luck_395a: u16,
    e_level_395c: u16,
    e_dmg_min_395e: u16,
    e_dmg_max_3960: u16,
    e_hp_3962: u16,
    e_hpmax_3964: u16,
    e_broken_jaw_3966: u8,
    e_broken_leg_3967: u8,
    e_armor_3968: u16,
    e_beer_396a: u16,
    e_money_396c: u16,
    e_hlam_396e: u16,
    #[allow(dead_code)]
    e_randseed_367e: u32,
}

#[derive(Deserialize)]
struct FightRow {
    index: usize,
    /// Draws the guest had spent when it reached `1000:3d11`. Asserted, so a
    /// fight sitting at the wrong point in the draw stream fails even when
    /// the stream and the enemy record are each right on their own.
    first_draw_index: usize,
    /// `1000:441d` stops inside this fight.
    combat_prompts: usize,
    enemy: EnemyRecord,
}

/// One `1000:441d` stop. `#[serde(deny_unknown_fields)]` here and on
/// [`FinalState`] is what makes "every sampled variable is checked" a claim
/// rather than a hope: a column the capture carries and this struct does not
/// name fails the parse, and every test in the file fails with it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptRow {
    index: usize,
    #[allow(dead_code)]
    turn: usize,
    fight: usize,
    draws_before: usize,
    p_hp_38ac: u16,
    p_hpmax_38ae: u16,
    r_e_hp_3962: u16,
    r_e_hpmax_3964: u16,
    p_broken_jaw_38b0: u8,
    p_broken_leg_38b1: u8,
    r_e_broken_jaw_3966: u8,
    r_e_broken_leg_3967: u8,
    p_tooth_guard_394a: u8,
    #[allow(dead_code)]
    r_randseed_367e: u32,
}

/// The 35 guest variables `tools/rngtrace` reads at the end of a run.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalState {
    strength_389e: u16,
    agility_38a0: u16,
    vitality_38a2: u16,
    luck_38a4: u16,
    level_38a6: u16,
    dmg_min_38a8: u16,
    dmg_max_38aa: u16,
    hp_38ac: u16,
    hpmax_38ae: u16,
    class_389c: u16,
    broken_jaw_38b0: u8,
    broken_leg_38b1: u8,
    unk_38b2: u16,
    has_mobile_38bb: u8,
    ring_38c1: u8,
    street_cred_38cb: i32,
    xp_38ce: u32,
    xp_threshold_38d0: u32,
    district_3692: u8,
    flag_market_3694: u8,
    flag_3695: u8,
    flag_den_3696: u8,
    flag_girl_3697: u8,
    flag_vet_3698: u8,
    flag_club_3699: u8,
    flag_gym_369a: u8,
    den_errand_1_3b78: u8,
    den_errand_2_3b79: u8,
    beer_38c3: u16,
    money_38c7: u16,
    hlam_38c9: u16,
    // The last ROLLED enemy's loot words, still in guest memory at the end
    // of the run. They are named so `deny_unknown_fields` still forces this
    // struct to describe the whole sample, but they are deliberately NOT
    // asserted: they belong to whichever encounter was generated last, fought
    // or not, and this port keeps no field for a rolled-but-not-fought
    // opponent. Asserting them would need `Game` to carry one, which is a
    // field invented to satisfy a test rather than recovered from the
    // original.
    #[allow(dead_code)]
    enemy_beer_396a: u16,
    #[allow(dead_code)]
    enemy_money_396c: u16,
    #[allow(dead_code)]
    enemy_hlam_396e: u16,
    randseed_367e: u32,
}

#[derive(Deserialize)]
struct Run {
    label: String,
    seed_hex: String,
    class_value: u16,
    loaded_save: bool,
    district_key: String,
    combat_answer: String,
    walks_requested: usize,
    turns_completed: usize,
    guest_left_the_game: bool,
    ended_at_turn_marker: bool,
    lines_the_game_read: Vec<String>,
    final_state: FinalState,
    fights: Vec<FightRow>,
    combat_prompts: Vec<PromptRow>,
    draws: Vec<Draw>,
}

#[derive(Deserialize)]
struct Trace {
    runs: Vec<Run>,
    fights_total: usize,
    draws_total: usize,
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn trace() -> Trace {
    let bytes = std::fs::read(repo("data/combat_trace.json")).expect("read data/combat_trace.json");
    serde_json::from_slice(&bytes).expect("parse data/combat_trace.json")
}

fn run_named(label: &str) -> Run {
    trace()
        .runs
        .into_iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("data/combat_trace.json has no run {label}"))
}

/// `1000:71b8` stores the class prompt's answer plus 3, so this is its
/// inverse.
fn answer_for(class_value: u16) -> u16 {
    class_value
        .checked_sub(3)
        .filter(|a| *a <= 3)
        .unwrap_or_else(|| panic!("class {class_value} is not one a creation answer can produce"))
}

/// Rebuild the character a run used.
///
/// The `.SAV` offsets are the fighter record's own arithmetic --
/// `.SAV off = 0x200 + (addr - 0x389c)` -- applied to addresses
/// `docs/re/save-format.md` and `src/game.rs` already name. Task 13 adds the
/// seven flags the post-kill item table reads, which no earlier replay needed
/// because no earlier replay ever won a fight.
fn game_for(run: &Run) -> Game {
    let seed = u32::from_str_radix(run.seed_hex.trim_start_matches("0x"), 16)
        .unwrap_or_else(|_| panic!("run {}: bad seed_hex {}", run.label, run.seed_hex));
    if !run.loaded_save {
        let (player, progress) = new_character("^7 test", answer_for(run.class_value));
        return Game::new(player, progress, seed);
    }

    let path = repo(&format!("orig/SAVE_R{}.SAV", run.district_key));
    let b = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let u16at = |off: usize| u16::from_le_bytes([b[off], b[off + 1]]);
    let player = Fighter {
        name: "^7 vor".to_string(),
        class: u16at(0x200),
        strength: u16at(0x202),
        agility: u16at(0x204),
        vitality: u16at(0x206),
        luck: u16at(0x208),
        level: u16at(0x20a),
        dmg_min: u16at(0x20c),
        dmg_max: u16at(0x20e),
        hp: u16at(0x210),
        hpmax: u16at(0x212),
        broken_jaw: b[0x214] != 0,
        broken_leg: b[0x215] != 0,
        armor: u16::from(b[0x216]),
        money: i32::from(u16at(0x22b)), // 20ae:38c7
        beer_dl: u16at(0x227),          // 20ae:38c3
        junk: u16at(0x22d),             // 20ae:38c9
        joints: u16at(0x229),           // 20ae:38c5
        ..Fighter::default()
    };
    let progress = Progress {
        xp: u32::from(u16at(0x232)),        // 20ae:38ce
        threshold: u32::from(u16at(0x234)), // 20ae:38d0
    };
    let mut g = Game::new(player, progress, seed);
    g.district = run
        .district_key
        .parse::<u8>()
        .unwrap_or_else(|_| panic!("run {}: bad district_key", run.label));
    g.dark_glasses = b[0x217] != 0; // 20ae:38b3
    g.weapon_kastet_38ba = b[0x21e] != 0; // 20ae:38ba
    g.has_mobile = b[0x21f] != 0; // 20ae:38bb
    g.prison_tattoo = b[0x220] != 0; // 20ae:38bc
    g.charm_krestik_38bd = b[0x221] != 0; // 20ae:38bd
    g.charm_ring_38be = b[0x222] != 0; // 20ae:38be
    g.oneshot_gift_1 = b[0x223] != 0; // 20ae:38bf
    g.oneshot_gift_2 = b[0x224] != 0; // 20ae:38c0
    g.ring_gospodi_pomilui = b[0x225] != 0; // 20ae:38c1
    g.weapon_nozhik_38c2 = b[0x226] != 0; // 20ae:38c2
    g.pontovost_street = i32::from(u16at(0x22f)); // 20ae:38cb
    g.buff_countdown = b[0x231]; // 20ae:38cd
    g.tooth_guard = b[0x2ae] != 0; // 20ae:394a
    g.weapon_dubinka_394b = b[0x2af] != 0; // 20ae:394b
    g.weapon_tesak_394c = b[0x2b0] != 0; // 20ae:394c
    g.dealer_order_placed = b[0x2b1] != 0; // 20ae:394d
    g.church_visits = b[0x2b5]; // 20ae:3951

    // The seven discovery flags live in `places.sav`, not the record; a
    // capture that loaded a save started this district fresh. Same
    // reconstruction `tests/wander_sequence.rs` uses, and the capture's own
    // first per-turn sample is what settles it there.
    g.places.reset_for_new_district();
    // 1000:73bb runs on the load path too: a Вор gets the dealers back.
    g.places.mark_found(Location::BigMarket);
    g
}

/// An iterator over the run's recorded input that counts what was taken.
struct Fed {
    lines: Vec<String>,
    taken: usize,
}

impl Iterator for Fed {
    type Item = std::io::Result<String>;
    fn next(&mut self) -> Option<Self::Item> {
        let out = self.lines.get(self.taken).cloned();
        if out.is_some() {
            self.taken += 1;
        }
        out.map(Ok)
    }
}

/// Drive one captured run to completion, returning the finished game, the
/// draws it made, its fight channels, and how many recorded lines it used.
fn drive(run: &Run) -> (Game, Vec<gopnik::rng::Draw>, FightLog, usize) {
    let mut g = game_for(run);
    g.rng.start_log();
    g.start_fight_log();
    let mut input = Fed {
        lines: run.lines_the_game_read.clone(),
        taken: 0,
    };
    // `turns_completed` counts turns that came BACK to the street prompt, so
    // a run whose player died is one turn short of the turns it actually
    // played: the fatal one never returned. `1000:5053` takes the guest out
    // of the process, so that turn's draws are in the capture and the turn
    // after it does not exist.
    for _ in 0..run.turns_completed + usize::from(run.guest_left_the_game) {
        g.walk(&mut input).expect("walk");
    }
    let taken = input.taken;
    let draws = g.rng.take_log();
    let fights = g.take_fight_log();
    (g, draws, fights, taken)
}

/// Panic naming the diverging index and four draws of context either side.
fn diverged(label: &str, got: &[gopnik::rng::Draw], want: &[Draw], i: usize) -> ! {
    let lo = i.saturating_sub(4);
    let hi = (i + 5).min(got.len().max(want.len()));
    let mut ctx = String::new();
    for j in lo..hi {
        let p = got
            .get(j)
            .map(|d| format!("{} n={} r={}", d.site, d.n, d.r))
            .unwrap_or_else(|| "-- (port made no draw here)".to_string());
        let o = want
            .get(j)
            .map(|d| format!("{} n={} r={}", d.site, d.n, d.r))
            .unwrap_or_else(|| "-- (original made no draw here)".to_string());
        ctx.push_str(&format!(
            "{}{:>5}  port: {:<34} orig: {}\n",
            if j == i { ">>" } else { "  " },
            j,
            p,
            o
        ));
    }
    panic!(
        "run {label}: draw sequence diverges from data/combat_trace.json at index {i} \
         (port made {} draws, the original made {}).\n{ctx}",
        got.len(),
        want.len()
    );
}

fn first_mismatch(got: &[gopnik::rng::Draw], want: &[Draw]) -> Option<usize> {
    let n = got.len().min(want.len());
    (0..n)
        .find(|&i| got[i].site != want[i].site || got[i].n != want[i].n || got[i].r != want[i].r)
        .or(if got.len() == want.len() {
            None
        } else {
            Some(n)
        })
}

/// Replay one captured run and assert every channel it carries.
fn replay(label: &str) -> Game {
    let run = run_named(label);
    assert!(
        !run.draws.is_empty() && !run.fights.is_empty(),
        "run {label}: the capture has no draws or no fights, so replaying it \
         would assert nothing"
    );
    let (g, got, log, taken) = drive(&run);

    // The draw stream, whole, including the count.
    if let Some(i) = first_mismatch(&got, &run.draws) {
        diverged(label, &got, &run.draws, i);
    }

    // The input. A port that ran out of lines mid-fight stops silently
    // (`lines.next()` returning `None` clears `running`), which would leave a
    // short draw stream looking like a finished one -- except that the draw
    // comparison above includes the count. This makes the same failure
    // visible in its own terms.
    assert_eq!(
        taken,
        run.lines_the_game_read.len(),
        "run {label}: the port consumed {taken} of the {} lines the guest's \
         own ReadLns consumed",
        run.lines_the_game_read.len()
    );

    // The fight channel: one entry per 1000:3d11 stop, same enemy record.
    assert_eq!(
        log.fights.len(),
        run.fights.len(),
        "run {label}: the port entered combat {} time(s), the guest {}",
        log.fights.len(),
        run.fights.len()
    );
    for (want, (at_draw, got)) in run.fights.iter().zip(&log.fights) {
        let w = &want.enemy;
        let at = format!("run {label} fight {}", want.index);
        assert_eq!(
            *at_draw, want.first_draw_index,
            "{at}: the fight started after a different number of draws"
        );
        assert_eq!(
            log.prompts.iter().filter(|p| p.fight == want.index).count(),
            want.combat_prompts,
            "{at}: number of `Битва\\` prompts"
        );
        assert_eq!(got.class, w.e_class_3952, "{at}: 20ae:3952 class");
        assert_eq!(got.strength, w.e_strength_3954, "{at}: 20ae:3954");
        assert_eq!(got.agility, w.e_agility_3956, "{at}: 20ae:3956");
        assert_eq!(got.vitality, w.e_vitality_3958, "{at}: 20ae:3958");
        assert_eq!(got.luck, w.e_luck_395a, "{at}: 20ae:395a");
        assert_eq!(got.level, w.e_level_395c, "{at}: 20ae:395c");
        assert_eq!(got.dmg_min, w.e_dmg_min_395e, "{at}: 20ae:395e");
        assert_eq!(got.dmg_max, w.e_dmg_max_3960, "{at}: 20ae:3960");
        assert_eq!(got.hp, w.e_hp_3962, "{at}: 20ae:3962");
        assert_eq!(got.hpmax, w.e_hpmax_3964, "{at}: 20ae:3964");
        assert_eq!(got.broken_jaw, w.e_broken_jaw_3966 != 0, "{at}: 20ae:3966");
        assert_eq!(got.broken_leg, w.e_broken_leg_3967 != 0, "{at}: 20ae:3967");
        assert_eq!(got.armor, w.e_armor_3968, "{at}: 20ae:3968");
        assert_eq!(got.beer_dl, w.e_beer_396a, "{at}: 20ae:396a loot beer");
        assert_eq!(got.money, i32::from(w.e_money_396c), "{at}: 20ae:396c");
        assert_eq!(got.junk, w.e_hlam_396e, "{at}: 20ae:396e Хлам");
    }

    // The per-round channel: one entry per 1000:441d stop, both fighters'
    // hp and all four break flags, at the same point in the draw stream.
    assert_eq!(
        log.prompts.len(),
        run.combat_prompts.len(),
        "run {label}: the port read {} combat prompt(s), the guest {}",
        log.prompts.len(),
        run.combat_prompts.len()
    );
    for (want, got) in run.combat_prompts.iter().zip(&log.prompts) {
        let at = format!("run {label} combat prompt {}", want.index);
        assert_eq!(got.fight, want.fight, "{at}: which fight");
        assert_eq!(
            got.draws_before, want.draws_before,
            "{at}: draws spent before the prompt"
        );
        assert_eq!(got.player_hp, want.p_hp_38ac, "{at}: 20ae:38ac");
        assert_eq!(got.player_hpmax, want.p_hpmax_38ae, "{at}: 20ae:38ae");
        assert_eq!(got.enemy_hp, want.r_e_hp_3962, "{at}: 20ae:3962");
        assert_eq!(got.enemy_hpmax, want.r_e_hpmax_3964, "{at}: 20ae:3964");
        assert_eq!(
            got.player_broken_jaw,
            want.p_broken_jaw_38b0 != 0,
            "{at}: 20ae:38b0 broken jaw"
        );
        assert_eq!(
            got.player_broken_leg,
            want.p_broken_leg_38b1 != 0,
            "{at}: 20ae:38b1 broken leg"
        );
        assert_eq!(
            got.enemy_broken_jaw,
            want.r_e_broken_jaw_3966 != 0,
            "{at}: 20ae:3966 enemy broken jaw"
        );
        assert_eq!(
            got.enemy_broken_leg,
            want.r_e_broken_leg_3967 != 0,
            "{at}: 20ae:3967 enemy broken leg"
        );
    }
    g
}

/// Assert the whole 35-variable end state.
///
/// Only meaningful for a run whose drive ended at the top-level prompt; see
/// this file's module doc for what a post-death dump holds instead.
fn assert_final_state(label: &str, run: &Run, g: &Game) {
    assert!(
        run.ended_at_turn_marker,
        "run {label}: the capture says the drive did NOT end at a turn marker, \
         so its final_state is not a turn-boundary state and must not be \
         asserted against a replay"
    );
    let f = &run.final_state;
    let b = |v: u8| v != 0;
    let found = |loc: Location| u8::from(g.places.is_found(loc));

    assert_eq!(g.player.strength, f.strength_389e, "{label}: 20ae:389e");
    assert_eq!(g.player.agility, f.agility_38a0, "{label}: 20ae:38a0");
    assert_eq!(g.player.vitality, f.vitality_38a2, "{label}: 20ae:38a2");
    assert_eq!(g.player.luck, f.luck_38a4, "{label}: 20ae:38a4");
    assert_eq!(g.player.level, f.level_38a6, "{label}: 20ae:38a6");
    assert_eq!(g.player.dmg_min, f.dmg_min_38a8, "{label}: 20ae:38a8");
    assert_eq!(g.player.dmg_max, f.dmg_max_38aa, "{label}: 20ae:38aa");
    assert_eq!(g.player.hp, f.hp_38ac, "{label}: 20ae:38ac");
    assert_eq!(g.player.hpmax, f.hpmax_38ae, "{label}: 20ae:38ae");
    assert_eq!(g.player.class, f.class_389c, "{label}: 20ae:389c");
    assert_eq!(
        g.player.broken_jaw,
        b(f.broken_jaw_38b0),
        "{label}: 20ae:38b0"
    );
    assert_eq!(
        g.player.broken_leg,
        b(f.broken_leg_38b1),
        "{label}: 20ae:38b1"
    );
    assert_eq!(g.player.armor, f.unk_38b2, "{label}: 20ae:38b2");
    assert_eq!(g.has_mobile, b(f.has_mobile_38bb), "{label}: 20ae:38bb");
    assert_eq!(g.ring_gospodi_pomilui, b(f.ring_38c1), "{label}: 20ae:38c1");
    assert_eq!(g.pontovost_street, f.street_cred_38cb, "{label}: 20ae:38cb");
    assert_eq!(g.progress.xp, f.xp_38ce, "{label}: 20ae:38ce");
    assert_eq!(
        g.progress.threshold, f.xp_threshold_38d0,
        "{label}: 20ae:38d0"
    );
    assert_eq!(g.district, f.district_3692, "{label}: 20ae:3692");
    assert_eq!(
        found(Location::Market),
        f.flag_market_3694,
        "{label}: 20ae:3694"
    );
    assert_eq!(
        found(Location::BigMarket),
        f.flag_3695,
        "{label}: 20ae:3695"
    );
    assert_eq!(found(Location::Den), f.flag_den_3696, "{label}: 20ae:3696");
    assert_eq!(
        found(Location::Girl),
        f.flag_girl_3697,
        "{label}: 20ae:3697"
    );
    assert_eq!(found(Location::Vet), f.flag_vet_3698, "{label}: 20ae:3698");
    assert_eq!(
        found(Location::Club),
        f.flag_club_3699,
        "{label}: 20ae:3699"
    );
    assert_eq!(found(Location::Gym), f.flag_gym_369a, "{label}: 20ae:369a");
    assert_eq!(
        g.den_errand_1_pending,
        b(f.den_errand_1_3b78),
        "{label}: 20ae:3b78"
    );
    assert_eq!(
        g.den_errand_2_pending,
        b(f.den_errand_2_3b79),
        "{label}: 20ae:3b79"
    );
    // The purse. `1000:523e`..`1000:5251` is what fills it on a win, and this
    // port did not reproduce that block until Task 13 -- `docs/re/gaps.md`
    // recorded it as the reason `Fighter::junk` stayed 0.
    assert_eq!(g.player.beer_dl, f.beer_38c3, "{label}: 20ae:38c3 beer");
    assert_eq!(
        u16::try_from(g.player.money).expect("money fits a word"),
        f.money_38c7,
        "{label}: 20ae:38c7 money"
    );
    assert_eq!(g.player.junk, f.hlam_38c9, "{label}: 20ae:38c9 Хлам");
    assert_eq!(
        g.rng.state(),
        f.randseed_367e,
        "{label}: 20ae:367e RandSeed"
    );
}

/// Run A -- a fresh Подтсан (class 3) in district 1. It accepts the encounter
/// on turn 2 and loses: **30 combat prompts**, the longest fight captured, so
/// it is the run that pins the crowd's `Random(10)` at `1000:4135` firing
/// once per prompt from the fifth onward (26 of them) and its `Random(18)`
/// at `1000:4145`. It is also the only run where the PLAYER's jaw is broken
/// (`20ae:38b0` from prompt 13 to the end), and it ends in the death block at
/// `1000:5053`.
#[test]
fn run_a_replays_exactly() {
    replay("A");
}

/// Run B -- `SAVE_R2.SAV` at district 2, 25 walks, **six fights, all won**.
/// The only run that reaches the victory block's own draws in quantity:
/// `1000:52d5` and `1000:5402` and `1000:5454` six times each, plus the
/// Нарк's `1000:5427` joint roll. It is the run whose whole 35-variable end
/// state is asserted, and the enemy's jaw is broken in two of its fights
/// (`20ae:3966`).
#[test]
fn run_b_replays_exactly() {
    replay("B");
}

/// Run C -- `SAVE_R3.SAV` at district 3, which ships the зубная защита
/// (`.SAV 0x2ae` = 1). Two fights won, the third lost. The guard's own
/// `Random(4)` at `1000:47fe` is NOT exercised here -- no jaw break landed on
/// the player -- but the run is the reason the branch had to be modelled:
/// without it, a single jaw break in a `SAVE_R3`/`R4`/`R5` replay would
/// desynchronise the whole stream from that draw onward.
#[test]
fn run_c_replays_exactly() {
    replay("C");
}

/// Run D -- a fresh Вор (class 6) in district 1 that accepts five encounters
/// and answers `run` at every one of them. It keeps the already-recovered
/// flee path (`1000:48e1`, the level-0 arm at `1000:4ade`) pinned inside this
/// file too: five fights, five combat prompts, and **zero** draws anywhere in
/// `[0x3d11, 0x584c)`, which is the live form of "no arm of the flee path
/// draws".
#[test]
fn run_d_replays_exactly() {
    replay("D");
}

/// Run B's whole 35-variable end state -- the channel the draw comparison is
/// blind to. The loot award, the post-kill gifts, the item table's stat
/// grants and the level-up grants are all invisible to the draw stream.
#[test]
fn run_b_final_state_matches() {
    let run = run_named("B");
    let g = replay("B");
    assert_final_state("B", &run, &g);
}

/// Every run whose drive ended at the turn marker must exist, and there must
/// be at least one -- otherwise [`assert_final_state`] would be a test that
/// never runs on anything.
#[test]
fn at_least_one_run_reaches_the_final_state_channel() {
    let t = trace();
    let usable: Vec<&str> = t
        .runs
        .iter()
        .filter(|r| r.ended_at_turn_marker)
        .map(|r| r.label.as_str())
        .collect();
    assert!(
        !usable.is_empty(),
        "no captured run ended at a turn marker, so the whole-state channel \
         has nothing to assert against"
    );
    assert!(
        usable.contains(&"B"),
        "run B is the one `run_b_final_state_matches` asserts; usable runs are {usable:?}"
    );
}

/// The fights the capture holds, counted from the file itself, and the sites
/// they reach. This is the coverage claim made falsifiable: if a later change
/// captured fewer fights, or fights that never enter the blow loop, the claim
/// that "combat has an oracle" would quietly stop being true.
#[test]
fn the_capture_covers_the_blow_loop_the_crowd_and_the_victory_block() {
    let t = trace();
    assert_eq!(t.runs.len(), 4, "captured runs");
    for r in &t.runs {
        assert!(
            r.turns_completed <= r.walks_requested,
            "run {}: {} turns completed out of {} requested",
            r.label,
            r.turns_completed,
            r.walks_requested
        );
    }
    assert_eq!(
        run_named("D").combat_answer,
        "run",
        "run D is the flee run: it must answer `run` at 1000:48e1"
    );
    for lab in ["A", "B", "C"] {
        assert_eq!(
            run_named(lab).combat_answer,
            "k",
            "run {lab} must answer `k` at 1000:4440"
        );
    }
    assert_eq!(t.fights_total, 15, "fights captured");
    assert_eq!(t.draws_total, 1900, "draws captured");

    let mut sites: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for r in &t.runs {
        for d in &r.draws {
            *sites.entry(d.site.clone()).or_default() += 1;
        }
    }
    // Every site the capture must actually contain for this file to be an
    // oracle for combat at all, with the block each belongs to.
    for (site, what) in [
        ("1000:4135", "the crowd's Random(10)"),
        ("1000:4145", "the crowd's Random(18) taunt pick"),
        ("1000:4460", "player accuracy"),
        ("1000:4497", "player damage span"),
        ("1000:44b8", "player crit"),
        ("1000:44e3", "player crit taunt"),
        ("1000:4571", "player break roll"),
        ("1000:4595", "player break limb"),
        ("1000:4683", "enemy accuracy"),
        ("1000:46ba", "enemy damage span"),
        ("1000:46db", "enemy crit"),
        ("1000:4706", "enemy crit taunt"),
        ("1000:4794", "enemy break roll"),
        ("1000:47be", "enemy break limb"),
        ("1000:52d5", "the victory block's Random(30) gift gate"),
        ("1000:5402", "the victory block's Random(district*25)"),
        ("1000:5427", "the Нарк's joint roll"),
        ("1000:5454", "the victory block's Random(district*40)"),
    ] {
        let n = sites.get(site).copied().unwrap_or(0);
        assert!(n > 0, "{site} ({what}) never fires in the capture");
    }

    // Runs A and B between them must contain a break on each side, or the
    // break EFFECT is still asserted by nothing.
    let a = run_named("A");
    assert!(
        a.combat_prompts.iter().any(|p| p.p_broken_jaw_38b0 != 0),
        "run A no longer contains a player jaw break"
    );
    let b = run_named("B");
    assert!(
        b.combat_prompts.iter().any(|p| p.r_e_broken_jaw_3966 != 0),
        "run B no longer contains an enemy jaw break"
    );
    // And run C must still be the one that carries the зубная защита.
    let c = run_named("C");
    assert!(
        c.combat_prompts.iter().all(|p| p.p_tooth_guard_394a == 1),
        "run C no longer loads a save with the зубная защита at 20ae:394a"
    );
}
