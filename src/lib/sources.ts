/**
 * What the front end knows about each package source, in one place so the
 * pages and the snapshot gate cannot drift apart. Nothing here talks to the
 * backend; these are presentational facts about what arrives over the wire.
 */
import type {
  ArtifactKey,
  ArtifactKind,
  InstanceNote,
  ManagerInstance,
  ReadOnlyReason,
  Snapshot,
  SourceError,
  UninstallBlocked,
  Unavailable,
  UpdateBlocked,
} from "./types";
import type { InstalledShow } from "./families";
import { displayToken } from "./format";
import { FAILURE_CAUSE_KEYS, failureCause } from "./failureCause";

/** i18n key holding each adapter's human name. The `standalone-*` ids are
 *  the tools with their own installer (`standalone::all` in
 *  crates/banager-core/src/adapters/standalone/mod.rs), one per recipe. */
export const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
  npm: "adapters.npm",
  pipx: "adapters.pipx",
  uv: "adapters.uv",
  pip: "adapters.pip",
  cargo: "adapters.cargo",
  ollama: "adapters.ollama",
  "standalone-claude": "adapters.standalone-claude",
  "standalone-rustup": "adapters.standalone-rustup",
  "standalone-agy": "adapters.standalone-agy",
  "standalone-grok": "adapters.standalone-grok",
  "standalone-codex": "adapters.standalone-codex",
};

/** The adapter ids of the tools with their own installer, one per recipe
 *  in `recipes::RECIPES` (crates/banager-core/src/adapters/standalone/
 *  recipes.rs). A union so `STANDALONE_SUMMARY_KEYS` is a `Record` over
 *  it: a tool added here without a summary key there fails `tsc`. */
export type StandaloneAdapterId =
  | "standalone-claude"
  | "standalone-rustup"
  | "standalone-agy"
  | "standalone-grok"
  | "standalone-codex";

/**
 * One line per standalone tool, for the description slot of its rows on
 * the Installed and Updates pages (`toolDescription`): what the tool is.
 * Where it came from needs no saying: the row's avatar and name are the
 * tool's own, since a tool with its own installer is its own source.
 * `InstalledArtifact.description` is a bare string that cannot be
 * localised, so the standalone adapter's inventory leaves it `null` and
 * the line's i18n key is kept here by adapter id, with its text in both
 * locale files.
 */
export const STANDALONE_SUMMARY_KEYS: Record<StandaloneAdapterId, string> = {
  "standalone-claude": "standalone.summary.standalone-claude",
  "standalone-rustup": "standalone.summary.standalone-rustup",
  "standalone-agy": "standalone.summary.standalone-agy",
  "standalone-grok": "standalone.summary.standalone-grok",
  "standalone-codex": "codexStandalone.summary",
};

/**
 * The summary key for `adapterId`, or `null` for a source that is not a
 * standalone tool (or one this build has no line for).
 * `hasOwnProperty`, not truthiness: an id like "toString" finds a
 * function on the prototype, not a key.
 */
export function standaloneSummaryKey(adapterId: string): string | null {
  return Object.prototype.hasOwnProperty.call(STANDALONE_SUMMARY_KEYS, adapterId)
    ? STANDALONE_SUMMARY_KEYS[adapterId as StandaloneAdapterId]
    : null;
}

/**
 * What a row says a tool is when its source gave no description, and the
 * window has no line of its own for it (`DescribedTool.translated`), by
 * the source that lists it -- so no row reads 「暂无简介」/"No description".
 * Most sources never give one: npm's, pip's, pipx's, uv's, Cargo's and
 * Ollama's inventories leave `description` null, and so do some of
 * Homebrew's casks. Each line holds for everything its source can list:
 *
 * - Homebrew: a formula, or a cask that is not an app (a font, a driver),
 *   is a Homebrew package; a cask with an app is an app installed with
 *   Homebrew (`homebrewApp`, below).
 * - npm: whatever `npm ls -g` lists is an npm package -- not always a
 *   command-line tool, and not always installed with npm (npm itself and
 *   corepack come with Node).
 * - pip: a Python package, not "installed with pip": the packages that
 *   come with Python are listed too.
 * - pipx and uv: each lists only the tools it installed, and installs
 *   only a package with commands to run.
 * - Cargo: `.crates2.json` records what `cargo install` put in its `bin`
 *   folder, programs.
 * - Ollama: a model -- not "local": a cloud model is listed too.
 *
 * A source this build has no line for says who installed it.
 */
const FALLBACK_DESCRIPTION_KEYS: Record<string, string> = {
  brew: "toolRow.fallback.homebrewPackage",
  npm: "toolRow.fallback.npmPackage",
  pip: "toolRow.fallback.pythonPackage",
  pipx: "toolRow.fallback.pipxTool",
  uv: "toolRow.fallback.uvTool",
  cargo: "toolRow.fallback.cargoProgram",
  ollama: "toolRow.fallback.ollamaModel",
};

/** What `toolDescription` needs to know about the tool on a row. */
export interface DescribedTool {
  /** The source's own sentence, when it gave one. */
  description: string | null | undefined;
  /**
   * The tool's line in the window's language, where it has one
   * (`useTranslatedDescription`, src/lib/toolDescriptions.ts): while the
   * window is in Chinese, Chinese, translated from the description the
   * tool's source gives it; while it is in English, for an npm, PyPI or
   * crates.io package only, English, rewritten from the description its
   * registry gives it. Left out, or `null`, where there is none.
   */
  translated?: string | null;
  kind: ArtifactKind;
  /**
   * For a Homebrew cask, where its app is: set only for a cask with an
   * app (`parse_info_installed` in crates/banager-core/src/adapters/brew/
   * parse.rs).
   */
  path: string | null | undefined;
}

/** `text`, or `null` for none: `null`, `undefined` or empty. */
function nonEmpty(text: string | null | undefined): string | null {
  return text === null || text === undefined || text === "" ? null : text;
}

/**
 * A row's one line about what it is, on the Installed and the Updates
 * page alike, so a tool reads the same on both, and in the inspector: the
 * tool's line in the window's language (`translated`); else the source's
 * own description; else, for a standalone tool, its summary
 * (`standaloneSummaryKey`) -- which is in the window's language already,
 * in both, and so is never replaced by a translated line; else a line
 * from its source (`FALLBACK_DESCRIPTION_KEYS`). Never empty.
 *
 * One line, never two: where the window's line takes the place of the
 * source's own words, those words are not shown under it, not even in the
 * inspector. A window in Chinese that says what a tool is in Chinese and
 * then again in English reads as a translation left half done.
 *
 * `adapterId` is the instance's, or the part of the key's `instance_id`
 * before any `:` when the snapshot lacks the instance (the id starts with
 * its adapter's); `sourceLabel` is the source's name in the user's
 * language.
 */
export function toolDescription(
  t: Translate,
  tool: DescribedTool,
  adapterId: string,
  sourceLabel: string,
): string {
  const summaryKey = standaloneSummaryKey(adapterId);
  const translated = summaryKey === null ? nonEmpty(tool.translated) : null;
  if (translated !== null) return translated;
  const own = nonEmpty(tool.description);
  if (own !== null) return own;
  if (summaryKey !== null) return t(summaryKey);
  if (adapterId === "brew" && tool.kind === "Cask" && tool.path !== null && tool.path !== undefined) {
    return t("toolRow.fallback.homebrewApp");
  }
  return Object.prototype.hasOwnProperty.call(FALLBACK_DESCRIPTION_KEYS, adapterId)
    ? t(FALLBACK_DESCRIPTION_KEYS[adapterId])
    : t("toolRow.fallback.other", { source: sourceLabel });
}

/**
 * Whether Banager may offer operations on this source. The front-end
 * mirror of `ManagerInstance::writable()`; `Session::issue_plan` refuses
 * anything this would have hidden, so a stale snapshot can only ever cost
 * an error message, never an unintended command.
 *
 * `read_only_reason` replaced a hardcoded `READ_ONLY_ADAPTER_IDS = {"pip"}`
 * here. That list could only ever be right about pip, whose read-only-ness
 * is a property of the tool; npm's depends on where Node was installed on
 * *this* machine, which no list of adapter ids can know.
 */
export function canWrite(instance: ManagerInstance): boolean {
  return instance.read_only_reason === null;
}

/**
 * Whether Banager reached this source on the last refresh. The front-end
 * mirror of `ManagerInstance::available()`, and what `healthy: boolean`
 * used to be -- except the wire now also says *why*, which is what lets
 * the copy differ between "start it" and "it did not answer".
 */
export function isAvailable(instance: ManagerInstance): boolean {
  return instance.status.unavailable === null;
}

