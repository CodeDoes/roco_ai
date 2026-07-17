//! Sampling functions for token generation.
//!
//! Provides temperature-scaled and top-p sampling, grammar-constrained
//! sampling (masking disallowed token logits to `f32::NEG_INFINITY`),
//! and helper functions for grammar integration.

use roco_engine::CompletionRequest;

/// Sample the next token from a probability distribution.
pub fn sample_token(probs: &[f32], temperature: f32, top_p: f32, top_a: f32) -> u32 {
    if temperature == 0.0 {
        return probs.iter().enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
    }
    let mut sorted: Vec<_> = probs.iter().copied().enumerate()
        .filter(|&(_, p)| p.is_finite())
        .collect();
    if sorted.is_empty() { return 0; }
    sorted.sort_unstable_by(|a, b| a.1.total_cmp(&b.1).reverse());

    // Top-A Cutoff: limit = top_a * max_prob^2. Only p >= limit are kept.
    if top_a > 0.0 {
        let max_prob = sorted[0].1;
        let limit = top_a * max_prob * max_prob;
        sorted.retain(|&(_, p)| p >= limit);
        if sorted.is_empty() { return 0; }
    }

    let mut cum = 0.0f32;
    let mut keep = sorted.len();
    for (_, p) in sorted.iter() {
        cum += p;
        if cum >= top_p { break; }
        keep -= 1;
    }
    sorted.truncate(keep);

    let sum: f32 = sorted.iter().map(|(_, p)| p.powf(1.0 / temperature)).sum();
    let weighted: Vec<(usize, f32)> = sorted
        .into_iter()
        .map(|(id, p)| (id, p.powf(1.0 / temperature) / sum))
        .collect();
    let r = fastrand::f32();
    let mut cum = 0.0f32;
    for (id, p) in &weighted {
        cum += p;
        if r <= cum { return *id as u32; }
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
    debug_assert_eq!(probs.len(), allowed.len(), "vocab length mismatch");
    let mut any_allowed = false;
    for (p, &ok) in probs.iter_mut().zip(allowed) {
        if !ok {
            *p = f32::NEG_INFINITY;
        } else {
            any_allowed = true;
        }
    }
    if !any_allowed { return None; }

    let token = sample_token(probs, temperature, top_p, top_a);
    if token != 0 || allowed[0] {
        return Some(token);
    }
    // Token 0 (EOS) not allowed — sample from finite-probability tokens only.
    let candidates: Vec<(usize, f32)> = probs.iter().enumerate()
        .filter(|(_, &p)| p.is_finite())
        .map(|(i, &p)| (i, p.powf(1.0 / temperature)))
        .collect();
    if candidates.is_empty() { return None; }
    let sum: f32 = candidates.iter().map(|(_, w)| w).sum();
    let r = fastrand::f32();
    let mut cum = 0.0f32;
    for (id, w) in &candidates {
        cum += w / sum;
        if r <= cum { return Some(*id as u32); }
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
        if !g.trim().is_empty() { return Some(g.clone()); }
    }
    match std::env::var("RWKV_GRAMMAR") {
        Ok(g) if !g.trim().is_empty() => Some(g),
        _ => None,
    }
}
