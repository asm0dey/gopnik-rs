use gopnik::combat::{resolve_blow, Blow};
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
        for (bi, want) in case.expected_blows.iter().enumerate() {
            let Blow { hit, damage } = resolve_blow(&mut rng, &a, &d);
            assert_eq!(hit, want.hit, "case {ci} blow {bi}: hit");
            assert_eq!(damage, want.damage, "case {ci} blow {bi}: damage");
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
