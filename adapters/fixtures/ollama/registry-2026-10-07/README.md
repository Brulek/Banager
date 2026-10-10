# Ollama registry recording, 2026-10-07

Not an `ollama` version: the folder is named after the day (UTC) the
registry answered. Nothing here is hand-written or edited.

Captured, read-only, on BrulekdeMacBook-Pro.local (macOS 27.0) for the
r40 review, at 2026-10-07 20:53 UTC (2026-10-08 04:53 on that Mac's
clock), and saved byte for byte:

- `GET https://registry.ollama.ai/v2/library/gpt-oss/manifests/120b-cloud`
  with `Accept: application/vnd.docker.distribution.manifest.v2+json`
  (Banager's own header for this request) ->
  `registry-manifest-gpt-oss-120b-cloud.json`

264 bytes, SHA-256
`ac7f7a1e778577c4418f6a25e46e0b45dced6746c75422d4b343aa1495a022ed`. The
same request made again at 2026-10-07 22:25 UTC (HTTP 200) returned the
same bytes.

`gpt-oss:120b-cloud` is one of Ollama's cloud models: its manifest has
`"layers":[]`, so its 307-byte config is the whole model. `ollama pull`
writes a manifest as the registry sent it, so this is also what the local
file holds once the model is pulled, and its SHA-256 is the digest
`/api/tags` gives for it. The tests in
`crates/banager-core/src/adapters/ollama/registry_change_tests.rs` use it
for the case where only a config can change, and the robustness test feeds
it to the manifest parsers.
