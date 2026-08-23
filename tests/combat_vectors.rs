use gopnik::combat::{blows_per_round, resolve_blow, resolve_blow_nth, Swing};
use gopnik::model::Fighter;
use gopnik::rng::Rng;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct FighterSpec {
    level: u16,
    strength: u16,
    agility: u16,
    vitality: u16,
    luck: u16,
    armor: u16,
    dmg_min: u16,
    dmg_max: u16,
    hp: u16,
    hpmax: u16,
    broken_jaw: bool,
    broken_leg: bool,
}

impl FighterSpec {
    fn build(&self, name: &str) -> Fighter {
        Fighter {
            name: name.to_string(),
            level: self.level,
            hp: self.hp,
            hpmax: self.hpmax,
            strength: self.strength,
            agility: self.agility,
            vitality: self.vitality,
            luck: self.luck,
            armor: self.armor,
            dmg_min: self.dmg_min,
            dmg_max: self.dmg_max,
            broken_jaw: self.broken_jaw,
            broken_leg: self.broken_leg,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct ExpectedBlow {
    hit: bool,
    damage: u16,
}

#[derive(Deserialize)]
struct Case {
    seed: u32,
    attacker: FighterSpec,
    defender: FighterSpec,
    expected_blows: Vec<ExpectedBlow>,
    /// Full blow count for the round, counted off the screen independently
    /// of `expected_blows`/`resolve_blow_nth`. See the `blows_in_round`
    /// assertion below.
    blows_in_round: u16,
    /// Which half of the round the case captured -- `"player"` or `"enemy"`.
    /// It has been in `data/combat_vectors.json` since the file was first
    /// written; Task 13 gave `resolve_blow_nth` a [`Swing`] argument (the two
    /// mirrors have different `Random` call sites, and
    /// `data/combat_trace.json` records the site of every draw), so this
    /// column is now read instead of ignored.
    attacker_is: String,
}

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
}

#[test]
fn combat_matches_original() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("combat_vectors.json");
    let v: Vectors = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    assert!(
        v.cases.len() >= 20,
        "need >=20 captured cases, got {}",
        v.cases.len()
    );

    for (ci, case) in v.cases.iter().enumerate() {
        let mut rng = Rng::new(case.seed);
        let a = case.attacker.build("A");
        let d = case.defender.build("D");

        // expected_blows is the whole round, in order. Asserting entry `bi`
        // against resolve_blow_nth(.., bi) -- not resolve_blow, which is
        // always index 0 -- exercises budget_at's per-blow accuracy for
        // every index a capture reached, not just a round's opening blow.
        for (bi, want) in case.expected_blows.iter().enumerate() {
            // `defender_tooth_guard` is false for every case on purpose:
            // `tools/capture_combat_vectors.py` SKIPS rounds where the player
            // owns the зубная защита (`docs/re/combat.md`, "Enemy-as-attacker
            // cases"), because a jaw break there spends the extra
            // `Random(4)` at 1000:47fe. So no case here can exercise it, and
            // asserting anything else would be asserting about data that does
            // not exist.
            let swing = match case.attacker_is.as_str() {
                "player" => Swing::player(),
                "enemy" => Swing::enemy(false),
                other => panic!("case {ci}: unknown attacker_is {other:?}"),
            };
            let o = resolve_blow_nth(&mut rng, &a, &d, bi as u16, swing);
            assert_eq!(o.hit, want.hit, "case {ci} blow {bi}: hit");
            assert_eq!(o.damage, want.damage, "case {ci} blow {bi}: damage");
        }

        // blows_in_round is read straight off the screen -- independent of
        // expected_blows and of resolve_blow_nth -- so comparing it against
        // blows_per_round(a, d) is a free, non-circular check of the blow
        // budget/count formula. blows_per_round can only ever be >= the
        // observed count: a round ends early exactly when the defender dies
        // mid-round (1000:4629 / 1000:48c6), never for any other reason. So
        // when the recorded blows did not deal lethal damage, the round must
        // have run its full budgeted length and the two must agree exactly.
        let bpr = blows_per_round(&a, &d);
        assert!(
            bpr >= case.blows_in_round,
            "case {ci}: blows_per_round {bpr} is less than the observed \
             blows_in_round {}",
            case.blows_in_round
        );
        let dealt: u32 = case.expected_blows.iter().map(|b| b.damage as u32).sum();
        if dealt < case.defender.hp as u32 {
            assert_eq!(
                bpr, case.blows_in_round,
                "case {ci}: defender survived the round (dealt {dealt} < hp \
                 {}) but blows_per_round disagrees with the screen count",
                case.defender.hp
            );
        }
    }
}

#[test]
fn damage_never_exceeds_defender_hp_underflow() {
    let mut rng = Rng::new(1);
    let a = FighterSpec {
        level: 6,
        strength: 90,
        agility: 120,
        vitality: 45,
        luck: 49,
        armor: 0,
        dmg_min: 20,
        dmg_max: 40,
        hp: 325,
        hpmax: 325,
        broken_jaw: false,
        broken_leg: false,
    }
    .build("A");
    let d = FighterSpec {
        level: 1,
        strength: 5,
        agility: 5,
        vitality: 5,
        luck: 1,
        armor: 0,
        dmg_min: 1,
        dmg_max: 2,
        hp: 3,
        hpmax: 3,
        broken_jaw: false,
        broken_leg: false,
    }
    .build("D");
    for _ in 0..1000 {
        let b = resolve_blow(&mut rng, &a, &d);
        assert!(b.damage < 10_000, "implausible damage {}", b.damage);
    }
}
