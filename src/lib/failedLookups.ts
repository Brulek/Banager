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
 *
 * Whether the Overview may say all is well is another question, with its
 * own answer (`unsuccessfulLookupsOf`): any lookup that did not succeed,
 * mendable or not, keeps that away (independent review r6, F5).
 */
import type { UpdateCandidate } from "./types";
import { notHidden, type HidingSettings } from "./updateState";
import { warningMessage } from "./warnings";
import { FAILURE_CAUSE_KEYS, lookupFailureCause, type FailureCause } from "./failureCause";
import type { SourceNoticeSpec } from "./sources";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * The one cause a person knows (`lookupFailureCause`) that every row of
 * `candidates` with a tool's own words (`Message`) gives -- 「网络连接失败」
 * when nothing could be reached -- for the line over the rows Banager
 * could not check. Null when there is no such row, when one of them says
 * nothing `failureCause` reads, or when they disagree: then the line does
 * not claim a cause for all of them, and each row's chip says its own.
 */
export function sharedCannotCheckCause(candidates: UpdateCandidate[]): FailureCause | null {
  let shared: FailureCause | null = null;
  for (const candidate of candidates) {
    for (const warning of candidate.warnings) {
      const raw = warningMessage(warning);
      if (raw === null) continue;
      const cause = lookupFailureCause(raw);
      if (cause === null || (shared !== null && cause !== shared)) return null;
      shared = cause;
    }
  }
  return shared;
}

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
 * The warnings that mark a "could not check" row (`checkable: false`) as
 * one Banager never looks up, by design, rather than one whose lookup did
 * not succeed:
 *
 * - `NonRegistrySource`: a crate installed from a git repository or a
 *   local path (`CargoAdapter::check_updates` in
 *   crates/banager-core/src/adapters/cargo.rs), with no crates.io version
 *   to compare with.
 * - `NotLookedUpHere` (`LookupFailure::not_looked_up` in
 *   crates/banager-core/src/adapters/mod.rs), where no request is made on
 *   this Mac: the models of an Ollama on another Mac
 *   (`OllamaAdapter::check_updates`); an Ollama model from another
 *   registry, `hf.co/…` (`OllamaAdapter::check_one_model`), or whose local
 *   manifest is not there -- the models kept elsewhere through
 *   `OLLAMA_MODELS` -- or is in a protected place
 *   (`OllamaAdapter::compare_digests`); Antigravity CLI on an Intel Mac or
 *   under Rosetta (`manifest_arch_allowed`); and Claude Code whose settings
 *   are kept in a protected place, so the channel it follows is not known
 *   (`StandaloneAdapter::published`).
 *
 * Not a lookup that fails the same way at every check, though nothing
 * the user does here mends it either: a model made with `ollama create`,
 * which the registry answers 404 for at every check, keeps the all good
 * away, as a crate crates.io has no such name for does.
 *
 * The other tools and sources Banager never checks list no row at all, so
 * there is nothing of theirs to leave out here: Codex's and opencode's own
 * installs (`Latest::Unchecked`, `UNCHECKED_STANDALONE` in
 * src/lib/uncheckedStandalone.ts), a launcher left without its program
 * (`LauncherOnly`), a Python with no pip (`NoPip`), an Ollama at an
 * `https://` address (`HttpsHostRefused`), and the Homebrew apps that
 * update themselves while Settings leaves them out (`leftOutOfUpdateCheck`).
 */
const NEVER_LOOKED_UP: readonly UpdateCandidate["warnings"][number][] = ["NonRegistrySource", "NotLookedUpHere"];

/**
 * Whether Banager tried to find `candidate`'s newest version and did not
 * (`checkable: false`), whatever the reason -- no answer, which checking
 * again can mend (`isFailedLookup`), or a certificate rustls would not
 * accept, an answer that would not parse, a redirect or host the client
 * refuses, a registry that has no such thing, which it cannot. Not a row
 * Banager never looks up (`NEVER_LOOKED_UP`). Any such row keeps the
 * Overview from its all good (independent review r6, F5): that tool's
 * version is not known, whatever the others' are.
 */
export function isUnsuccessfulLookup(candidate: UpdateCandidate): boolean {
  return (
    isFailedLookup(candidate) ||
    (!candidate.checkable && !candidate.warnings.some((warning) => NEVER_LOOKED_UP.includes(warning)))
  );
}

/**
 * The rows the Updates page lists (`notHidden`) whose lookup did not
 * succeed (`isUnsuccessfulLookup`): what decides whether the Overview may
 * claim all good -- where `failedLookupsOf`, of them only those checking
 * again can mend, decides whether it offers Check Again.
 */
export function unsuccessfulLookupsOf(
  updates: UpdateCandidate[],
  settings: HidingSettings,
  nowMs: number = Date.now(),
): UpdateCandidate[] {
  return notHidden(updates, settings, nowMs).filter(isUnsuccessfulLookup);
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
