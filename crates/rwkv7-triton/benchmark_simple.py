#!/usr/bin/env python3
"""
RWKV-7 WKV Kernel Benchmark (PyTorch reference)

Compares:
1. Naive PyTorch (baseline)
2. Fused operations (simulating UberKernel)
3. Albatross CUDA (reference numbers)
"""

import time
import torch
import torch.nn.functional as F

# Config
B, T, C, H, N = 1, 128, 4096, 64, 64
HEAD_SIZE = N

def wkv_naive(r, w, k, v, a, b, state):
    """Naive PyTorch WKV - one loop per token."""
    y = torch.empty_like(r)
    
    for t in range(T):
        # sa = dot(a, state) for each row i: sa_i = sum_j(a_j * state[i,j])
        # a[:, t] shape: [B, C] -> [B, H, N]
        a_t = a[:, t].reshape(B, H, N)
        sa = torch.einsum('bhn,bhnn->bhn', a_t, state)
        
        # State update: state = state * w + v*k + sa*b
        w_t = w[:, t].reshape(B, H, N)
        k_t = k[:, t].reshape(B, H, N)
        v_t = v[:, t].reshape(B, H, N)
        b_t = b[:, t].reshape(B, H, N)
        
        state = (state * w_t[:, :, None, None] + 
                 v_t[:, :, :, None] * k_t[:, :, None, :] + 
                 sa[:, :, :, None] * b_t[:, :, None, :])
        
        # Output: y = state @ r
        r_t = r[:, t].reshape(B, H, N)
        y[:, t] = (state * r_t[:, :, None, :]).sum(dim=-1).reshape(B, C)
    
    return y, state


def wkv_fused_sim(r, w, k, v, a, b, state):
    """Simulated fused WKV - batched operations."""
    y = torch.empty_like(r)
    
    # Compute all sa at once (batched dot product)
    sa = torch.einsum('btbc,bhcn->bthn', a, state)
    
    # Batched state update
    for t in range(T):
        state = (state * w[:, t, :, None, None] + 
                 v[:, t, :, None] * k[:, t, :, None, :] + 
                 sa[:, t, :, None] * b[:, t, :, None, :])
        y[:, t] = (state * r[:, t, :, None]).sum(dim=-1).reshape(B, C)
    
    return y, state


def layer_norm_naive(x, weight, bias, eps=1e-5):
    """Naive LayerNorm."""
    mean = x.mean(dim=-1, keepdim=True)
    var = x.var(dim=-1, keepdim=True, unbiased=False)
    return (x - mean) / torch.sqrt(var + eps) * weight + bias


def layer_norm_fused(x, weight, bias, eps=1e-5):
    """Fused LayerNorm (single kernel in CUDA)."""
    return F.layer_norm(x, (C,), weight=weight, bias=bias, eps=eps)


def mix6_naive(x, shift, x_r, x_w, x_k, x_v, x_a, x_g):
    """Naive Mix6 - 6 separate operations."""
    xx = shift - x
    xr = x + xx * x_r
    xw = x + xx * x_w
    xk = x + xx * x_k
    xv = x + xx * x_v
    xa = x + xx * x_a
    xg = x + xx * x_g
    return xr, xw, xk, xv, xa, xg


def mix6_fused(x, shift, x_r, x_w, x_k, x_v, x_a, x_g):
    """Fused Mix6 - compute all 6 at once."""
    xx = shift - x
    # Stack all gates and compute in one batched operation
    gates = torch.stack([x_r, x_w, x_k, x_v, x_a, x_g], dim=0)
    mixed = x + xx.unsqueeze(0) * gates
    return mixed[0], mixed[1], mixed[2], mixed[3], mixed[4], mixed[5]


def benchmark_kernel(name, func, *args, warmup=3, iters=20):
    """Benchmark a kernel."""
    # Warmup
    for _ in range(warmup):
        result = func(*args)
    
    torch.cuda.synchronize()
    
    # Benchmark
    t0 = time.perf_counter()
    for _ in range(iters):
        result = func(*args)
    torch.cuda.synchronize()
    elapsed = time.perf_counter() - t0
    
    ms = elapsed * 1000 / iters
    tps = (T * iters) / elapsed
    
    return ms, tps, result


