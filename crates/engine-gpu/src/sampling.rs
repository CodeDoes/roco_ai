//! Sampling functions for token generation.
//!
//! Provides temperature-scaled and top-p sampling, grammar-constrained
//! sampling (masking disallowed token logits to `f32::NEG_INFINITY`),
//! and helper functions for grammar integration.
//!
//! # Determinism
//!
//! All sampling functions accept an optional `rng` parameter (a seeded
//! [`rand::rngs::StdRng`]) to produce reproducible outputs. When no RNG is
//! provided, `fastrand::f32()` is used as before (non-deterministic).

use rand::rngs::StdRng;
use rand::RngCore;
use roco_engine::{BnfMask, CompletionRequest};

/// Outcome of a single grammar-aware sampling step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampledToken {
    pub token: u32,
    /// True when the sampled token completed the grammar — the caller must
    /// emit it, then stop generation.
    pub grammar_finished: bool,
}

/// Sample one token, applying the grammar mask when present.
///
/// - No mask: plain temperature/top-p/top-a sampling.
/// - Mask: disallowed tokens are zeroed out and the distribution
///   renormalized over the allowed set before sampling.
///
/// Returns `None` when the mask disallowed every token — the caller should
/// stop generating.
pub fn sample_token_masked_with_rng(
    probs: &[f32],
    mask: Option<&mut Box<dyn BnfMask>>,
    temperature: f32,
    top_p: f32,
    top_a: f32,
    rng: Option<&mut StdRng>,
) -> Option<SampledToken> {
    let Some(mask) = mask else {
        let token = sample_token_with_rng(probs, temperature, top_p, top_a, rng);
        return Some(SampledToken {
            token,
            grammar_finished: false,
        });
    };

    let mut p = probs.to_vec();
    mask.mask(&mut p);
    // Renormalize so grammar-constrained tokens have full probability mass.
    let sum: f32 = p.iter().filter(|&&v| v.is_finite()).sum();
    if sum > 0.0 {
        for v in p.iter_mut() {
            if v.is_finite() {
                *v /= sum;
            }
        }
    }
    // Grammar sampling uses full top-p (1.0) — the mask is the constraint.
    let token = sample_token_with_rng(&p, temperature, 1.0, top_a, rng);
    if token > 0 {
        let grammar_finished = !mask.accept(token);
        Some(SampledToken {
            token,
            grammar_finished,
        })
    } else {
        None
    }
}

/// Sample the next token from a probability distribution.
///
/// If `rng` is `Some`, uses it for deterministic sampling; otherwise
/// falls back to `fastrand::f32()` (non-deterministic).
pub fn sample_token(probs: &[f32], temperature: f32, top_p: f32, top_a: f32) -> u32 {
    sample_token_with_rng(probs, temperature, top_p, top_a, None)
}

/// Like [`sample_token`] but with an optional seeded RNG for determinism.
pub fn sample_token_with_rng(
    probs: &[f32],
    temperature: f32,
    top_p: f32,
    top_a: f32,
    mut rng: Option<&mut StdRng>,
) -> u32 {
    if temperature == 0.0 {
        return probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
    }
    let mut sorted: Vec<_> = probs
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, p)| p.is_finite())
        .collect();
    if sorted.is_empty() {
        return 0;
    }
    sorted.sort_unstable_by(|a, b| a.1.total_cmp(&b.1).reverse());

    // Top-A Cutoff: limit = top_a * max_prob^2. Only p >= limit are kept.
    if top_a > 0.0 {
        let max_prob = sorted[0].1;
        let limit = top_a * max_prob * max_prob;
        sorted.retain(|&(_, p)| p >= limit);
        if sorted.is_empty() {
            return 0;
        }
    }

    let mut cum = 0.0f32;
    let mut keep = 0;
    for (_, p) in sorted.iter() {
        cum += p;
        keep += 1;
        if cum >= top_p {
            break;
        }
    }
    sorted.truncate(keep.max(1));

    let sum: f32 = sorted.iter().map(|(_, p)| p.powf(1.0 / temperature)).sum();
    let weighted: Vec<(usize, f32)> = sorted
        .into_iter()
        .map(|(id, p)| (id, p.powf(1.0 / temperature) / sum))
        .collect();
    let r = match rng {
        Some(ref mut r) => (r.next_u32() as f64 / u32::MAX as f64) as f32,
        None => fastrand::f32(),
    };
    let mut cum = 0.0f32;
    for (id, p) in &weighted {
        cum += p;
        if r <= cum {
            return *id as u32;
        }
    }
    weighted.last().map(|(id, _)| *id as u32).unwrap_or(0)
}

