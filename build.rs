//! Compile-time codegen for the three runtime data tables.
//!
//! `tools/extract_tables.py` writes `data/items.json`, `data/shops.json` and
//! `data/enemies.json` by reading `orig/g.exe`; those files never change at
//! runtime, so this script parses them once, at build time, and emits one
//! Rust source file (`$OUT_DIR/tables.rs`) declaring `static ITEMS`,
//! `static SHOPS` and `static ENEMIES` array literals. `src/data.rs`
//! `include!`s that file and hands the arrays back as `&'static [T]` --
//! there is no `serde_json` in the shipped binary at all.
//!
//! This file parses with `serde_json::Value` rather than mirroring the
//! `Item` / `ShopEntry` / `Enemy` / `EnemyStats` structs from `src/data.rs`:
//! a second copy of the schema here could silently drift from the one those
//! structs define. A missing field, a wrong type, or an unknown `kind`
//! panics the build with the file name and the row's `id` -- malformed data
//! must never reach a binary.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde_json::Value;

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = Path::new(&out_dir).join("tables.rs");

    println!("cargo:rerun-if-changed=data/items.json");
    println!("cargo:rerun-if-changed=data/shops.json");
    println!("cargo:rerun-if-changed=data/enemies.json");

    let mut out = String::new();
    emit_items(&mut out);
    emit_shops(&mut out);
    emit_enemies(&mut out);

    fs::write(&dest, out).expect("failed to write generated tables.rs");
}

fn load_array(path: &str) -> Vec<Value> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let value: Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: invalid JSON: {e}"));
    match value {
        Value::Array(rows) => rows,
        other => panic!("{path}: expected a top-level JSON array, got {other:?}"),
    }
}

/// Row identifier used in panic messages -- falls back to the row's index
/// when the row doesn't even have an `id` field to report.
/// Names a row in build-failure messages. Shop rows carry no `id` -- they are
/// keyed by the shop they belong to and the key the player types -- so name
/// them by that pair rather than falling through to a bare index, which is
/// what a maintainer chasing a shop-data panic would otherwise get.
fn row_label(path: &str, row: &Value, index: usize) -> String {
    if let Some(id) = row.get("id").and_then(Value::as_str) {
        return format!("{path}: row {id:?}");
    }
    match (
        row.get("shop").and_then(Value::as_str),
        row.get("key").and_then(Value::as_str),
    ) {
        (Some(shop), Some(key)) => format!("{path}: row {shop}/{key}"),
        _ => format!("{path}: row #{index}"),
    }
}

fn req_str<'a>(path: &str, row: &'a Value, label: &str, field: &str) -> &'a str {
    row.get(field)
        .unwrap_or_else(|| panic!("{label}: missing field {field:?}"))
        .as_str()
        .unwrap_or_else(|| panic!("{label}: field {field:?} is not a string ({path})"))
}

fn req_i32(path: &str, row: &Value, label: &str, field: &str) -> i32 {
    let n = row
        .get(field)
        .unwrap_or_else(|| panic!("{label}: missing field {field:?}"))
        .as_i64()
        .unwrap_or_else(|| panic!("{label}: field {field:?} is not an integer ({path})"));
    i32::try_from(n).unwrap_or_else(|_| panic!("{label}: field {field:?} out of i32 range"))
}

fn req_u16(path: &str, row: &Value, label: &str, field: &str) -> u16 {
    let n = row
        .get(field)
        .unwrap_or_else(|| panic!("{label}: missing field {field:?}"))
        .as_u64()
        .unwrap_or_else(|| panic!("{label}: field {field:?} is not an unsigned integer ({path})"));
    u16::try_from(n).unwrap_or_else(|_| panic!("{label}: field {field:?} out of u16 range"))
}

fn req_bool(path: &str, row: &Value, label: &str, field: &str) -> bool {
    row.get(field)
        .unwrap_or_else(|| panic!("{label}: missing field {field:?}"))
        .as_bool()
        .unwrap_or_else(|| panic!("{label}: field {field:?} is not a boolean ({path})"))
}

