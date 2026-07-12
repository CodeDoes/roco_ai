#!/usr/bin/env python3
"""Convert RWKV GGUF model to ST (SafeTensors) format.

ST format = SafeTensors with .st extension, float16, RWKV tensor names.
Used by web-rwkv and ai00_server for inference.

Usage: python convert_gguf_to_st.py --input model.gguf --output model.st
"""

import argparse
import os
import sys

import numpy as np
import torch
from gguf import GGUFReader, dequantize
from gguf.constants import GGMLQuantizationType
from safetensors.torch import save_file

# GGUF → ST tensor name mapping for non-block tensors
HEAD_MAP = {
    "token_embd.weight": "emb.weight",
    "token_embd_norm.weight": "blocks.0.ln0.weight",
    "token_embd_norm.bias": "blocks.0.ln0.bias",
    "output.weight": "head.weight",
    "output_norm.weight": "ln_out.weight",
    "output_norm.bias": "ln_out.bias",
}

# GGUF suffix → ST suffix mapping for block tensors (per layer)
BLOCK_SUFFIX_MAP = {
    "attn_norm.weight": "ln1.weight",
    "attn_norm.bias": "ln1.bias",
    "attn_norm_2.weight": "ln2.weight",
    "attn_norm_2.bias": "ln2.bias",
    "time_mix_ln.weight": "att.ln_x.weight",
    "time_mix_ln.bias": "att.ln_x.bias",
    "time_mix_key.weight": "att.key.weight",
    "time_mix_value.weight": "att.value.weight",
    "time_mix_receptance.weight": "att.receptance.weight",
    "time_mix_output.weight": "att.output.weight",
    "channel_mix_key.weight": "ffn.key.weight",
    "channel_mix_value.weight": "ffn.value.weight",
    "channel_mix_lerp_k.weight": "ffn.x_k",
    "time_mix_w0.weight": "att.w0",
    "time_mix_w1.weight": "att.w1",
    "time_mix_w2.weight": "att.w2",
    "time_mix_a0.weight": "att.a0",
    "time_mix_a1.weight": "att.a1",
    "time_mix_a2.weight": "att.a2",
    "time_mix_g1.weight": "att.g1",
    "time_mix_g2.weight": "att.g2",
    "time_mix_v0.weight": "att.v0",
    "time_mix_v1.weight": "att.v1",
    "time_mix_v2.weight": "att.v2",
    "time_mix_r_k.weight": "att.r_k",
    "time_mix_k_k.weight": "att.k_k",
    "time_mix_k_a.weight": "att.k_a",
    # time_mix_lerp_fused handled separately - splits into 6 vectors
}

# Tensor suffixes that need last-two-dims transpose.
# For V7 models, the Rust converter (web-rwkv-converter) does NOT transpose
# w1/w2/a1/a2/g1/g2/v1/v2/r_k. Only V6-specific time_mix* and lora.0 are transposed.
# See: ai00_server/crates/converter/src/main.rs
TRANSPOSE_SUFFIXES = [
    # V6: time_mix_w1, time_mix_w2, time_decay_w1, time_decay_w2
    # V7: none of these match - keep tensors in their original .pth shape
]

# Learp fused tensor - splits into 6 vectors
LERF_FUSED_NAMES = ["x_r", "x_w", "x_k", "x_v", "x_a", "x_g"]


def decode_tensor(tensor) -> torch.Tensor:
    """Convert GGUF tensor to torch.Tensor in PyTorch shape convention.

    GGUF stores shapes in column-major (Fortran) order.
    PyTorch uses row-major (C) order, so we reverse the shape.
    """
    data = tensor.data
    qtype = tensor.tensor_type
    gguf_shape = [int(x) for x in tensor.shape]
    pytorch_shape = gguf_shape[::-1]

    if qtype == GGMLQuantizationType.F32:
        arr = np.frombuffer(data, dtype=np.float32).copy()
        return torch.from_numpy(arr.reshape(pytorch_shape))
    elif qtype == GGMLQuantizationType.F16:
        arr = np.frombuffer(data, dtype=np.float16).copy()
        return torch.from_numpy(arr.reshape(pytorch_shape))
    elif qtype == GGMLQuantizationType.BF16:
        arr = np.frombuffer(data, dtype=np.uint16).copy()
        arr32 = np.zeros(len(arr), dtype=np.uint32)
        arr32[:] = arr.astype(np.uint32) << 16
        return torch.from_numpy(arr32.view(np.float32)).to(torch.bfloat16).reshape(pytorch_shape)
    else:
        weights = dequantize(data, qtype).copy()
        t = torch.from_numpy(weights)
        if t.dim() == 1 and len(pytorch_shape) > 1:
            t = t.reshape(pytorch_shape)
        return t


# Tensors that should be reshaped from [emb] → [1, 1, emb] in the ST format.
# In RWKV-7 the web-rwkv loader expects these constant/value vectors as
# 3D matrices with two leading singleton axes. GGUF stores them flat as 1D.
#
# This is derived from comparing the harness converter's pth_to_st output
# (which knows the correct layout) against the GGUF tensor shapes.
RWKV7_3D_SCALAR_NAMES = {
    "a0", "k_a", "k_k", "v0", "w0",
    "x_r", "x_w", "x_k", "x_v", "x_a", "x_g",
}