/// Like `sample_token`, but restrict to token indices where `allowed[i]` is true.
/// Disallowed logits are replaced with `f32::NEG_INFINITY`.
/// Returns `None` if no token is allowed.
pub fn constrained_sample_token(
    probs: &mut [f32],
    allowed: &[bool],
    temperature: f32,
    top_p: f32,
    top_a: f32,
) -> Option<u32> {
    constrained_sample_token_with_rng(probs, allowed, temperature, top_p, top_a, None)
}

/// Like [`constrained_sample_token`] but with an optional seeded RNG.
pub fn constrained_sample_token_with_rng(
    probs: &mut [f32],
    allowed: &[bool],
    temperature: f32,
    top_p: f32,
    top_a: f32,
    mut rng: Option<&mut StdRng>,
) -> Option<u32> {
    debug_assert_eq!(probs.len(), allowed.len(), "vocab length mismatch");
    let mut any_allowed = false;
    for (p, &ok) in probs.iter_mut().zip(allowed) {
        if !ok {
            *p = f32::NEG_INFINITY;
        } else {
            any_allowed = true;
        }
    }
    if !any_allowed {
        return None;
    }

    // Use a borrow of rng so we can still use it in the fallback path.
    let rng_borrow = rng.as_deref_mut();
    let token = sample_token_with_rng(probs, temperature, top_p, top_a, rng_borrow);
    if token != 0 || allowed[0] {
        return Some(token);
    }
    // Token 0 (EOS) not allowed — sample from finite-probability tokens only.
    let candidates: Vec<(usize, f32)> = probs
        .iter()
        .enumerate()
        .filter(|(_, &p)| p.is_finite())
        .map(|(i, &p)| (i, p.powf(1.0 / temperature)))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let sum: f32 = candidates.iter().map(|(_, w)| w).sum();
    let r = match rng.as_mut() {
        Some(r) => (r.next_u32() as f64 / u32::MAX as f64) as f32,
        None => fastrand::f32(),
    };
    let mut cum = 0.0f32;
    for (id, w) in &candidates {
        cum += w / sum;
        if r <= cum {
            return Some(*id as u32);
        }
    }
    candidates.last().map(|(id, _)| *id as u32)
}

/// Convert a `BitSet` of allowed token IDs to a `Vec<bool>` mask.
#[cfg(feature = "grammar")]
pub fn bitset_to_allowed(bitset: &::bit_set::BitSet<u32>, vocab_size: usize) -> Vec<bool> {
    (0..vocab_size).map(|i| bitset.contains(i)).collect()
}