fn opt_str<'a>(path: &str, row: &'a Value, label: &str, field: &str) -> Option<&'a str> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_str()
                .unwrap_or_else(|| panic!("{label}: field {field:?} is not a string ({path})")),
        ),
    }
}

fn opt_i32(path: &str, row: &Value, label: &str, field: &str) -> Option<i32> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(v) => {
            let n = v
                .as_i64()
                .unwrap_or_else(|| panic!("{label}: field {field:?} is not an integer ({path})"));
            Some(
                i32::try_from(n)
                    .unwrap_or_else(|_| panic!("{label}: field {field:?} out of i32 range")),
            )
        }
    }
}

fn opt_u16(path: &str, row: &Value, label: &str, field: &str) -> Option<u16> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(v) => {
            let n = v.as_u64().unwrap_or_else(|| {
                panic!("{label}: field {field:?} is not an unsigned integer ({path})")
            });
            Some(
                u16::try_from(n)
                    .unwrap_or_else(|_| panic!("{label}: field {field:?} out of u16 range")),
            )
        }
    }
}

fn req_str_array<'a>(path: &str, row: &'a Value, label: &str, field: &str) -> Vec<&'a str> {
    row.get(field)
        .unwrap_or_else(|| panic!("{label}: missing field {field:?}"))
        .as_array()
        .unwrap_or_else(|| panic!("{label}: field {field:?} is not an array ({path})"))
        .iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("{label}: field {field:?} has a non-string element ({path})"))
        })
        .collect()
}

fn req_u16_array(path: &str, row: &Value, label: &str, field: &str) -> Vec<u16> {
    row.get(field)
        .unwrap_or_else(|| panic!("{label}: missing field {field:?}"))
        .as_array()
        .unwrap_or_else(|| panic!("{label}: field {field:?} is not an array ({path})"))
        .iter()
        .map(|v| {
            let n = v.as_u64().unwrap_or_else(|| {
                panic!("{label}: field {field:?} has a non-integer element ({path})")
            });
            u16::try_from(n)
                .unwrap_or_else(|_| panic!("{label}: field {field:?} has an out-of-range element"))
        })
        .collect()
}

/// `{:?}` on `&str` is Rust's own `Debug` escaping: it escapes what must be
/// escaped (quotes, backslashes, control characters) and leaves printable
/// non-ASCII -- the Russian text -- alone. That is exactly what a `&'static
/// str` literal in the generated source needs, and it is why this script
/// never hand-rolls escaping.
fn str_lit(s: &str) -> String {
    format!("{s:?}")
}

const ITEM_KINDS: &[&str] = &["weapon", "suit", "charm", "armor"];

