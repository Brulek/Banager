/**
 * What the front end knows about each package source, in one place so the
 * pages and the snapshot gate cannot drift apart. Nothing here talks to the
 * backend; these are presentational facts about what arrives over the wire.
 */
import type {
  ArtifactKey,
  ManagerInstance,
  ReadOnlyReason,
  SourceError,
  UninstallBlocked,
  Unavailable,
  UpdateBlocked,
} from "./types";
import { displayToken } from "./format";

/** i18n key holding each adapter's human name. */
export const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
  npm: "adapters.npm",
  pipx: "adapters.pipx",
  uv: "adapters.uv",
  pip: "adapters.pip",
  cargo: "adapters.cargo",
  ollama: "adapters.ollama",
};

/**
 * The `sourceNotice.*` key prefix whose `.title` and `.description` explain
 * each read-only reason.
 *
 * The two reasons need genuinely different copy and the difference matters:
 * pip cannot be driven at all, so the way out is to install Python tools
 * with pipx or uv; an npm whose prefix is root-owned works fine, so the way
 * out is to reinstall Node with Homebrew. Telling the npm user about pipx
 * -- which the Updates page did for every read-only row before the wire
 * carried a reason -- sends someone who does not write code to install a
 * Python tool to fix their JavaScript packages.
 */
export const READ_ONLY_NOTICE_KEYS: Record<ReadOnlyReason, string> = {
  ByDesign: "sourceNotice.pipReadOnly",
  PrefixNotWritable: "sourceNotice.prefixNotWritable",
};

/**
 * Whether Canager may offer operations on this source. The front-end
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
 * Whether Canager reached this source on the last refresh. The front-end
 * mirror of `ManagerInstance::available()`, and what `healthy: boolean`
 * used to be -- except the wire now also says *why*, which is what lets
 * the copy differ between "start it" and "it did not answer".
 */
export function isAvailable(instance: ManagerInstance): boolean {
  return instance.status.unavailable === null;
}

/** Which axis a notice speaks for: what Canager may do, or what it knows. */
export type SourceNoticeAxis = "capability" | "state";

/**
 * Something the notice offers to do about itself. An id, not a callback:
 * this module stays pure so both pages can call it, and each page wires
 * the id to its own mutation.
 */
export type SourceNoticeActionId = "openOllama" | "retry";

/**
 * One banner a source needs rendered, as data: which i18n keys say it,
 * what to interpolate into them, and what (if anything) the user can do
 * about it. No `t()` and no JSX, so the rule can be tested directly and,
 * more to the point, so both pages answer from the same rule -- the two
 * pages' notices used to be different code, which is how the Updates page
 * came to say
 * "Everything is up to date" for a source it had not managed to ask.
 */
export interface SourceNoticeSpec {
  /** Stable React key: one instance can need more than one notice. */
  id: string;
  axis: SourceNoticeAxis;
  variant: "info" | "warning";
  titleKey: string;
  descriptionKey: string;
  /** Interpolation values, already in the user's language. */
  values?: Record<string, string>;
  /** What the notice offers to do about itself, if anything. */
  action?: { id: SourceNoticeActionId; labelKey: string };
}