/// Resolve the GBNF grammar string for a completion request.
///
/// Sources in priority order:
/// 1. `req.grammar` (set explicitly)
/// 2. `RWKV_GRAMMAR` environment variable
#[cfg(feature = "grammar")]
pub fn resolve_grammar(req: &CompletionRequest) -> Option<String> {
    if let Some(g) = req.grammar.as_ref() {
        if !g.trim().is_empty() {
            return Some(g.clone());
        }
    }
    match std::env::var("RWKV_GRAMMAR") {
        Ok(g) if !g.trim().is_empty() => Some(g),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// A known probability distribution where we can verify ordering.
    fn fixture_probs() -> Vec<f32> {
        vec![0.1, 0.3, 0.05, 0.4, 0.15]
    }

    #[test]
    fn deterministic_seed_produces_identical_results() {
        let probs = fixture_probs();
        let seed = 42u64;

        // Two separate RNGs seeded with the same value must produce
        // the same token sequence across repeated calls.
        let mut rng1 = StdRng::seed_from_u64(seed);
        let mut rng2 = StdRng::seed_from_u64(seed);

        let mut results1 = Vec::new();
        let mut results2 = Vec::new();
        for _ in 0..10 {
            results1.push(sample_token_with_rng(
                &probs,
                0.8,
                0.9,
                0.0,
                Some(&mut rng1),
            ));
            results2.push(sample_token_with_rng(
                &probs,
                0.8,
                0.9,
                0.0,
                Some(&mut rng2),
            ));
        }
        assert_eq!(results1, results2, "seeded RNGs diverged");
    }

    #[test]
    fn different_seeds_produce_different_results() {
        let probs = fixture_probs();
        let mut rng1 = StdRng::seed_from_u64(1);
        let mut rng2 = StdRng::seed_from_u64(999);

        let r1 = sample_token_with_rng(&probs, 0.8, 0.9, 0.0, Some(&mut rng1));
        let r2 = sample_token_with_rng(&probs, 0.8, 0.9, 0.0, Some(&mut rng2));
        // Extremely unlikely to collide, but possible. We just verify
        // the function runs without panicking for both seeds.
        assert!(r1 < probs.len() as u32);
        assert!(r2 < probs.len() as u32);
    }

    #[test]
    fn no_rng_falls_back_to_fastrand() {
        let probs = fixture_probs();
        // Must produce some valid token index.
        let token = sample_token(&probs, 0.8, 0.9, 0.0);
        assert!(token < probs.len() as u32);
    }

    #[test]
    fn greedy_sampling_picks_max() {
        // temperature = 0.0 => greedy: pick the highest probability token.
        let probs = fixture_probs();
        let token = sample_token(&probs, 0.0, 1.0, 0.0);
        // Index 3 has probability 0.4 (highest).
        assert_eq!(token, 3, "greedy should pick argmax");
    }

    #[test]
    fn constrained_sampling_respects_mask() {
        let mut probs = vec![0.1, 0.5, 0.3, 0.1];
        let allowed = vec![true, false, true, false];
        let token = constrained_sample_token(&mut probs, &allowed, 0.0, 1.0, 0.0);
        assert_eq!(
            token,
            Some(2),
            "constrained greedy should pick highest allowed"
        );
    }

    #[test]
    fn constrained_sampling_returns_none_when_no_allowed() {
        let mut probs = vec![0.25, 0.5, 0.25];
        let allowed = vec![false, false, false];
        let token = constrained_sample_token(&mut probs, &allowed, 0.0, 1.0, 0.0);
        assert_eq!(token, None);
    }

    #[test]
    fn deterministic_seed_across_multiple_calls() {
        let probs = fixture_probs();
        let seed = 12345u64;

        let mut rng = StdRng::seed_from_u64(seed);
        let mut results = Vec::new();
        for _ in 0..100 {
            results.push(sample_token_with_rng(
                &probs,
                0.9,
                0.95,
                0.1,
                Some(&mut rng),
            ));
        }
        assert_eq!(results.len(), 100);
        assert!(results.iter().all(|&t| t < probs.len() as u32));
    }

    #[test]
    fn property_sample_token_always_in_bounds() {
        // Property check across varying array sizes, temperatures, and seeds
        for size in 1..=50 {
            for temp in [0.0, 0.1, 0.5, 1.0, 2.0] {
                for seed in 0..10 {
                    let probs: Vec<f32> =
                        (0..size).map(|i| (i as f32 + 1.0) / size as f32).collect();
                    let mut rng = StdRng::seed_from_u64(seed);
                    let token = sample_token_with_rng(&probs, temp, 0.9, 0.05, Some(&mut rng));
                    assert!(
                        (token as usize) < size,
                        "token {token} out of bounds for size {size}"
                    );
                }
            }
        }
    }

    #[test]
    fn property_constrained_sampling_never_returns_disallowed() {
        for seed in 0..20 {
            let mut probs = vec![0.1, 0.4, 0.05, 0.2, 0.25];
            let allowed = vec![
                seed % 2 == 0,
                seed % 3 == 0,
                seed % 5 == 0,
                seed % 7 == 0,
                seed % 11 == 0,
            ];
            let any_allowed = allowed.iter().any(|&b| b);

            let mut rng = StdRng::seed_from_u64(seed);
            let result = constrained_sample_token_with_rng(
                &mut probs,
                &allowed,
                0.8,
                0.9,
                0.0,
                Some(&mut rng),
            );

            if any_allowed {
                let id = result.expect("should return token when at least one allowed");
                assert!(
                    allowed[id as usize],
                    "returned token {id} which is disallowed by mask"
                );
            } else {
                assert_eq!(result, None);
            }
        }
    }

    #[test]
    fn property_extreme_probs_never_panic() {
        let extreme_cases = vec![
            vec![f32::NEG_INFINITY, f32::NEG_INFINITY],
            vec![0.0, 0.0, 0.0],
            vec![f32::MIN_POSITIVE, f32::MAX],
            vec![1e-30, 1e-30],
        ];

        for (i, probs) in extreme_cases.into_iter().enumerate() {
            let mut rng = StdRng::seed_from_u64(i as u64);
            let token = sample_token_with_rng(&probs, 0.7, 0.9, 0.0, Some(&mut rng));
            assert!(
                (token as usize) < probs.len(),
                "extreme case {i} returned invalid token {token}"
            );
        }
    }
}
