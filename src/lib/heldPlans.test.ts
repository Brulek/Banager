import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  CHANGED_SINCE_SHOWN,
  mayPlanAgain,
  PLAN_LIFETIME_MS,
  PLANS_HELD,
  samePlan,
  startShown,
  type StartThrough,
} from "./heldPlans";
import type { IssuedPlan, OpRequest, Plan } from "./types";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PLANS_RS = readFileSync(path.join(ROOT, "crates/banager-core/src/session/plans.rs"), "utf-8");

const request = (name: string): OpRequest => ({
  kind: "Upgrade",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Formula",
  name,
});

/** What brew's upgrade plan for `req` looks like (`BrewAdapter::plan`). */
function brewPlan(req: OpRequest, more: Partial<Plan> = {}): Plan {
  return {
    request: req,
    action: {
      Command: {
        program: "/opt/homebrew/bin/brew",
        args: ["upgrade", "--formula", req.name],
        env: [["HOMEBREW_NO_AUTO_UPDATE", "1"]],
      },
    },
    needs_password: false,
    locks: ["brew:/opt/homebrew"],
    cancel_policy: "KillThenReconcile",
    warnings: [],
    affected: [],
    timeout_secs: 1800,
    ...more,
  };
}

/**
 * The backend's held plans, as `Session::issue` and `Session::submit` keep
 * them: at most `PLANS_HELD`, the oldest let go first when one more is
 * issued, each submitted at most once, anything not held refused as
 * `unknown` (`submit_operation_error` in src-tauri/src/ipc.rs).
 * `planFor` is what planning `req` for the `nth` time (from 1) comes out
 * as.
 */
function heldBackend(planFor: (req: OpRequest, nth: number) => Plan = (req) => brewPlan(req)) {
  const held = new Map<string, IssuedPlan>();
  const times = new Map<string, number>();
  const planned: OpRequest[] = [];
  const submitted: IssuedPlan[] = [];
  let next = 0;
  const through: StartThrough = {
    async plan(req) {
      planned.push(req);
      while (held.size >= PLANS_HELD) held.delete(held.keys().next().value!);
      const nth = (times.get(req.name) ?? 0) + 1;
      times.set(req.name, nth);
      next += 1;
      const issued: IssuedPlan = { id: next.toString(16).padStart(32, "0"), plan: planFor(req, nth), issued_at: 0 };
      held.set(issued.id, issued);
      return issued;
    },
    async submit(planId) {
      const issued = held.get(planId);
      if (issued === undefined) throw new Error(JSON.stringify({ kind: "unknown" }));
      held.delete(planId);
      submitted.push(issued);
      return submitted.length;
    },
  };
  return { through, held, planned, submitted };
}

const names = (count: number) => Array.from({ length: count }, (_, i) => `tool-${String(i).padStart(4, "0")}`);

/** Plans every update at once, as `openConfirm` does, and what was shown of each. */
async function preview(backend: ReturnType<typeof heldBackend>, tools: string[]): Promise<IssuedPlan[]> {
  return Promise.all(tools.map((name) => backend.through.plan(request(name))));
}

/** Starts each in turn, as `confirmAndSubmit` does: each one's opId or what it was refused with. */
async function startEach(
  backend: ReturnType<typeof heldBackend>,
  shown: IssuedPlan[],
  planAgain: boolean,
): Promise<Array<number | string>> {
  const out: Array<number | string> = [];
  for (const issued of shown) {
    try {
      out.push(await startShown(issued, issued.plan.request, planAgain, backend.through));
    } catch (e) {
      out.push((e as Error).message);
    }
  }
  return out;
}

describe("PLANS_HELD and PLAN_LIFETIME_MS", () => {
  it("are the backend's own numbers", () => {
    expect(PLANS_RS).toContain(`pub(crate) const MAX_ISSUED_PLANS: usize = ${PLANS_HELD};`);
    expect(PLANS_RS).toContain(
      `pub(crate) const PLAN_LIFETIME: Duration = Duration::from_secs(${PLAN_LIFETIME_MS / 1000});`,
    );
  });
});

