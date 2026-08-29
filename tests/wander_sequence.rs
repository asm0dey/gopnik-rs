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
//! handler is the same one: `run`. That is not a shortcut, it is the only
//! string that answers all three prompt kinds the way the driver did.
//!
//! * At a question (the girl encounter, the fight encounter, the mage's
//!   save) the original compares the answer against the literal `y`
//!   (file `0x9BF3`) and treats **everything else** as a decline --
//!   `1000:b548`, `1000:b696` and `1000:b718` are all `jnz`. `run` is not
//!   `y`, so it declines exactly as the driver's `n` did.
//! * At the `^0Битва\` prompt `run` is the flee token
//!   (`1000:48dc`, file `0x4C8B`) -- which is what the driver typed there.
//!
//! It became relevant with Task 11f: run A's turn 7 rolls class 8, and the
//! cop encounter at `1000:b76a` starts a fight with **no question asked**
//! (`1000:b81a` sets the accept flag directly). A test that could only say
//! `n` would sit at that combat prompt forever.
//!
//! ## All five runs replay, as of Task 11f
//!
//! Runs A, B and E were **deliberately red** from Task 11c until Task 11f,
//! each diverging at the first draw of `FUN_1000_0d14`
//! (`1000:0d26`): A at index 18, B at 63, E at 79. The cause was single and
//! enumerated -- the random-encounter opponent and the fight flow around it
//! were not recovered, `Game::pick_enemy` was an admitted approximation, and
//! because the RNG is one shared stream a single bucket-3 turn desynchronised
//! everything after it.
//!
//! Task 11f recovered `1000:0d14`..`1000:11bf` and the three fight-flow sites
//! (`1000:b5f1`, `1000:b725`, `1000:b792`), and all five runs now replay their
//! whole draw stream -- 1387 draws in total. The assertions were never
//! narrowed to make that happen: `replay()` still compares site, `n` and `r`
//! for the **whole** run including the draw count
//! (`first_mismatch(.., usize::MAX)`).
//!
//! ## Prefix assertions -- added, not substituted
//!
//! Draws 3, 4, 9, 10 and 11 (the phone gags, the ring's heal, the thief's two
//! rolls) occur only in runs B and E. While those runs were red overall, a
//! regression in any of them changed nothing observable, so
//! `run_{a,b,e}_matches_the_preamble_prefix` were added to assert
//! the first `N` draws, `N` being the run's own divergence index at the time.
//! They are kept now that the whole-run assertions are green: each one also
//! asserts that the sites it exists for actually occur inside its window, so
//! it still cannot pass by covering nothing, and it localises a regression to
//! the preamble instead of only reporting it from wherever the whole-run
//! comparison happens to break.
//!
//! ## The state channel
//!
//! The draw sequence is only one of two channels. `apply_class_bonus`'s flags,
//! the church's five stat arms and three gift arms, the errand flags and the
//! level-up grants are all invisible to it. `run_*_final_state_matches` assert
//! the capture's whole 29-variable `final_state`, and since Task 11f that is
//! **all five runs**, not just C and D: the two channels are independent, so
//! five matching end states is a real second check on the encounter generator
//! and not a restatement of the draw comparison.

use gopnik::combat_dispatch::Pistol;
use gopnik::game::Game;
use gopnik::locations::Location;
use gopnik::model::Fighter;
use gopnik::progress::{new_character, Progress, GAINS_PER_LEVEL, MAX_LEVEL};
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

/// One row of `data/rng_trace.json`'s `sites_not_in_catalogue` -- the
/// summary the capture wrote of every `Random` site it saw that
/// `docs/re/wander.md`'s preamble catalogue does not cover. It is a
/// *different* field from `runs[].draws`, aggregated across all five runs.
#[derive(Deserialize)]
struct SiteSummary {
    count: usize,
    n_values: Vec<u16>,
}

