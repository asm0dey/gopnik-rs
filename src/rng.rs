//! Reimplementation of the original's pseudo-random generator.
//!
//! Constants and structure are transcribed from `docs/re/rng.md`, which
//! cites the Ghidra addresses they were read from: `System.@Rand` at
//! `1f78:11a8` (the multiplier's low word is the literal `$8405` stored at
//! `1f78:11de`; the high word `$0808` is synthesised from shifts) and
//! `System.Random(Word)` at `1f78:114b`.
//!
//! `Randomize` (`1f78:11e0`) seeds from DOS `INT 21h`/`AH=2Ch`. That is a
//! host-clock policy decision rather than part of the generator, so it is
//! not reproduced here; `Rng::new` takes the seed explicitly. The original's
//! load image ships `RandSeed` (`20ae:367e`) as `0`.

/// The original's `System.RandSeed`, stepped by the Borland Pascal LCG.
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng { state: seed }
    }

    /// One step of `@Rand` (`1f78:11a8`):
    /// `RandSeed := (RandSeed * $08088405 + 1) mod 2^32`.
    ///
    /// Returns the **new** state, matching the original, which leaves the
    /// updated seed in `DX:AX`.
    pub fn next_u32(&mut self) -> u32 {
        const MULT: u32 = 0x0808_8405;
        const INC: u32 = 1;
        self.state = self.state.wrapping_mul(MULT).wrapping_add(INC);
        self.state
    }

    /// The original's `Random(Range: Word): Word` (`1f78:114b`): step the
    /// seed, then take the high 32 bits of the widening product with `n`.
    /// This is not a modulo — see `docs/re/rng.md`.
    ///
    /// `below(0)` returns `0`, as the original does.
    pub fn below(&mut self, n: u16) -> u16 {
        let r = self.next_u32() as u64;
        ((r * n as u64) >> 32) as u16
    }
}
