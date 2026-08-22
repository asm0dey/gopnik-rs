//! Task 11c: differential replay of the wander turn's `Random` sequence
//! against `data/rng_trace.json`.
//!
//! ## What this test is
//!
//! `data/rng_trace.json` holds **five live runs of `orig/g.exe`** captured by
//! `tools/rngtrace` under qemu with `RandSeed` pinned by patching a COPY of
//! the binary: 1387 draws recorded as `{i, turn, site, n, r}` in execution
//! order, plus a 29-variable `final_state` per run read out of the guest's
//! own memory. That file is the oracle. It is never regenerated to match this
//! port and never edited to make a run pass -- if the port disagrees with it,
//! the port is wrong until proven otherwise.
//!
//! Each test below rebuilds the character the capture used, seeds
//! [`gopnik::rng::Rng`] with the run's own `seed_hex`, drives
//! `walks_requested` walks, and asserts the port's `(site, n, r)` sequence
//! equals the run's `draws` **for the whole run**. Call sites are recorded,
//! not just values, so a missing draw cannot be mistaken for a reordered one.
//!
//! ## The input policy is the capture driver's, not an invention
//!
//! `tools/rngtrace/driver.py::walk` types a fixed script, and this test
//! replays it rather than guessing:
//!
//! * street prompt -> `w`   (one walk)
//! * a question    -> `n`   (decline the encounter, decline the mage's save)
//! * combat prompt -> `run`
//! * anything else -> Enter
//!
//! The port has no screen to classify, so every line this test feeds a
//! handler is `n`: the only prompts a walk turn can raise in this port are
//! questions (the girl encounter, the fight encounter, the mage's save), and
//! `n` is what the driver answered at all three. `run`/Enter never became
//! relevant, because the driver never accepted a fight.
//!
//! ## Runs A, B and E are expected to fail, and why that is deliberate
//!
//! The port models the wander preamble (`docs/re/wander.md`,
//! `data/wander.json`). It does **not** model `FUN_1000_0d14`
//! (`1000:0d14`..`1000:11c2`), the routine that rolls a random-encounter
//! opponent, nor the fight-flow draws at `1000:b5f1` / `1000:b725` /
//! `1000:b792` downstream of it -- `docs/re/wander.md` puts all of those
//! outside its catalogue, and `Game::pick_enemy` is an explicit
//! approximation. So every run that reaches wander bucket 3 diverges at the
//! first encounter and cannot recover: the RNG is a single shared stream.
//!
//! Runs C and D never reach bucket 3 and therefore replay end to end,
//! including the church's own draws.
//!
//! The assertions are NOT narrowed to the part that passes. A failing
//! assertion that names the exact divergence is the honest state of the port;
//! shrinking the comparison to the preamble would make the suite green while
//! checking less than it appears to.
//!
//! ## Prefix assertions -- added, not substituted
//!
//! Draws 3, 4, 9, 10 and 11 (the phone gags, the ring's heal, the thief's two
//! rolls) occur only in runs B and E, both of which fail overall. A regression
//! in any of them therefore changed nothing observable: a red test is red
//! either way. `run_{a,b,e}_matches_the_prefix_before_its_divergence` close
//! that hole by asserting the first `N` draws of those runs, where `N` is the
//! run's own enumerated divergence index. They are **additional** tests: the
//! whole-run assertions above are untouched and still fail. Each prefix test
//! also asserts that the sites it exists for actually occur inside its window,
//! so it cannot pass by covering nothing.
//!
//! ## The state channel
//!
//! The draw sequence is only one of two channels. `apply_class_bonus`'s flags,
//! the church's five stat arms and three gift arms, the errand flags and the
//! level-up grants are all invisible to it. `run_{c,d}_final_state_matches`
//! assert the capture's whole 29-variable `final_state` for the two runs that
//! replay end to end, so those are checked too.
//!
//! Runs A, B and E get **no** `final_state` assertion. Their draw streams
//! diverge partway, so their end states legitimately differ from the capture's
//! and asserting them would either fail for the already-enumerated reason or
//! have to be weakened to a subset -- and the draw assertion wins over any
//! state assertion that would conflict with it.

use gopnik::game::Game;
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

/// The 29 guest variables `tools/rngtrace` reads out of the guest's own
/// memory at the end of a run, named `<meaning>_<DS offset>`.
///
/// `#[serde(deny_unknown_fields)]` is what makes "all 29 fields are checked"
/// a claim rather than a hope: if `data/rng_trace.json` carries a variable
/// this struct does not name, parsing the oracle fails and every test in this
/// file fails with it. A field list that silently ignored the rest would be
/// the "check that cannot fail" `docs/re/METHODOLOGY.md` warns about.
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
    randseed_367e: u32,
}

