//! The embedded tables must parse, and must still contain the rows Task 10
//! extracted from `orig/g.exe`. The expected values here are the ones the
//! DOSBox-X oracle printed on the original's own screens -- see the
//! "Oracle cross-checks" section of `docs/re/tables.md` for the runs.
//!
//! These tests exercise only runtime fields -- `Item`, `ShopEntry` and
//! `Enemy` no longer carry a Ghidra address or file offset at all (see
//! `docs/re/tables.md`, "Runtime vs. provenance"), so there is nothing
//! provenance-shaped left here to assert on or to move to a
//! `data/*.provenance.json` read; that split lives entirely in
//! `tools/test_extract_tables.py`, which checks both files.

use gopnik::data;
use gopnik::progress::CLASS_WEIGHTS;

#[test]
fn items_load_and_contain_known_entries() {
    let items = data::items();
    assert!(
        items.len() >= 15,
        "expected >=15 items, got {}",
        items.len()
    );
    let tesak = items
        .iter()
        .find(|i| i.name == "Тесак")
        .expect("Тесак missing");
    assert_eq!(tesak.kind, "weapon");
    assert_eq!(tesak.bonus, 9);
    // Тесак is only ever found, never sold (docs/re/tables.md,
    // "Prices are deliberately null...").
    assert!(!tesak.sold, "Тесак should not be sold");
    assert!(tesak.price.is_none());

    // The original's own status screen prints these two side by side as
    // "Костюм Adidas(+2) Крутая кожанка(+4)".
    let adidas = items
        .iter()
        .find(|i| i.name == "Костюм Adidas")
        .expect("Костюм Adidas missing");
    assert_eq!(adidas.kind, "suit");
    assert_eq!(adidas.bonus, 2);
    // Adidas has a null price, but it *is* sold under a paraphrased shop
    // name -- a different unknown than Тесак's "never sold".
    assert!(adidas.sold, "Костюм Adidas should be sold");

    // No item name may carry the `^N` markup into user-visible text.
    for i in items {
        assert!(!i.name.contains('^'), "markup leaked into {:?}", i.name);
    }
}

#[test]
fn loot_only_items_are_never_sold() {
    let items = data::items();
    let loot_only: std::collections::BTreeSet<&str> = [
        "Крестик",
        "Кольцо \"Гс\"",
        "Кольцо \"Пг\"",
        "Мега Кольцо",
        "Кольцо \"Гп\"",
        "Нож",
        "Тесак",
    ]
    .into_iter()
    .collect();

    for item in items {
        let expect_sold = !loot_only.contains(item.name);
        assert_eq!(
            item.sold, expect_sold,
            "{}: sold should be {expect_sold}",
            item.name
        );
        if !item.sold {
            assert!(item.price.is_none(), "{}: loot-only but priced", item.name);
        }
    }
}

#[test]
fn tables_are_static_and_complete() {
    // Borrowing into a `'static` binding does not compile against a `Vec`
    // returned by value -- this is the compile-time half of the assertion.
    let items: &'static [gopnik::data::Item] = gopnik::data::items();
    let shops: &'static [gopnik::data::ShopEntry] = gopnik::data::shops();
    let enemies: &'static [gopnik::data::Enemy] = gopnik::data::enemies();

    // Row counts are pinned to what tools/extract_tables.py extracts.
    assert_eq!(items.len(), 15, "item row count");
    assert_eq!(shops.len(), 18, "shop row count");
    assert_eq!(enemies.len(), 13, "enemy row count");

    // Two calls hand back the same memory: no per-call parse, no allocation.
    assert!(std::ptr::eq(gopnik::data::items(), gopnik::data::items()));
}

