//! Deterministic RNG for consensus replay. Pcg64 (rand_pcg) drives a value-stable
//! u64 stream. Float conversions live here and route transcendentals through libm,
//! so every target agrees bit-for-bit.

use core::f64::consts::PI;
use rand_core::{Rng as _, SeedableRng};
use rand_pcg::Pcg64;

pub struct Rng {
    core: Pcg64,
}

impl Rng {
    /// Seed a generator. The same seed reproduces on every target.
    pub fn new(seed: u64) -> Self {
        Rng {
            core: Pcg64::seed_from_u64(seed),
        }
    }

    /// f64 in [0, 1). Top 53 bits divided by 2^53, which is exact in IEEE and needs
    /// no libm.
    pub fn unit(&mut self) -> f64 {
        (self.core.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// f64 in [min, max).
    pub fn range(&mut self, min: f64, max: f64) -> f64 {
        min + self.unit() * (max - min)
    }

    /// A uniform point on the unit sphere (Archimedes' z-method). sqrt/cos/sin go
    /// through libm to stay bit-identical across targets.
    pub fn unit_vector(&mut self) -> [f64; 3] {
        let z = 2.0 * self.unit() - 1.0;
        let phi = 2.0 * PI * self.unit();
        let r = libm::sqrt(1.0 - z * z);
        [r * libm::cos(phi), r * libm::sin(phi), z]
    }

    /// A fresh u32 seed, e.g. to hand the fBm noise its own stream.
    pub fn next_seed(&mut self) -> u32 {
        self.core.next_u32()
    }
}
