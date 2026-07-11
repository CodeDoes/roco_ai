#!/usr/bin/env python3
"""
RWKV-7 model converter: PTH ↔ SafeTensors (web-rwkv format)

Matches the harness conversion that produces working models.

Usage:
    python convert.py -i model.pth -o model.st          # PTH → ST
    python convert.py -i model.st -o model.pth          # ST → PTH (auto-detect)
    python convert.py -i model.pth -o model.st --dry-run

Requires: torch, safetensors  (pip install torch safetensors)

RWKV-7 transpose rules (from reverse-engineering the harness):
    In each `blocks.N.att.*` block, the LoRA-like matrices get transposed:
        a1, a2, g1, g2, v1, v2, w1, w2
    All other tensors (weights, vectors, norms) keep their shape.
    dtype: bfloat16 → float16 (forward), float16 → bfloat16 (reverse)
"""

import argparse
import sys
import time
from pathlib import Path

try:
    import torch
except ImportError:
    print("Error: pip install torch", file=sys.stderr)
    sys.exit(1)
try:
    from safetensors.torch import save_file, load_file
except ImportError:
    print("Error: pip install safetensors", file=sys.stderr)
    sys.exit(1)

# Tensors in attention blocks that need transposing (RWKV-7 LoRA-like matrices)
ATT_TRANSPOSE_SUFFIXES = {"a1", "a2", "g1", "g2", "v1", "v2", "w1", "w2"}


def needs_transpose(key: str) -> bool:
    """True if this tensor should be transposed during PTH↔ST conversion."""
    if ".att." not in key:
        return False
    suffix = key.rsplit(".", 1)[-1]
    return suffix in ATT_TRANSPOSE_SUFFIXES


def pth_to_st(sd: dict) -> dict:
    result = {}
    for k, v in sd.items():
        t = v.to(torch.float16)
        if t.dim() >= 2 and needs_transpose(k):
            t = t.transpose(-2, -1).contiguous()
        result[k] = t
    return result


def st_to_pth(sd: dict) -> dict:
    result = {}
    for k, v in sd.items():
        t = v.to(torch.bfloat16)
        if t.dim() >= 2 and needs_transpose(k):
            t = t.transpose(-2, -1).contiguous()
        result[k] = t
    return result


def main():
    p = argparse.ArgumentParser(description="RWKV-7 PTH ↔ SafeTensors converter")
    p.add_argument("-i", "--input", required=True)
    p.add_argument("-o", "--output", required=True)
    p.add_argument("--reverse", action="store_true")
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()

    inp = Path(args.input)
    out = Path(args.output)
    if not inp.exists():
        print(f"Error: {inp} not found", file=sys.stderr)
        sys.exit(1)

    # Auto-detect direction
    reverse = args.reverse
    if not reverse and inp.suffix == ".st":
        reverse = True
        print("Auto-detected ST → PTH (use --reverse to force)")
    elif reverse and inp.suffix == ".pth":
        reverse = False
        print("Warning: input is .pth, ignoring --reverse")

    mode = "ST → PTH" if reverse else "PTH → ST"
    print(f"Loading {inp} …")
    t0 = time.time()
    if inp.suffix == ".pth":
        sd = torch.load(inp, map_location="cpu", weights_only=True)
    else:
        sd = load_file(str(inp), device="cpu")
    print(f"  {len(sd)} tensors, {inp.stat().st_size / 1e9:.1f} GB in {time.time()-t0:.1f}s")

    print(f"Converting ({mode}) …")
    t0 = time.time()
    converted = st_to_pth(sd) if reverse else pth_to_st(sd)
    print(f"  {time.time()-t0:.1f}s")

    # Summary
    changed = 0
    for k in sorted(sd):
        o = sd[k]
        c = converted[k]
        if list(o.shape) != list(c.shape) or o.dtype != c.dtype:
            changed += 1
            if changed <= 5:
                print(f"  {k}: {list(o.shape)} ({o.dtype}) → {list(c.shape)} ({c.dtype})")
    if changed > 5:
        print(f"  … and {changed-5} more")
    print(f"  {changed}/{len(sd)} tensors changed")

    if args.dry_run:
        return

    print(f"Writing {out} …")
    t0 = time.time()
    if out.suffix == ".pth":
        torch.save(converted, out)
    else:
        save_file(converted, str(out))
    print(f"  {out.stat().st_size / 1e9:.1f} GB in {time.time()-t0:.1f}s — done.")


if __name__ == "__main__":
    main()
