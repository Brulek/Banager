import type { OpRequest, Plan, PlanAction, Warning } from "../lib/types";
import type { LogLine } from "./mockPlans";

/**
 * The preview's stand-in for what `BrewAdapter::plan` builds for a keg-only
 * Homebrew formula linked into the prefix with `brew link`, whose link
 * Homebrew recorded (y1-keg; crates/banager-core/src/adapters/brew/mod.rs,
 * `relink_after_upgrade`, and `brew::links`): its update is followed by
 * `brew link --formula --force <name>`, run only if Homebrew did not link
 * it back, before any `brew cleanup`, and its preview says so first. The
 * pretend prefix has node@22 linked with `brew link --force`, so after the
 * update Homebrew links it back itself and the log says no link was
 * needed. The other such formula, openssl@3, has its `bin/openssl` held
 * by another program: its update is not offered
 * (`UpdateBlocked::LinkTaken`, mockData.ts), and its row names the file.
 */
const LINKED_BY_HAND: Record<string, string[]> = {
  "node@22": ["corepack", "node", "npm", "npx"],
};

/** `plan`, with the `brew link` after the update of a formula linked by hand. */
export function withMockKegLinks(plan: Plan, request: OpRequest): Plan {
  if (request.kind !== "Upgrade" || request.artifact_kind !== "Formula" || !request.instance_id.startsWith("brew:")) {
    return plan;
  }
  const commands = LINKED_BY_HAND[request.name];
  if (commands === undefined) return plan;
  const link = ["link", "--formula", "--force", request.name];
  let action: PlanAction;
  if ("Command" in plan.action) {
    action = { CommandThen: { ...plan.action.Command, then: [link] } };
  } else if ("CommandThen" in plan.action) {
    action = { CommandThen: { ...plan.action.CommandThen, then: [link, ...plan.action.CommandThen.then] } };
  } else {
    return plan;
  }
  const relinks: Warning = { HomebrewRelinksAfterUpdate: { name: request.name, commands } };
  return { ...plan, action, warnings: [relinks, ...plan.warnings] };
}

/** What the log says once such an update succeeded: Homebrew linked it back itself. */
export function mockRelinkLines(plan: Plan): LogLine[] {
  const links =
    "CommandThen" in plan.action && plan.action.CommandThen.then.some((argv) => argv[0] === "link");
  return links ? [{ note: { StillLinkedAfterUpdate: { name: plan.request.name } } }] : [];
}