#[derive(Deserialize)]
struct Trace {
    runs: Vec<Run>,
    sites_not_in_catalogue: std::collections::BTreeMap<String, SiteSummary>,
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
        // Task 11i: the per-turn capture reads `20ae:38c3` and `20ae:38c9`,
        // which the 29-variable `final_state` never carried -- and run E's
        // first sample showed the loaded save starting with 20 half-litres of
        // beer and 65 Хлам while this reconstruction started it at zero. Same
        // record arithmetic as the money above (`.SAV off = 0x200 + (addr -
        // 0x389c)`), on addresses `docs/re/gaps.md:283` already names.
        beer_dl: u16at(0x227),  // 20ae:38c3
        junk: u16at(0x22d),     // 20ae:38c9
        ..Fighter::default()
    };
    // `array[1..40] of string[2]` at `.SAV 0x236` (`20ae:38d2`, reached
    // through Borland's biased base `20ae:38cf`): element `n` is a length
    // byte then two code characters at `0x236 + (n - 1) * 3`. The flee
    // penalty at `1000:4954` is the only reader, and a fleeing run needs it.
    let mut growth_log = [[0u8; GAINS_PER_LEVEL]; MAX_LEVEL as usize + 1];
    for (n, entry) in growth_log.iter_mut().enumerate().skip(1) {
        let base = 0x236 + (n - 1) * 3;
        let len = usize::from(b[base]).min(GAINS_PER_LEVEL);
        entry[..len].copy_from_slice(&b[base + 1..base + 1 + len]);
    }
    let progress = Progress {
        xp: u32::from(u16at(0x232)),        // 20ae:38ce
        threshold: u32::from(u16at(0x234)), // 20ae:38d0
        growth_log,
    };
    let mut g = Game::new(player, progress, seed);
    g.district = run
        .district_key
        .parse::<u8>()
        .unwrap_or_else(|_| panic!("run {}: bad district_key", run.label));
    g.dark_glasses = b[0x217] != 0; // 20ae:38b3
    g.has_mobile = b[0x21f] != 0; // 20ae:38bb
    g.prison_tattoo = b[0x220] != 0; // 20ae:38bc
    g.oneshot_gift_1 = b[0x223] != 0; // 20ae:38bf
    g.oneshot_gift_2 = b[0x224] != 0; // 20ae:38c0
    g.ring_gospodi_pomilui = b[0x225] != 0; // 20ae:38c1
    g.pontovost_street = i32::from(u16at(0x22f)); // 20ae:38cb
    g.buff_countdown = b[0x231]; // 20ae:38cd

    // 20ae:394d / 394e / 394f, three adjacent bytes and a word: the pistol,
    // its silencer and its magazine. `20ae:394d` used to be read into a field
    // called `dealer_order_placed`; it is the pistol flag.
    g.pistol = Pistol {
        owned: b[0x2b1] != 0,
        silencer: b[0x2b2] != 0,
        cartridges: u16at(0x2b3) as i16,
    };
    g.church_visits = b[0x2b5]; // 20ae:3951

    // The seven discovery flags are NOT in the record: they live in
    // `places.sav`, and run E's guest ended with den/girl/club/gym clear
    // (`final_state`) while nothing in a wander turn can clear them -- so
    // this run began with them clear. Vet and Market are set during the run
    // by draws 5 and 6 (turns 3 and 19 of `runs[E].draws`), which used to
    // leave their starting value undetermined by the capture. Task 11i's
    // per-turn capture settles it: run E's `turn 1` sample -- the state at
    // the first `1000:ae63` stop, before any `w` -- reads `20ae:3694 = 0`
    // and `20ae:3698 = 0`, with only `20ae:3695` set. That is what this
    // reconstruction already assumed, and it is now observed rather than
    // argued.
    g.places.reset_for_new_district();
    // 1000:73bb runs on the load path too (`docs/re/wander.md`, "What
    // reaches 1000:73bb"): a Вор gets the dealers back.
    g.places.mark_found(Location::Dealers);
    g
}