#[derive(Deserialize)]
struct Run {
    label: String,
    seed_hex: String,
    class_value: u16,
    loaded_save: bool,
    district_key: String,
    walks_requested: usize,
    draws: Vec<Draw>,
    final_state: FinalState,
}

#[derive(Deserialize)]
struct Trace {
    runs: Vec<Run>,
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn trace() -> Trace {
    let bytes = std::fs::read(repo("data/rng_trace.json")).expect("read data/rng_trace.json");
    serde_json::from_slice(&bytes).expect("parse data/rng_trace.json")
}

fn run_named(label: &str) -> Run {
    trace()
        .runs
        .into_iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("data/rng_trace.json has no run {label}"))
}

/// The class answer that produces `class_value`. `1000:71b8` stores the
/// prompt's answer plus 3, so this is the inverse of that add.
fn answer_for(class_value: u16) -> u16 {
    class_value
        .checked_sub(3)
        .filter(|a| *a <= 3)
        .unwrap_or_else(|| panic!("class {class_value} is not one a creation answer can produce"))
}

/// Rebuild the character a run used.
///
/// A run that created a character is reproduced by
/// [`gopnik::progress::new_character`] plus [`Game::new`], which between them
/// are the original's `1000:7140`..`1000:71e8` (the stat block) and
/// `1000:6dbe` + `1000:73bb` (the vet/market flags, the class bonus and the
/// den's loan credit).
///
/// A run that loaded a save is reproduced from the `.SAV` bytes themselves:
/// the original's load is one 694-byte `BlockRead` into `DS:369c`
/// (`1000:6c01`), and `DS:369c + 0x200 = DS:389c`, so every wander global
/// inside the fighter record comes back with it. The offsets below are that
/// arithmetic applied to `data/wander.json`'s `globals` addresses, not new
/// claims: `.SAV off = 0x200 + (addr - 0x389c)`.
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
        armor: u16::from(b[0x216]), // 20ae:38b2, the church's "защиту" byte
        money: i32::from(u16at(0x22b)), // 20ae:38c7
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
    g.has_mobile = b[0x21f] != 0; // 20ae:38bb
    g.oneshot_gift_1 = b[0x223] != 0; // 20ae:38bf
    g.oneshot_gift_2 = b[0x224] != 0; // 20ae:38c0
    g.ring_gospodi_pomilui = b[0x225] != 0; // 20ae:38c1
    g.pontovost_street = i32::from(u16at(0x22f)); // 20ae:38cb
    g.buff_countdown = b[0x231]; // 20ae:38cd
    g.dealer_order_placed = b[0x2b1] != 0; // 20ae:394d
    g.church_visits = b[0x2b5]; // 20ae:3951

    // The seven discovery flags are NOT in the record: they live in
    // `places.sav`, and run E's guest ended with den/girl/club/gym clear
    // (`final_state`) while nothing in a wander turn can clear them -- so
    // this run began with them clear. Vet and Market are set during the run
    // by draws 5 and 6 (turns 3 and 19 of `runs[E].draws`), which leaves
    // their starting value undetermined by the capture; they are started
    // clear here, and since no discovery flag gates any draw in the
    // preamble the choice cannot move the sequence either way.
    g.places.reset_for_new_district();
    // 1000:73bb runs on the load path too (`docs/re/wander.md`, "What
    // reaches 1000:73bb"): a Вор gets the dealers back.
    g.places.mark_found(Location::BigMarket);
    g
}

/// `n` for every prompt: see the module doc for why it is the driver's
/// answer at all three prompts a walk turn can raise. Bounded rather than
/// endless so a handler that ignores the line cannot spin forever.
fn declines(n: usize) -> std::vec::IntoIter<std::io::Result<String>> {
    (0..n)
        .map(|_| Ok("n".to_string()))
        .collect::<Vec<_>>()
        .into_iter()
}

/// Where each red run's draw sequence first disagrees with the oracle, as a
/// 0-based index into `runs[].draws` -- i.e. the index of the first
/// **mismatching** draw, so `0..N` is the matching prefix and draw `N` is the
/// port's `Game::pick_enemy` standing in for the original's `1000:0d26`.
///
/// These are not fresh claims: they are the indices
/// `run_{a,b,e}_replays_exactly` print when they fail, and the ones
/// `docs/re/gaps.md`'s "The random-encounter opponent" table records. They are
/// used only to bound the prefix assertions below; the whole-run assertions
/// do not read them.
const A_DIVERGES_AT: usize = 18;
const B_DIVERGES_AT: usize = 63;
const E_DIVERGES_AT: usize = 79;

