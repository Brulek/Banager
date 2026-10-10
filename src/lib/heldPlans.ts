/**
 * Update all of more tools than the backend holds previews for.
 *
 * The backend keeps at most `PLANS_HELD` previewed plans for `submit`
 * (`MAX_ISSUED_PLANS` in crates/banager-core/src/session/plans.rs, a
 * bound on what a page that previews over and over can make it hold).
 * Past it, issuing a plan lets the oldest held go, and submitting that one
 * is refused as `unknown`. The update confirmation plans every chosen
 * update before Update can be pressed (`openConfirm` in
 * src/components/UpdateConfirm.tsx), so a batch of more than that has
 * already lost its first plans when the person confirms it -- each of them
 * shown, none of them startable as it was.
 *
 * Such a plan is worked out again just before its turn, and started only
 * if it is the very plan the person was shown: the same command, the same
 * notes, the same everything (`samePlan`). One that came out different is
 * not started (`CHANGED_SINCE_SHOWN`), so "every command is shown before
 * it runs" holds for every update of the batch. Only in a batch of more
 * than `PLANS_HELD`: in a smaller one an `unknown` refusal is said as it
 * is. A plan let go is always the oldest held, and no other sheet plans
 * while the confirmation is up -- save the last few plans of a sheet
 * closed while it was still preparing, which the backend may still be
 * working out (four at once, `PLANS_AT_ONCE`): a batch of about 1,022 to
 * 1,024 opened right after such a sheet can lose its first plan or two,
 * and those are refused as `unknown`, never run unshown. And only while
 * the batch is younger than a plan's lifetime: past it, a plan let go is
 * refused as expired, as one still held is (`LetGo`), never given a longer
 * life than it would have had.
 *
 * Upgrade plans only, each a `Command` or a `CommandThen`
 * (`isUpgradeCommand`): every field of one is on the wire, so `samePlan`
 * sees all of it. A path-list
 * uninstall's plan also carries what its preview found on disk, which
 * never leaves the backend (`PlanAction::TrashPaths`' `previewed` in
 * crates/banager-core/src/model.rs); worked out again, it would be checked
 * against the disk as it is now, not as the person was shown it.
 */
import { isUnknownPlan } from "./sources";
import type { IssuedPlan, OpRequest, Plan, PlanId } from "./types";

/** `MAX_ISSUED_PLANS` in crates/banager-core/src/session/plans.rs. */
export const PLANS_HELD = 1024;

/** `PLAN_LIFETIME` there (ten minutes), in milliseconds. */
export const PLAN_LIFETIME_MS = 10 * 60 * 1000;

/**
 * `BatchItem.submitError` for an update whose plan, worked out again, was
 * not the one shown: the page's own reason, never the backend's, worded by
 * the confirmation itself (`refusalOf`).
 */
export const CHANGED_SINCE_SHOWN = '{"kind":"changed_since_shown"}';

/** Whether two values read from the wire are the same, field for field, in any key order. */
function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((value, index) => sameValue(value, b[index]));
  }
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = Object.keys(left);
  return (
    keys.length === Object.keys(right).length &&
    keys.every((key) => Object.prototype.hasOwnProperty.call(right, key) && sameValue(left[key], right[key]))
  );
}

/**
 * Whether `again` is the plan that was shown: what it runs, its
 * environment, its notes, whether it asks for the password, whether it can
 * be cancelled -- every field the confirmation draws from and every one it
 * does not.
 */
export function samePlan(shown: Plan, again: Plan): boolean {
  return sameValue(shown, again);
}

/**
 * What `submit_operation_error` (src-tauri/src/ipc.rs) sends for a plan
 * older than its lifetime: said too of a plan let go once its batch is that
 * old, as the backend would have said it of that plan had it still held
 * it, so that one cause reads as one sentence (`planRefused.expired`).
 */
export const EXPIRED = '{"kind":"expired"}';

/**
 * What becomes of a plan of a batch the backend no longer holds
 * (`unknown`):
 *
 * - `asSent`: its refusal is said as it came -- a batch of no more than
 *   the backend holds, where nothing of it is let go but by accident;
 * - `planAgain`: it is worked out again (`startShown`) -- more than that,
 *   and no older than a plan's lifetime;
 * - `expired`: refused as `EXPIRED` -- more than that, and older.
 */
export type LetGo = "asSent" | "planAgain" | "expired";

/**
 * `LetGo` for a batch of `planned` plans, asked for at `askedAt`, at `now`:
 * `performance.now()` readings -- a clock nothing can set -- the first
 * taken before the first plan was asked for, so the age is never less than
 * the oldest plan's. Its lifetime's last moment is still within it, as
 * `has_expired` in plans.rs has it.
 */
export function letGoPolicy(planned: number, askedAt: number, now: number): LetGo {
  if (planned <= PLANS_HELD) return "asSent";
  return now - askedAt <= PLAN_LIFETIME_MS ? "planAgain" : "expired";
}

/**
 * Whether `plan` is an upgrade that runs a command -- or two: a Homebrew
 * update the `brew cleanup` of its old versions follows (`CommandThen`,
 * U9), every field of which is on the wire too. The only kind `startShown`
 * works out again.
 */
export function isUpgradeCommand(plan: Plan): boolean {
  return plan.request.kind === "Upgrade" && ("Command" in plan.action || "CommandThen" in plan.action);
}

/** What `startShown` asks the backend through: `plan_operation` and `submit_operation`. */
export interface StartThrough {
  plan(request: OpRequest): Promise<IssuedPlan>;
  submit(planId: PlanId): Promise<number>;
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * Starts the update the person was shown as `shown`, and resolves with its
 * operation's id. Rejects with the backend's refusal as it came,
 * `EXPIRED`, or `CHANGED_SINCE_SHOWN`.
 *
 * When the backend no longer holds `shown` (`unknown`) and `shown` is an
 * upgrade's command (`isUpgradeCommand`), `letGo` says what then: with
 * `planAgain`, `request` is planned again, and the new plan submitted only
 * when it is `shown`'s (`samePlan`) and the original monotonic
 * `batchDeadline` has not passed. Re-planning never renews that deadline.
 * Otherwise nothing more is asked.
 */
export async function startShown(
  shown: IssuedPlan,
  request: OpRequest,
  letGo: LetGo,
  through: StartThrough,
  batchDeadline: number,
): Promise<number> {
  try {
    return await through.submit(shown.id);
  } catch (e) {
    if (letGo === "asSent" || !isUpgradeCommand(shown.plan) || !isUnknownPlan(messageOf(e))) throw e;
    if (letGo === "expired" || performance.now() > batchDeadline) throw new Error(EXPIRED);
  }
  const again = await through.plan(request);
  if (!samePlan(shown.plan, again.plan)) throw new Error(CHANGED_SINCE_SHOWN);
  if (performance.now() > batchDeadline) throw new Error(EXPIRED);
  return through.submit(again.id);
}