/**
 * Why each note, while it stands, makes the source refuse to plan an
 * uninstall -- the i18n key of the sentence that says so -- or null for a
 * note that refuses nothing. Homebrew is rewriting the list `brew uses`
 * reads (`IndexUpdating`), so its uninstall preview is refused rather than
 * shown with dependents it may have missed (`AdapterError::IndexUpdating`
 * in `plan`, crates/banager-core/src/adapters/brew/mod.rs). A list that
 * could not be downloaded is still read, and a launcher left without its
 * program is what Uninstall finishes. A `Record`, so a note added to
 * `InstanceNote` without an answer here fails `tsc`.
 */
const UNINSTALL_HOLD_KEYS: Record<InstanceNote, string | null> = {
  IndexMayBeStale: null,
  IndexUpdating: "installed.uninstallHold.IndexUpdating",
  NotOnPath: null,
  ShadowedByHomebrew: null,
  ShadowedByNpm: null,
  ShadowedByOther: null,
  LauncherOnly: null,
};

/**
 * Why `instance` refuses an uninstall until a note of its goes away
 * (`UNINSTALL_HOLD_KEYS`), or null when none does. The Installed page then
 * shows Uninstall disabled, with this why, rather than one the dialog
 * could only refuse; the core refreshes by itself when Homebrew's update
 * ends (`Session::background_change`), which clears the note and brings
 * the button back.
 */
export function uninstallHoldKey(instance: ManagerInstance): string | null {
  for (const note of instance.status.notes) {
    // A note this build does not know holds nothing: it has no entry.
    const key = UNINSTALL_HOLD_KEYS[note];
    if (key) return key;
  }
  return null;
}

/**
 * Something the notice offers to do about itself. An id, not a callback:
 * this module stays pure so every page can call it, and `SourceNotices`
 * wires the id to what carries it out -- `checkAgain` to the header's
 * Check again (`useCheckAgain`), `showTool` to the Installed page with
 * the source's tool selected (`useShowSourceTool`), `showList` to the
 * Installed page's 「显示」 popup.
 */
export type SourceNoticeActionId = "openOllama" | "checkAgain" | "showTool" | "showList";

/**
 * A notice's button, as data: which action, and its words. `showTool`
 * names the source whose tool it shows.
 */
export type SourceNoticeAction =
  | { id: "openOllama" | "checkAgain"; labelKey: string }
  | { id: "showTool"; labelKey: string; instanceId: string }
  /**
   * The Installed page's own lines over 所有工具 (`discoverNotices`,
   * src/lib/families.ts), never a source's: the list under the 「显示」
   * popup's choice `show`.
   */
  | { id: "showList"; labelKey: string; show: InstalledShow };

/**
 * One notice a source needs rendered, as data: which i18n keys say it,
 * what to interpolate into them, and what (if anything) the user can do
 * about it. No `t()` and no JSX, so the rule can be tested directly and,
 * more to the point, so every page answers from the same rule -- the two
 * list pages' notices used to be different code, which is how the Updates
 * page came to say "Everything is up to date" for a source it had not
 * managed to ask.
 *
 * Only what Banager found out about the source this time: not running,
 * not answering, a list it could not download, another copy that runs
 * instead. What a source lets Banager do at all -- pip, or an npm whose
 * folder the account cannot write, being read-only -- is each of its
 * rows' "View only" chip, on both lists (`READ_ONLY_DETAIL_KEYS`).
 */
export interface SourceNoticeSpec {
  /** Stable React key: one instance can need more than one notice. */
  id: string;
  variant: "info" | "warning";
  titleKey: string;
  descriptionKey: string;
  /** Interpolation values, already in the user's language; a `count` picks the plural. */
  values?: Record<string, string | number>;
  /** What the notice offers to do about itself, if anything. */
  action?: SourceNoticeAction;
}

/**
 * The word the user types to run a standalone tool: the file name of its
 * launcher (`~/.local/bin/claude` → `claude`). A standalone instance's
 * `exe_path` is the launcher, not the resolved binary
 * (`StandaloneAdapter::detect`). Falls back to the whole path for one
 * with no file name, which no adapter produces.
 */
function commandNameOf(instance: ManagerInstance): string {
  const name = instance.exe_path.split("/").pop();
  return name !== undefined && name.length > 0 ? name : instance.exe_path;
}

/**
 * Every notice `instance` needs, in the order they should be rendered:
 * whether it answered, then what it said about itself. `sourceLabel` is
 * the source's name as the user reads it -- resolved by the caller
 * through `ADAPTER_LABEL_KEYS`, because keeping `t()` out of here is what
 * makes this testable and shareable.
 *
 * `rowsOnScreen` is how many rows for this source the caller is about to
 * draw underneath the notice, and it changes one sentence: a source that
 * did not answer keeps its last known rows, and the copy that describes
 * them is a lie when there are none. That is not a corner case. The
 * snapshot is in memory only -- `Session::new` starts from
 * `Snapshot::empty()` and nothing is written to disk -- so on the first
 * refresh after every launch there is nothing to carry forward, and a
 * source whose CLI fails outright (cargo, when `cargo --version` does)
 * has nothing to carry forward ever. It defaults to 0, the copy that
 * claims nothing: a caller that has not counted must not promise rows.
 */