/// `run` for every prompt: see the module doc for why one string answers
/// all three prompt kinds the way `tools/rngtrace/driver.py` answered them.
/// Bounded rather than endless so a handler that ignores the line cannot
/// spin forever.
fn declines(n: usize) -> std::vec::IntoIter<std::io::Result<String>> {
    (0..n)
        .map(|_| Ok("run".to_string()))
        .collect::<Vec<_>>()
        .into_iter()
}

/// Where each of these runs' draw sequences used to first disagree with the
/// oracle, as a 0-based index into `runs[].draws`: the index of the first
/// `1000:0d26`, which is where `Game::pick_enemy`'s approximation stood in
/// for `FUN_1000_0d14` before Task 11f recovered it.
///
/// They are no longer divergence points -- all five runs replay in full --
/// and they are kept only as the window bound for the prefix assertions
/// below, which is the one thing they were ever used for. The whole-run
/// assertions do not read them.
const A_PREFIX: usize = 18;
const B_PREFIX: usize = 63;
const E_PREFIX: usize = 79;

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
        found(Location::Dealers),
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
/// Six of its turns reach wander bucket 3, and turn 7 rolls class 8: the
/// only cop encounter in the captures that gets past the notice roll, so
/// this is the run that exercises `1000:b801`, the fight it starts and the
/// `run` that ends it.
#[test]
fn run_a_replays_exactly() {
    replay("A");
}

/// Run B -- a fresh Вор (class 6) in district 1, 25 walks, 325 draws. The
/// only runs that exercise draws 10 and 11 (`1000:b2fa`, `1000:b321`) are
/// this one and E.
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
/// one where draws 10/11 use a district other than 1 (`n` = 60 and 15). It
/// is also the only run with the зоновская наколка (`20ae:38bc`), which
/// halves the ordinary encounter's notice roll at `1000:b5f1` from 36 to
/// 18 -- both values appear in the capture, 18 here and 36 at the cop's
/// unhalved `1000:b792`.
#[test]
fn run_e_replays_exactly() {
    replay("E");
}

