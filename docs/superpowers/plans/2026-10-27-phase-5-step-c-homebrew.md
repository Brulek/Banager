# Phase 5 step C: the empty Mac's front door, and Homebrew in Terminal (2026-10-27)

Step C of `docs/superpowers/2026-09-29-phase-5-proposal.md` §五, after
steps A and B. Q1 was answered (a) by the author with the rest of the
proposal (item R9 of `Banager/待你拍板的事项.md`): Homebrew is installed
by the person, in Terminal; Banager copies the command, opens Terminal,
and looks for Homebrew to appear. It never runs the installer and never
sees the password.

## What the user sees

- **The front door.** Opened on a Mac with nothing to manage -- no
  Homebrew, and nothing listed but by pip (macOS's own Python) -- Banager
  goes to Discover once the first check is done, if the person has not
  chosen a page meanwhile (`useFrontDoor`). The Installed and Updates
  pages' "No tools to manage" gets Go to Discover.
- **Discover without Homebrew.** The line over the list says to install
  Homebrew first; Homebrew's row has Install; the others say "Needs
  Homebrew". On an Intel Mac Homebrew's row says it is not supported,
  with no button (its installer refuses there).
- **The Homebrew sheet.** What happens (installed in Terminal, by you; the
  password is typed there only); five steps (Open Terminal; ⌘V and
  Return; the password; Return at "Press RETURN" and the Command Line
  Tools; "Installation successful!"); What the Command Does (five plain
  lines read from Homebrew/install's `install.sh` at `85d1a3f`); Show
  Command, the exact command; on macOS before 15, Homebrew's own warning
  in a sentence. Open Terminal copies the command and runs `open -a
  Terminal`. While open, it looks every 2 s; two looks in a row with
  `/opt/homebrew/bin/brew` there and no `brew update` running mean it is
  installed: it says so, checks again, and Continue closes it.
- After that check, Homebrew's row says Installed and the six install from
  here (step B).

## Decisions taken on the author's behalf (2026-10-27)

1. **The front door's rule.** No Homebrew, and every listed row a pip
   source's; at the first finished check of a launch only, and only from
   the Overview. A Mac with any other source's tool, or Homebrew with
   nothing in it, opens as before.
2. **Two looks in a row.** `bin/brew` is in place a little before the
   installer's last step, `brew update`, takes its lock; two looks two
   seconds apart that both see it there and unlocked is the done signal.
3. **PATH, without a second shell.** Once installed, `/opt/homebrew/bin`
   goes at the end of Banager's `PATH` for the rest of the launch
   (`login_path::add_folder`), as `/etc/paths.d/homebrew` puts it for new
   Terminal windows; the login shell is not run again, so "one shell run
   at launch" stays true.
4. **Go to Discover on every "No tools to manage",** not only where
   Homebrew is missing: Discover is where to start either way.
5. **Not verified on a clean Mac.** S0 (a clean macOS VM walk-through) has
   not been done; the step lists and the five lines are read from the
   script, not watched. Before merging, the author walks through it once
   on a clean VM (Apple silicon, macOS 15+, and 14).

## The work

- Rust: `brew::installer_look`, `INSTALLER_PREFIX`; `look_for_homebrew`
  and `open_terminal` (src-tauri/src/ipc.rs, `open -a` shared with Open
  Ollama); `login_path::add_folder`, applied to every `PATH` reader and
  to the command environment; tests (`login_path_added_test.rs`).
- Window: `src/lib/homebrewSetup.ts` (command, `isEmptyMac`,
  `useFrontDoor`), `HomebrewSetupSheet`, Discover's Homebrew row and
  intro, Go to Discover; words in three locales; mock `open_terminal` and
  `look_for_homebrew`.
- Docs: what-we-run.md "Homebrew itself", the PATH paragraph, the
  launches paragraph; ui-preview.md; reference and README lines.