/**
 * Every notice `instance` needs, in the order they should be rendered:
 * capability first (what Canager may do at all), then state (what it
 * managed to find out). `sourceLabel` is the source's name as the user
 * reads it -- resolved by the caller through `ADAPTER_LABEL_KEYS`, because
 * keeping `t()` out of here is what makes this testable and shareable.
 *
 * The two axes are independent and both can apply at once: a read-only pip
 * whose interpreter has gone missing is read-only *and* silent.
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

  if (instance.read_only_reason !== null) {
    const prefix = READ_ONLY_NOTICE_KEYS[instance.read_only_reason];
    notices.push({
      id: `${instance.id}:read-only`,
      axis: "capability",
      variant: "info",
      titleKey: `${prefix}.title`,
      descriptionKey: `${prefix}.description`,
    });
  }

  const unavailable = instance.status.unavailable;
  if (unavailable === "NotRunning") {
    // One state, one sentence, named through `sourceLabel` so it reads in
    // the user's language. Ollama is the only source Canager can start --
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
      axis: "state",
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
      axis: "state",
      variant: "warning",
      titleKey: "sourceNotice.unreachable.title",
      // Two sentences for one state, chosen by what is actually on screen.
      // Neither offers a way out: the one it used to offer ("Reopening
      // Canager usually fixes this") is simply wrong for a source that
      // will fail the same way on the next launch, and promising a
      // recovery that may not happen is the pattern this phase exists to
      // remove.
      descriptionKey:
        rowsOnScreen > 0
          ? "sourceNotice.unreachable.descriptionWithRows"
          : "sourceNotice.unreachable.description",
      values: { source: sourceLabel },
    });
  } else if (unavailable === "RefusesAsRoot") {
    // Its own copy because its own action: Canager was started with
    // `sudo`, Homebrew will not run that way, and the way out is to quit
    // and open Canager again normally. No button -- Canager cannot
    // relaunch itself out from under root, and offering to do what it
    // cannot is the pattern this phase exists to remove.
    notices.push({
      id: `${instance.id}:refuses-as-root`,
      axis: "state",
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
        axis: "state",
        variant: "warning",
        titleKey: "sourceNotice.indexMayBeStale.title",
        descriptionKey: "sourceNotice.indexMayBeStale.description",
        action: { id: "retry", labelKey: "sourceNotice.indexMayBeStale.action" },
      });
    } else if (note === "IndexUpdating") {
      // Nothing has failed: the download is still going. So an "info"
      // notice, and no button -- there is nothing for the user to do, and
      // a "Try again" here could only wait on the same download. The core
      // refreshes by itself when it ends (`Session::background_change`),
      // which is what clears this.
      notices.push({
        id: `${instance.id}:index-updating`,
        axis: "state",
        variant: "info",
        titleKey: "sourceNotice.indexUpdating.title",
        descriptionKey: "sourceNotice.indexUpdating.description",
      });
    } else {
      const unhandled: never = note;
      void unhandled;
    }
  }

  return notices;
}

/**
 * Whether this source has anything to say at all -- exactly
 * `sourceNoticesFor(...).length > 0`, expressed that way so the two can
 * never drift.
 *
 * `SnapshotStatus` asks this as well as the pages do: its zero-artifact
 * empty state replaces `children` outright, so without it a Mac whose only
 * source is a stopped Ollama shows "Nothing installed yet" and the Open
 * Ollama button is unreachable. One rule, one place.
 */
export function hasSourceNotice(instance: ManagerInstance): boolean {
  return sourceNoticesFor(instance, "").length > 0;
}

/**
 * The command that releases the pin on `key`, for the user to run
 * themselves: Canager does not unpin, which would be a new write
 * operation. Both `UPDATE_BLOCKED_KEYS.Pinned` and
 * `UNINSTALL_BLOCKED_KEYS.Pinned` build their command with this. Their
 * producers are brew's `parse_outdated` and `parse_info_installed`
 * (crates/canager-core/src/adapters/brew/parse.rs) and pipx's
 * `parse_outdated` (adapters/pipx.rs), so the command is built for
 * whichever tool owns the key. The tool is the instance's `adapter_id`; for an instance
 * the snapshot lacks, the part of the key's `instance_id` before any
 * `:`, which is the adapter id (`instance_id` in
 * crates/canager-core/src/model.rs writes it first and asserts it has
 * no `:` of its own).
 *
 * pipx: `pipx unpin <name>`. That is how pipx itself spells it when it
 * refuses a pinned upgrade ("Run `pipx unpin {result.environment}` to
 * unpin it.", pipx 1.17.3's `commands/upgrade.py:473`), and `unpin`
 * takes one positional ENVIRONMENT and nothing else (`_add_unpin`,
 * `main.py:969-978`). The name is the one `parse_outdated` read off
 * the line, and pipx canonicalizes it into the venv's directory name
 * (`_venv_dir`, `main.py:1752-1753`; `get_venv_dir`, `venv.py:142-144`).
 * The program is the instance's `exe_path`, the pipx Canager found on
 * its own PATH (`resolve_exe` in `PipxAdapter::detect`), which
 * Terminal's PATH may not include.
 *
 * brew: `--cask` because `brew unpin <name>` resolves a formula first
 * (`to_resolved_formulae_to_casks` in Homebrew's `cmd/unpin.rb`), and
 * a formula can share a cask's name.
 *
 * The program is the instance's `exe_path`, the absolute path of the
 * brew that owns this package, not a bare `brew`: Canager finds brew
 * by absolute path (`CANDIDATE_PATHS` in
 * crates/canager-core/src/adapters/brew/mod.rs) and lists
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

/** What `UPDATE_BLOCKED_KEYS` holds for one reason. */
interface UpdateBlockedCopy {
  /** The row's badge on the Updates page, in place of "Update". */
  badge: string;
  /** The row's description: why, and what the user can do about it. The
   *  Updates page fills `{{source}}` with the owning source's label
   *  (`sourceLabelFor`) and `{{command}}` with `command` below, so one
   *  sentence serves every tool that produces the reason. */
  description: string;
  /**
   * `description` for a package that updates itself
   * (`InstalledArtifact.auto_updates`), or `null` when this reason needs
   * no separate sentence for one. A reason whose `description` promises
   * the package stays where it is cannot make that promise to an app that
   * updates itself outside the tool.
   */
  selfUpdatingDescription: string | null;
  /** The command `description`'s `{{command}}` stands for, from the row's
   *  own key and the instance that key's `instance_id` names (`undefined`
   *  only if the snapshot lacks it, which `refresh` never produces: it
   *  builds `updates` only from instances it also puts in `instances`).
   *  The Updates page renders it as code (`withCommand` in
   *  src/pages/UpdatesPage.tsx), not as a word of the sentence. */
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
    description: "updates.blocked.Pinned.description",
    // A pinned cask with `auto_updates true` can still move: `brew pin`
    // itself warns it "may update itself outside Homebrew despite being
    // pinned" (Homebrew's `cmd/pin.rb`). `description` says Homebrew is
    // keeping the package at its version, which such an app does not
    // honour, so it gets a sentence that promises only what Homebrew and
    // Canager will not do. Such a cask reaches this page mostly through
    // `brew outdated --greedy` (`include_self_updating`, `check_updates`
    // in crates/canager-core/src/adapters/brew/mod.rs), but Homebrew's own
    // environment settings can list it without that flag
    // (`outdated_version` in Homebrew's `cask/cask.rb`), which is why the
    // page asks the package's `auto_updates` and not the setting. The
    // sentence names Homebrew, not `{{source}}`: of `Pinned`'s two
    // producers only brew ever sets `auto_updates` (`parse_list` in
    // crates/canager-core/src/adapters/pipx.rs writes `false`).
    selfUpdatingDescription: "updates.blocked.Pinned.descriptionSelfUpdating",
    // The description's promise that the update appears "at the latest
    // the next time you start Canager" rests on the refresh every start
    // runs (`refreshIntoCache(queryClient, "initial")` in src/lib/events.ts)
    // and on `parse_outdated` reading `pinned` afresh each time. It names
    // Canager, not "it", because "it" has just meant the package, and for
    // a pinned cask that is an app "open it" reads as "open that app".
    // "Start" is the right verb: the page has no refresh button (the only
    // one is `SnapshotStatus`'s retry after a failed refresh), and closing
    // the window quits, since `run` in src-tauri/src/lib.rs has no
    // `ExitRequested` handler to keep the app alive without one.
    command: unpinCommand,
    refused: "updates.blocked.Pinned.refused",
  },
};