describe("startShown, for Update all of more tools than the backend holds", () => {
  it("is what the review found without it: the first plans of 1,100 are let go before Update is pressed", async () => {
    const backend = heldBackend();
    const shown = await preview(backend, names(1100));
    expect(backend.held.size).toBe(PLANS_HELD);
    const started = await startEach(backend, shown, false);
    const refused = started.filter((result) => typeof result === "string");
    expect(refused).toHaveLength(1100 - PLANS_HELD);
    expect(new Set(refused)).toEqual(new Set([JSON.stringify({ kind: "unknown" })]));
  });

  it("starts every one of 1,100, each with exactly the plan shown, planning again only those let go", async () => {
    const backend = heldBackend();
    const tools = names(1100);
    const shown = await preview(backend, tools);
    const started = await startEach(backend, shown, mayPlanAgain(shown.length, 0, 1000));
    expect(started.every((result) => typeof result === "number")).toBe(true);
    expect(backend.submitted.map((issued) => issued.plan.request.name)).toEqual(tools);
    backend.submitted.forEach((issued, i) => expect(samePlan(shown[i].plan, issued.plan)).toBe(true));
    // No plan id was submitted twice.
    expect(new Set(backend.submitted.map((issued) => issued.id)).size).toBe(1100);
    // The 76 let go while it was shown, and the one the first of them, planned
    // again with 1,024 held, let go in its turn: planned again once each.
    expect(backend.planned.length - 1100).toBe(1100 - PLANS_HELD + 1);
  });

  it("starts 3,000 too, the backend never holding more than it may", async () => {
    const backend = heldBackend();
    const shown = await preview(backend, names(3000));
    const started = await startEach(backend, shown, mayPlanAgain(shown.length, 0, 0));
    expect(started.filter((result) => typeof result === "number")).toHaveLength(3000);
    expect(backend.held.size).toBeLessThanOrEqual(PLANS_HELD);
  });

  it("does not start one whose plan came out different, and starts the rest", async () => {
    // tool-0003's second plan names a different command: it was let go,
    // and what it would run now is not what was shown.
    const backend = heldBackend((req, nth) =>
      req.name === "tool-0003" && nth > 1
        ? brewPlan(req, { action: { Command: { program: "/usr/local/bin/brew", args: ["upgrade", req.name], env: [] } } })
        : brewPlan(req),
    );
    const shown = await preview(backend, names(1100));
    const started = await startEach(backend, shown, true);
    expect(started[3]).toBe(CHANGED_SINCE_SHOWN);
    expect(started.filter((result) => typeof result === "number")).toHaveLength(1099);
    expect(backend.submitted.some((issued) => issued.plan.request.name === "tool-0003")).toBe(false);
  });

  it("does not start one whose notes came out different either", async () => {
    const backend = heldBackend((req, nth) =>
      req.name === "tool-0000" && nth > 1 ? brewPlan(req, { needs_password: true }) : brewPlan(req),
    );
    const shown = await preview(backend, names(1100));
    expect((await startEach(backend, shown, true))[0]).toBe(CHANGED_SINCE_SHOWN);
  });

  it("says the backend's refusal as it came when planning again is refused, or when it was not let go", async () => {
    const listed = heldBackend();
    const shown = await preview(listed, names(1100));
    // The update is gone from the list since: planning it again is refused.
    listed.through.plan = async () => {
      throw new Error(JSON.stringify({ kind: "not_listed" }));
    };
    await expect(startShown(shown[0], shown[0].plan.request, true, listed.through)).rejects.toThrow(
      JSON.stringify({ kind: "not_listed" }),
    );
    // Held, but refused for another reason: nothing is planned again.
    const expired = heldBackend();
    const [one] = await preview(expired, ["jq"]);
    expired.through.submit = async () => {
      throw new Error(JSON.stringify({ kind: "expired" }));
    };
    await expect(startShown(one, one.plan.request, true, expired.through)).rejects.toThrow(
      JSON.stringify({ kind: "expired" }),
    );
    expect(expired.planned).toHaveLength(1);
  });
});

describe("mayPlanAgain", () => {
  it("is only for a batch of more than the backend holds", () => {
    expect(mayPlanAgain(PLANS_HELD, 0, 0)).toBe(false);
    expect(mayPlanAgain(PLANS_HELD + 1, 0, 0)).toBe(true);
  });

  it("and only within a plan's lifetime of asking for them, its last moment included", () => {
    expect(mayPlanAgain(2000, 1000, 1000 + PLAN_LIFETIME_MS)).toBe(true);
    expect(mayPlanAgain(2000, 1000, 1000 + PLAN_LIFETIME_MS + 1)).toBe(false);
  });
});

describe("samePlan", () => {
  it("goes by every field, not by the order the keys came in", () => {
    const plan = brewPlan(request("jq"));
    const reordered = Object.fromEntries(Object.entries(plan).reverse()) as unknown as Plan;
    expect(samePlan(plan, reordered)).toBe(true);
    expect(samePlan(plan, brewPlan(request("jq"), { warnings: ["CompilesLocally"] }))).toBe(false);
    expect(samePlan(plan, brewPlan(request("jq"), { cancel_policy: "NoCancel" }))).toBe(false);
    expect(
      samePlan(
        plan,
        brewPlan(request("jq"), {
          action: { Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", "--formula", "jq"], env: [] } },
        }),
      ),
    ).toBe(false);
    expect(samePlan(plan, brewPlan(request("yq")))).toBe(false);
  });
});
