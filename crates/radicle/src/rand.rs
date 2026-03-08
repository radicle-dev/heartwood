//! Random number generation.
//!
//! This module integrates with the `rand` crate.

use rand::rngs::ChaCha20Rng;

/// See [`rand::Rng`] and [`std::random::RandomSource`].
// NOTE: We introduce our own trait (and a blanket implementation, see below)
// so that we control the type, and do not leak [`rand::Rng`].
// If `rand` reaches a stable release, or the standard library stabilizes
// `random`, we might remove this trait and drop in either of the two.
pub trait Rng {
    fn fill_bytes(&mut self, bytes: &mut [u8]);
}

impl<T: rand::Rng> Rng for T {
    fn fill_bytes(&mut self, bytes: &mut [u8]) {
        self.fill_bytes(bytes);
    }
}

/// Analogous to [`rand::make_rng`], constructing this struct via
/// [`Default::default`] allows to conveniently obtain an instance of seeded
/// random number generator in the context of Radicle.
///
/// If the environment variable with name [`crate::profile::env::RAD_RNG_SEED`]
/// is specified, its value is used to seed a CSPRNG. Note that in this mode,
/// the returned PRNG will only be cryptograpically secure if the provided seed
/// is.
// NOTE: We introduce our own struct, so that we control the type and do not
// leak [`rand::rngs::ChaCha20Rng`].
pub struct DefaultRng(ChaCha20Rng);

impl rand::TryRng for DefaultRng {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        self.0.try_next_u32()
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        self.0.try_next_u64()
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        self.0.try_fill_bytes(dst)
    }
}

impl Default for DefaultRng {
    fn default() -> Self {
        use rand::SeedableRng as _;

        Self(match crate::profile::env::rng_seed() {
            Some(seed) => ChaCha20Rng::seed_from_u64(seed),
            #[cfg(not(test))]
            None => ChaCha20Rng::try_from_rng(&mut rand::rngs::SysRng)
                .expect("system RNG must be available"),
            #[cfg(test)]
            None => ChaCha20Rng::seed_from_u64(0),
        })
    }
}
