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
 * than `PLANS_HELD`: in a smaller one nothing of it can have been let go
 * -- a plan let go is always the oldest held, and nothing else plans while
 * the confirmation is up -- and an `unknown` refusal there is said as it
 * is. And only while the batch is younger than a plan's lifetime: a plan
 * held that long would be refused as expired, and one let go is not given
 * a longer life than it would have had.
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
 * Whether a batch's plans may have been let go, and may be worked out
 * again: more of them than the backend holds, and the batch asked for
 * them no longer than a plan's lifetime ago. `askedAt` and `now` are
 * `performance.now()` readings -- a clock nothing can set -- taken before
 * the first plan was asked for, so the age is never less than the oldest
 * plan's.
 */
export function mayPlanAgain(planned: number, askedAt: number, now: number): boolean {
  return planned > PLANS_HELD && now - askedAt <= PLAN_LIFETIME_MS;
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
 * operation's id. Rejects with the backend's refusal as it came, or with
 * `CHANGED_SINCE_SHOWN`.
 *
 * `planAgain` (`mayPlanAgain`): when the backend no longer holds `shown`
 * (`unknown`), `request` is planned again, and the new plan submitted only
 * when it is `shown`'s (`samePlan`). Otherwise -- or when it is not --
 * nothing more is asked.
 */
export async function startShown(
  shown: IssuedPlan,
  request: OpRequest,
  planAgain: boolean,
  through: StartThrough,
): Promise<number> {
  try {
    return await through.submit(shown.id);
  } catch (e) {
    if (!planAgain || !isUnknownPlan(messageOf(e))) throw e;
  }
  const again = await through.plan(request);
  if (!samePlan(shown.plan, again.plan)) throw new Error(CHANGED_SINCE_SHOWN);
  return through.submit(again.id);
}
