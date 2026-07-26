# RFC 0008: Offline Inference Protocol
Status: Enforced for Local Deployment

## Protocol Rules
- **Model Pathing:** Local file path mandatory (e.g. `.st` safetensors weights). Remote URL fetching disabled.
- **Daemon Interface:** Local HTTP/IPC daemon (`roco-inferd`) serves RWKV inference via `web-rwkv` / WGPU thread.
- **Strict Grammar:** When `strict_grammar = true`, outgoing text generation passes through token-level kbnf BNF grammar decoder.
- **No Remote Fallback:** System returns explicit error if local model binary is missing or daemon is unreachable; cloud API fallbacks prohibited.