/// Drive one captured run to completion, returning the finished [`Game`] --
/// so the *state* channel can be checked as well as the draw sequence -- and
/// every draw the port made, in order.
fn drive(run: &Run) -> (Game, Vec<gopnik::rng::Draw>) {
    let mut g = game_for(run);
    g.rng.start_log();
    let mut input = declines(4 * run.walks_requested + 16);
    for _ in 0..run.walks_requested {
        g.walk(&mut input).expect("walk");
    }
    let got = g.rng.take_log();
    (g, got)
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
        "run {label}: draw sequence diverges from data/rng_trace.json at index {i} \
         (port made {} draws, the original made {}).\n{ctx}",
        got.len(),
        want.len()
    );
}

/// The first index at which the port and the oracle disagree, considering
/// only the first `limit` draws of each. `None` means they agree there.
fn first_mismatch(got: &[gopnik::rng::Draw], want: &[Draw], limit: usize) -> Option<usize> {
    let n = limit.min(got.len()).min(want.len());
    (0..n)
        .find(|&i| got[i].site != want[i].site || got[i].n != want[i].n || got[i].r != want[i].r)
        .or(if got.len().min(limit) == want.len().min(limit) {
            None
        } else {
            Some(n)
        })
}

/// Replay one captured run and assert the **whole** draw sequence. Returns
/// the finished game so a caller can go on to check the state channel.
fn replay(label: &str) -> Game {
    let run = run_named(label);
    let (g, got) = drive(&run);
    if let Some(i) = first_mismatch(&got, &run.draws, usize::MAX) {
        diverged(label, &got, &run.draws, i);
    }
    g
}

/// Assert the port's first `n` draws equal the oracle's, and that every site
/// in `must_cover` occurs within them.
///
/// This is **in addition to** the whole-run assertion for the same run, never
/// instead of it: see this file's module doc, "Prefix assertions".
///
/// `must_cover` is not decoration. A prefix assertion whose window happened to
/// miss the draws it was added for would pass while checking nothing, which is
/// the defect class `docs/re/METHODOLOGY.md` names; naming the sites makes the
/// coverage claim itself falsifiable.
fn replay_prefix(label: &str, n: usize, must_cover: &[&str]) {
    let run = run_named(label);
    let (_, got) = drive(&run);
    assert!(
        run.draws.len() >= n,
        "run {label}: the capture has only {} draws, fewer than the prefix of {n} \
         this test claims to check",
        run.draws.len()
    );
    assert!(
        got.len() >= n,
        "run {label}: the port made only {} draws, fewer than the prefix of {n} \
         this test claims to check -- the port lost draws it used to make",
        got.len()
    );
    if let Some(i) = first_mismatch(&got, &run.draws, n) {
        diverged(label, &got, &run.draws, i);
    }
    for site in must_cover {
        assert!(
            got[..n].iter().any(|d| d.site == *site),
            "run {label}: the first {n} draws do not include {site}, so this \
             prefix assertion does not cover what it claims to"
        );
    }
}

/// Assert all 29 guest variables `data/rng_trace.json` recorded at the end of
/// the run against the finished port `Game`.
///
/// The draw comparison is blind to every one of these: `apply_class_bonus`'s
/// flags, the church's five stat arms and three gift arms, the errand flags
/// and the level-up grants could all be wrong and the sequence would still
/// match. This is the second channel.
///
/// Address for each field is in its name (`20ae:<offset>`); the mapping onto
/// port fields is `docs/re/save-format.md`'s record layout plus
/// `data/wander.json`'s `globals`.
fn assert_final_state(label: &str, run: &Run, g: &Game) {
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
    // 20ae:38b2 is the armour byte -- see docs/re/gaps.md, "Opened by Task 11b".
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
    assert_eq!(
        g.rng.state(),
        f.randseed_367e,
        "{label}: 20ae:367e RandSeed"
    );
}

/// Run A -- a fresh Подтсан (class 3) in district 1, 30 walks, 393 draws.
///
/// **Expected to fail** at index 18; see the module doc.
#[test]
fn run_a_replays_exactly() {
    replay("A");
}

/// Run B -- a fresh Вор (class 6) in district 1, 25 walks, 325 draws. The
/// only runs that exercise draws 10 and 11 (`1000:b2fa`, `1000:b321`) are
/// this one and E.
///
/// **Expected to fail** at index 63; see the module doc.
#[test]
fn run_b_replays_exactly() {
    replay("B");
}

/// Run C -- 3 walks, 30 draws, and the church fires on turn 1 taking its
/// `Random(5) == 0` arm: draw 15 plus the level-up routine's two draws at
/// `1000:25fe`.
#[test]
fn run_c_replays_exactly() {
    replay("C");
}

/// Run D -- 3 walks, 29 draws, church on turn 1 taking the `Random(5) == 1`
/// arm: draw 15 plus draw 16's `Random(4)` stat blessing.
#[test]
fn run_d_replays_exactly() {
    replay("D");
}

