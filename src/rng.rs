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

/// One recorded `Random` call: which call site made it, what `n` it pushed
/// and what it returned.
///
/// The shape is `data/rng_trace.json`'s `runs[].draws` minus the bookkeeping
/// fields (`i`, `turn`), so a captured run and a replayed one can be compared
/// element by element. `site` is a Ghidra `1000:xxxx` address, or
/// [`UNATTRIBUTED`] where the port's draw has no single address in the
/// original because the routine around it has not been recovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draw {
    pub site: &'static str,
    pub n: u16,
    pub r: u16,
}

/// The `site` of a draw this port makes without a recovered call site behind
/// it. Deliberately not a plausible-looking address: naming one would be a
/// claim about `orig/g.exe` that nothing supports.
pub const UNATTRIBUTED: &str = "<unattributed>";

/// The original's `System.RandSeed`, stepped by the Borland Pascal LCG.
///
/// `log` is the differential test's recording hook (`tests/wander_sequence.rs`
/// replays five captured runs of the original against it). It is `None` for
/// every generator the game itself builds, so a real session allocates
/// nothing and pays one `Option` branch per draw; only a caller that asks for
/// it with [`Rng::start_log`] gets a recording.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
    log: Option<Vec<Draw>>,
}

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng {
            state: seed,
            log: None,
        }
    }

    /// Begin recording every draw, discarding anything already recorded.
    pub fn start_log(&mut self) {
        self.log = Some(Vec::new());
    }

    /// Take the recorded draws and stop recording. Empty when
    /// [`Rng::start_log`] was never called.
    pub fn take_log(&mut self) -> Vec<Draw> {
        self.log.take().unwrap_or_default()
    }

    /// The current `RandSeed` value, e.g. to snapshot a generator for later
    /// restoration (`state` / `set_state` round-trip).
    pub fn state(&self) -> u32 {
        self.state
    }

    /// Restore a previously read `state()`, e.g. to reset a generator to a
    /// known point without allocating a new one.
    pub fn set_state(&mut self, state: u32) {
        self.state = state;
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
    ///
    /// Equivalent to [`Rng::below_at`] with [`UNATTRIBUTED`]: use that
    /// instead wherever the original's call site is known, so the
    /// differential replay can tell a missing draw from a reordered one.
    pub fn below(&mut self, n: u16) -> u16 {
        self.below_at(UNATTRIBUTED, n)
    }

    /// [`Rng::below`], recording `site` as the Ghidra address of the
    /// `9a 4b 11 78 0f` (`call 0f78:114b`) this call reproduces.
    pub fn below_at(&mut self, site: &'static str, n: u16) -> u16 {
        let r = self.next_u32() as u64;
        let r = ((r * n as u64) >> 32) as u16;
        if let Some(log) = self.log.as_mut() {
            log.push(Draw { site, n, r });
        }
        r
    }
}
