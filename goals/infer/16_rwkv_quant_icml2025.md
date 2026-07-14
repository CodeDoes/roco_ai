# RWKVQuant Integration (ICML 2025)

Intent: Replace the current blanket NF4/Int8 quantization with RWKVQuant's
**proxy-guided hybrid of Scalar Quantization (SQ) and Vector Quantization (VQ)**,
which is the first comprehensive PTQ framework specifically designed for RWKV.

## Reference paper

**RWKVQuant: Quantizing the RWKV Family with Proxy Guided Hybrid of Scalar
and Vector Quantization** (ICML 2025, Houmo AI).

arXiv: 2505.03803v1 — source at `~/Downloads/arXiv-2505.03803v1/`.

## Why our current quantization is suboptimal

Today we pick **one** quantization for the entire model:
- FP16 file < 1.5 GB → no quant
- FP16 file ≥ 1.5 GB + coop matrix → NF4 (all layers)
- FP16 file ≥ 1.5 GB, no coop → Int8 (all layers)

This blanket approach is inefficient because:
1. **Different weight tensors have different distributions** — some are
   uniform (well-suited to scalar quant like NF4), others have outliers
   (need outlier-aware handling or VQ)
2. **NF4 adds ~99% FLOP overhead** on RWKV because non-linear operators
   (token-shift, sigmoid, exp) block the rotation/smoothing fusion that
   makes NF4 efficient on Transformers
3. **RWKV weight outliers break uniform quant** — RWKV weight matrices
   have ranges like [-27, +27] vs LLaMA's [-2.5, +2.5]. 99.9% of weights
   are in [-1.5, 1.5]; the extremes are rare outliers that destroy
   quantization accuracy. (See [rwkv.cpp#12].)
4. **Uniform weights hurt VQ** — RWKV has more uniformly distributed
   weights than Transformers, making KMeans clustering 2-3× worse
   (cluster loss: RWKV-6-7B = 2.01 vs LLaMA-2-7B = 0.96 at 8 clusters)

[rwkv.cpp#12]: https://github.com/RWKV/rwkv.cpp/issues/12

## RWKVQuant's approach

### Hybrid SQ + VQ per weight tensor

For each weight tensor, decide: use SQ (scalar, like GPTQ) or VQ
(vector/codebook, like GPTVQ)? The optimal assignment is NP-hard
(O(2^M)), so they build a **coarse-to-fine proxy** that runs in O(M):

**Coarse proxy (P_c): Information Entropy of weight intervals**
1. Flatten + sort the weight tensor → W'
2. Compute adjacent intervals: G[i] = W'[i+1] - W'[i]
3. Normalize: G'[i] = G[i] / ΣG → treated as probability distribution
4. Compute IE: H(G') = -Σ G'[i] · log(G'[i])
5. P_c = H(uniform) - H(G') — gap from maximum entropy
   - High P_c → non-uniform → use VQ
   - Low P_c → uniform → check fine proxy

**Fine proxy (P_f): Taylor expansion to detect outliers**
- Taylor-expand P_c around the uniform distribution
- Yields a weighted sum of **higher-order central moments** (k=2..K):
  - k=2: variance (spread)
  - k=3: skewness (asymmetry)
  - k=4: kurtosis (long tail)
- P_f = Σ v_k · |M_k| where v_k = n^k / (k(k-1))
- High P_f → outliers present → use VQ
- Low P_f → clean uniform → use SQ

**Decision rule:**
```
if P_c < τ_c and P_f < τ_f:
    use SQ (GPTQ-style compensation)
else:
    use VQ (cluster-based codebook)
```

### Codebook optimization for element-wise multiplication

RWKV's projection layers use `x ⊙ μ` (element-wise multiply), not
matrix multiply. Standard VQ minimizes ||μ - Deq(Q(μ))||², but the
real loss is:

```
L = Σ X[i,j]² · (Δμ'[i,j])²
```

So RWKVQuant weights the KMeans by **X²** (calibration activations
squared) — positions with larger activations get tighter clusters.
Uses percentile-based clipping before averaging to handle outliers.

## Results from the paper

| Model | Bits | Accuracy loss | Memory saving | Speedup |
|---|---|---|---|---|
| RWKV-6-14B | ~3-bit | < 1% (LAMBADA) | 2.83× | 2.14× |
| RWKV-6-7B | ~3-bit | < 1% (LAMBADA) | 2.7× | 2.0× |
| RWKV-7-2.9B | ~3-bit | < 1% (various) | 2.5× | 1.8× |

RWKVQuant outperforms both pure SQ and pure VQ across all model sizes.

## Implementation plan

### Phase 1: Proxy analysis
- Implement P_c (IE-based uniformity) and P_f (moment-based outlier
  detection) in `crates/inference/src/quant.rs`
- Run on the 2.9B model to profile which weights want SQ vs VQ
- Log the distribution: what % of tensors prefer each method?

### Phase 2: Hybrid quantizer
- Add GPTQ-style compensation SQ for the uniform tensors
- Add weighted KMeans VQ (activation-weighted codebooks) for the
  outlier tensors
- Replace the current `auto_quant()` policy with proxy-guided selection

### Phase 3: Runtime integration
- Optimize the element-wise multiply path for VQ-decoded weights
- Benchmark tok/s vs current NF4 on the 2.9B
- Target: ~3-bit effective, <1% accuracy loss, ≥2× speedup

## Dependencies

| Dep | Goal | Status |
|---|---|---|
| `infer/quantize_model` | Current NF4/Int8 quantization | ✅ Done |
| `infer/inference` | RWKV inference engine | ✅ Done |

## Constraints

- The paper's code is promised at `https://anonymous.4open.science/r/RWKVQuant-5B27/`
  — check if it's been de-anonymized
- web-rwkv's current `Quant::NF4` / `Quant::Int8` types need extension
  to support VQ codebooks
- The calibration dataset for activation-weighted VQ needs a small
  representative corpus (the paper uses 128 samples from C4)
- The proxy thresholds (τ_c, τ_f) may need tuning per model size

## User notes

- The paper specifically covers RWKV-6 and RWKV-7 families — directly
  applicable to our 2.9B g1h model
- The element-wise multiplication optimization is unique to RWKV's
  architecture (the `μ` weights in time mixing / channel mixing)
- This is a **forward-looking goal** — our current NF4 works and
  generates ~16-20 tok/s. RWKVQuant could push that to ~35-40 tok/s
  at similar or better accuracy.
- **rwkv.cpp#12 context**: The original rwkv.cpp author (saharNooby)
  found Q4_0/INT4 breaks RWKV catastrophically (3B: loss 4.69 vs 2.07 FP16).
  Q4_1_O (outlier-aware: stores per-block outlier as-is) helps (loss 2.41)
  but is 2× slower than FP16. BlinkDL suggested row/col rescaling ("mx my rx ry")
  similar to SmoothQuant. The key insight: 99.9% of RWKV weights are in [-1.5, 1.5];
  the extremes are rare outliers that destroy naive quantization.
- Our current web-rwkv NF4 path works because NF4 uses per-channel
  quantization with floating-point scales, which partially handles outliers.
  But it's still a blanket approach — the proxy can tell us which layers
  would benefit from skipping quant entirely.