# `r_k` is special: in ST it's (clock_count, head_dim), not (emb,). GGUF
# stores it flat. We re-derive shape from metadata: clock_count = emb / head_dim.
RWKV7_R_K_NAME = "r_k"


def main_reshape(st_suffix: str, t: torch.Tensor, head_dim: int, embedding: int) -> torch.Tensor:
    """Apply RWKV-7-specific reshape rules to a decoded ST tensor."""
    if st_suffix in RWKV7_3D_SCALAR_NAMES and t.dim() == 1 and t.shape[0] == embedding:
        # [emb] → [1, 1, emb]
        return t.view(1, 1, embedding)
    if st_suffix == RWKV7_R_K_NAME and t.dim() == 1:
        # [numel] → [clock_count, head_dim]
        assert embedding % head_dim == 0, (
            f"emb={embedding} not divisible by head_dim={head_dim}; "
            f"can't reshape r_k of size {t.shape[0]}"
        )
        clock_count = embedding // head_dim
        return t.view(clock_count, head_dim)
    return t


def convert_gguf_to_st(input_path: str, output_path: str) -> None:
    reader = GGUFReader(input_path)

    # Pull RWKV-7 hyperparameters from GGUF metadata so we know how to reshape
    # the special tensors (`a0`, `k_a`, `r_k`, etc.) into web-rwkv's expected layout.
    def _u32(key):
        if key not in reader.fields:
            return None
        parts = reader.fields[key].parts
        # last element is the scalar value (uint32/float32 encoded array)
        return int(parts[-1])

    def _f32(key):
        if key not in reader.fields:
            return None
        return float(reader.fields[key].parts[-1])

    embedding = _u32("rwkv7.embedding_length") or 0
    head_dim   = _u32("rwkv7.wkv.head_size")      or 0
    print(f"rwkv7 meta: embedding={embedding}, head_size={head_dim}")

    st_tensors = {}

    # Count blocks
    num_blocks = 0
    for t in reader.tensors:
        if t.name.startswith("blk.0."):
            num_blocks += 1

    # Find max block index
    max_block = 0
    for t in reader.tensors:
        if t.name.startswith("blk.") and t.name[4] != ".":
            idx = t.name.split(".")[1]
            try:
                max_block = max(max_block, int(idx))
            except ValueError:
                pass

    print(f"Model has {max_block + 1} blocks")

    for tensor in reader.tensors:
        name = tensor.name

        # Handle head tensors (non-block)
        if name in HEAD_MAP:
            st_name = HEAD_MAP[name]
            t = decode_tensor(tensor)

            # emb.weight and head.weight: GGUF shape [2560, 65536] reversed → [65536, 2560]
            # which matches .pth/.st format. No extra transpose needed.

            print(f"  {st_name}: {t.shape} {t.dtype}")
            st_tensors[st_name] = t.contiguous().half()
            continue

        # Handle block tensors
        if name.startswith("blk."):
            parts = name.split(".", 2)
            if len(parts) < 3:
                continue
            try:
                block_idx = int(parts[1])
            except ValueError:
                continue
            suffix = parts[2]

            # Handle lerp fused tensor - split into 6 vectors
            if suffix == "time_mix_lerp_fused.weight":
                t = decode_tensor(tensor)
                t = t.squeeze()
                if t.dim() == 2 and t.shape[0] == 6:
                    for i, lerp_name in enumerate(LERF_FUSED_NAMES):
                        st_name = f"blocks.{block_idx}.att.{lerp_name}"
                        vec = t[i]
                        vec = main_reshape(lerp_name, vec, head_dim, embedding)
                        print(f"  {st_name}: {vec.shape} {vec.dtype}")
                        st_tensors[st_name] = vec.contiguous().half()
                else:
                    print(f"  WARNING: unexpected lerp_fused shape {t.shape}")
                continue

            if suffix in BLOCK_SUFFIX_MAP:
                st_suffix = BLOCK_SUFFIX_MAP[suffix]
                st_name = f"blocks.{block_idx}.{st_suffix}"
                t = decode_tensor(tensor)

                # Apply transpose for specific tensors
                for trans_suffix in TRANSPOSE_SUFFIXES:
                    if st_suffix == trans_suffix:
                        if t.dim() >= 2:
                            t = t.transpose(-2, -1).contiguous()
                        break

                # RWKV-7 reshape: [emb] → [1, 1, emb], or reshape r_k to matrix.
                t = main_reshape(st_suffix, t, head_dim, embedding)

                print(f"  {st_name}: {t.shape} {t.dtype}")
                st_tensors[st_name] = t.contiguous().half()
                continue

            print(f"  WARNING: unhandled block tensor: {name}")

    print(f"\nTotal tensors: {len(st_tensors)}")

    # Save to ST file
    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)
    save_file(st_tensors, output_path, metadata={"format": "st"})
    print(f"Saved to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Convert RWKV GGUF model to ST format")
    parser.add_argument("--input", "-i", required=True, help="Path to input GGUF model")
    parser.add_argument("--output", "-o", default="model.st", help="Path to output ST model")
    args = parser.parse_args()
    convert_gguf_to_st(args.input, args.output)
