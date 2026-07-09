//! Inference-time token generation, sampling strategies, and batching.
//!
//! Model-agnostic: this module owns the *decoding* logic — greedy / temperature
//! / top-k / top-p (nucleus) sampling and the autoregressive generation loop —
//! behind a [`GenerativeModel`] trait. A real backend (e.g. a local RWKV model
//! in `rwkv.rs`) only has to implement `next_logits`; everything here works
//! the same. The randomness source is injected (`rand01`) so sampling is fully
//! deterministic and testable without a real RNG or a downloaded model.

/// A vocabulary token identifier.
pub type TokenId = u32;

/// Decoding hyperparameters (§2.2F-style sampling discipline).
#[derive(Debug, Clone)]
pub struct SamplingParams {
    /// 0.0 forces greedy (argmax). Typical small values (0.1–0.2) for
    /// deterministic-ish tasks; higher for creative tasks.
    pub temperature: f32,
    /// Keep only the top-`top_k` logits (0 = disabled).
    pub top_k: usize,
    /// Nucleus cutoff: keep the smallest set of tokens whose cumulative
    /// probability >= `top_p` (1.0 = disabled).
    pub top_p: f32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            top_k: 0,
            top_p: 1.0,
        }
    }
}

/// Greedy decode: the highest-logit token.
pub fn argmax(logits: &[f32]) -> TokenId {
    logits
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i as TokenId)
        .unwrap_or(0)
}

