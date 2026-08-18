//! Compares `gopnik::rng` against `data/rng_vectors.json`.
//!
//! Those vectors are NOT produced by this crate. They come from
//! `tools/gen_rng_vectors.py`, which interprets the original's own
//! instruction bytes at `1f78:11a8` / `1f78:114b` out of `orig/g.exe`.
//! See `docs/re/rng.md`. If you ever regenerate them from `src/rng.rs`
//! these tests become circular and worthless.

use gopnik::rng::Rng;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct BelowCase {
    n: u16,
    expected: Vec<u16>,
}

#[derive(Deserialize)]
struct Vectors {
    seed: u32,
    next_u32: Vec<u32>,
    below: Vec<BelowCase>,
}

fn vectors() -> Vectors {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("rng_vectors.json");
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| {
        panic!("read {}: {e}", p.display());
    }))
    .expect("parse rng_vectors.json")
}

#[test]
fn raw_sequence_matches_original() {
    let v = vectors();
    assert!(v.next_u32.len() >= 64, "need >=64 captured outputs");
    let mut r = Rng::new(v.seed);
    for (i, want) in v.next_u32.iter().enumerate() {
        assert_eq!(r.next_u32(), *want, "next_u32 diverges at index {i}");
    }
}

#[test]
fn below_matches_original() {
    let v = vectors();
    assert!(!v.below.is_empty(), "need at least one modulus case");
    for case in &v.below {
        let mut r = Rng::new(v.seed);
        for (i, want) in case.expected.iter().enumerate() {
            assert_eq!(r.below(case.n), *want, "below({}) diverges at {i}", case.n);
        }
    }
}

#[test]
fn below_stays_in_range() {
    let mut r = Rng::new(12345);
    for _ in 0..10_000 {
        assert!(r.below(37) < 37);
    }
}

#[test]
fn same_seed_produces_the_same_sequence() {
    let mut a = Rng::new(0xdead_beef);
    let mut b = Rng::new(0xdead_beef);
    for _ in 0..1000 {
        assert_eq!(a.next_u32(), b.next_u32());
    }
}