/**
 * Reads the `{"kind": "update_blocked", "reason": ...}` payload
 * `update_blocked_json` in src-tauri/src/ipc.rs sends when
 * `Session::issue_plan` or `Session::submit` refuses to update one package.
 * `null` for anything else, including a reason this build has no copy for
 * -- which `planErrorMessage` then shows verbatim rather than guessing at.
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
  /** The Installed page row's description in place of the package's blurb:
   *  why there is no Uninstall button, and what the user can do about it.
   *  The page fills `{{source}}` with the owning source's label and
   *  `{{command}}` with `command` below, rendered as code
   *  (`withCommand` in src/components/withCommand.tsx). */
  description: string;
  /** The command both sentences' `{{command}}` stands for. */
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
    // The promise that Uninstall comes back "the next time it checks, at
    // the latest the next time you start Canager" rests on every refresh
    // reading the inventory again (`adapter.inventory` in `refresh_round`,
    // crates/canager-core/src/session/refresh.rs), on `parse_info_installed`
    // reading `pinned` afresh each time, and on the refresh every start
    // runs (`refreshIntoCache(queryClient, "initial")` in src/lib/events.ts).
    description: "installed.blocked.Pinned.description",
    // `UninstallBlocked::Pinned`'s only producer is brew
    // (`parse_info_installed`), so this is always `brew unpin`, built from
    // the owning instance's `exe_path`, `--cask` for a cask.
    command: unpinCommand,
    refused: "installed.blocked.Pinned.refused",
  },
};

/**
 * Reads the `{"kind": "uninstall_blocked", "reason": ...}` payload
 * `uninstall_blocked_json` in src-tauri/src/ipc.rs sends when
 * `Session::issue_plan` or `Session::submit` refuses to uninstall one
 * package. `null` for anything else, including a reason this build has no
 * copy for.
 *
 * Only the uninstall dialog (src/components/UninstallDialog.tsx) can
 * receive this payload, because `blocked_uninstall` in
 * crates/canager-core/src/session/plans.rs refuses nothing but an
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
 * `parseNotActionable`'s result, in the exact copy the source's own
 * notice already uses for each reason (`READ_ONLY_NOTICE_KEYS`,
 * `sourceNotice.notRunning`, `sourceNotice.unreachable`,
 * `sourceNotice.refusesAsRoot`) -- so this refusal never reads as a raw
 * Rust enum. `sourceLabel` is the adapter's
 * name in the user's language, exactly as `sourceNoticesFor` takes it.
 *
 * Both axes can be set at once (a read-only source can also be silent),
 * so both parts are joined when present, same as `sourceNoticesFor`
 * pushing more than one notice for one instance. The read-only copy needs
 * no source name (spec §7's wording is self-contained); the state copy
 * always names one, same as `sourceNoticesFor`.
 */
