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
struct SeedVectors {
    seed: u32,
    next_u32: Vec<u32>,
    below: Vec<BelowCase>,
}

#[derive(Deserialize)]
struct Vectors {
    seeds: Vec<SeedVectors>,
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
    assert!(v.seeds.len() >= 2, "need >=2 seed cases (0 and 0xFFFFFFFF)");
    for sv in &v.seeds {
        assert!(sv.next_u32.len() >= 64, "need >=64 captured outputs");
        let mut r = Rng::new(sv.seed);
        for (i, want) in sv.next_u32.iter().enumerate() {
            assert_eq!(
                r.next_u32(),
                *want,
                "next_u32 diverges at index {i} for seed {:#010x}",
                sv.seed
            );
        }
    }
}

#[test]
fn below_matches_original() {
    let v = vectors();
    for sv in &v.seeds {
        assert!(!sv.below.is_empty(), "need at least one modulus case");
        for case in &sv.below {
            let mut r = Rng::new(sv.seed);
            for (i, want) in case.expected.iter().enumerate() {
                assert_eq!(
                    r.below(case.n),
                    *want,
                    "below({}) diverges at {i} for seed {:#010x}",
                    case.n,
                    sv.seed
                );
            }
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

/// `src/rng.rs` documents `below(0) == 0` and `below(1) == 0` (the range
/// mapping is `(state * n) >> 32`, so both n=0 and n=1 always give 0 no
/// matter what the seed draws); pin both directly rather than only by
/// inference from the shape of the formula.
#[test]
fn below_edge_cases() {
    let mut r = Rng::new(0xdead_beef);
    for _ in 0..100 {
        assert_eq!(r.below(0), 0);
    }
    let mut r = Rng::new(0xdead_beef);
    for _ in 0..100 {
        assert_eq!(r.below(1), 0);
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
