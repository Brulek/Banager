# ollama 0.34.1 fixtures

Recorded 2026-09-20 on BrulekdeMacBook-Pro.local (macOS 27, Apple Silicon) by running the commands below and
saving their output byte for byte. Nothing here is hand-written or edited — if
a parser disagrees with one of these files, the parser is wrong.

Captured:
- `GET http://127.0.0.1:11434/api/tags` -> `api-tags.json`
- `~/.ollama/models/manifests/registry.ollama.ai/library/qwen3.8/27b-mlx` -> `local-manifest-qwen3.8-27b-mlx.json`
- `GET https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx` with
  `Accept: application/vnd.docker.distribution.manifest.v2+json` -> `registry-manifest-qwen3.8-27b-mlx.json`

The registry answers anonymously; no token exchange is needed. Both manifests
here carry 1209 layers with identical digest sets and the same config digest,
so this pair is the "already up to date" case. Note the layer count: this is an
MLX safetensors model split into many shards, so it is an unusually wide
example — a GGUF model typically has around five layers. Comparing the **set of
layer digests** (not the serialized file) is what the adapter does, because
Ollama rewrites the local manifest on disk and a byte comparison produces false
"outdated" results.
