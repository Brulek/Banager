/**
 * The "why" behind a status chip about an update, in at most two short
 * sentences: what the Updates page's chips open, and what the Installed
 * page's chips and detail drawer say about the same update. One set of
 * builders, so a pinned package, a source that did not answer or a row
 * Banager could not check reads the same on both pages.
 */
import type { ReactNode } from "react";
import type { TFunction } from "i18next";
import { READ_ONLY_DETAIL_KEYS, UNAVAILABLE_DETAIL_KEYS, UPDATE_BLOCKED_KEYS } from "../lib/sources";
import { FAILURE_CAUSE_KEYS, lookupFailureCause } from "../lib/failureCause";
import type { InstalledArtifact, ManagerInstance, UpdateBlocked, UpdateCandidate } from "../lib/types";
import { warningMessage, warningText } from "../lib/warnings";
import { COMMAND_SLOT, withCommand } from "./withCommand";
import { CopyButton } from "./CopyButton";

/**
 * A chip's detail, a sentence to a line; the lines after the first are the
 * quieter kind. Block `span`s, not `<p>`s: a popover's panel can sit in a
 * line of text (`Popover`), where a paragraph may not.
 */
export function detailLines(lines: ReactNode[]): ReactNode {
  return lines.map((line, index) => (
    <span key={index} data-detail-line="" className={index === 0 ? "block break-words" : "mt-1.5 block break-words text-muted"}>
      {line}
    </span>
  ));
}

/**
 * Why a row could not be checked, for its "Can't check" chip: that it
 * could not, then its reason. A warning with a key of its own
 * (`NonRegistrySource`) was written for this audience and is always
 * given. A `Message` is raw text off the wire -- a tool's stderr, an HTTP
 * error -- kept verbatim on purpose, and it is behind "Show technical
 * details", which is exactly what spec §6 says the right shape is. Where
 * its words say why in a way a person knows (`failureCause`: no network,
 * `lookupFailureCause`: no network), that is said in a person's words
 * whether or not technical details are on (`failure.line.*`), before the
 * words themselves. Distinct, so a row carrying the same reason twice says
 * it once. A lookup a later check can get past
 * (`TransientLookupFailure`) says to check again later -- but not after a
 * cause whose own line already says what to do. A row no check will mend
 * (a 404, a tool not looked up on this Mac, `NonRegistrySource`) says no
 * such thing.
 */
export function cannotCheckDetail(
  t: TFunction,
  candidate: UpdateCandidate,
  showTechnicalDetails: boolean,
): ReactNode {
  const reasons = new Set<string>();
  let causeSaid = false;
  // A person's words first -- a warning's own sentence, the cause a
  // tool's words give -- then, with technical details on, those words
  // themselves: a secure connection's sentence comes after its `Message`
  // on the wire, and would otherwise follow rustls's words (a2 review 5).
  for (const warning of candidate.warnings) {
    if (warning === "TransientLookupFailure") continue;
    const raw = warningMessage(warning);
    if (raw === null) {
      const text = warningText(t, warning);
      if (text !== null && text !== "") reasons.add(text);
      continue;
    }
    const cause = lookupFailureCause(raw);
    if (cause !== null) {
      reasons.add(t(FAILURE_CAUSE_KEYS[cause].line));
      causeSaid = true;
    }
  }
  if (showTechnicalDetails) {
    for (const warning of candidate.warnings) {
      const raw = warningMessage(warning);
      if (raw !== null && raw !== "") reasons.add(raw);
    }
  }
  const tryLater = candidate.warnings.includes("TransientLookupFailure") && !causeSaid;
  return detailLines([
    t("updates.cannotCheckShort"),
    ...reasons,
    ...(tryLater ? [t("warnings.transientLookupFailure")] : []),
  ]);
}

/**
 * A blocked row's chip detail: why the tool will not update it, and what
 * the user can do instead -- the unpin command, set as code in the
 * sentence; "open it once", with the name to type in Terminal set into
 * the sentence where typing it opens this copy (`copy.typed`), or else
 * the command that opens it under it while technical details are on; or,
 * for a package Homebrew disabled, nothing to run. Then the row's own
 * note (`copy.note`): what pipx's unpin also does, Homebrew's suggested
 * replacement. `artifact` is the row's inventory entry, which the typed
 * name and the replacement are read from; without it neither is said.
 * A command set into the sentence -- the unpin -- has Copy Command under
 * it (`CopyButton`), whatever "Show technical details" says: it is the
 * one thing the sentence asks the person to do, and selecting it by hand
 * from a popover is no way to ask that (walk-2 W2-13).
 */
