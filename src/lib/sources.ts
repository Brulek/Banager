/**
 * What the front end knows about each package source, in one place so the
 * pages and the snapshot gate cannot drift apart. Nothing here talks to the
 * backend; these are presentational facts about what arrives over the wire.
 */
import type { ManagerInstance, ReadOnlyReason, Unavailable } from "./types";

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
 * more to the point, so both pages answer from the same rule -- the
 * Installed page's group headers and the Updates page's top notices used
 * to be different code, which is how the Updates page came to say
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
 */
export function sourceNoticesFor(
  instance: ManagerInstance,
  sourceLabel: string,
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
    // the user's language. Ollama is the only source Canager can start, so
    // it is the only one whose notice carries a button -- but it used to
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
      descriptionKey: "sourceNotice.unreachable.description",
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
 * at all. Returns `null` for every other backend error (an unknown
 * instance, an unregistered adapter, a submit's expired/unknown plan id),
 * which `planErrorMessage` below then shows verbatim exactly as before
 * this existed -- those are either bugs nobody but a developer should see,
 * or already-plain-English text the app already shows as-is.
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
    parts.push(t("sourceNotice.unreachable.description", { source: sourceLabel }));
  } else if (reason.unavailable === "RefusesAsRoot") {
    parts.push(t("sourceNotice.refusesAsRoot.description", { source: sourceLabel }));
  }
  return parts.join(" ");
}

/**
 * What a `plan_operation`/`submit_operation` rejection should read as:
 * `notActionableMessage` when `raw` is the actionability gate's JSON
 * payload, otherwise `raw` verbatim. Every call site that renders a plan
 * or submit error (`UpdatesPage`, `UninstallDialog`) goes through this
 * instead of showing the backend's string directly, so the one refusal
 * that can reach a real person is never a raw Rust `{:?}`.
 */
export function planErrorMessage(t: Translate, raw: string, sourceLabel: string): string {
  const reason = parseNotActionable(raw);
  return reason ? notActionableMessage(t, reason, sourceLabel) : raw;
}