export function notActionableMessage(
  t: Translate,
  reason: NotActionableReason,
  sourceLabel: string,
): string {
  const parts: string[] = [];
  if (reason.read_only !== null) {
    parts.push(t(`${READ_ONLY_NOTICE_KEYS[reason.read_only]}.description`));
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
 * (`UpdatesPage`, `UninstallDialog`) goes through this instead of showing
 * the backend's string directly, so no refusal that can reach a real
 * person is ever a raw Rust `{:?}` or this project's own English reaching
 * someone reading Canager in another language.
 */
export function planErrorMessage(t: Translate, raw: string, sourceLabel: string): string {
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
  return planFailureMessage(t, raw, sourceLabel) ?? raw;
}

/**
 * The `planRefused.*` key for each kind `plan_operation_error` sends with
 * no field but its `kind` -- Canager's own reasons, whose Rust wording is
 * for logs and is dropped before it reaches the wire. `index_updating` is
 * brew's uninstall preview declining to read Homebrew's catalogue while
 * `brew update` rewrites it (`catalogue_stamp` in
 * crates/canager-core/src/adapters/brew/mod.rs).
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
 * it could not start the tool. That is not Canager's text, so it cannot be
 * translated -- but the sentence around it is. `invalid_name` and
 * `program_missing` carry data (the name, the path), not prose.
 */
function planFailureMessage(t: Translate, raw: string, sourceLabel: string): string | null {
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
      return t("planRefused.spawnFailed", { source: sourceLabel, detail: text(p.detail) });
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
 * a recognised failure, otherwise `raw` verbatim -- the same fallback
 * `planErrorMessage` uses, so an unexpected error is still visible rather
 * than swallowed. Before this existed the button's failures were never
 * shown at all; the backend reported nothing, and so there was nothing to
 * render.
 */
export function openOllamaErrorMessage(t: Translate, raw: string): string {
  const reason = parseOpenOllamaFailure(raw);
  return reason ? t(OPEN_OLLAMA_FAILURE_KEYS[reason]) : raw;
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
 * caller's own frame (`settings.saveError`, `updates.ignoreFailed`) to
 * interpolate. The three reasons a person can act on are worded here;
 * `other` quotes the operating system's own description verbatim inside
 * a translated phrase, since that text is the system's, not Canager's.
 * Anything that is not the payload at all is shown verbatim, the same
 * fallback `planErrorMessage` uses, so an unexpected error is still
 * visible rather than swallowed.
 */
export function settingsSaveErrorMessage(t: Translate, raw: string): string {
  const p = parseErrorPayload(raw);
  if (!p || p.kind !== "settings_save_failed") return raw;
  const key: unknown =
    typeof p.reason === "string" ? SETTINGS_SAVE_FAILURE_KEYS[p.reason] : undefined;
  if (typeof key === "string") return t(key);
  return t("settingsSaveFailed.other", {
    detail: typeof p.detail === "string" ? p.detail : "",
  });
}

/**
 * How many distinct sources a refresh failed for -- the count the stale
 * banner's "couldn't finish for {{count}} sources" copy promises.
 * `Snapshot.errors` is not that count: `refresh()`
 * (`session/refresh.rs`) can push more than one `SourceError` for the
 * same instance in one round -- inventory and check-updates fail
 * independently, and each failure gets its own entry -- so a single
 * broken Homebrew reads as two failed sources, or four for two broken
 * prefixes. Deduplicating by `instance_id` is what turns "failed calls"
 * back into "failed sources".
 *
 * One `instance_id` here is not always a `ManagerInstance.id`: when an
 * adapter's own `detect()` panics or is cancelled, `refresh()` has no
 * instance to blame yet (detect is what produces instances) and pushes
 * the error against the bare adapter id instead (e.g. `"brew"`, not
 * `"brew:/opt/homebrew"`). That is still one distinct failed source --
 * the whole adapter, this round -- and every real `ManagerInstance.id`
 * is namespaced as `"<adapter_id>:<path>"` (see `InstanceId` in
 * `model.rs`), so a bare adapter id can never collide with one and
 * double-count or merge with it. Counting distinct `instance_id` values
 * is therefore correct across both shapes without telling them apart.
 */
export function failedSourceCount(errors: SourceError[]): number {
  return new Set(errors.map((e) => e.instance_id)).size;
}
