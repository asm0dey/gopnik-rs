//! Reimplementation of the original's pseudo-random generator.

/// The original's `System.RandSeed`, stepped by the Borland Pascal LCG.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng { state: seed }
    }

    /// The generator's current seed. There is deliberately no setter: every
    /// generator is started from a seed and then only stepped, and the save
    /// format carries no seed field for one to be restored from.
    pub fn state(&self) -> u32 {
        self.state
    }

    /// One step: `RandSeed := (RandSeed * $08088405 + 1) mod 2^32`. Returns
    /// the NEW state, matching the original, which leaves the updated seed
    /// in `DX:AX`.
    pub fn next_u32(&mut self) -> u32 {
        const MULT: u32 = 0x0808_8405;
        const INC: u32 = 1;
        self.state = self.state.wrapping_mul(MULT).wrapping_add(INC);
        self.state
    }

    /// The original's `Random(Range: Word): Word` -- step the seed, then
    /// take the high 32 bits of the widening product with `n`. Not a
    /// modulo. `below(0)` returns `0`, as the original does.
    pub fn below(&mut self, n: u16) -> u16 {
        let r = self.next_u32() as u64;
        ((r * n as u64) >> 32) as u16
    }
}
