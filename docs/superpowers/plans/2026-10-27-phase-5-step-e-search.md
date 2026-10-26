# Phase 5 step E: search Homebrew from Discover (2026-10-27)

Step E of `docs/superpowers/2026-09-29-phase-5-proposal.md` §五, after
step B; Q3 answered "only Homebrew" (item R9). A search field on Discover
finds Homebrew's own formulae and casks, and Install on a result goes the
way a catalog entry's does, by an id, never a package name.

## What the user sees

- A search field under Discover's first line, where a Homebrew that
  answered and can be written to is there. Typing two characters or more
  searches after 0.6 s; while it has text, the results are in the list's
  place, and clearing it brings the suggestions back.
- Each result: logo, name, a mark -- "Suggested" for a catalog entry,
  "Not suggested" otherwise -- what it is (the window's own words for
  the tool where it has them, else Homebrew's description), and Install,
  or "Installed". Install opens the same sheet as a suggestion's, which
  says the result is not one of the suggestions.
- "Homebrew has nothing matching …", the text it does not take, or why it
  could not search, in place of the results.

## How it runs

- The backlog's fix to `BrewAdapter::search` comes first: `brew search
  --formula`, `--cask` (each a list of one kind, no guessing which group
  is which), and `--desc` for the descriptions of those only -- a package
  only a description matched is not listed, since `--desc` prints a
  third-party tap's package under its short name (from the review of
  95555b8d); the name itself first; a search that finds nothing (exit 1, "No
  formulae or casks found") is no failure. Recorded output of Homebrew
  7.0.10 in `adapters/fixtures/brew/7.0.10/`.
- `validate_search_query`: spaces allowed (the old check refused them);
  refused before anything runs: a leading `-` (`brew search --macports`
  opens a web page), a `/` (a regex, or a tap-qualified name whose
  formula Homebrew loads), control characters, over 64 characters, and
  fewer than two letters, digits, `@` or `+` (Homebrew drops the rest, so
  `视频` matched all 16,000 packages).
- `Session::search_homebrew` keeps each result under a random id for ten
  minutes (at most 2,000); `plan_found` plans an install of exactly that,
  through `issue_plan`. IPC: `search_homebrew`, `plan_install_found`.

## Decisions taken on the author's behalf (2026-10-27)

1. **Homebrew's own packages only.** A third-party tap's names
   (`user/tap/name`) are left out of the results, though `brew search`
   lists those of taps the Mac has.
2. **At most 40 results**, names before description matches.
3. **Results expire after ten minutes** (as a previewed plan does); the
   page keeps a search five minutes, so what it lists can be installed.
4. **No search before Homebrew is there**: the field shows only with a
   Homebrew that can install.

## The work

- Rust: `brew::search`, `validate_search_query`, `search_names`;
  `session/search.rs` (`Found`, `search_homebrew`, `plan_found`);
  `catalog::id_installing`; IPC and tests.
- Window: `DiscoverSearch.tsx`, the install sheet for a result,
  `EndedWithout.tsx` shared; words in three locales; the mock's pretend
  Homebrew (`mockSearch.ts`).
- Docs: what-we-run.md (Homebrew's read-only rows, "Searching Homebrew",
  the never-list); README and reference.