/// Run E -- `SAVE_R3.SAV` loaded at district 3: a Вор with the phone and the
/// ring, so this is the only run that reaches draws 3, 4 and 9, and the only
/// one where draws 10/11 use a district other than 1 (`n` = 60 and 15).
///
/// **Expected to fail** at index 79; see the module doc.
#[test]
fn run_e_replays_exactly() {
    replay("E");
}

/// Run C's whole 29-variable end state, the channel the draw comparison
/// cannot see. Among other things this pins the class-3 bonus
/// (`1000:73cf`/`1000:73d4`, girl + club) and the church's forced level-up
/// on its `Random(5) == 0` arm: `str 4`, `vit 4`, `hpmax 34`, `level 1`,
/// `threshold 20`.
#[test]
fn run_c_final_state_matches() {
    let run = run_named("C");
    let (g, _) = drive(&run);
    assert_final_state("C", &run, &g);
}

/// Run D's whole 29-variable end state. Pins draw 16's arm 3
/// (`1000:7fff`, the `Random(4)` stat blessing) as the luck grant: `luck 4`
/// where the character was created with 3.
#[test]
fn run_d_final_state_matches() {
    let run = run_named("D");
    let (g, _) = drive(&run);
    assert_final_state("D", &run, &g);
}

/// The first 18 draws of run A -- the whole matching prefix of a run whose
/// full assertion is red. Six turns' worth of the ordinary preamble, plus
/// draws 1 and 2 firing and then stopping (`1000:af68`, `1000:afc7`).
#[test]
fn run_a_matches_the_prefix_before_its_divergence() {
    replay_prefix(
        "A",
        A_DIVERGES_AT,
        &[
            "1000:af68",
            "1000:b186",
            "1000:b1b8",
            "1000:b1ea",
            "1000:b21c",
        ],
    );
}

/// The first 63 draws of run B. This is the committed check on **draws 10 and
/// 11** -- the Вор's two thief draws, `1000:b2fa` (`Random(district * 20)`)
/// and `1000:b321` (`Random(district * 5)`, reached only when
/// `luck >= draw 10`). Before this test they were exercised only by tests
/// that fail overall.
#[test]
fn run_b_matches_the_prefix_before_its_divergence() {
    replay_prefix("B", B_DIVERGES_AT, &["1000:b2fa", "1000:b321"]);
}

/// The first 79 draws of run E. This is the committed check on **draws 3, 4
/// and 9** -- the two phone gags (`1000:b030` `Random(200)`, `1000:b0dc`
/// `Random(100)`) and the ring's injury heal (`1000:b272` `Random(20)`) --
/// and on draws 10 and 11 at a district other than 1 (`n` = 60 and 15).
/// Before this test all five were exercised only by tests that fail overall.
#[test]
fn run_e_matches_the_prefix_before_its_divergence() {
    replay_prefix(
        "E",
        E_DIVERGES_AT,
        &[
            "1000:b030",
            "1000:b0dc",
            "1000:b272",
            "1000:b2fa",
            "1000:b321",
        ],
    );
}

/// The church turn produces no encounter: `1000:8282` (`c6 06 70 39 00`)
/// zeroes the already-rolled bucket on every path out of `1000:7c67`, so a
/// turn whose draw 13 came up `0` ends with nothing, whatever draw 12 rolled.
///
/// **Run C's turn 1 is the live proof of it.** Its draw 12 (`1000:b353`)
/// returns `8`, i.e. `wander_roll = 9`, which is bucket 3 -- the fight
/// encounter. Draw 13 then returns `0`, and the run's own trace shows *no*
/// enemy-generation draws on that turn: the very next entry is draw 14 at
/// `1000:b3ae`. The bucket was rolled and thrown away.
///
/// Asserted here through the input script rather than through the draws (the
/// full-sequence test above already covers those): an encounter reads a
/// line, a cancelled turn does not.
#[test]
fn the_church_cancels_an_already_rolled_bucket_three_turn() {
    let run = run_named("C");
    // The oracle's own numbers, so this test states what it relies on.
    assert_eq!(
        (run.draws[6].site.as_str(), run.draws[6].r),
        ("1000:b353", 8)
    );
    assert_eq!(
        (run.draws[7].site.as_str(), run.draws[7].r),
        ("1000:b39e", 0)
    );
    assert_eq!(run.draws[8].site, "1000:7f63", "the church was entered");

    let mut g = game_for(&run);
    let mut input = declines(8);
    g.walk(&mut input).unwrap();
    assert_eq!(
        input.count(),
        8,
        "wander_roll 9 is bucket 3, but the church zeroed it at 1000:8282: \
         the turn must not reach an encounter prompt"
    );
}
