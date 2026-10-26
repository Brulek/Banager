# Phase 5 step D: Ollama, and a model to start with (2026-10-27)

Step D of `docs/superpowers/2026-09-29-phase-5-proposal.md` §五, after
step B. Ollama's app installs from Discover like the other casks, and a
ninth entry, a model to start with, installs with the existing `ollama
pull` plan.

## What the user sees

- **Ollama** has Install where Homebrew is there and macOS is 14 or later
  (the cask `ollama-app`). Once installed from here its line says to open
  it, with Open Ollama while it is not running.
- **Gemma 3** (under Local Models): "Google's open AI model, sized for
  this Mac: gemma3:4b, about 3.4 GB". Its state: Installed where any
  Ollama source lists a model; Install where an Ollama answers and can be
  written to; Open Ollama where Ollama's app is there and not running;
  "Ollama can't download now" for any other Ollama that cannot pull;
  "Needs Ollama" with none. Its sheet says Ollama downloads it from its
  library and about how much, and shows `ollama pull <model>`. Once
  pulled, its line says `ollama run <model>`, with Copy.

## Decisions taken on the author's behalf (2026-10-27)

1. **Gemma 3, by memory.** The largest Gemma 3 whose download is at most
   about two-fifths of the least memory it is chosen for: 1b under 8 GiB,
   4b from 8 GiB (39%), 12b from 24 GiB, 27b from 48 GiB; the 4b where
   `hw.memsize` cannot be read. Only through this Mac's own Ollama: one
   at another machine's `OLLAMA_HOST` neither pulls it nor counts.
   Gemma 3 (Google) answers in Chinese well, has no "thinking" output to
   puzzle a beginner, and the 4b and up read images too. Qwen3 was the
   other candidate (stronger in Chinese, but prints its reasoning).
2. **Sizes fixed in the app.** Read from registry.ollama.ai's manifests on
   2026-10-27 (layers plus config): 806,067,407 / 3,350,272,403 /
   8,164,039,732 / 17,413,409,652 bytes. Nothing is fetched to show them;
   a release that changes the choice reads them again.
3. **Any model counts.** A Mac with any Ollama model has "a model to start
   with": the row says Installed rather than offering a second one.
4. **No free-space check yet.** The proposal's "剩余空间" line is left
   for later: the sheet says the download's size, and `ollama pull`
   itself stops when the disk is full.

## The work

- `catalog.rs`: `CatalogInstall::OllamaModel`, `starter_model`,
  `Host::memory_bytes` (`hw.memsize`), the `model` entry,
  `Recognizer::AnyModel`, `NeedsOllama` / `OllamaNotReady`,
  `install_request` through the first Ollama that can pull; Ollama's
  `installs_here`.
- Window: `wordsOf`, `commandOf`, `installTargetOf` (src/lib/catalog.ts);
  the sheet's Ollama line; Open Ollama on the rows; words in three
  locales; the mock's model, its pull, and the stopped Ollama a mock
  install of the app leaves.
- Docs: what-we-run.md "Discover"; ui-preview.md; reference and README.
