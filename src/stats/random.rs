//! Seeded random sampling so every report is reproducible.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub type Rng = ChaCha8Rng;

#[must_use]
pub fn seeded(seed: u64) -> Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// Poisson draw: Knuth's product method for small means, else a normal approximation with
/// continuity rounding (adequate for the power simulations' large-count regime).
pub fn poisson(rng: &mut Rng, mean: f64) -> u64 {
    if mean <= 0.0 {
        return 0;
    }
    if mean < 30.0 {
        let limit = (-mean).exp();
        let mut product: f64 = rng.random();
        let mut count = 0;
        loop {
            if product <= limit {
                return count;
            }
            product *= rng.random::<f64>();
            count += 1;
        }
    }
    let draw = standard_normal(rng).mul_add(mean.sqrt(), mean).round();
    // Clamped at zero and far below u64::MAX for any plausible hockey count.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = draw.max(0.0) as u64;
    count
}

/// Box–Muller standard normal draw.
pub fn standard_normal(rng: &mut Rng) -> f64 {
    let u1: f64 = rng.random::<f64>().max(f64::MIN_POSITIVE);
    let u2: f64 = rng.random();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Uniform draw in [0, 1).
pub fn uniform(rng: &mut Rng) -> f64 {
    rng.random()
}

pub fn index(rng: &mut Rng, upper: usize) -> usize {
    rng.random_range(0..upper)
}

/// In-place Fisher–Yates shuffle.
pub fn shuffle<T>(rng: &mut Rng, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = rng.random_range(0..=i);
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poisson_draws_have_the_requested_mean() {
        let mut rng = seeded(7);
        for mean in [0.5, 4.0, 45.0] {
            let n = 20_000;
            let total: u64 = (0..n).map(|_| poisson(&mut rng, mean)).sum();
            let observed = total as f64 / f64::from(n);
            assert!((observed - mean).abs() < 0.05 * mean.max(1.0), "{mean}: {observed}");
        }
    }

    #[test]
    fn same_seed_same_sequence() {
        let a: Vec<u64> = (0..5).map({ let mut r = seeded(1); move |_| poisson(&mut r, 3.0) }).collect();
        let b: Vec<u64> = (0..5).map({ let mut r = seeded(1); move |_| poisson(&mut r, 3.0) }).collect();
        assert_eq!(a, b);
    }
}