/// Every site in `data/rng_trace.json`'s `sites_not_in_catalogue` must fire
/// exactly as often as the capture saw it, with exactly the same set of `n`,
/// summed over all five runs.
///
/// This reads `sites_not_in_catalogue`, a field of the oracle distinct from
/// the per-run `draws` arrays the replay tests compare against -- but it is
/// **not an independent channel**: `sites_not_in_catalogue` is exactly
/// derivable from `runs[].draws` (same counts, same `n` sets, over all 17
/// sites), so this test cannot fail unless a replay test fails too. What it
/// actually checks is that the oracle's own aggregate agrees with its
/// per-run arrays -- a consistency check worth having, since the two are
/// computed differently in `data/rng_trace.json`, but not a second source of
/// truth on Task 11f's recovered `n` formulas.
///
/// It cannot pass vacuously. The assertions below require the map to be
/// non-empty, every count to be non-zero, and the port to have made at least
/// one draw at every site named -- so a port that stopped drawing at, say,
/// `1000:0d91` (five stops out of 1387 draws, easy to lose) fails here rather
/// than reporting "0 == 0".
#[test]
fn the_uncatalogued_sites_fire_as_often_as_the_capture_saw_them() {
    let t = trace();
    assert!(
        !t.sites_not_in_catalogue.is_empty(),
        "the oracle's sites_not_in_catalogue is empty, so this test checks nothing"
    );

    let mut got: std::collections::BTreeMap<String, (usize, std::collections::BTreeSet<u16>)> =
        std::collections::BTreeMap::new();
    for label in ["A", "B", "C", "D", "E"] {
        let run = run_named(label);
        let (_, draws) = drive(&run);
        for d in draws {
            let e = got.entry(d.site.to_string()).or_default();
            e.0 += 1;
            e.1.insert(d.n);
        }
    }

    for (site, want) in &t.sites_not_in_catalogue {
        assert!(
            want.count > 0,
            "{site}: the capture recorded 0 stops, so asserting it proves nothing"
        );
        let (count, ns) = got
            .get(site)
            .unwrap_or_else(|| panic!("{site}: the port never drew here at all"));
        assert_eq!(count, &want.count, "{site}: number of draws");
        let want_ns: std::collections::BTreeSet<u16> = want.n_values.iter().copied().collect();
        assert_eq!(ns, &want_ns, "{site}: the set of n values pushed");
    }
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

/// Run A's whole 29-variable end state. Task 11f added it: before the
/// encounter generator landed this run's draw stream diverged at index 18
/// and its end state legitimately could not be asserted. It can now, and
/// leaving it out would have kept a hole exactly where the new code runs --
/// run A is the only captured run that enters a fight at all (turn 7's cop).
#[test]
fn run_a_final_state_matches() {
    let run = run_named("A");
    let (g, _) = drive(&run);
    assert_final_state("A", &run, &g);
}

/// Run B's whole 29-variable end state. Same reason as run A's.
#[test]
fn run_b_final_state_matches() {
    let run = run_named("B");
    let (g, _) = drive(&run);
    assert_final_state("B", &run, &g);
}

/// Run E's whole 29-variable end state -- the only loaded-save run, so this
/// is the only committed check that the `.SAV`-derived starting state
/// survives 25 walks unchanged where the original left it unchanged.
#[test]
fn run_e_final_state_matches() {
    let run = run_named("E");
    let (g, _) = drive(&run);
    assert_final_state("E", &run, &g);
}

/// The first 18 draws of run A: six turns' worth of the ordinary preamble,
/// plus draws 1 and 2 firing and then stopping (`1000:af68`, `1000:afc7`).
#[test]
fn run_a_matches_the_preamble_prefix() {
    replay_prefix(
        "A",
        A_PREFIX,
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
/// `luck >= draw 10`) -- localised to the preamble.
#[test]
fn run_b_matches_the_preamble_prefix() {
    replay_prefix("B", B_PREFIX, &["1000:b2fa", "1000:b321"]);
}

/// The first 79 draws of run E. This is the committed check on **draws 3, 4
/// and 9** -- the two phone gags (`1000:b030` `Random(200)`, `1000:b0dc`
/// `Random(100)`) and the ring's injury heal (`1000:b272` `Random(20)`) --
/// and on draws 10 and 11 at a district other than 1 (`n` = 60 and 15).
#[test]
fn run_e_matches_the_preamble_prefix() {
    replay_prefix(
        "E",
        E_PREFIX,
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

// ---------------------------------------------------------------------------
// Task 11i: the per-turn state channel (`data/state_trace.json`).
//
// `data/rng_trace.json` scores draws, and its `final_state` is one sample per
// run, at the end. `data/state_trace.json` holds the SAME variables sampled at
// every turn marker (`1000:ae63`, the top-level prompt's `ReadLn`) of the same
// five runs, plus six the end-of-run sample never carried: the purse
// (`20ae:38c3` beer, `20ae:38c7` money, `20ae:38c9` Хлам) and the rolled
// enemy's three loot words (`20ae:396a`/`396c`/`396e`).
//
// **Where the expected values come from.** Every number in
// `data/state_trace.json` was read out of `orig/g.exe`'s own memory under
// qemu+gdb by `tools/rngtrace/run.py`. None of it is computed by this port --
// which is the only reason the comparison below is an oracle and not a mirror.
// The capture tool refuses to publish a run whose draw stream is not
// draw-for-draw identical to the same run in `data/rng_trace.json`, so a
// sample's `turn` indexes the same turns that file's draws carry.
//
// **What it can and cannot say.** One sample per turn shows a turn's NET
// effect. It does not order the changes inside a turn, and a value that moved
// and moved back within one turn leaves no trace here. A mismatch falsifies a
// claim about what the turn did; a match corroborates it. Neither establishes
// flow -- that still comes from the disassembly, with an address and a tier
// (`docs/re/METHODOLOGY.md`).

/// One turn's sample. `#[serde(deny_unknown_fields)]` for the same reason
/// [`FinalState`] has it: a variable the capture carries and this struct does
/// not name would otherwise be silently ignored, and "all 35 are checked"
/// would be a hope rather than a claim.
///
/// Three of the 35 are captured but NOT asserted below, and they are named
/// here so that stays visible rather than becoming an accidental omission:
/// `enemy_beer_396a`, `enemy_money_396c` and `enemy_hlam_396e` are the rolled
/// opponent's loot words, which the original writes on every bucket-3 turn
/// (`FUN_1000_0d14`, ahead of the notice roll and the question) and keeps in
/// globals afterwards. This port has no such globals: it builds the opponent
/// as a value and only retains one after a fight. Asserting them would need a
/// port change, which is not this test's business; capturing them is what
/// makes that gap measurable.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateSample {
    turn: usize,
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
    /// `20ae:38c3`, beer in half-litres (`docs/re/gaps.md:283`; the victory
    /// block's `add [0x38c3],ax` at `1000:5241`).
    beer_38c3: u16,
    /// `20ae:38c7`, the player's money (`docs/re/tables.md:191`, the shop
    /// affordability compare `3B 06 C7 38`; `add [0x38c7],ax` at `1000:5248`).
    money_38c7: u16,
    /// `20ae:38c9`, Хлам (`docs/re/gaps.md:283`; `add [0x38c9],ax` at
    /// `1000:524f`).
    hlam_38c9: u16,
    /// `20ae:396a`, the rolled enemy's beer drop (`docs/re/progression.md:233`;
    /// `mov ax,[0x396a]` at `1000:523e`). Captured, not asserted -- see the
    /// struct doc.
    enemy_beer_396a: u16,
    /// `20ae:396c`, the rolled enemy's money drop (`1000:5245`). Captured, not
    /// asserted.
    enemy_money_396c: u16,
    /// `20ae:396e`, the rolled enemy's Хлам drop (`1000:524c`). Captured, not
    /// asserted.
    enemy_hlam_396e: u16,
}

#[derive(Deserialize)]
struct Alignment {
    equals_rng_trace_draws: bool,
    draws_compared: usize,
}

#[derive(Deserialize)]
struct StateRun {
    label: String,
    seed_hex: String,
    walks_requested: usize,
    alignment_with_rng_trace: Alignment,
    samples: Vec<StateSample>,
}

#[derive(Deserialize)]
struct StateTrace {
    runs: Vec<StateRun>,
}

fn state_trace() -> StateTrace {
    let bytes = std::fs::read(repo("data/state_trace.json")).expect("read data/state_trace.json");
    serde_json::from_slice(&bytes).expect("parse data/state_trace.json")
}

fn state_run_named(label: &str) -> StateRun {
    state_trace()
        .runs
        .into_iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("data/state_trace.json has no run {label}"))
}

/// Assert one turn's sample against the port's live [`Game`].
///
/// 32 of the capture's 35 variables; the three the port keeps no global for
/// are named in [`StateSample`]'s doc with the reason.
fn assert_state_sample(label: &str, s: &StateSample, g: &Game) {
    let turn = s.turn;
    let b = |v: u8| v != 0;
    let found = |loc: Location| u8::from(g.places.is_found(loc));
    let at = |what: &str| format!("{label} turn {turn}: {what}");

    assert_eq!(g.player.strength, s.strength_389e, "{}", at("20ae:389e"));
    assert_eq!(g.player.agility, s.agility_38a0, "{}", at("20ae:38a0"));
    assert_eq!(g.player.vitality, s.vitality_38a2, "{}", at("20ae:38a2"));
    assert_eq!(g.player.luck, s.luck_38a4, "{}", at("20ae:38a4"));
    assert_eq!(g.player.level, s.level_38a6, "{}", at("20ae:38a6"));
    assert_eq!(g.player.dmg_min, s.dmg_min_38a8, "{}", at("20ae:38a8"));
    assert_eq!(g.player.dmg_max, s.dmg_max_38aa, "{}", at("20ae:38aa"));
    assert_eq!(g.player.hp, s.hp_38ac, "{}", at("20ae:38ac"));
    assert_eq!(g.player.hpmax, s.hpmax_38ae, "{}", at("20ae:38ae"));
    assert_eq!(g.player.class, s.class_389c, "{}", at("20ae:389c"));
    assert_eq!(
        g.player.broken_jaw,
        b(s.broken_jaw_38b0),
        "{}",
        at("20ae:38b0")
    );
    assert_eq!(
        g.player.broken_leg,
        b(s.broken_leg_38b1),
        "{}",
        at("20ae:38b1")
    );
    assert_eq!(g.player.armor, s.unk_38b2, "{}", at("20ae:38b2"));
    assert_eq!(g.has_mobile, b(s.has_mobile_38bb), "{}", at("20ae:38bb"));
    assert_eq!(
        g.ring_gospodi_pomilui,
        b(s.ring_38c1),
        "{}",
        at("20ae:38c1")
    );
    assert_eq!(
        g.pontovost_street,
        s.street_cred_38cb,
        "{}",
        at("20ae:38cb")
    );
    assert_eq!(g.progress.xp, s.xp_38ce, "{}", at("20ae:38ce"));
    assert_eq!(
        g.progress.threshold,
        s.xp_threshold_38d0,
        "{}",
        at("20ae:38d0")
    );
    assert_eq!(g.district, s.district_3692, "{}", at("20ae:3692"));
    assert_eq!(
        found(Location::Market),
        s.flag_market_3694,
        "{}",
        at("20ae:3694")
    );
    assert_eq!(
        found(Location::Dealers),
        s.flag_3695,
        "{}",
        at("20ae:3695")
    );
    assert_eq!(found(Location::Den), s.flag_den_3696, "{}", at("20ae:3696"));
    assert_eq!(
        found(Location::Girl),
        s.flag_girl_3697,
        "{}",
        at("20ae:3697")
    );
    assert_eq!(found(Location::Vet), s.flag_vet_3698, "{}", at("20ae:3698"));
    assert_eq!(
        found(Location::Club),
        s.flag_club_3699,
        "{}",
        at("20ae:3699")
    );
    assert_eq!(found(Location::Gym), s.flag_gym_369a, "{}", at("20ae:369a"));
    assert_eq!(
        g.den_errand_1_pending,
        b(s.den_errand_1_3b78),
        "{}",
        at("20ae:3b78")
    );
    assert_eq!(
        g.den_errand_2_pending,
        b(s.den_errand_2_3b79),
        "{}",
        at("20ae:3b79")
    );
    // The three Task 11i purse words. The original keeps them as words
    // (`1000:5241`/`1000:5248`/`1000:524f` are `add [mem],ax`), and the port's
    // money is a signed counter because shops debit it, so the comparison is
    // made in i32 rather than truncating the port's value into a u16.
    assert_eq!(
        i32::from(g.player.beer_dl),
        i32::from(s.beer_38c3),
        "{}",
        at("20ae:38c3 (beer)")
    );
    assert_eq!(
        g.player.money,
        i32::from(s.money_38c7),
        "{}",
        at("20ae:38c7 (money)")
    );
    assert_eq!(
        i32::from(g.player.junk),
        i32::from(s.hlam_38c9),
        "{}",
        at("20ae:38c9 (Хлам)")
    );
    assert_eq!(g.rng.state(), s.randseed_367e, "{}", at("20ae:367e RandSeed"));
}

/// Drive one run turn by turn, asserting the capture's sample after each.
///
/// Sample `turn 1` is the state at the FIRST stop at `1000:ae63`, which is the
/// prompt the game reaches straight after character creation (or after loading
/// the save) and before the first `w` is typed -- so it is asserted against the
/// freshly built [`Game`], before any walk. Sample `turn k+1` is asserted after
/// the k-th walk.
fn replay_state(label: &str) {
    let sr = state_run_named(label);
    let run = run_named(label);
    assert_eq!(
        sr.seed_hex, run.seed_hex,
        "{label}: the two captures used different seeds, so their turns are not the same turns"
    );
    assert_eq!(
        sr.walks_requested, run.walks_requested,
        "{label}: the two captures walked different numbers of turns"
    );
    assert!(
        sr.alignment_with_rng_trace.equals_rng_trace_draws
            && sr.alignment_with_rng_trace.draws_compared == run.draws.len(),
        "{label}: data/state_trace.json is not aligned with data/rng_trace.json's \
         run {label} ({} draws compared, {} in the draw oracle), so its samples \
         describe a different history",
        sr.alignment_with_rng_trace.draws_compared,
        run.draws.len()
    );
    assert_eq!(
        sr.samples.len(),
        run.walks_requested + 1,
        "{label}: a run of {} walks stops at the top-level prompt {} times \
         (once before the first `w`, once after each)",
        run.walks_requested,
        run.walks_requested + 1
    );
    assert!(
        sr.samples.iter().enumerate().all(|(i, s)| s.turn == i + 1),
        "{label}: the samples are not one per consecutive turn"
    );

    let mut g = game_for(&run);
    let mut input = declines(4 * run.walks_requested + 16);
    assert_state_sample(label, &sr.samples[0], &g);
    for s in &sr.samples[1..] {
        g.walk(&mut input).expect("walk");
        assert_state_sample(label, s, &g);
    }
}

/// The state that a run's samples actually move, so a per-turn assertion
/// cannot pass by comparing a constant with itself.
fn fields_that_move(samples: &[StateSample]) -> Vec<&'static str> {
    let mut moved = Vec::new();
    let mut check = |name: &'static str, f: &dyn Fn(&StateSample) -> i64| {
        if samples.iter().any(|s| f(s) != f(&samples[0])) {
            moved.push(name);
        }
    };
    check("20ae:389e strength", &|s| i64::from(s.strength_389e));
    check("20ae:38a4 luck", &|s| i64::from(s.luck_38a4));
    check("20ae:38a6 level", &|s| i64::from(s.level_38a6));
    check("20ae:38ac hp", &|s| i64::from(s.hp_38ac));
    check("20ae:38ae hpmax", &|s| i64::from(s.hpmax_38ae));
    check("20ae:38ce xp", &|s| i64::from(s.xp_38ce));
    check("20ae:38c7 money", &|s| i64::from(s.money_38c7));
    check("20ae:38c3 beer", &|s| i64::from(s.beer_38c3));
    check("20ae:38c9 hlam", &|s| i64::from(s.hlam_38c9));
    check("20ae:3b78 errand 1", &|s| i64::from(s.den_errand_1_3b78));
    check("20ae:3b79 errand 2", &|s| i64::from(s.den_errand_2_3b79));
    check("20ae:3694 market", &|s| i64::from(s.flag_market_3694));
    check("20ae:3698 vet", &|s| i64::from(s.flag_vet_3698));
    check("20ae:367e RandSeed", &|s| i64::from(s.randseed_367e));
    // The three enemy loot words are not asserted against the port (see
    // `StateSample`'s doc), but they ARE read here: a captured column nothing
    // ever looks at would rot silently.
    check("20ae:396a enemy beer", &|s| i64::from(s.enemy_beer_396a));
    check("20ae:396c enemy money", &|s| i64::from(s.enemy_money_396c));
    check("20ae:396e enemy hlam", &|s| i64::from(s.enemy_hlam_396e));
    moved
}

/// Run A, turn by turn: 31 samples over 30 walks. The variables that move here
/// are the two den errands burning on their `0` (`1000:af71`, `1000:afd0`),
/// the market/vet discovery flags, HP, and `RandSeed`.
#[test]
fn run_a_per_turn_state_matches() {
    replay_state("A");
}

/// Run B, turn by turn: the Вор's thefts (`1000:b321` -> `add [0x38c7],ax` at
/// `1000:b32d`) make `20ae:38c7` a moving value here, which is the point of
/// widening the sampled table.
#[test]
fn run_b_per_turn_state_matches() {
    replay_state("B");
}

/// Run C, turn by turn: four samples, and the church's forced level-up on its
/// `Random(5) == 0` arm lands between two of them.
#[test]
fn run_c_per_turn_state_matches() {
    replay_state("C");
}

/// Run D, turn by turn: four samples, the `Random(4)` stat blessing arm.
#[test]
fn run_d_per_turn_state_matches() {
    replay_state("D");
}

/// Run E, turn by turn: the loaded save, at district 3, with the phone and the
/// ring -- 26 samples over 25 walks.
#[test]
fn run_e_per_turn_state_matches() {
    replay_state("E");
}

/// A per-turn comparison against a run whose state never changed would pass
/// while checking nothing. Every run must move at least one sampled variable,
/// and the thefts must make the money at `20ae:38c7` one of them somewhere in
/// the five.
#[test]
fn the_per_turn_samples_are_not_all_the_same_state() {
    let t = state_trace();
    assert_eq!(t.runs.len(), 5, "data/state_trace.json must hold all five runs");
    let mut money_moved_in = Vec::new();
    for r in &t.runs {
        assert!(
            r.samples.len() >= 4,
            "run {}: {} samples is too few to be a transition trace",
            r.label,
            r.samples.len()
        );
        let moved = fields_that_move(&r.samples);
        assert!(
            !moved.is_empty(),
            "run {}: not one sampled variable changes across the whole run, so \
             asserting it turn by turn proves nothing",
            r.label
        );
        if moved.contains(&"20ae:38c7 money") {
            money_moved_in.push(r.label.clone());
        }
    }
    assert!(
        !money_moved_in.is_empty(),
        "no run moves 20ae:38c7, so the money column of the widened table is \
         never actually exercised"
    );
}

/// The two captures are separate runs of the original, in separate VMs, months
/// apart -- so their end states agreeing is a real check on both, and on the
/// determinism the pinned seed is supposed to buy. No port code takes part in
/// this one.
#[test]
fn the_state_capture_ends_where_the_draw_capture_ended() {
    for label in ["A", "B", "C", "D", "E"] {
        let run = run_named(label);
        let sr = state_run_named(label);
        let last = sr.samples.last().expect("samples");
        let f = &run.final_state;
        assert_eq!(last.strength_389e, f.strength_389e, "{label}: 20ae:389e");
        assert_eq!(last.agility_38a0, f.agility_38a0, "{label}: 20ae:38a0");
        assert_eq!(last.vitality_38a2, f.vitality_38a2, "{label}: 20ae:38a2");
        assert_eq!(last.luck_38a4, f.luck_38a4, "{label}: 20ae:38a4");
        assert_eq!(last.level_38a6, f.level_38a6, "{label}: 20ae:38a6");
        assert_eq!(last.hp_38ac, f.hp_38ac, "{label}: 20ae:38ac");
        assert_eq!(last.hpmax_38ae, f.hpmax_38ae, "{label}: 20ae:38ae");
        assert_eq!(last.class_389c, f.class_389c, "{label}: 20ae:389c");
        assert_eq!(last.xp_38ce, f.xp_38ce, "{label}: 20ae:38ce");
        assert_eq!(
            last.xp_threshold_38d0, f.xp_threshold_38d0,
            "{label}: 20ae:38d0"
        );
        assert_eq!(last.district_3692, f.district_3692, "{label}: 20ae:3692");
        assert_eq!(
            last.street_cred_38cb, f.street_cred_38cb,
            "{label}: 20ae:38cb"
        );
        assert_eq!(
            last.den_errand_1_3b78, f.den_errand_1_3b78,
            "{label}: 20ae:3b78"
        );
        assert_eq!(
            last.den_errand_2_3b79, f.den_errand_2_3b79,
            "{label}: 20ae:3b79"
        );
        assert_eq!(last.randseed_367e, f.randseed_367e, "{label}: 20ae:367e");
    }
}
