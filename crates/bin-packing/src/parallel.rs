//! Runtime parallelism helpers.
//!
//! When the `parallel` feature is enabled **and** the host has more than one
//! available core, iteration helpers in this module delegate to
//! [`rayon`]'s work-stealing thread pool. On single-core hosts (or when the
//! feature is disabled) the same helpers fall back to sequential iteration
//! so callers never need `#[cfg]` at the call site.

/// Returns `true` when parallel execution is both compiled-in and useful
/// on the current host (i.e. more than one hardware thread is available).
pub(crate) fn use_parallel() -> bool {
    cfg!(feature = "parallel") && core_count() > 1
}

/// Number of hardware threads available to the process (cached after first call).
fn core_count() -> usize {
    static CORES: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CORES.get_or_init(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
}

/// Derive a deterministic per-iteration seed from a base seed and index.
///
/// Used by multistart / GRASP loops so each parallel iteration gets its own
/// independent [`SmallRng`](rand::rngs::SmallRng) without sharing mutable
/// state across threads.
pub(crate) fn iteration_seed(base: u64, index: usize) -> u64 {
    base.wrapping_add(index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Map indices `0..count` through a closure, collecting results in parallel
/// when available. Each closure receives its index and must return `T`.
///
/// When [`use_parallel`] is `true`, iterations execute on rayon's thread
/// pool. Otherwise they run sequentially.
pub(crate) fn par_map_indexed<T, F>(count: usize, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(usize) -> T + Send + Sync,
{
    #[cfg(feature = "parallel")]
    if use_parallel() {
        use rayon::prelude::*;
        return (0..count).into_par_iter().map(&f).collect();
    }

    (0..count).map(f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_parallel_returns_consistent_value() {
        // Just verify it doesn't panic and returns a bool.
        let _ = use_parallel();
    }

    #[test]
    fn par_map_indexed_empty_range_returns_empty() {
        let results: Vec<usize> = par_map_indexed(0, |i| i);
        assert!(results.is_empty());
    }

    #[test]
    fn par_map_indexed_collects_in_order() {
        let results = par_map_indexed(10, |i| i * 2);
        assert_eq!(results, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
    }

    #[test]
    fn par_map_indexed_preserves_closured_state() {
        let offset = 100;
        let results = par_map_indexed(5, |i| i + offset);
        assert_eq!(results, vec![100, 101, 102, 103, 104]);
    }

    #[test]
    fn iteration_seed_produces_distinct_values_per_index() {
        let base = 42;
        let seeds: Vec<u64> = (0..10).map(|i| iteration_seed(base, i)).collect();
        // All seeds should be unique.
        let mut deduped = seeds.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(seeds.len(), deduped.len());
    }

    #[test]
    fn iteration_seed_is_deterministic() {
        assert_eq!(iteration_seed(0, 0), iteration_seed(0, 0));
        assert_eq!(iteration_seed(99, 5), iteration_seed(99, 5));
    }
}
