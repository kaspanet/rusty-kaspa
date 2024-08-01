pub mod index {
    use rand::{distributions::Uniform, prelude::Distribution, Rng};
    use std::collections::HashSet;

    /// Adaptation of [`rand::seq::index::sample`] for the case where there exists an a priory filter
    /// of indices which should not be selected.
    ///
    /// Assumes `|filter| << length`.
    ///
    /// The argument `capacity` can be used to ensure a larger allocation within the returned vector.
    pub fn sample<R>(rng: &mut R, length: u32, amount: u32, capacity: u32, filter: HashSet<u32>) -> Vec<u32>
    where
        R: Rng + ?Sized,
    {
        const C: [f32; 2] = [270.0, 330.0 / 9.0];
        let j = if length < 500_000 { 0 } else { 1 };
        if (length as f32) < C[j] * (amount as f32) {
            sample_inplace(rng, length, amount, capacity, filter)
        } else {
            sample_rejection(rng, length, amount, capacity, filter)
        }
    }

    /// Adaptation of [`rand::seq::index::sample_inplace`] for the case where there exists an a priory filter
    /// of indices which should not be selected.
    ///
    /// Assumes `|filter| << length`.
    ///
    /// The argument `capacity` can be used to ensure a larger allocation within the returned vector.
    fn sample_inplace<R>(rng: &mut R, length: u32, amount: u32, capacity: u32, filter: HashSet<u32>) -> Vec<u32>
    where
        R: Rng + ?Sized,
    {
        debug_assert!(amount <= length);
        debug_assert!(filter.len() <= amount as usize);
        let mut indices: Vec<u32> = Vec::with_capacity(length.max(capacity) as usize);
        indices.extend(0..length);
        for i in 0..amount {
            let mut j: u32 = rng.gen_range(i..length);
            // Assumes |filter| << length
            while filter.contains(&j) {
                j = rng.gen_range(i..length);
            }
            indices.swap(i as usize, j as usize);
        }
        indices.truncate(amount as usize);
        debug_assert_eq!(indices.len(), amount as usize);
        indices
    }

    /// Adaptation of [`rand::seq::index::sample_rejection`] for the case where there exists an a priory filter
    /// of indices which should not be selected.
    ///
    /// Assumes `|filter| << length`.
    ///
    /// The argument `capacity` can be used to ensure a larger allocation within the returned vector.
    fn sample_rejection<R>(rng: &mut R, length: u32, amount: u32, capacity: u32, mut filter: HashSet<u32>) -> Vec<u32>
    where
        R: Rng + ?Sized,
    {
        debug_assert!(amount < length);
        debug_assert!(filter.len() <= amount as usize);
        let distr = Uniform::new(0, length);
        let mut indices = Vec::with_capacity(amount.max(capacity) as usize);
        for _ in indices.len()..amount as usize {
            let mut pos = distr.sample(rng);
            while !filter.insert(pos) {
                pos = distr.sample(rng);
            }
            indices.push(pos);
        }

        assert_eq!(indices.len(), amount as usize);
        indices
    }
}
