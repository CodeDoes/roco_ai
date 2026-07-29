#!/usr/bin/env python3
"""
RWKV-7 WKV Kernel Benchmark

Compares performance across:
1. Triton (NVIDIA/AMD)
2. OpenCL (Any GPU)
3. PyTorch (Reference)
4. Albatross CUDA (Reference)

Run: python benchmark.py
"""

import time
import torch
from typing import Optional


def benchmark_pytorch():
    """Reference PyTorch implementation."""
    print("\n=== PyTorch Reference ===")
    
    B, T, C, H, N = 1, 128, 4096, 64, 64
    device = "cuda" if torch.cuda.is_available() else "cpu"
    
    r = torch.randn(B, T, C, device=device, dtype=torch.float32)
    w = torch.randn(B, T, C, device=device, dtype=torch.float32)
    k = torch.randn(B, T, C, device=device, dtype=torch.float32)
    v = torch.randn(B, T, C, device=device, dtype=torch.float32)
    a = torch.randn(B, T, C, device=device, dtype=torch.float32).sigmoid()
    b = torch.randn(B, T, C, device=device, dtype=torch.float32)
    state = torch.zeros(B, H, N, N, device=device, dtype=torch.float32)
    
    def wkv_forward(r, w, k, v, a, b, state):
        y = torch.empty_like(r)
        for t in range(T):
            # sa = dot(a, state)
            sa = torch.einsum('bhc,bhcn->bhn', a[:, t], state)
            
            # State update
            state = state * w[:, t, :, None, None] + \
                    v[:, t, :, None] * k[:, t, :, None, :] + \
                    sa[:, :, None] * b[:, t, :, None, :]
            
            # Output
            y[:, t] = torch.einsum('bhcn,bhc->bhn', state, r[:, t]).reshape(B, C)
        
        return y, state
    
    # Warmup
    for _ in range(3):
        y, state = wkv_forward(r, w, k, v, a, b, state)
    
    if device == "cuda":
        torch.cuda.synchronize()
    
    t0 = time.perf_counter()
    iters = 5
    for _ in range(iters):
        y, state = wkv_forward(r, w, k, v, a, b, state)
    
    if device == "cuda":
        torch.cuda.synchronize()
    
    elapsed = time.perf_counter() - t0
    tps = (T * iters) / elapsed
    
    print(f"  Time: {elapsed*1000/iters:.2f} ms")
    print(f"  Throughput: {tps:.0f} tok/s")
    print(f"  Config: B={B}, T={T}, C={C}, H={H}, N={N}")
    print(f"  Device: {device}")
    
    return tps