export function blockedDetail(
  t: TFunction,
  candidate: UpdateCandidate,
  reason: UpdateBlocked,
  instance: ManagerInstance | undefined,
  source: string,
  showTechnicalDetails: boolean,
  artifact?: InstalledArtifact,
): ReactNode {
  const copy = UPDATE_BLOCKED_KEYS[reason];
  const command = copy.command(candidate.key, instance);
  const note = copy.note(candidate.key, instance, artifact);
  const noteLines = note === null ? [] : [t(note.key, note.options)];
  if (copy.commandInDetail) {
    return detailLines([
      withCommand(t(copy.detail, { command: COMMAND_SLOT, source }), command),
      ...noteLines,
      ...(command === ""
        ? []
        : [
            <span data-detail-copy="" className="flex justify-start">
              <CopyButton text={command} label={t("common.copyCommand")} />
            </span>,
          ]),
    ]);
  }
  const typed = copy.typed === null ? null : copy.typed.name(artifact, instance);
  if (copy.typed !== null && typed !== null) {
    return detailLines([withCommand(t(copy.typed.detail, { command: COMMAND_SLOT, source }), typed), ...noteLines]);
  }
  return detailLines([
    t(copy.detail, { source }),
    ...noteLines,
    ...(showTechnicalDetails && command !== ""
      ? [withCommand(t("updates.runInTerminal", { command: COMMAND_SLOT }), command)]
      : []),
  ]);
}

/**
 * What to do about a row whose source did not answer, by why it did not.
 * An instance missing from the snapshot, which `refresh` never produces,
 * reads as one that did not answer.
 */
export function unavailableDetail(
  t: TFunction,
  instance: ManagerInstance | undefined,
  source: string,
): ReactNode {
  return detailLines([
    t(UNAVAILABLE_DETAIL_KEYS[instance?.status.unavailable ?? "NotResponding"], { source }),
  ]);
}

/** A read-only source's row: why, and the way out for that reason. */
export function readOnlyDetail(t: TFunction, instance: ManagerInstance | undefined): ReactNode | undefined {
  const reason = instance?.read_only_reason ?? null;
  return reason === null ? undefined : detailLines([t(READ_ONLY_DETAIL_KEYS[reason])]);
}

/** What a row's version column says about an update (`ToolRow`'s `version` and `newVersion`). */
export interface UpdateVersionColumn {
  /** "7.1 → 7.2"; a word where a version would mean nothing; null for nothing. */
  version: string | null;
  /** "7.2": what the column says alone once the window is too narrow for both. */
  newVersion?: string;
}

/**
 * The version column of a row with an update listed -- the Updates page's
 * rows, and the Installed page's where an update is to be had, in place of
 * an "Update available" word: "7.1 → 7.2", in tabular numerals.
 *
 * A `Digest` candidate is Ollama: `current` is the local manifest digest
 * that /api/tags reported and `target` is the registry manifest's config
 * digest -- **different hash spaces**, not two readings of one
 * identifier, and they will not be equal even after a successful pull.
 * The adapter's own comment (crates/banager-core/src/adapters/ollama/
 * mod.rs) says never to render them as a version jump, and a 64-hex
 * string is not something to put in front of this audience either way:
 * such a row says "New version" -- only when it was checked. A row
 * Banager could not check has no version to move to (its `target` is its
 * installed version, `uncheckable_candidate` in crates/banager-core/src/
 * adapters/mod.rs), so it shows the version it has, and a model's
 * nothing at all.
 */
export function updateVersionColumn(t: TFunction, candidate: UpdateCandidate): UpdateVersionColumn {
  const { current, target } = candidate;
  if (!candidate.checkable) {
    return { version: candidate.channel === "Digest" || current === "" ? null : current };
  }
  if (candidate.channel === "Digest") return { version: t("updates.newVersion") };
  if (current !== "" && target !== "") {
    return { version: t("updates.versionChange", { current, target }), newVersion: target };
  }
  return { version: target !== "" ? target : current !== "" ? current : null };
}