/// Sample a token from `logits` under `params`.
///
/// `rand01` must return a uniform float in `[0, 1)`; it is only consulted when
/// sampling is stochastic (temperature > 0).
pub fn sample(logits: &[f32], params: &SamplingParams, mut rand01: impl FnMut() -> f32) -> TokenId {
    if logits.is_empty() {
        return 0;
    }
    if params.temperature == 0.0 {
        return argmax(logits);
    }

    // 1. Temperature scaling.
    let mut scaled: Vec<f32> = logits
        .iter()
        .map(|l| l / params.temperature.max(1e-6))
        .collect();

    // 2. Top-k: zero out everything outside the top-k.
    if params.top_k > 0 && params.top_k < scaled.len() {
        let mut idx: Vec<usize> = (0..scaled.len()).collect();
        idx.sort_by(|a, b| scaled[*b].partial_cmp(&scaled[*a]).unwrap());
        for &i in &idx[params.top_k..] {
            scaled[i] = f32::NEG_INFINITY;
        }
    }

    // 3. Softmax.
    let max = scaled
        .iter()
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = scaled.iter().map(|s| (s - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    let mut probs: Vec<f32> = exps.iter().map(|e| e / sum).collect();

    // 4. Top-p (nucleus): keep the smallest prefix whose mass >= top_p.
    if params.top_p < 1.0 {
        let mut order: Vec<usize> = (0..probs.len()).collect();
        order.sort_by(|a, b| probs[*b].partial_cmp(&probs[*a]).unwrap());
        let mut cum = 0.0f32;
        let mut cut = order.len();
        for (rank, &i) in order.iter().enumerate() {
            cum += probs[i];
            if cum >= params.top_p {
                cut = rank + 1;
                break;
            }
        }
        for &i in &order[cut..] {
            probs[i] = 0.0;
        }
        let s2: f32 = probs.iter().sum();
        if s2 > 0.0 {
            for p in &mut probs {
                *p /= s2;
            }
        }
    }

    // 5. Inverse-CDF sampling.
    let r = rand01();
    let mut cum = 0.0f32;
    for (i, p) in probs.iter().enumerate() {
        cum += p;
        if r < cum {
            return i as TokenId;
        }
    }
    (probs.len() - 1) as TokenId
}

/// A model that, given a token context, returns next-token logits.
///
/// This is the seam a real backend implements. `rwkv.rs` will provide a
/// stateful RWKV implementation; for now `tests` use a constant-logits mock.
pub trait GenerativeModel {
    fn vocab_size(&self) -> usize;
    fn next_logits(&self, context: &[TokenId]) -> Vec<f32>;
}

/// Autoregressively generate up to `max_tokens` tokens, stopping early at
/// `stop` (e.g. an EOS id). Returns only the *generated* tokens.
pub fn generate(
    model: &dyn GenerativeModel,
    prompt: &[TokenId],
    params: &SamplingParams,
    max_tokens: usize,
    stop: TokenId,
    mut rand01: impl FnMut() -> f32,
) -> Vec<TokenId> {
    let mut ctx = prompt.to_vec();
    for _ in 0..max_tokens {
        let logits = model.next_logits(&ctx);
        let t = sample(&logits, params, &mut rand01);
        if t == stop {
            break;
        }
        ctx.push(t);
    }
    ctx[prompt.len()..].to_vec()
}

/// Generate for multiple prompts (sequential fan-out).
pub fn batch_generate(
    model: &dyn GenerativeModel,
    prompts: &[Vec<TokenId>],
    params: &SamplingParams,
    max_tokens: usize,
    stop: TokenId,
    mut rand01: impl FnMut() -> f32,
) -> Vec<Vec<TokenId>> {
    prompts
        .iter()
        .map(|p| generate(model, p, params, max_tokens, stop, &mut rand01))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argmax_picks_highest_logit() {
        assert_eq!(argmax(&[1.0, 5.0, 3.0]), 1);
    }

    #[test]
    fn greedy_is_deterministic_and_ignores_rng() {
        let params = SamplingParams {
            temperature: 0.0,
            ..Default::default()
        };
        // rand01 returning a crazy value must be ignored under greedy.
        let t = sample(&[1.0, 9.0, 3.0], &params, || 0.999);
        assert_eq!(t, 1);
    }

    #[test]
    fn top_k_one_collapses_to_argmax() {
        let params = SamplingParams {
            temperature: 1.0,
            top_k: 1,
            ..Default::default()
        };
        // only the top logit survives, so sampling must pick it.
        let t = sample(&[1.0, 9.0, 3.0], &params, || 0.5);
        assert_eq!(t, 1);
    }

    #[test]
    fn top_p_small_with_r0_picks_argmax() {
        let params = SamplingParams {
            temperature: 1.0,
            top_p: 0.0001,
            ..Default::default()
        };
        // nucleus keeps only the single highest token; r=0 selects it.
        let t = sample(&[1.0, 9.0, 3.0], &params, || 0.0);
        assert_eq!(t, 1);
    }

    #[test]
    fn sample_returns_a_valid_token_id() {
        let params = SamplingParams {
            temperature: 0.8,
            top_k: 0,
            top_p: 1.0,
        };
        let t = sample(&[1.0, 2.0, 3.0, 4.0], &params, || 0.42);
        assert!((t as usize) < 4);
    }

    /// Constant-logits mock: always peaks at `peak`.
    struct ConstLogitsModel {
        vocab: usize,
        peak: TokenId,
    }
    impl GenerativeModel for ConstLogitsModel {
        fn vocab_size(&self) -> usize {
            self.vocab
        }
        fn next_logits(&self, _ctx: &[TokenId]) -> Vec<f32> {
            let mut v = vec![-1e9; self.vocab];
            v[self.peak as usize] = 1e9;
            v
        }
    }

    #[test]
    fn generate_loop_emits_repeated_peak_tokens() {
        let model = ConstLogitsModel {
            vocab: 10,
            peak: 2,
        };
        let out = generate(
            &model,
            &[],
            &SamplingParams::default(),
            3,
            999,
            || 0.5,
        );
        assert_eq!(out, vec![2, 2, 2]);
    }

    #[test]
    fn generate_stops_at_stop_token() {
        let model = ConstLogitsModel {
            vocab: 10,
            peak: 2,
        };
        // stop == peak => generation halts immediately (EOS not emitted).
        let out = generate(
            &model,
            &[],
            &SamplingParams::default(),
            5,
            2,
            || 0.5,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn batch_generate_runs_multiple_prompts() {
        let model = ConstLogitsModel {
            vocab: 10,
            peak: 4,
        };
        let prompts = vec![vec![0u32], vec![1u32], vec![2u32]];
        let outs = batch_generate(
            &model,
            &prompts,
            &SamplingParams::default(),
            2,
            999,
            || 0.5,
        );
        assert_eq!(outs, vec![vec![4, 4], vec![4, 4], vec![4, 4]]);
    }
}