#[test]
fn class_weights_agree_with_progress_table() {
    // progress::CLASS_WEIGHTS and data/enemies.json's growth_weights are two
    // copies of the same 44 bytes at 20ae:0002 -- tests/progression.rs
    // checks CLASS_WEIGHTS against data/xp.json, an independent artifact;
    // this checks it against the Task 10 table too.
    let enemies = data::enemies();
    for (class, want) in CLASS_WEIGHTS.iter().enumerate() {
        let row = enemies
            .iter()
            .find(|e| e.class as usize == class)
            .unwrap_or_else(|| panic!("no data/enemies.json row for class {class}"));
        assert_eq!(
            row.growth_weights,
            want.as_slice(),
            "class {class}: data/enemies.json vs progress::CLASS_WEIGHTS"
        );
    }
}

#[test]
fn shop_prices_match_the_original_screens() {
    let shops = data::shops();
    let mar: Vec<_> = shops.iter().filter(|r| r.shop == "mar").collect();
    let bmar: Vec<_> = shops.iter().filter(|r| r.shop == "bmar").collect();
    assert_eq!(mar.len(), 9);
    assert_eq!(bmar.len(), 9);

    let mar_prices: Vec<i32> = mar.iter().map(|r| r.price).collect();
    assert_eq!(mar_prices, vec![2, 5, 10, 15, 15, 25, 30, 30, 50]);
    let bmar_prices: Vec<i32> = bmar.iter().map(|r| r.price).collect();
    assert_eq!(bmar_prices, vec![15, 30, 20, 10, 25, 50, 150, 70, 60]);

    // The original prints the ammo price on the silencer row but debits the
    // silencer's own price. Confirmed on screen: the row reads
    // "9 - 70 руб. Глушитель." and buying it took the player from 970 to 910.
    let silencer = bmar[8];
    assert_eq!(silencer.price, 60);
    assert_eq!(silencer.displayed_price, 70);
}

#[test]
fn enemies_carry_the_two_scripted_bosses() {
    let enemies = data::enemies();
    assert_eq!(enemies.len(), 13);

    let generated: Vec<_> = enemies.iter().filter(|e| e.generated).collect();
    assert_eq!(
        generated.len(),
        10,
        "classes 0..9 are the random encounters"
    );
    for e in &generated {
        assert!(e.stats.is_none(), "{} has no fixed stat block", e.name);
    }

    let v0 = enemies
        .iter()
        .find(|e| e.id == "rektor_ngu_v0")
        .expect("rektor_ngu_v0 missing");
    let s0 = v0.stats.as_ref().expect("v0 stats");
    assert_eq!(v0.level, Some(125));
    assert_eq!(s0.strength, 41);
    assert_eq!(s0.hpmax, 666);
    assert_eq!(s0.armor, 60);

    let v1 = enemies
        .iter()
        .find(|e| e.id == "rektor_ngu_v1")
        .expect("rektor_ngu_v1 missing");
    let s1 = v1.stats.as_ref().expect("v1 stats");
    assert_eq!(v1.level, Some(160));
    assert_eq!(s1.hpmax, 1000);
    assert_eq!(s1.armor, 80);
    assert_eq!(s1.dmg_min, 25);
    assert_eq!(s1.dmg_max, 50);
}

#[test]
fn boss_stat_blocks_round_trip_into_a_fighter() {
    let enemies = data::enemies();
    let v0 = enemies
        .iter()
        .find(|e| e.id == "rektor_ngu_v0")
        .expect("rektor_ngu_v0 missing");
    let f = v0.to_fighter().expect("v0 has a stat block");
    assert_eq!(f.name, "Ректор НГУ");
    assert_eq!(f.class, 10);
    assert_eq!(f.level, 125);
    assert_eq!(f.hp, 666);
    assert_eq!(f.hpmax, 666);
    assert_eq!(f.strength, 41);
    assert_eq!(f.agility, 50);
    assert_eq!(f.vitality, 123);
    assert_eq!(f.luck, 36);
    assert_eq!(f.armor, 60);
    assert_eq!(f.dmg_min, 20);
    assert_eq!(f.dmg_max, 41);
    assert!(!f.broken_jaw && !f.broken_leg);

    // A generated class has nothing to convert.
    let gopnik = enemies.iter().find(|e| e.id == "gopnik").unwrap();
    assert!(gopnik.to_fighter().is_none());
}