export function sourceNoticesFor(
  instance: ManagerInstance,
  sourceLabel: string,
  rowsOnScreen = 0,
): SourceNoticeSpec[] {
  const notices: SourceNoticeSpec[] = [];

  const unavailable = instance.status.unavailable;
  if (unavailable === "NotRunning") {
    // One state, one sentence, named through `sourceLabel` so it reads in
    // the user's language. Ollama is the only source Banager can start --
    // and `OllamaAdapter::detect` says NotRunning only when it can: the
    // daemon is on this Mac and Ollama.app is installed; otherwise it says
    // NotResponding -- so it is the only one whose notice carries a
    // button. It used to
    // get a second wording (`sourceNotice.ollamaNotRunning`) to go with the
    // button, and the refusal `issue_plan` returns for that very same
    // stopped Ollama kept using this one. Two sentences for one state; the
    // button is the only part that actually differs.
    notices.push({
      id: `${instance.id}:not-running`,
      variant: "warning",
      titleKey: "sourceNotice.notRunning.title",
      descriptionKey: "sourceNotice.notRunning.description",
      values: { source: sourceLabel },
      ...(instance.adapter_id === "ollama"
        ? { action: { id: "openOllama" as const, labelKey: "sourceNotice.openOllama" } }
        : {}),
    });
  } else if (unavailable === "NotResponding") {
    notices.push({
      id: `${instance.id}:unreachable`,
      variant: "warning",
      titleKey: "sourceNotice.unreachable.title",
      // Two sentences for one state, chosen by what is actually on screen.
      // Each ends with the one next step there is, the one its rows' chips
      // give too: check again later -- with the notice's own Check again
      // there to do it, so the sentence need not say which button. Nothing
      // more: the way out they used to offer ("Reopening Banager usually
      // fixes this") is simply wrong for a source that will fail the same
      // way on the next launch, and promising a recovery that may not
      // happen is the pattern this phase exists to remove.
      descriptionKey:
        rowsOnScreen > 0
          ? "sourceNotice.unreachable.descriptionWithRows"
          : // Under its title, which already names the source; the
            // self-contained sentence is the refusal's
            // (`notActionableMessage`).
            "sourceNotice.unreachable.detail",
      values: { source: sourceLabel },
      action: { id: "checkAgain", labelKey: "header.checkAgain" },
    });
  } else if (unavailable === "RefusesAsRoot") {
    // Its own copy because its own action: Banager was started with
    // `sudo`, Homebrew will not run that way, and the way out is to quit
    // and open Banager again normally. No button -- Banager cannot
    // relaunch itself out from under root, and offering to do what it
    // cannot is the pattern this phase exists to remove.
    notices.push({
      id: `${instance.id}:refuses-as-root`,
      variant: "warning",
      titleKey: "sourceNotice.refusesAsRoot.title",
      descriptionKey: "sourceNotice.refusesAsRoot.description",
      values: { source: sourceLabel },
    });
  }

  for (const note of instance.status.notes) {
    if (note === "IndexMayBeStale") {
      notices.push({
        id: `${instance.id}:index-may-be-stale`,
        variant: "warning",
        titleKey: "sourceNotice.indexMayBeStale.title",
        descriptionKey: "sourceNotice.indexMayBeStale.description",
        // The header's own words for the same check: one name for every
        // button that runs it.
        action: { id: "checkAgain", labelKey: "header.checkAgain" },
      });
    } else if (note === "IndexUpdating") {
      // Nothing has failed: the download is still going. So an "info"
      // notice, and no button -- there is nothing for the user to do, and
      // a Check again here could only wait on the same download. The core
      // refreshes by itself when it ends (`Session::background_change`),
      // which is what clears this.
      notices.push({
        id: `${instance.id}:index-updating`,
        variant: "info",
        titleKey: "sourceNotice.indexUpdating.title",
        descriptionKey: "sourceNotice.indexUpdating.description",
      });
    } else if (note === "NotOnPath") {
      // The four PATH notes of a standalone tool (spec §七): typing its
      // name may not run this copy, because no executable with that name
      // on PATH is this copy, or because another program with that name
      // comes first.
      // Info, not warning: the install works, the user just needs to know
      // what typing its name does. `{{command}}` is the launcher's file
      // name -- the word the user types -- not its path, which this
      // app's audience would not recognise. Plain text: `SourceNotice`
      // renders `t(key, values)`, never `withCommand`.
      notices.push({
        id: `${instance.id}:not-on-path`,
        variant: "info",
        titleKey: "sourceNotice.notOnPath.title",
        descriptionKey: "sourceNotice.notOnPath.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "ShadowedByHomebrew") {
      notices.push({
        id: `${instance.id}:shadowed-by-homebrew`,
        variant: "info",
        titleKey: "sourceNotice.shadowedByHomebrew.title",
        descriptionKey: "sourceNotice.shadowedByHomebrew.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "ShadowedByNpm") {
      notices.push({
        id: `${instance.id}:shadowed-by-npm`,
        variant: "info",
        titleKey: "sourceNotice.shadowedByNpm.title",
        descriptionKey: "sourceNotice.shadowedByNpm.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "ShadowedByOther") {
      notices.push({
        id: `${instance.id}:shadowed-by-other`,
        variant: "info",
        titleKey: "sourceNotice.shadowedByOther.title",
        descriptionKey: "sourceNotice.shadowedByOther.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "LauncherOnly") {
      // The half-uninstalled state: this launcher cannot run, so a
      // warning. Of its three ways out, reinstalling and putting the files
      // back from the Trash are done outside the app; the one done in it is
      // the tool's own Uninstall, which finishes the job -- its preview
      // lists the program directory as already gone and moves the link
      // (spec §3.3, §6.2). So the notice's button shows the tool on the
      // Installed page, selected, its Uninstall… in the inspector; Check
      // again, the last step after a reinstall, is the toolbar's ⟳.
      notices.push({
        id: `${instance.id}:launcher-only`,
        variant: "warning",
        titleKey: "sourceNotice.launcherOnly.title",
        descriptionKey: "sourceNotice.launcherOnly.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
        action: { id: "showTool", labelKey: "sourceNotice.showTool", instanceId: instance.id },
      });
    } else {
      const unhandled: never = note;
      void unhandled;
    }
  }

  return notices;
}

/**
 * Whether this source has a notice for the pages to show -- exactly
 * `sourceNoticesFor(...).length > 0`, expressed that way so the two can
 * never drift.
 *
 * `nothingFound` asks this for `SnapshotStatus` and the Overview: the
 * zero-artifact empty state replaces the page outright, so without it a
 * Mac whose only source is a stopped Ollama shows "Banager found nothing
 * installed" and the Open Ollama button is unreachable. A read-only
 * source with nothing installed has nothing to show -- its notice is its
 * rows' chip -- so it gets the empty state like any other. One rule, one
 * place.
 */
export function hasSourceNotice(instance: ManagerInstance): boolean {
  return sourceNoticesFor(instance, "").length > 0;
}

/**
 * The command that releases the pin on `key`, for the user to run
 * themselves: Banager does not unpin, which would be a new write
 * operation. Both `UPDATE_BLOCKED_KEYS.Pinned` and
 * `UNINSTALL_BLOCKED_KEYS.Pinned` build their command with this. Their
 * producers are brew's `parse_outdated` and `parse_info_installed`
 * (crates/banager-core/src/adapters/brew/parse.rs) and pipx's
 * `parse_outdated` (adapters/pipx.rs), so the command is built for
 * whichever tool owns the key. The tool is the instance's `adapter_id`; for an instance
 * the snapshot lacks, the part of the key's `instance_id` before any
 * `:`, which is the adapter id (`instance_id` in
 * crates/banager-core/src/model.rs writes it first and asserts it has
 * no `:` of its own).
 *
 * pipx: `pipx unpin <name>`. That is how pipx itself spells it when it
 * refuses a pinned upgrade ("Run `pipx unpin {result.environment}` to
 * unpin it.", pipx 1.17.3's `commands/upgrade.py:473`), and `unpin`
 * takes one positional ENVIRONMENT and nothing else (`_add_unpin`,
 * `main.py:969-978`). The name is the one `parse_outdated` read off
 * the line, and pipx canonicalizes it into the venv's directory name
 * (`_venv_dir`, `main.py:1752-1753`; `get_venv_dir`, `venv.py:142-144`).
 * The program is the instance's `exe_path`, the pipx Banager found on
 * its own PATH (`resolve_exe` in `PipxAdapter::detect`), which
 * Terminal's PATH may not include.
 *
 * brew: `--cask` because `brew unpin <name>` resolves a formula first
 * (`to_resolved_formulae_to_casks` in Homebrew's `cmd/unpin.rb`), and
 * a formula can share a cask's name.
 *
 * The program is the instance's `exe_path`, the absolute path of the
 * brew that owns this package, not a bare `brew`: Banager finds brew
 * by absolute path (`CANDIDATE_PATHS` in
 * crates/banager-core/src/adapters/brew/mod.rs) and lists
 * /opt/homebrew and /usr/local side by side, while Terminal's `brew`
 * is whichever one PATH finds first, or none. On a Mac migrated from
 * Intel, a formula pinned in /usr/local would get "not pinned" from
 * /opt/homebrew/bin/brew and the row would say Pinned for good.
 * `/usr/local/bin/brew unpin` runs on Apple silicon: Homebrew refuses
 * that prefix only in `perform_preinstall_checks` (its
 * `Library/Homebrew/install.rb`), which `unpin` never calls. Every
 * token goes through `displayToken`, as `CommandPreview` does, so a
 * path with a space in it still pastes as one argument.
 */
export function unpinCommand(key: ArtifactKey, instance: ManagerInstance | undefined): string {
  const adapterId = instance?.adapter_id ?? key.instance_id.split(":")[0];
  // The bare program name only for an instance the snapshot does not
  // have, which `UpdateBlockedCopy.command`'s doc says cannot happen.
  if (adapterId === "pipx") {
    return [instance?.exe_path ?? "pipx", "unpin", key.name].map(displayToken).join(" ");
  }
  const program = instance?.exe_path ?? "brew";
  const args = key.kind === "Cask" ? ["unpin", "--cask", key.name] : ["unpin", key.name];
  return [program, ...args].map(displayToken).join(" ");
}

/**
 * The command that opens a `SelfUpdatesOnly` row's tool once: the tool
 * itself -- its launcher, which is the standalone instance's `exe_path`
 * (`StandaloneAdapter::detect`) -- with no arguments. Opening it is what
 * makes it check for updates, while its automatic updates are on (spec
 * §4.4); `<launcher> --version` would not (agy 1.2.10 never reaches its
 * updater from `--version`, spec §3.4). Quoted by `displayToken` when the
 * path has a space, like the unpin commands. The bare name when the
 * snapshot lacks the instance, which `refresh` never produces.
 */
function launcherCommand(key: ArtifactKey, instance: ManagerInstance | undefined): string {
  return displayToken(instance?.exe_path ?? key.name);
}

/** What `UPDATE_BLOCKED_KEYS` holds for one reason. */
interface UpdateBlockedCopy {
  /** The row's status chip on the Updates page, where "Update" would be. */
  badge: string;
  /**
   * The chip's detail: why there is no Update button, and what the user
   * can do instead, in at most two short sentences. The Updates page
   * fills `{{source}}` with the owning source's label (`sourceLabelFor`)
   * and, where the sentence has one, `{{command}}` with `command` below,
   * set as code (`withCommand` in src/components/withCommand.tsx), so one
   * sentence serves every tool that produces the reason. It promises
   * nothing about when the update will be offered, so it holds as well
   * for a row whose source did not answer the last check -- which keeps
   * its chip and gets no button until the source answers again
   * (`updateStateOf` in src/lib/updateState.ts checks `blocked` before
   * `sourceUnavailable`) -- and for an app that updates itself.
   */
  detail: string;
  /**
   * Whether `detail` sets the command into its sentence, or leaves it out
   * of the sentence: then the command is a line of its own under it, only
   * while Settings' "Show technical details" is on.
   */
  commandInDetail: boolean;
  /** The command, from the row's own key and the instance that key's
   *  `instance_id` names (`undefined` only if the snapshot lacks it, which
   *  `refresh` never produces: it builds `updates` only from instances it
   *  also puts in `instances`). Also what the row's "Copy command" copies. */
  command: (key: ArtifactKey, instance: ManagerInstance | undefined) => string;
  /** `planErrorMessage`'s sentence for the gate's `update_blocked`
   *  refusal, which only a stale Updates page can reach. It is given only
   *  the source's label (`planErrorMessage`'s `sourceLabel`), not the
   *  package, so it must hold for a self-updating one too. */
  refused: string;
}

/**
 * The copy for each reason the tool will refuse to update one package
 * (`UpdateCandidate.blocked`). A `Record` over the whole `UpdateBlocked`
 * union: a variant added there without copy here fails `tsc`, where a
 * `switch` with a default branch would render nothing and compile.
 */
export const UPDATE_BLOCKED_KEYS: Record<UpdateBlocked, UpdateBlockedCopy> = {
  Pinned: {
    badge: "updates.blocked.Pinned.badge",
    // "It's pinned in {{source}}. To update it, first run {{command}} in
    // Terminal." `{{source}}`, not Homebrew: pipx pins too. It says what
    // stands in the way and what removes it, and nothing about when
    // Banager will offer the update or whether the package stays where
    // it is -- a pinned cask that updates itself may move anyway (`brew
    // pin` warns of it, Homebrew's `cmd/pin.rb`), and a row whose source
    // did not answer is offered nothing until it does.
    detail: "updates.blocked.Pinned.detail",
    commandInDetail: true,
    command: unpinCommand,
    refused: "updates.blocked.Pinned.refused",
  },
  SelfUpdatesOnly: {
    badge: "updates.blocked.SelfUpdatesOnly.badge",
    // The tool installs its updates itself (agy: when it starts, at most
    // every 15 minutes, agy.md §4) and offers no command Banager may run,
    // so the detail says what does work: open it once. The launcher that
    // opens it is a path, which is a technical detail: it is shown under
    // the sentence only while "Show technical details" is on.
    detail: "updates.blocked.SelfUpdatesOnly.detail",
    commandInDetail: false,
    command: launcherCommand,
    refused: "updates.blocked.SelfUpdatesOnly.refused",
  },
};

/**
 * A read-only source's rows, in the detail of their "View only" chip on
 * both lists: two short sentences, different for each reason, and the
 * difference matters -- pip cannot be driven at all, so the way out is to
 * install Python tools with pipx or uv; an npm whose prefix is root-owned
 * works fine, so the way out is a Node installed with Homebrew. Telling
 * the npm user about pipx -- which the Updates page did for every
 * read-only row before the wire carried a reason -- sends someone who
 * does not write code to install a Python tool to fix their JavaScript
 * packages. The very sentences a refusal for that source says
 * (`notActionableMessage`), so the chip and the refusal cannot disagree:
 * npm's promises to manage only the npm packages installed with a Node
 * from Homebrew, since the ones in the old folder do not move over.
 */
export const READ_ONLY_DETAIL_KEYS: Record<ReadOnlyReason, string> = {
  ByDesign: "sourceNotice.pipReadOnly.description",
  PrefixNotWritable: "sourceNotice.prefixNotWritable.description",
};

/**
 * A row whose source did not answer the last check, in the detail of its
 * "Can't update now" chip on both lists, and of the Installed page's
 * "Can't uninstall now": what is wrong and what to do, per reason, because
 * "check again later" is no help for an Ollama that is not running or a
 * Banager started with `sudo`. The page fills `{{source}}`.
 */
export const UNAVAILABLE_DETAIL_KEYS: Record<Unavailable, string> = {
  NotRunning: "updates.unavailableDetail.NotRunning",
  NotResponding: "updates.unavailableDetail.NotResponding",
  RefusesAsRoot: "updates.unavailableDetail.RefusesAsRoot",
};

/**
 * Reads the `{"kind": "update_blocked", "reason": ...}` payload
 * `update_blocked_json` in src-tauri/src/ipc.rs sends when
 * `Session::issue_plan` or `Session::submit` refuses to update one package.
 * `null` for anything else, including a reason this build has no copy for
 * -- which `planErrorMessage` then does not guess at: it is the backend's
 * own words, shown with "Show technical details" on.
 */
export function parseUpdateBlocked(message: string): UpdateBlocked | null {
  const p = parseErrorPayload(message);
  if (!p || p.kind !== "update_blocked" || typeof p.reason !== "string") return null;
  return Object.prototype.hasOwnProperty.call(UPDATE_BLOCKED_KEYS, p.reason)
    ? (p.reason as UpdateBlocked)
    : null;
}

/** What `UNINSTALL_BLOCKED_KEYS` holds for one reason. */
interface UninstallBlockedCopy {
  /** The Installed page row's chip: 「已固定」, 「需手动卸载」. */
  badge: string;
  /** The chip's detail, and what the row's drawer says under it: why
   *  there is no Uninstall button, and what the user can do about it, in
   *  at most two sentences. The page fills `{{source}}` with the owning
   *  source's label and `{{command}}` with `command` below, rendered as
   *  code (`withCommand` in src/components/withCommand.tsx). It promises
   *  nothing about when a button will be offered, so it holds as well for
   *  a row whose source did not answer the last check -- which gets no
   *  Uninstall button until the source answers again, whatever the user
   *  does about the reason. */
  description: string;
  /** The command the sentences' `{{command}}` stands for. */
  command: (key: ArtifactKey, instance: ManagerInstance | undefined) => string;
  /** The uninstall dialog's sentence for the gate's `uninstall_blocked`
   *  refusal, which only a stale Installed page can reach. Filled the same
   *  way as `description`. */
  refused: string;
}

/**
 * The copy for each reason the tool will refuse to uninstall one package
 * (`InstalledArtifact.uninstall_blocked`). A `Record` over the whole
 * `UninstallBlocked` union, so a variant added there without copy here
 * fails `tsc`.
 */
export const UNINSTALL_BLOCKED_KEYS: Record<UninstallBlocked, UninstallBlockedCopy> = {
  Pinned: {
    // The same word the Updates page's pinned row uses, so a package that
    // is pinned reads "Pinned" on both pages.
    badge: "updates.blocked.Pinned.badge",
    // "It's pinned in {{source}}. To uninstall it, first run {{command}}
    // in Terminal." What stands in the way and what removes it, and
    // nothing about when Uninstall comes back: that needs a check that
    // finds Homebrew answering, which a pin's sentence cannot promise.
    description: "installed.blocked.Pinned.description",
    // `UninstallBlocked::Pinned`'s only producer is brew
    // (`parse_info_installed`), so this is always `brew unpin`, built from
    // the owning instance's `exe_path`, `--cask` for a cask.
    command: unpinCommand,
    refused: "installed.blocked.Pinned.refused",
  },
  NoSafeMethod: {
    badge: "installed.blocked.NoSafeMethod.badge",
    // No command: unlike a pin there is nothing the user can run to make
    // Banager able to uninstall it, so the sentence has no `{{command}}`
    // slot and `withCommand` returns it as plain text.
    description: "installed.blocked.NoSafeMethod.description",
    command: () => "",
    refused: "installed.blocked.NoSafeMethod.refused",
  },
  UvToolDirSet: {
    badge: "installed.blocked.UvToolDirSet.badge",
    // Why, and when uv does it -- only when no other uv tool is left --
    // with no command set apart to copy and no pointer to Terminal:
    // `uv tool uninstall` is the very command that deletes the folder
    // above the tools folder, run for the last tool in a Terminal that
    // sets `UV_TOOL_DIR` too.
    description: "installed.blocked.UvToolDirSet.description",
    command: () => "",
    refused: "installed.blocked.UvToolDirSet.refused",
  },
};

/**
 * One source's own words for a reason, where B's sentence would be false
 * of it. rustup's row carries `NoSafeMethod` when Rust is not in its
 * standard folders (`rustup::uninstall_blocked` in
 * crates/banager-core/src/adapters/standalone/rustup.rs) -- not because it
 * has no uninstall command, which is what `UNINSTALL_BLOCKED_KEYS`'s
 * sentence says. Keyed by adapter id, then reason; a missing entry means
 * B's copy. Literal keys, so `completeness.test.ts` finds each one.
 */
const UNINSTALL_BLOCKED_OVERRIDES: Partial<
  Record<StandaloneAdapterId, Partial<Record<UninstallBlocked, UninstallBlockedCopy>>>
> = {
  "standalone-rustup": {
    NoSafeMethod: {
      badge: "installed.blocked.NoSafeMethod.badge",
      description: "installed.blocked.NoSafeMethod.standalone-rustup.description",
      command: () => "",
      refused: "installed.blocked.NoSafeMethod.standalone-rustup.refused",
    },
  },
  // Codex's own install is listed only: whether it has an uninstall
  // command was not looked into, so B's "has no uninstall command" would
  // be a claim; what is true is that this build does not remove it (D5).
  "standalone-codex": {
    NoSafeMethod: {
      badge: "installed.blocked.NoSafeMethod.badge",
      description: "codexStandalone.uninstallDescription",
      command: () => "",
      refused: "installed.blocked.NoSafeMethod.refused",
    },
  },
};

/**
 * The copy for `reason` on a row of `adapterId`: the source's own words
 * where it has them (`UNINSTALL_BLOCKED_OVERRIDES`), else
 * `UNINSTALL_BLOCKED_KEYS`. `adapterId` is the instance's `adapter_id`
 * (the Installed page has it; the uninstall dialog finds the instance in
 * the snapshot, and passes `undefined` when it cannot).
 */
export function uninstallBlockedCopy(
  reason: UninstallBlocked,
  adapterId: string | undefined,
): UninstallBlockedCopy {
  const overrides =
    adapterId !== undefined && Object.prototype.hasOwnProperty.call(UNINSTALL_BLOCKED_OVERRIDES, adapterId)
      ? UNINSTALL_BLOCKED_OVERRIDES[adapterId as StandaloneAdapterId]
      : undefined;
  return overrides?.[reason] ?? UNINSTALL_BLOCKED_KEYS[reason];
}

/**
 * Reads the `{"kind": "uninstall_blocked", "reason": ...}` payload
 * `uninstall_blocked_json` in src-tauri/src/ipc.rs sends when
 * `Session::issue_plan` or `Session::submit` refuses to uninstall one
 * package. `null` for anything else, including a reason this build has no
 * copy for.
 *
 * Only the uninstall dialog (src/components/UninstallDialog.tsx) can
 * receive this payload, because `blocked_uninstall` in
 * crates/banager-core/src/session/plans.rs refuses nothing but an
 * `Uninstall`. It reads it before `planErrorMessage`, because its sentence
 * carries the unpin command as code, and a plain string cannot.
 */
export function parseUninstallBlocked(message: string): UninstallBlocked | null {
  const p = parseErrorPayload(message);
  if (!p || p.kind !== "uninstall_blocked" || typeof p.reason !== "string") return null;
  return Object.prototype.hasOwnProperty.call(UNINSTALL_BLOCKED_KEYS, p.reason)
    ? (p.reason as UninstallBlocked)
    : null;
}

/**
 * The reasons a path-list uninstall preview can be refused by one of its
 * checks (`removal::plan_removal` in
 * crates/banager-core/src/adapters/standalone/removal.rs), as
 * `plan_operation_error` in src-tauri/src/ipc.rs spells them -- by hand,
 * in snake_case, one `match` arm each. Mirrored here as a union so the
 * copy table below is a `Record` over it: a reason without a sentence
 * fails `tsc`.
 */
export type UninstallUnsafeReason =
  | "outside_home"
  | "shared_folder"
  | "missing"
  | "not_owned_by_you"
  | "not_what_instructions_expect"
  | "overlaps_kept";

/** The `planRefused.uninstallUnsafe.*` sentence for each reason; each
 *  interpolates `{{path}}` (home folder abbreviated on the Rust side). */
export const UNINSTALL_UNSAFE_KEYS: Record<UninstallUnsafeReason, string> = {
  outside_home: "planRefused.uninstallUnsafe.outsideHome",
  shared_folder: "planRefused.uninstallUnsafe.sharedFolder",
  missing: "planRefused.uninstallUnsafe.missing",
  not_owned_by_you: "planRefused.uninstallUnsafe.notOwnedByYou",
  not_what_instructions_expect: "planRefused.uninstallUnsafe.notWhatInstructionsExpect",
  overlaps_kept: "planRefused.uninstallUnsafe.overlapsKept",
};

/**
 * Reads the `{"kind":"uninstall_unsafe","path":…,"reason":…}` payload
 * `plan_operation_error` sends when a path-list uninstall preview refused
 * one of its checks. `null` for anything else, including a reason this
 * build has no copy for or a payload without its path, which
 * `planErrorMessage` then does not guess at: it is the backend's own
 * words, shown with "Show technical details" on.
 */
export function parseUninstallUnsafe(
  message: string,
): { path: string; reason: UninstallUnsafeReason } | null {
  const p = parseErrorPayload(message);
  if (!p || p.kind !== "uninstall_unsafe") return null;
  if (typeof p.path !== "string" || typeof p.reason !== "string") return null;
  if (!Object.prototype.hasOwnProperty.call(UNINSTALL_UNSAFE_KEYS, p.reason)) return null;
  return { path: p.path, reason: p.reason as UninstallUnsafeReason };
}

/** What `Session::issue_plan`'s actionability gate (spec §2.5) refused, as
 *  `plan_operation_error` in src-tauri/src/ipc.rs put it on the wire. */
export interface NotActionableReason {
  read_only: ReadOnlyReason | null;
  unavailable: Unavailable | null;
}

/**
 * Reads the JSON `plan_operation_error` puts on the wire for the one
 * refusal a stale snapshot or a genuine TOCTOU can surface to a real
 * person -- both `InstalledPage` and `UpdatesPage` hide every control for
 * an instance that fails this gate, so it should not normally be reachable
 * at all. Returns `null` for every other backend error, including the
 * other `kind`s `submit_operation_error` and `plan_operation_error` send
 * (`source_gone`, `expired`, `unknown`, `refused`, `spawn_failed`, ...),
 * each read further down.
 */
export function parseNotActionable(message: string): NotActionableReason | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(message);
  } catch {
    return null;
  }
  if (
    typeof parsed !== "object" ||
    parsed === null ||
    (parsed as Record<string, unknown>).kind !== "not_actionable"
  ) {
    return null;
  }
  const p = parsed as { read_only?: ReadOnlyReason | null; unavailable?: Unavailable | null };
  return {
    read_only: p.read_only ?? null,
    unavailable: p.unavailable ?? null,
  };
}

/** Whatever `useTranslation()`'s `t` needs to look a key up; kept minimal,
 *  same convention as `Translate` in src/lib/warnings.ts. */
type Translate = (key: string, options?: Record<string, string>) => string;

/**
 * The adapter an instance id names: its adapter's id, alone or before a
 * `:` and where the instance is (`instance_id` in
 * crates/banager-core/src/model.rs). For naming a source the snapshot has
 * lost.
 */
export function adapterIdOf(instanceId: string): string {
  return instanceId.split(":")[0];
}

/** A source's name in the user's language, by its adapter id; the id itself for one this build has no name for. */
export function adapterLabel(t: Translate, adapterId: string): string {
  return Object.prototype.hasOwnProperty.call(ADAPTER_LABEL_KEYS, adapterId)
    ? t(ADAPTER_LABEL_KEYS[adapterId])
    : adapterId;
}

/** `path` with the home folder as `~`, as Finder and Terminal show one: the front end has no `HOME`, and a Mac's is `/Users/<name>`. */
function withHomeAsTilde(path: string): string {
  return path.replace(/^\/Users\/[^/]+(?=\/|$)/, "~");
}

/**
 * Folders whose name says nothing about which copy of a tool lives under
 * them: a place is named by the last part of its path that is not one of
 * these (`placeName`). Lower case; compared case aside.
 */
const PLACELESS_FOLDERS = new Set(["usr", "local", "opt", "bin", "sbin", "lib", "libexec", "share", "library"]);

/**
 * The last part of `path` that tells a place apart -- `homebrew` for
 * `~/homebrew`, `linuxbrew` for `/home/linuxbrew/.linuxbrew`, `cargo` for
 * `~/.cargo` -- without a hidden folder's leading dot; null for a path
 * made only of folders every Mac has (`/usr/local`).
 */
function lastTellingPart(path: string): string | null {
  const parts = path.split("/");
  for (let index = parts.length - 1; index >= 0; index--) {
    const part = parts[index].replace(/^\.+/, "");
    if (part !== "" && part !== "~" && !PLACELESS_FOLDERS.has(part.toLowerCase())) return part;
  }
  return null;
}

/** Where Homebrew installs itself on each kind of Mac, by the name a person knows the Mac by. */
const HOMEBREW_PLACE_KEYS: Record<string, string> = {
  "/opt/homebrew": "common.place.appleSilicon",
  "/usr/local": "common.place.intel",
};

/**
 * A source's place in the words a person knows it by, not its path: a
 * Homebrew in /opt/homebrew is Apple silicon's (「Apple芯片」) and one in
 * /usr/local an Intel Mac's (「Intel」) -- the one Migration Assistant
 * carries over -- and anything else is named by the last telling part of
 * its prefix (`lastTellingPart`), or else by the whole prefix.
 */
function placeName(t: Translate, instance: ManagerInstance): string {
  const prefix = instance.prefix.replace(/(?<=.)\/+$/, "");
  if (instance.adapter_id === "brew" && Object.prototype.hasOwnProperty.call(HOMEBREW_PLACE_KEYS, prefix)) {
    return t(HOMEBREW_PLACE_KEYS[prefix]);
  }
  const shown = withHomeAsTilde(prefix);
  return lastTellingPart(shown) ?? shown;
}

/**
 * Where each of `group` -- sources of one kind -- is, in words that tell
 * them apart: the name of its place (`placeName`: 「Apple芯片」,
 * 「Intel」, `homebrew`); should two share one, its whole prefix
 * (`/opt/homebrew`, `~/homebrew`); should two share that, the program's
 * own path; or else the rest of its id, which is unique (`instance_id` in
 * crates/banager-core/src/model.rs).
 */
function placesOf(t: Translate, group: readonly ManagerInstance[]): string[] {
  const distinct = (places: string[]) => new Set(places).size === places.length;
  const named = group.map((instance) => placeName(t, instance));
  if (distinct(named)) return named;
  const prefixes = group.map((instance) => withHomeAsTilde(instance.prefix));
  if (distinct(prefixes)) return prefixes;
  const programs = group.map((instance) => withHomeAsTilde(instance.exe_path));
  if (distinct(programs)) return programs;
  return group.map((instance) => instance.id.slice(instance.id.indexOf(":") + 1) || instance.id);
}

/**
 * Each source's name, by instance id: the sidebar's row for it, the
 * Installed page's title while it shows that source alone, the words that
 * page's headings, rows and notices name it by, and the Overview's lines.
 * Its kind's name -- and, where this Mac has two sources of one kind, a
 * Homebrew in /opt/homebrew and one left in /usr/local by an Intel Mac,
 * which of the two it is after it: 「Homebrew（Apple芯片）」,
 * 「Homebrew（Intel）」 (spec R8, `placesOf`), so that the two are never two
 * rows of one name, as Mail tells two accounts' Inboxes apart. The only
 * one of its kind is its kind's name alone: 「Homebrew」.
 */
export function instanceLabels(t: Translate, instances: readonly ManagerInstance[]): Map<string, string> {
  const labels = new Map<string, string>();
  for (const [id, { source, place }] of instanceNames(t, instances)) {
    labels.set(id, place === null ? source : t("common.sourceWithPlace", { source, place }));
  }
  return labels;
}

/**
 * `instanceLabels`' two parts, by instance id: the kind's name, and where
 * the source is -- null for the only one of its kind. The sidebar sets
 * them apart, the place quieter after the name, where the whole would be
 * cut short in its width.
 */
export function instanceNames(
  t: Translate,
  instances: readonly ManagerInstance[],
): Map<string, { source: string; place: string | null }> {
  const byKind = new Map<string, ManagerInstance[]>();
  for (const instance of instances) {
    const group = byKind.get(instance.adapter_id) ?? [];
    group.push(instance);
    byKind.set(instance.adapter_id, group);
  }
  const names = new Map<string, { source: string; place: string | null }>();
  for (const [adapterId, group] of byKind) {
    const source = adapterLabel(t, adapterId);
    const places = group.length === 1 ? null : placesOf(t, group);
    group.forEach((instance, index) => names.set(instance.id, { source, place: places?.[index] ?? null }));
  }
  return names;
}

/**
 * The first warning this source's notices give (`sourceNoticesFor`) --
 * not running, not answering, a list it could not download, a launcher
 * with no program -- or null: what the sidebar marks its row with ⚠︎ for
 * (spec §3.1), in the words the notice says it in. News that is no
 * problem -- which copy runs, a list being updated -- gets no ⚠︎.
 */
export function sourceWarningOf(
  instance: ManagerInstance,
  sourceLabel: string,
  rowsOnScreen: number,
): SourceNoticeSpec | null {
  return sourceNoticesFor(instance, sourceLabel, rowsOnScreen).find((notice) => notice.variant === "warning") ?? null;
}

/**
 * `parseNotActionable`'s result, in the exact copy the source's rows and
 * notice already use for each reason (`READ_ONLY_DETAIL_KEYS`,
 * `sourceNotice.notRunning`, `sourceNotice.unreachable`,
 * `sourceNotice.refusesAsRoot`) -- so this refusal never reads as a raw
 * Rust enum. `sourceLabel` is the adapter's
 * name in the user's language, exactly as `sourceNoticesFor` takes it.
 *
 * Both halves can be set at once (a read-only source can also be silent),
 * so both parts are joined when present. The read-only copy needs no
 * source name (spec §7's wording is self-contained); the state copy
 * always names one, same as `sourceNoticesFor`.
 */
export function notActionableMessage(
  t: Translate,
  reason: NotActionableReason,
  sourceLabel: string,
): string {
  const parts: string[] = [];
  if (reason.read_only !== null) {
    parts.push(t(READ_ONLY_DETAIL_KEYS[reason.read_only]));
  }
  if (reason.unavailable === "NotRunning") {
    parts.push(t("sourceNotice.notRunning.description", { source: sourceLabel }));
  } else if (reason.unavailable === "NotResponding") {
    // The self-contained half of the pair: a refusal has no rows under it,
    // so the sentence about what is listed would make no sense here.
    parts.push(t("sourceNotice.unreachable.description", { source: sourceLabel }));
  } else if (reason.unavailable === "RefusesAsRoot") {
    parts.push(t("sourceNotice.refusesAsRoot.description", { source: sourceLabel }));
  }
  return parts.join(" ");
}

/**
 * What a `plan_operation`/`submit_operation` rejection should read as:
 * `notActionableMessage` when `raw` is the actionability gate's JSON
 * payload, the matching `planRefused.*` copy for every other structured
 * kind `submit_operation_error` and `plan_operation_error` send, otherwise
 * `raw` verbatim. Every call site that renders a plan or submit error
 * (`UpdateConfirm`, `UninstallDialog`) goes through this instead of showing
 * the backend's string directly, so no refusal that can reach a real
 * person is ever a raw Rust `{:?}` or this project's own English reaching
 * someone reading Banager in another language.
 *
 * `raw` verbatim, and another program's words quoted in a sentence (a
 * tool that would not start: macOS's reason), only with "Show technical
 * details" on (`technical`), as every raw error. Without it, a `raw` that
 * `failureCause` reads is said in a person's words, and any other is
 * null: the caller's sentence then says what happened without it
 * (`refusalSentence`).
 */
export function planErrorMessage(
  t: Translate,
  raw: string,
  sourceLabel: string,
  technical: boolean,
): string | null {
  const reason = parseNotActionable(raw);
  if (reason) return notActionableMessage(t, reason, sourceLabel);
  const blocked = parseUpdateBlocked(raw);
  if (blocked) return t(UPDATE_BLOCKED_KEYS[blocked].refused, { source: sourceLabel });
  // No `source` interpolation on purpose: the instance is gone from the
  // snapshot, so the caller's `sourceLabel` has fallen back to the raw
  // instance id ("brew:/opt/homebrew"), which is the kind of string this
  // whole function exists to keep off the screen.
  if (isSourceGone(raw)) return t("planRefused.sourceGone");
  if (isExpired(raw)) return t("planRefused.expired");
  if (isUnknownPlan(raw)) return t("planRefused.unknown");
  const failure = planFailureMessage(t, raw, sourceLabel, technical);
  if (failure !== null) return failure;
  if (technical) return raw;
  const cause = failureCause(raw);
  return cause === null ? null : t(FAILURE_CAUSE_KEYS[cause].line);
}

/**
 * Each sentence a plan or submit refusal is said in, and the same
 * sentence without the refusal's words (`refusalSentence`): what did not
 * happen and, where waiting can help, to try again later. An uninstall
 * that did not start says nothing more: the dialog is already checking
 * again, and says so, with Uninstall to press once more.
 */
const REFUSAL_PLAIN_KEYS = {
  "updates.planFailed": "updates.planFailedPlain",
  "updates.submitFailed": "updates.submitFailedPlain",
  "uninstall.planError": "uninstall.planErrorPlain",
  "uninstall.submitError": "uninstall.submitErrorPlain",
} as const;

/** A sentence a plan or submit refusal is said in (`refusalSentence`). */
export type RefusalFrame = keyof typeof REFUSAL_PLAIN_KEYS;

/**
 * A plan or submit refusal as the screen says it: `frame` -- 「无法准备此次
 * 更新：…」 -- around `planErrorMessage`'s words, or, where those would
 * have been nothing but the backend's or another program's own and "Show
 * technical details" is off, the frame's plain sentence alone.
 */
export function refusalSentence(
  t: Translate,
  frame: RefusalFrame,
  raw: string,
  sourceLabel: string,
  technical: boolean,
): string {
  const message = planErrorMessage(t, raw, sourceLabel, technical);
  return message === null ? t(REFUSAL_PLAIN_KEYS[frame]) : t(frame, { message });
}

/**
 * What `planErrorMessage`'s sentence leaves for its ⓘ, or null when it has
 * nothing more to say: which of its checks a path-list uninstall's
 * `not_what_instructions_expect` covers. The sentence stands alone
 * without it. Banager's own refusal (`refused`) has nothing more: its
 * sentence already says the error is an internal one, and whose fault it
 * is was reassurance, not a next step (the polish-3 copy rules, 规则 3).
 */
export function planErrorDetail(t: Translate, raw: string): string | null {
  const p = parseErrorPayload(raw);
  if (p === null) return null;
  if (parseUninstallUnsafe(raw)?.reason === "not_what_instructions_expect") {
    return t("planRefused.uninstallUnsafe.notWhatInstructionsExpectDetail");
  }
  return null;
}

/**
 * The `planRefused.*` key for each kind `plan_operation_error` sends with
 * no field but its `kind` -- Banager's own reasons, whose Rust wording is
 * for logs and is dropped before it reaches the wire. `index_updating` is
 * brew's uninstall preview declining to read Homebrew's catalogue while
 * `brew update` rewrites it (`catalogue_stamp` in
 * crates/banager-core/src/adapters/brew/mod.rs).
 */
const PLAN_FAILURE_KEYS: Record<string, string> = {
  output_too_large: "planRefused.outputTooLarge",
  index_updating: "planRefused.indexUpdating",
  refused: "planRefused.refused",
};

/**
 * The rest of `plan_operation_error`'s kinds (src-tauri/src/ipc.rs), in
 * the user's language; `null` for anything that is not one of them.
 *
 * One of them quotes another program verbatim, inside a sentence that
 * says what happened: `spawn_failed` carries the operating system's reason
 * it could not start the tool. That is not Banager's text, so it cannot be
 * translated -- but the sentence around it is -- and it is quoted only
 * with "Show technical details" on (`technical`); without it, the
 * sentence says that the tool could not start. `invalid_name` and
 * `program_missing` carry data (the name, the path), not prose.
 * `uninstall_unsafe` carries the path a path-list uninstall preview
 * refused and which check refused it (`parseUninstallUnsafe`).
 */
function planFailureMessage(t: Translate, raw: string, sourceLabel: string, technical: boolean): string | null {
  const p = parseErrorPayload(raw);
  if (!p) return null;
  // `typeof`, not truthiness: a `kind` like "toString" finds a function
  // on the object's prototype, not a key.
  const key: unknown = PLAN_FAILURE_KEYS[p.kind as string];
  if (typeof key === "string") return t(key, { source: sourceLabel });
  const text = (value: unknown) => (typeof value === "string" ? value : "");
  switch (p.kind) {
    case "invalid_name":
      return t("planRefused.invalidName", { name: text(p.name), source: sourceLabel });
    case "program_missing":
      return t("planRefused.programMissing", { program: text(p.program) });
    case "spawn_failed":
      return technical
        ? t("planRefused.spawnFailed", { source: sourceLabel, detail: text(p.detail) })
        : t("planRefused.spawnFailedPlain", { source: sourceLabel });
    case "uninstall_unsafe": {
      const refused = parseUninstallUnsafe(raw);
      return refused ? t(UNINSTALL_UNSAFE_KEYS[refused.reason], { path: refused.path }) : null;
    }
    default:
      return null;
  }
}

/**
 * Reads the `{"kind": "...", ...}` envelope every structured rejection in
 * src-tauri/src/ipc.rs uses. `null` for anything that is not a JSON object
 * with a string `kind`, including a plain string; each decoder in this
 * file reads its own fields off the result rather than repeating the parse.
 */
function parseErrorPayload(message: string): Record<string, unknown> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(message);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const p = parsed as Record<string, unknown>;
  return typeof p.kind === "string" ? p : null;
}

/**
 * The bare `kind` of `submit_operation_error`'s three reasons that carry
 * no extra fields (`source_gone`, `expired`, `unknown` -- `not_actionable`'s
 * own two fields go through `parseNotActionable` above instead); the three
 * functions below each compare the result to their one kind.
 */
function submitErrorKind(message: string): string | null {
  const p = parseErrorPayload(message);
  return p ? (p.kind as string) : null;
}

/**
 * Whether `raw` is the other refusal `Session::submit`'s actionability
 * re-check can produce: the instance the preview was built against is not
 * in the snapshot at all any more, so there is no read-only/unavailable
 * reason to name and `not_actionable` with two nulls would decode to an
 * empty message. `submit_operation_error` in src-tauri/src/ipc.rs is what
 * puts this on the wire.
 */
function isSourceGone(message: string): boolean {
  return submitErrorKind(message) === "source_gone";
}

/**
 * Whether `raw` is `SubmitError::Expired`: the preview is more than 10
 * minutes old (spec's plan lifetime), so `Session::submit` refused to run
 * it blind. `submit_operation_error` used to send this as
 * `SubmitError::Expired`'s own `Display`, hardcoded English that reached a
 * zh-CN user unlocalised; it now sends `{"kind": "expired"}` like every
 * other structured refusal, and `planRefused.expired` (both locales) is
 * what tells the person to look at the preview again.
 */
function isExpired(message: string): boolean {
  return submitErrorKind(message) === "expired";
}

/**
 * Whether `raw` is `SubmitError::Unknown`: `plan_id` was never issued, or
 * was already consumed by an earlier submit of the same preview. Same
 * unlocalised-`Display` history as `isExpired` above; `planRefused.unknown`
 * is what tells the person to start the action again.
 */
function isUnknownPlan(message: string): boolean {
  return submitErrorKind(message) === "unknown";
}

/**
 * Why the Open Ollama button could not open Ollama, exactly as
 * `open_ollama_failed_json` in src-tauri/src/ipc.rs puts it on the wire.
 *
 * `not_installed`: there is no Ollama.app in /Applications or
 * ~/Applications. `detect` withholds the button in that case, so this is
 * reached only through a snapshot taken before the app was removed -- but
 * it is the case the whole fix is about (`brew install ollama` installs
 * the command-line tool and no app), so its copy says what to do, not
 * merely that something failed.
 *
 * `launch_failed`: the app is there and `open -a Ollama` said no.
 */
export type OpenOllamaFailure = "not_installed" | "launch_failed";

const OPEN_OLLAMA_FAILURE_KEYS: Record<OpenOllamaFailure, string> = {
  not_installed: "sourceNotice.openOllamaFailed.notInstalled",
  launch_failed: "sourceNotice.openOllamaFailed.launchFailed",
};

/**
 * What a failure's sentence leaves for its "Details": why the app is
 * missing when someone installed Ollama with Homebrew, which is what
 * `brew install ollama` does without it.
 */
const OPEN_OLLAMA_FAILURE_DETAIL_KEYS: Partial<Record<OpenOllamaFailure, string>> = {
  not_installed: "sourceNotice.openOllamaFailed.notInstalledDetail",
};

/**
 * Reads the payload `open_ollama_app` rejects with, the same way
 * `parseNotActionable` reads the actionability gate's. `null` for anything
 * else, including a `reason` this build does not know -- which
 * `openOllamaErrorMessage` then shows verbatim rather than guessing at.
 */
export function parseOpenOllamaFailure(message: string): OpenOllamaFailure | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(message);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const p = parsed as Record<string, unknown>;
  if (p.kind !== "ollama_open_failed") return null;
  return p.reason === "not_installed" || p.reason === "launch_failed" ? p.reason : null;
}

/**
 * What a rejected Open Ollama press should read as: the localised copy for
 * a recognised failure; otherwise `raw` verbatim with "Show technical
 * details" on, so an unexpected error is still visible rather than
 * swallowed, and without it that Ollama didn't open and what to do, as
 * every other raw error is kept behind that setting. Before this existed
 * the button's failures were never shown at all; the backend reported
 * nothing, and so there was nothing to render.
 */
export function openOllamaErrorMessage(t: Translate, raw: string, technical: boolean): string {
  const reason = parseOpenOllamaFailure(raw);
  if (reason) return t(OPEN_OLLAMA_FAILURE_KEYS[reason]);
  return technical ? raw : t("sourceNotice.openOllamaFailed.other");
}

/** The "Details" of `openOllamaErrorMessage`'s sentence, or null when it has none. */
export function openOllamaErrorDetail(t: Translate, raw: string): string | null {
  const reason = parseOpenOllamaFailure(raw);
  const key = reason === null ? undefined : OPEN_OLLAMA_FAILURE_DETAIL_KEYS[reason];
  return key === undefined ? null : t(key);
}

/** Why writing the settings file failed, as `settings_save_error` in
 *  src-tauri/src/ipc.rs puts it on the wire. */
const SETTINGS_SAVE_FAILURE_KEYS: Record<string, string> = {
  permission_denied: "settingsSaveFailed.permissionDenied",
  disk_full: "settingsSaveFailed.diskFull",
  read_only: "settingsSaveFailed.readOnly",
};

/**
 * What a rejected `set_settings` should read as, as a phrase for the
 * caller's own frame (`settings.saveError`, `updates.saveChoiceFailed`) to
 * interpolate. The three reasons a person can act on are worded here;
 * `other` quotes the operating system's own description verbatim inside
 * a translated phrase, since that text is the system's, not Banager's.
 * Anything that is not the payload at all is shown verbatim, as
 * `planErrorMessage` shows one, so an unexpected error is still
 * visible rather than swallowed -- both, as every raw error, only with
 * "Show technical details" on (`technical`). Without it they are null,
 * and the caller's sentence says what happened without them
 * (`settingsSaveSentence`).
 */
export function settingsSaveErrorMessage(t: Translate, raw: string, technical: boolean): string | null {
  const p = parseErrorPayload(raw);
  if (!p || p.kind !== "settings_save_failed") return technical ? raw : null;
  const key: unknown =
    typeof p.reason === "string" ? SETTINGS_SAVE_FAILURE_KEYS[p.reason] : undefined;
  if (typeof key === "string") return t(key);
  if (!technical) return null;
  return t("settingsSaveFailed.other", {
    detail: typeof p.detail === "string" ? p.detail : "",
  });
}

/**
 * Each sentence a failed save is said in, and the same sentence without
 * its reason (`settingsSaveSentence`): still saying that the setting as it
 * was is back where it did, and otherwise to try again later.
 */
const SETTINGS_SAVE_PLAIN_KEYS = {
  "settings.saveError": "settings.saveErrorPlain",
  "updates.saveChoiceFailed": "updates.saveChoiceFailedPlain",
} as const;

/**
 * A rejected `set_settings` as the screen says it: `frame` around
 * `settingsSaveErrorMessage`'s phrase, or the frame's plain sentence
 * where there is no phrase to give without "Show technical details".
 */
export function settingsSaveSentence(
  t: Translate,
  frame: keyof typeof SETTINGS_SAVE_PLAIN_KEYS,
  raw: string,
  technical: boolean,
): string {
  const message = settingsSaveErrorMessage(t, raw, technical);
  return message === null ? t(SETTINGS_SAVE_PLAIN_KEYS[frame]) : t(frame, { message });
}

/**
 * The sources a refresh failed for, by adapter id, each once, in the order
 * `errors` first names them: the names 「部分检查未完成」 gives behind its ⓘ
 * (`failedSourceNames`, `unfinishedChecksNotice`), one a source.
 *
 * `Snapshot.errors` is not that list: `refresh()` (`session/refresh.rs`)
 * can push more than one `SourceError` for the same instance in one round
 * -- inventory and check-updates fail independently, and each failure
 * gets its own entry -- and two instances of one adapter, a Homebrew in
 * /opt/homebrew and another in /usr/local, are one source to the person
 * reading the notice, named once. An error goes to its instance's
 * adapter, or -- for an instance the snapshot no longer lists, or an
 * error against a bare adapter id -- to the adapter its id names
 * (`adapterIdOf`). A bare adapter id is what `refresh()` blames when an
 * adapter's own `detect()` panics or is cancelled: it has no instance to
 * blame yet (detect is what produces instances), and pushes the error
 * against `"brew"`, not `"brew:/opt/homebrew"` (see `InstanceId` in
 * `model.rs`).
 */
export function failedSourceAdapters(errors: SourceError[], instances: ManagerInstance[]): string[] {
  const adapters: string[] = [];
  for (const error of errors) {
    const adapterId =
      instances.find((instance) => instance.id === error.instance_id)?.adapter_id ??
      adapterIdOf(error.instance_id);
    if (!adapters.includes(adapterId)) adapters.push(adapterId);
  }
  return adapters;
}

/**
 * The sources a refresh failed for (`failedSourceAdapters`), by name, in
 * the user's language: what 「部分检查未完成」 says did not finish
 * (`unfinishedChecksNotice`), in place of a count that did not say which. Each adapter has a name of its
 * own in both languages, so no name comes twice.
 */
export function failedSourceNames(
  t: Translate,
  errors: SourceError[],
  instances: ManagerInstance[],
): string[] {
  return failedSourceAdapters(errors, instances).map((adapterId) => adapterLabel(t, adapterId));
}

/**
 * `names` as a sentence lists them, in the user's language: 「Homebrew、npm
 * 和 uv」, "Homebrew, npm and uv". One name alone; none, nothing.
 */
export function namesInSentence(t: Translate, names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return t("common.listAnd", {
    list: names.slice(0, -1).join(t("common.listSeparator")),
    last: names[names.length - 1],
  });
}

/**
 * The one notice for this round's checks that did not finish -- a
 * `SourceError` each (`Snapshot.errors`; `Snapshot.stale` is exactly
 * that there are any) -- or null when every one finished: 「部分检查未完成」,
 * a warning, the sources named behind its ⓘ with what that may leave
 * out (「pipx和Cargo这次未检查完，更新可能还没全部列出。」 -- "may":
 * a failed update check keeps last round's candidates, but the error
 * does not say which step failed; `failedSourceNames`), and Check
 * again, as a silent source's notice offers. It used to be a band of its own over the page, the web's way;
 * it is one of the list's notice lines now, folding with the others
 * (spec §3.8), and a row of the Overview's problems.
 *
 * Not the sources' own notices (`sourceNoticesFor`), which are about one
 * instance: an error can name a source the snapshot no longer lists, or
 * a bare adapter id when its detection failed (`failedSourceAdapters`).
 * But a source that did not answer at all -- not running, not responding,
 * refusing to run as root -- already says so in its own notice, with its
 * own button, and naming it here too would say it twice: its errors are
 * left out. `refresh()` carries none for such a source unless an
 * operation held it (crates/banager-core/src/session/refresh.rs).
 *
 * `inView`: the sources a page shows -- the Installed page on one source
 * -- whose errors alone it names: an error against an instance in view,
 * or one naming no listed instance but an adapter in view (its detection
 * failed). Every source when left out.
 */
export function unfinishedChecksNotice(
  t: Translate,
  errors: SourceError[],
  instances: ManagerInstance[],
  inView?: readonly ManagerInstance[],
): SourceNoticeSpec | null {
  const byId = new Map(instances.map((instance) => [instance.id, instance]));
  const adaptersInView = inView === undefined ? null : new Set(inView.map((instance) => instance.adapter_id));
  const unsaid = errors.filter((error) => {
    const instance = byId.get(error.instance_id);
    if (instance === undefined) return adaptersInView === null || adaptersInView.has(adapterIdOf(error.instance_id));
    return isAvailable(instance) && (inView === undefined || inView.some((each) => each.id === instance.id));
  });
  const names = failedSourceNames(t, unsaid, instances);
  if (names.length === 0) return null;
  return {
    id: "checks-unfinished",
    variant: "warning",
    titleKey: "sourceNotice.checksUnfinished.title",
    descriptionKey: "sourceNotice.checksUnfinished.description",
    values: { count: names.length, sources: namesInSentence(t, names) },
    action: { id: "checkAgain", labelKey: "header.checkAgain" },
  };
}

/**
 * What a finished check says when it found nothing to show (`nothingFound`):
 * no source at all, or sources with nothing installed. Its title and its
 * one sentence; what Banager works with, and where it looks, is
 * `emptyStates.supportedList`, behind 「详情」 after the sentence.
 */
export type NothingFound = "noSources" | "nothingInstalled";

export const NOTHING_FOUND_KEYS: Record<NothingFound, { title: string; description: string }> = {
  noSources: { title: "emptyStates.noSources.title", description: "emptyStates.noSources.description" },
  nothingInstalled: {
    title: "emptyStates.nothingInstalled.title",
    description: "emptyStates.nothingInstalled.description",
  },
};

/**
 * Whether a finished check found nothing to show, and which nothing, or
 * null when there is something: the one rule the pages' empty states and
 * the Overview's status row read, so they cannot disagree.
 *
 * - `noSources`: `detect` is Missing -- *every* source's detection came
 *   back with no instance, not Homebrew's alone.
 * - `nothingInstalled`: nothing installed *and* nothing to say about it.
 *   The second half is load-bearing: a source's notice line shows even
 *   with nothing of it installed -- an Ollama installed but not running,
 *   whose Open Ollama would be unreachable behind "No installed tools
 *   found" (`hasSourceNotice`) -- and so does a check that did not finish
 *   (`unfinishedChecksNotice`), where nothing listed may be only what it
 *   did not get to.
 *
 * Not the startup placeholder (`isStartupSnapshot`), which is no answer:
 * callers ask that first.
 */
export function nothingFound(
  t: Translate,
  snapshot: Pick<Snapshot, "detect" | "artifacts" | "instances" | "errors">,
): NothingFound | null {
  if (snapshot.detect === "Missing") return "noSources";
  if (
    snapshot.artifacts.length === 0 &&
    !snapshot.instances.some((instance) => hasSourceNotice(instance)) &&
    unfinishedChecksNotice(t, snapshot.errors, snapshot.instances) === null
  ) {
    return "nothingInstalled";
  }
  return null;
}