def benchmark_triton():
    """Triton implementation."""
    try:
        import triton
        import triton.language as tl
        
        print("\n=== Triton ===")
        
        @triton.jit
        def wkv7_kernel(
            B, T, C, H,
            r_ptr, w_ptr, k_ptr, v_ptr, a_ptr, b_ptr,
            state_ptr, y_ptr,
            HEAD_SIZE: tl.constexpr,
            BLOCK_SIZE: tl.constexpr,
        ):
            pid = tl.program_id(0)
            batch_id = pid // H
            head_id = pid % H
            
            state_size = C * HEAD_SIZE
            tid = tl.arange(0, BLOCK_SIZE)
            mask = tid < HEAD_SIZE
            
            state_offset = batch_id * state_size + head_id * HEAD_SIZE * HEAD_SIZE
            state = tl.load(state_ptr + state_offset + tid * HEAD_SIZE + tl.arange(0, HEAD_SIZE),
                             mask=mask[:, None] & tl.arange(0, HEAD_SIZE)[None, :],
                             other=0.0).to(tl.float32)
            
            n_seq_tokens = T // B
            start_t = batch_id * n_seq_tokens * C + head_id * HEAD_SIZE + tid
            
            for t_offset in range(n_seq_tokens):
                t = start_t + t_offset * C
                
                r = tl.load(r_ptr + t, mask=mask, other=0.0).to(tl.float32)
                w = tl.load(w_ptr + t, mask=mask, other=0.0).to(tl.float32)
                k = tl.load(k_ptr + t, mask=mask, other=0.0).to(tl.float32)
                v_val = tl.load(v_ptr + t, mask=mask, other=0.0).to(tl.float32)
                a = tl.load(a_ptr + t, mask=mask, other=0.0).to(tl.float32)
                b = tl.load(b_ptr + t, mask=mask, other=0.0).to(tl.float32)
                
                a_broadcast = tl.broadcast_to(a, (HEAD_SIZE, HEAD_SIZE))
                sa = tl.sum(state * a_broadcast, axis=1)
                
                w_broadcast = tl.broadcast_to(w, (HEAD_SIZE, HEAD_SIZE))
                k_broadcast = tl.broadcast_to(k, (HEAD_SIZE, HEAD_SIZE))
                b_broadcast = tl.broadcast_to(b, (HEAD_SIZE, HEAD_SIZE))
                v_broadcast = tl.broadcast_to(v_val[:, None], (HEAD_SIZE, HEAD_SIZE))
                sa_broadcast = tl.broadcast_to(sa[:, None], (HEAD_SIZE, HEAD_SIZE))
                
                state = state * w_broadcast + v_broadcast * k_broadcast + sa_broadcast * b_broadcast
                
                r_broadcast = tl.broadcast_to(r, (HEAD_SIZE, HEAD_SIZE))
                y = tl.sum(state * r_broadcast, axis=1)
                
                tl.store(y_ptr + t, y, mask=mask)
            
            tl.store(state_ptr + state_offset + tid * HEAD_SIZE + tl.arange(0, HEAD_SIZE),
                     state, mask=mask[:, None] & tl.arange(0, HEAD_SIZE)[None, :])
        
        B, T, C, H, N = 1, 128, 4096, 64, 64
        device = "cuda" if torch.cuda.is_available() else "cpu"
        
        if device == "cpu":
            print("  Triton requires CUDA, skipping")
            return 0
        
        r = torch.randn(B, T, C, device=device, dtype=torch.float16)
        w = torch.randn(B, T, C, device=device, dtype=torch.float16)
        k = torch.randn(B, T, C, device=device, dtype=torch.float16)
        v = torch.randn(B, T, C, device=device, dtype=torch.float16)
        a = torch.randn(B, T, C, device=device, dtype=torch.float16).sigmoid()
        b = torch.randn(B, T, C, device=device, dtype=torch.float16)
        state = torch.zeros(B, H, N, N, device=device, dtype=torch.float32)
        y = torch.empty(B, T, C, device=device, dtype=torch.float16)
        
        block_size = triton.next_power_of_2(N)
        grid = (B * H,)
        
        # Warmup
        for _ in range(3):
            wkv7_kernel[grid](
                B, T, C, H, r, w, k, v, a, b, state, y,
                HEAD_SIZE=N, BLOCK_SIZE=block_size,
            )
        
        torch.cuda.synchronize()
        
        t0 = time.perf_counter()
        iters = 10
        for _ in range(iters):
            wkv7_kernel[grid](
                B, T, C, H, r, w, k, v, a, b, state, y,
                HEAD_SIZE=N, BLOCK_SIZE=block_size,
            )
        
        torch.cuda.synchronize()
        elapsed = time.perf_counter() - t0
        tps = (T * iters) / elapsed
        
        print(f"  Time: {elapsed*1000/iters:.2f} ms")
        print(f"  Throughput: {tps:.0f} tok/s")
        print(f"  Config: B={B}, T={T}, C={C}, H={H}, N={N}")
        
        return tps
        
    except ImportError:
        print("\n=== Triton ===")
        print("  Not installed, skipping")
        return 0


def benchmark_albatross_reference():
    """Reference Albatross performance (from README)."""
    print("\n=== Albatross CUDA (Reference) ===")
    print("  Time: ~13.5 ms (B=1, T=128)")
    print("  Throughput: ~9500 tok/s")
    print("  Config: B=1, T=128, C=4096, H=64, N=64")
    print("  Device: RTX 5090")
    return 9500


def main():
    print("=" * 60)
    print("RWKV-7 WKV Kernel Benchmark")
    print("=" * 60)
    
    results = {}
    
    results["PyTorch"] = benchmark_pytorch()
    results["Triton"] = benchmark_triton()
    results["Albatross CUDA"] = benchmark_albatross_reference()
    
    print("\n" + "=" * 60)
    print("Summary")
    print("=" * 60)
    
    for name, tps in sorted(results.items(), key=lambda x: -x[1]):
        if tps > 0:
            print(f"  {name:20s}: {tps:8.0f} tok/s")
    
    # Compare to Albatross
    if results.get("Triton", 0) > 0 and results.get("Albatross CUDA", 0) > 0:
        ratio = results["Triton"] / results["Albatross CUDA"] * 100
        print(f"\n  Triton is {ratio:.1f}% of Albatross CUDA performance")


if __name__ == "__main__":
    main()
