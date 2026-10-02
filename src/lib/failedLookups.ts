/**
 * The tools the last check could not look up, said up front (walk-2 W2-1).
 *
 * Offline, every source that checks over the network turns each of its
 * tools into a "could not check" row (`uncheckable_candidate` in
 * crates/banager-core/src/adapters/mod.rs), and those rows sat folded
 * under the Updates page's 「另有N个无法在这里更新」 while the Overview said
 * 「3个工具可以更新 · 上次检查：刚才」: a check that had mostly failed read
 * as one that had worked. These are the facts already on the wire, counted
 * once, for both pages: nothing here changes which updates are offered,
 * counted on the sidebar or the Dock, or selected.
 */
import type { UpdateCandidate } from "./types";
import { notHidden, type HidingSettings } from "./updateState";
import { warningMessage } from "./warnings";
import { FAILURE_CAUSE_KEYS } from "./failureCause";
import type { SourceNoticeSpec } from "./sources";
import { sharedCannotCheckCause } from "../components/updateDetails";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * Whether the last check tried to look `candidate` up and could not:
 * Banager could not establish its remote version (`checkable: false`) and
 * a tool's own words say why (a `Message`) -- a network that did not
 * answer, an index that refused, a version it could not read. Not a row
 * that is never checkable by its nature (`NonRegistrySource`: a crate not
 * from crates.io), which no check will find, and which says so itself.
 */
export function isFailedLookup(candidate: UpdateCandidate): boolean {
  return !candidate.checkable && candidate.warnings.some((warning) => warningMessage(warning) !== null);
}

/**
 * The rows the Updates page lists (`notHidden`: not one the user hid)
 * whose lookup failed this check (`isFailedLookup`).
 */
export function failedLookupsOf(
  updates: UpdateCandidate[],
  settings: HidingSettings,
  nowMs: number = Date.now(),
): UpdateCandidate[] {
  return notHidden(updates, settings, nowMs).filter(isFailedLookup);
}

/**
 * The notice for `failed` (`failedLookupsOf`), or null when there are
 * none: a warning, first among the sources' notices on the Updates page
 * and a row of the Overview's problems, that says how many tools could
 * not be checked -- with the cause in a word where every one of their
 * tools' words gives the same one a person knows (`sharedCannotCheckCause`:
 * 「21个工具没有检查成功：网络连接失败」) -- that updates may be missing
 * because of it, what to do (the cause's own step, or else check again
 * later), and Check Again.
 */
export function failedLookupsNotice(t: Translate, failed: UpdateCandidate[]): SourceNoticeSpec | null {
  if (failed.length === 0) return null;
  const cause = sharedCannotCheckCause(failed);
  return {
    id: "lookups-failed",
    variant: "warning",
    titleKey: cause === null ? "updates.lookupsFailedTitle" : "updates.lookupsFailedTitleCause",
    descriptionKey: "updates.lookupsFailedDescription",
    values: {
      count: failed.length,
      ...(cause === null ? {} : { cause: t(FAILURE_CAUSE_KEYS[cause].word) }),
      next: cause === null ? t("updates.cannotCheckTryLater") : t(FAILURE_CAUSE_KEYS[cause].next),
    },
    action: { id: "checkAgain", labelKey: "header.checkAgain" },
  };
}
