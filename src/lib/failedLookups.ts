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
 *
 * Only a failure a later check can get past is counted
 * (`TransientLookupFailure`, which Rust sets only where it knows that):
 * a row no check will ever mend -- a model the registry answers 404 for,
 * Antigravity CLI on an Intel Mac, an answer that would not parse -- would
 * otherwise keep a warning and its Check Again on every page for good.
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
 * Whether Banager could not establish `candidate`'s remote version
 * (`checkable: false`) and a tool's own words say why (a `Message`): the
 * rows whose why is those words, hidden while "Show technical details" is
 * off, which the Updates page's line over them counts for its 「显示原因」.
 * Not a row whose why is Banager's own sentence (`NonRegistrySource`).
 */
export function saysWhyInToolWords(candidate: UpdateCandidate): boolean {
  return !candidate.checkable && candidate.warnings.some((warning) => warningMessage(warning) !== null);
}

/**
 * Whether the last check tried to look `candidate` up and could not, in a
 * way a later check can get past (`TransientLookupFailure`): the request
 * got no answer, a "not now" status, or the tool's words name the network.
 */
export function isFailedLookup(candidate: UpdateCandidate): boolean {
  return saysWhyInToolWords(candidate) && candidate.warnings.includes("TransientLookupFailure");
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
 * What to do about `failed`: the cause's own step where every one of
 * their tools' words gives the network (`sharedCannotCheckCause`), or
 * else check again later -- and the cause, for a title that names it.
 */
function lookupsCause(t: Translate, failed: UpdateCandidate[]) {
  const cause = sharedCannotCheckCause(failed);
  return cause === null
    ? { cause: null, word: null, next: t("warnings.transientLookupFailure"), lineKey: "warnings.transientLookupFailure" }
    : {
        cause,
        word: t(FAILURE_CAUSE_KEYS[cause].word),
        next: t(FAILURE_CAUSE_KEYS[cause].next),
        lineKey: FAILURE_CAUSE_KEYS[cause].line,
      };
}

/**
 * The Updates page's notice for `failed` (`failedLookupsOf`), or null
 * when there are none: a warning, first among the sources' notices, whose
 * line says how many tools could not be checked -- with the cause in a
 * word where it is the network (「21个工具没有检查成功：网络连接失败」) --
 * and whose ⓘ says that updates may be missing because of it and what to
 * do, beside Check Again.
 */
export function failedLookupsNotice(t: Translate, failed: UpdateCandidate[]): SourceNoticeSpec | null {
  if (failed.length === 0) return null;
  const { cause, word, next } = lookupsCause(t, failed);
  return {
    id: "lookups-failed",
    variant: "warning",
    titleKey: cause === null ? "updates.lookupsFailedTitle" : "updates.lookupsFailedTitleCause",
    descriptionKey: "updates.lookupsFailedDescription",
    values: { count: failed.length, ...(word === null ? {} : { cause: word }), next },
    action: { id: "checkAgain", labelKey: "header.checkAgain" },
  };
}

/**
 * The same for a row of the Overview's problems, under a status whose line
 * already says how many (「21个工具没有检查成功」): what that means, 「可能还有
 * 更新没有列出」, with why and what to do under it -- 「网络连接失败，请检查网络
 * 连接后重试。」, or check again later -- and Check Again. Said once there,
 * not twice (walk-2 review 1.4).
 */
export function failedLookupsProblem(t: Translate, failed: UpdateCandidate[]): SourceNoticeSpec | null {
  if (failed.length === 0) return null;
  return {
    id: "lookups-failed",
    variant: "warning",
    titleKey: "updates.lookupsFailedProblemTitle",
    descriptionKey: lookupsCause(t, failed).lineKey,
    action: { id: "checkAgain", labelKey: "header.checkAgain" },
  };
}