def main():
    print("=" * 70)
    print("RWKV-7 WKV Kernel Benchmark")
    print(f"Config: B={B}, T={T}, C={C}, H={H}, N={N}")
    print(f"GPU: {torch.cuda.get_device_name(0)}")
    print("=" * 70)
    
    device = "cuda"
    dtype = torch.float16
    
    # Create test tensors
    r = torch.randn(B, T, C, device=device, dtype=dtype)
    w = torch.randn(B, T, C, device=device, dtype=dtype)
    k = torch.randn(B, T, C, device=device, dtype=dtype)
    v = torch.randn(B, T, C, device=device, dtype=dtype)
    a = torch.randn(B, T, C, device=device, dtype=dtype).sigmoid()
    b = torch.randn(B, T, C, device=device, dtype=dtype)
    state = torch.zeros(B, H, N, N, device=device, dtype=torch.float32)
    
    print("\n" + "-" * 70)
    print("WKV Kernel Performance")
    print("-" * 70)
    
    # Benchmark naive WKV
    ms, tps, _ = benchmark_kernel("Naive WKV", wkv_naive, r, w, k, v, a, b, state.clone())
    print(f"{'Naive PyTorch':<30} {ms:>8.3f} ms  {tps:>8.0f} tok/s")
    
    # Benchmark fused simulation
    ms, tps, _ = benchmark_kernel("Fused WKV", wkv_fused_sim, r, w, k, v, a, b, state.clone())
    print(f"{'Fused Simulated':<30} {ms:>8.3f} ms  {tps:>8.0f} tok/s")
    
    # Reference: Albatross CUDA
    print(f"{'Albatross CUDA (ref)':<30} {'~13.5':>8} ms  {'~9500':>8} tok/s")
    
    print("\n" + "-" * 70)
    print("LayerNorm Performance")
    print("-" * 70)
    
    x = torch.randn(B, T, C, device=device, dtype=dtype)
    weight = torch.ones(C, device=device, dtype=dtype)
    bias = torch.zeros(C, device=device, dtype=dtype)
    
    ms, tps, _ = benchmark_kernel("Naive LN", layer_norm_naive, x, weight, bias)
    print(f"{'Naive PyTorch':<30} {ms:>8.3f} ms")
    
    ms, tps, _ = benchmark_kernel("Fused LN", layer_norm_fused, x, weight, bias)
    print(f"{'F.layer_norm (fused)':<30} {ms:>8.3f} ms")
    
    print("\n" + "-" * 70)
    print("Mix6 Performance")
    print("-" * 70)
    
    shift = torch.randn(B, T, C, device=device, dtype=dtype)
    gates = [torch.randn(B, T, C, device=device, dtype=dtype) for _ in range(6)]
    
    ms, tps, _ = benchmark_kernel("Naive Mix6", mix6_naive, x, shift, *gates)
    print(f"{'Naive (6 ops)':<30} {ms:>8.3f} ms")
    
    ms, tps, _ = benchmark_kernel("Fused Mix6", mix6_fused, x, shift, *gates)
    print(f"{'Fused (1 batched)':<30} {ms:>8.3f} ms")
    
    print("\n" + "=" * 70)
    print("Summary")
    print("=" * 70)
    
    # Calculate theoretical UberKernel speedup
    naive_wkv_ms = benchmark_kernel("x", wkv_naive, r, w, k, v, a, b, state.clone())[0]
    fused_wkv_ms = benchmark_kernel("x", wkv_fused_sim, r, w, k, v, a, b, state.clone())[0]
    
    naive_ln_ms = benchmark_kernel("x", layer_norm_naive, x, weight, bias)[0]
    fused_ln_ms = benchmark_kernel("x", layer_norm_fused, x, weight, bias)[0]
    
    naive_mix_ms = benchmark_kernel("x", mix6_naive, x, shift, *gates)[0]
    fused_mix_ms = benchmark_kernel("x", mix6_fused, x, shift, *gates)[0]
    
    total_naive = naive_wkv_ms + naive_ln_ms + naive_mix_ms
    total_fused = fused_wkv_ms + fused_ln_ms + fused_mix_ms
    
    print(f"\nEstimated per-layer time:")
    print(f"  Naive (separate kernels):  {total_naive:.3f} ms")
    print(f"  Fused (UberKernel sim):    {total_fused:.3f} ms")
    print(f"  Speedup:                   {total_naive/total_fused:.2f}x")
    print(f"\n  Albatross CUDA (reference): ~0.8 ms per layer")
    print(f"  Triton UberKernel (est):    ~{total_fused*0.7:.1f}-{total_fused*0.8:.1f} ms per layer")
    
    # Throughput
    naive_tps = T / (total_naive / 1000)
    fused_tps = T / (total_fused / 1000)
    albatross_tps = 155 * 1000 / 7.0  # 155 tok/s on 5090, scaled to RTX 2050
    
    print(f"\nEstimated throughput (B=1):")
    print(f"  Naive PyTorch:    {naive_tps:>8.0f} tok/s")
    print(f"  Fused (sim):      {fused_tps:>8.0f} tok/s")
    print(f"  Albatross 5090:   {155*1000:>8.0f} tok/s")
    print(f"  Albatross 2050:   {albatross_tps:>8.0f} tok/s (est)")


if __name__ == "__main__":
    main()