fn emit_items(out: &mut String) {
    const PATH: &str = "data/items.json";
    let rows = load_array(PATH);

    writeln!(out, "pub static ITEMS: &[Item] = &[").unwrap();
    for (i, row) in rows.iter().enumerate() {
        let label = row_label(PATH, row, i);
        let id = req_str(PATH, row, &label, "id");
        let name = req_str(PATH, row, &label, "name");
        let kind = req_str(PATH, row, &label, "kind");
        if !ITEM_KINDS.contains(&kind) {
            panic!("{label}: unknown kind {kind:?} (expected one of {ITEM_KINDS:?})");
        }
        let bonus = req_i32(PATH, row, &label, "bonus");
        let effect = opt_str(PATH, row, &label, "effect");
        let price = opt_i32(PATH, row, &label, "price");
        let sold = req_bool(PATH, row, &label, "sold");

        let effect_lit = match effect {
            Some(e) => format!("Some({})", str_lit(e)),
            None => "None".to_string(),
        };
        let price_lit = match price {
            Some(p) => format!("Some({p})"),
            None => "None".to_string(),
        };

        writeln!(
            out,
            "    Item {{ id: {}, name: {}, kind: {}, bonus: {bonus}, effect: {effect_lit}, price: {price_lit}, sold: {sold} }},",
            str_lit(id),
            str_lit(name),
            str_lit(kind),
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
}

fn emit_shops(out: &mut String) {
    const PATH: &str = "data/shops.json";
    let rows = load_array(PATH);

    writeln!(out, "pub static SHOPS: &[ShopEntry] = &[").unwrap();
    for (i, row) in rows.iter().enumerate() {
        let label = row_label(PATH, row, i);
        let shop = req_str(PATH, row, &label, "shop");
        let key = req_str(PATH, row, &label, "key");
        let text = req_str(PATH, row, &label, "text");
        let price = req_i32(PATH, row, &label, "price");
        let displayed_price = req_i32(PATH, row, &label, "displayed_price");
        let gate = opt_str(PATH, row, &label, "gate");
        let extra_gates = req_str_array(PATH, row, &label, "extra_gates");

        let gate_lit = match gate {
            Some(g) => format!("Some({})", str_lit(g)),
            None => "None".to_string(),
        };
        let extra_gates_lit = if extra_gates.is_empty() {
            "&[]".to_string()
        } else {
            let items: Vec<String> = extra_gates.iter().map(|g| str_lit(g)).collect();
            format!("&[{}]", items.join(", "))
        };

        writeln!(
            out,
            "    ShopEntry {{ shop: {}, key: {}, text: {}, price: {price}, displayed_price: {displayed_price}, gate: {gate_lit}, extra_gates: {extra_gates_lit} }},",
            str_lit(shop),
            str_lit(key),
            str_lit(text),
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
}

fn emit_enemies(out: &mut String) {
    const PATH: &str = "data/enemies.json";
    let rows = load_array(PATH);

    writeln!(out, "pub static ENEMIES: &[Enemy] = &[").unwrap();
    for (i, row) in rows.iter().enumerate() {
        let label = row_label(PATH, row, i);
        let id = req_str(PATH, row, &label, "id");
        let name = req_str(PATH, row, &label, "name");
        let class = req_u16(PATH, row, &label, "class");
        let level = opt_u16(PATH, row, &label, "level");
        let generated = req_bool(PATH, row, &label, "generated");
        let growth_weights = req_u16_array(PATH, row, &label, "growth_weights");

        let stats_val = row.get("stats").cloned().unwrap_or(Value::Null);
        let stats_lit = match stats_val {
            Value::Null => "None".to_string(),
            Value::Object(_) => {
                let strength = req_u16(PATH, &stats_val, &label, "strength");
                let agility = req_u16(PATH, &stats_val, &label, "agility");
                let vitality = req_u16(PATH, &stats_val, &label, "vitality");
                let luck = req_u16(PATH, &stats_val, &label, "luck");
                let dmg_min = req_u16(PATH, &stats_val, &label, "dmg_min");
                let dmg_max = req_u16(PATH, &stats_val, &label, "dmg_max");
                let hp = req_u16(PATH, &stats_val, &label, "hp");
                let hpmax = req_u16(PATH, &stats_val, &label, "hpmax");
                let armor = req_u16(PATH, &stats_val, &label, "armor");
                format!(
                    "Some(EnemyStats {{ strength: {strength}, agility: {agility}, vitality: {vitality}, luck: {luck}, dmg_min: {dmg_min}, dmg_max: {dmg_max}, hp: {hp}, hpmax: {hpmax}, armor: {armor} }})"
                )
            }
            other => panic!("{label}: field \"stats\" is not an object ({PATH}): {other:?}"),
        };

        let level_lit = match level {
            Some(l) => format!("Some({l})"),
            None => "None".to_string(),
        };
        let growth_weights_lit = if growth_weights.is_empty() {
            "&[]".to_string()
        } else {
            let items: Vec<String> = growth_weights.iter().map(|w| w.to_string()).collect();
            format!("&[{}]", items.join(", "))
        };

        writeln!(
            out,
            "    Enemy {{ id: {}, name: {}, class: {class}, level: {level_lit}, stats: {stats_lit}, growth_weights: {growth_weights_lit}, generated: {generated} }},",
            str_lit(id),
            str_lit(name),
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
}
