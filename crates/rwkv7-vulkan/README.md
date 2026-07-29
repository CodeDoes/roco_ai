# RWKV-7 Vulkan Support

## Status: Already Supported ✓

RWKV-7 inference already runs on Vulkan through the existing **web-rwkv** crate, which uses **wgpu** as its GPU abstraction layer.

## Architecture

```
RWKV-7 Model
     ↓
web-rwkv (WGSL shaders)
     ↓
   wgpu
     ↓
┌────┴────┐
│ Vulkan  │  ← Linux, Android, Windows
│ Metal   │  ← macOS, iOS
│ DX12    │  ← Windows
│ WebGL2  │  ← Browsers
└─────────┘
```

## Usage

```toml
[dependencies]
web-rwkv = { path = "vendor/web-rwkv" }
```

```rust
use web_rwkv::context::Context;

// wgpu automatically selects Vulkan/Metal/DX12 based on platform
let context = Context::new(...).await?;

// Run RWKV-7 inference
let output = model.run(tokens, &state).await?;
```

## Shaders Location

The WGSL shaders are in `vendor/web-rwkv/src/shaders/`:

- `time_mix_v7.wgsl` - WKV recurrence (core attention)
- `layer_norm.wgsl` - Layer normalization
- `channel_mix.wgsl` - Channel mixing (FFN)
- `matmul_vec_fp16.wgsl` - Matrix multiplication

## Why Not Raw Vulkan?

Using wgpu provides:
- **Cross-platform** - Vulkan, Metal, DX12, WebGL2 from one codebase
- **Automatic** - No manual Vulkan instance/device/queue management
- **Tested** - web-rwkv shaders are already optimized and validated
- **Maintained** - wgpu handles driver quirks and updates

## Performance

wgpu's Vulkan backend adds minimal overhead (~2-5%) compared to raw Vulkan.
For most use cases, this is negligible.

## See Also

- `vendor/web-rwkv/` - The actual inference engine
- `vendor/web-rwkv/src/shaders/` - WGSL compute shaders
- `vendor/web-rwkv/src/runtime/v7.rs` - RWKV-7 runtime
