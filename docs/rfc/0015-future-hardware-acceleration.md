# RFC 0015: Hardware Acceleration Roadmap
Status: Future / Hardware Target Specs

## Targets & Fallbacks
- **Current:** WGPU / Vulkan backend (`roco-inference` / `web-rwkv`), CPU fallback (`llvmpipe`).
- **Target Accelerators:** Vulkan (AMD/Intel/NVIDIA GPUs), Apple Metal, NVIDIA Jetson.
- **Constraints:** Must remain 100% local with zero cloud API dependencies.
