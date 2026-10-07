import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it, vi } from "vitest";
import {
  CHANGED_SINCE_SHOWN,
  EXPIRED,
  letGoPolicy,
  PLAN_LIFETIME_MS,
  PLANS_HELD,
  samePlan,
  startShown,
  type LetGo,
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
  letGo: LetGo,
): Promise<Array<number | string>> {
  const out: Array<number | string> = [];
  const batchDeadline = performance.now() + PLAN_LIFETIME_MS;
  for (const issued of shown) {
    try {
      out.push(await startShown(issued, issued.plan.request, letGo, backend.through, batchDeadline));
    } catch (e) {
      out.push((e as Error).message);
    }
  }
  return out;
}

const UNKNOWN = JSON.stringify({ kind: "unknown" });

describe("PLANS_HELD and PLAN_LIFETIME_MS", () => {
  it("are the backend's own numbers", () => {
    expect(PLANS_RS).toContain(`pub(crate) const MAX_ISSUED_PLANS: usize = ${PLANS_HELD};`);
    expect(PLANS_RS).toContain(
      `pub(crate) const PLAN_LIFETIME: Duration = Duration::from_secs(${PLAN_LIFETIME_MS / 1000});`,
    );
  });

  it("EXPIRED is what the backend sends for a plan past its lifetime", () => {
    expect(readFileSync(path.join(ROOT, "src-tauri/src/ipc.rs"), "utf-8")).toContain(
      'SubmitError::Expired => {\n            serde_json::json!({ "kind": "expired" }).to_string()',
    );
    expect(JSON.parse(EXPIRED)).toEqual({ kind: "expired" });
  });
});

describe("startShown, for Update all of more tools than the backend holds", () => {
  it("is what the review found without it: the first plans of 1,100 are let go before Update is pressed", async () => {
    const backend = heldBackend();
    const shown = await preview(backend, names(1100));
    expect(backend.held.size).toBe(PLANS_HELD);
    const started = await startEach(backend, shown, "asSent");
    const refused = started.filter((result) => typeof result === "string");
    expect(refused).toHaveLength(1100 - PLANS_HELD);
    expect(new Set(refused)).toEqual(new Set([UNKNOWN]));
  });

  it("starts every one of 1,100, each with exactly the plan shown, planning again only those let go", async () => {
    const backend = heldBackend();
    const tools = names(1100);
    const shown = await preview(backend, tools);
    const started = await startEach(backend, shown, letGoPolicy(shown.length, 0, 1000));
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
    const started = await startEach(backend, shown, letGoPolicy(shown.length, 0, 0));
    expect(started.filter((result) => typeof result === "number")).toHaveLength(3000);
    expect(backend.held.size).toBeLessThanOrEqual(PLANS_HELD);
  });

  it("refuses one let go as expired, in the held ones' words, once the batch is older than a plan's lifetime", async () => {
    const backend = heldBackend();
    const shown = await preview(backend, names(1100));
    const started = await startEach(backend, shown, letGoPolicy(shown.length, 0, PLAN_LIFETIME_MS + 1));
    expect(started.slice(0, 1100 - PLANS_HELD)).toEqual(Array(1100 - PLANS_HELD).fill(EXPIRED));
    // Nothing is planned again.
    expect(backend.planned).toHaveLength(1100);
  });

  it("allows a replacement at the original deadline, including a delay after planning resolves", async () => {
    const backend = heldBackend();
    const [shown] = await preview(backend, ["jq"]);
    backend.held.clear();
    let now = PLAN_LIFETIME_MS - 1000;
    const clock = vi.spyOn(performance, "now").mockImplementation(() => now);
    const plan = backend.through.plan;
    backend.through.plan = async (req) => {
      const replacement = await plan(req);
      queueMicrotask(() => { now = PLAN_LIFETIME_MS; });
      return replacement;
    };
    try {
      await expect(startShown(shown, shown.plan.request, "planAgain", backend.through, PLAN_LIFETIME_MS)).resolves.toBe(1);
      expect(backend.submitted).toHaveLength(1);
    } finally {
      clock.mockRestore();
    }
  });

  it.each(["submit", "plan", "continuation"] as const)("refuses an evicted plan when %s crosses the original batch deadline", async (delayed) => {
    const backend = heldBackend();
    const [shown] = await preview(backend, ["jq"]);
    backend.held.clear();
    let now = PLAN_LIFETIME_MS - 1000;
    const clock = vi.spyOn(performance, "now").mockImplementation(() => now);
    if (delayed === "continuation") {
      const plan = backend.through.plan;
      backend.through.plan = async (req) => {
        const replacement = await plan(req);
        queueMicrotask(() => { now += 5000; });
        return replacement;
      };
    } else if (delayed === "plan") {
      const plan = backend.through.plan;
      backend.through.plan = async (req) => { now += 5000; return plan(req); };
    } else {
      const submit = backend.through.submit;
      backend.through.submit = async (id) => { now += 5000; return submit(id); };
    }
    try {
      await expect(startShown(shown, shown.plan.request, "planAgain", backend.through, PLAN_LIFETIME_MS)).rejects.toThrow(EXPIRED);
      expect(backend.submitted).toEqual([]);
    } finally {
      clock.mockRestore();
    }
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
    const started = await startEach(backend, shown, "planAgain");
    expect(started[3]).toBe(CHANGED_SINCE_SHOWN);
    expect(started.filter((result) => typeof result === "number")).toHaveLength(1099);
    expect(backend.submitted.some((issued) => issued.plan.request.name === "tool-0003")).toBe(false);
  });

  it("does not start one whose notes came out different either", async () => {
    const backend = heldBackend((req, nth) =>
      req.name === "tool-0000" && nth > 1 ? brewPlan(req, { needs_password: true }) : brewPlan(req),
    );
    const shown = await preview(backend, names(1100));
    expect((await startEach(backend, shown, "planAgain"))[0]).toBe(CHANGED_SINCE_SHOWN);
  });

  it("says the backend's refusal as it came when planning again is refused, or when it was not let go", async () => {
    const listed = heldBackend();
    const shown = await preview(listed, names(1100));
    // The update is gone from the list since: planning it again is refused.
    listed.through.plan = async () => {
      throw new Error(JSON.stringify({ kind: "not_listed" }));
    };
    await expect(startShown(shown[0], shown[0].plan.request, "planAgain", listed.through, performance.now() + PLAN_LIFETIME_MS)).rejects.toThrow(
      JSON.stringify({ kind: "not_listed" }),
    );
    // Held, but refused for another reason: nothing is planned again.
    const expired = heldBackend();
    const [one] = await preview(expired, ["jq"]);
    expired.through.submit = async () => {
      throw new Error(EXPIRED);
    };
    await expect(startShown(one, one.plan.request, "planAgain", expired.through, performance.now() + PLAN_LIFETIME_MS)).rejects.toThrow(EXPIRED);
    expect(expired.planned).toHaveLength(1);
  });

  it("works out again only an upgrade's command: never an uninstall, least of all one that moves files to the Trash", async () => {
    const uninstall: OpRequest = { ...request("jq"), kind: "Uninstall" };
    const shown: IssuedPlan[] = [
      // brew's uninstall: a command, but not an upgrade.
      { id: "a".repeat(32), plan: brewPlan(uninstall), issued_at: 0 },
      // A tool with its own installer: its files to the Trash, with what the
      // preview found on disk kept by the backend alone.
      {
        id: "b".repeat(32),
        plan: brewPlan(uninstall, { action: { TrashPaths: { paths: ["~/.local/bin/claude"] } } }),
        issued_at: 0,
      },
    ];
    for (const issued of shown) {
      const backend = heldBackend();
      for (const letGo of ["planAgain", "expired"] as const) {
        await expect(startShown(issued, issued.plan.request, letGo, backend.through, performance.now() + PLAN_LIFETIME_MS)).rejects.toThrow(UNKNOWN);
      }
      expect(backend.planned).toEqual([]);
    }
  });
});

describe("startShown, for an update a brew cleanup follows (U9)", () => {
  it("works it out again like any upgrade's command, and starts it only when both commands are the ones shown", async () => {
    const cleanup = (req: OpRequest, then: string[][]): Plan =>
      brewPlan(req, {
        action: {
          CommandThen: {
            program: "/opt/homebrew/bin/brew",
            args: ["upgrade", "--formula", req.name],
            env: [["HOMEBREW_NO_AUTO_UPDATE", "1"]],
            then,
          },
        },
        warnings: [{ HomebrewCleansUpOldVersions: { versions: ["1.25.0"] } }],
      });
    const same = heldBackend((req) => cleanup(req, [["cleanup", req.name]]));
    const shown = await same.through.plan(request("wget"));
    await same.through.submit(shown.id);
    // Let go: planned again, and the same two commands start.
    await expect(startShown(shown, shown.plan.request, "planAgain", same.through, performance.now() + PLAN_LIFETIME_MS)).resolves.toBe(2);
    expect(same.planned).toHaveLength(2);
    // Planned again with another follow-up: not the plan shown, not started.
    const other = heldBackend((req, nth) =>
      cleanup(req, nth === 1 ? [["cleanup", req.name]] : [["cleanup", "--prune=all"]]),
    );
    const first = await other.through.plan(request("wget"));
    await other.through.submit(first.id);
    await expect(startShown(first, first.plan.request, "planAgain", other.through, performance.now() + PLAN_LIFETIME_MS)).rejects.toThrow(CHANGED_SINCE_SHOWN);
  });
});

describe("letGoPolicy", () => {
  it("says a refusal as it came in a batch of no more than the backend holds", () => {
    expect(letGoPolicy(PLANS_HELD, 0, 0)).toBe("asSent");
    expect(letGoPolicy(PLANS_HELD, 0, PLAN_LIFETIME_MS * 2)).toBe("asSent");
    expect(letGoPolicy(PLANS_HELD + 1, 0, 0)).toBe("planAgain");
  });

  it("plans again within a plan's lifetime of asking, its last moment included, and says expired after", () => {
    expect(letGoPolicy(2000, 1000, 1000 + PLAN_LIFETIME_MS)).toBe("planAgain");
    expect(letGoPolicy(2000, 1000, 1000 + PLAN_LIFETIME_MS + 1)).toBe("expired");
  });
});

describe("samePlan", () => {
  it("is not fooled by the order the keys came in", () => {
    const plan = brewPlan(request("jq"));
    const reordered = Object.fromEntries(Object.entries(plan).reverse()) as unknown as Plan;
    expect(samePlan(plan, reordered)).toBe(true);
  });

  // One change to each field of a plan: a field added to `Plan` and left
  // out here fails `tsc` (the `Record`) and the test below (`Object.keys`).
  const base = brewPlan(request("jq"));
  const CHANGED: Record<keyof Plan, Plan[keyof Plan]> = {
    // The request alone, its command still jq's.
    request: { ...base.request, name: "yq" },
    action: { Command: { program: "/usr/local/bin/brew", args: ["upgrade", "--formula", "jq"], env: [["HOMEBREW_NO_AUTO_UPDATE", "1"]] } },
    needs_password: true,
    locks: ["brew:/usr/local"],
    cancel_policy: "NoCancel",
    warnings: ["CompilesLocally"],
    affected: ["ffmpeg"],
    timeout_secs: 600,
  };

  it("tells apart a plan that differs in any one field", () => {
    expect(Object.keys(CHANGED).sort()).toEqual(Object.keys(base).sort());
    for (const key of Object.keys(base) as Array<keyof Plan>) {
      expect(samePlan(base, { ...base, [key]: CHANGED[key] }), key).toBe(false);
    }
  });

  it("tells apart a plan with a field the other has not, whichever was shown", () => {
    // A field this window's `Plan` does not know yet, on the wire all the
    // same: the confirmation drew nothing from it, and it is still compared.
    const more = { ...base, added_later: true } as Plan;
    expect(samePlan(base, more)).toBe(false);
    expect(samePlan(more, base)).toBe(false);
  });

  it("goes by the order of a command's arguments and environment, and counts each", () => {
    const command = (args: string[], env: [string, string][]): Plan => ({
      ...base,
      action: { Command: { program: "/opt/homebrew/bin/brew", args, env } },
    });
    const shown = command(["upgrade", "--formula", "jq"], [["A", "1"], ["B", "2"]]);
    expect(samePlan(shown, command(["upgrade", "--formula", "jq"], [["A", "1"], ["B", "2"]]))).toBe(true);
    // Arguments swapped.
    expect(samePlan(shown, command(["upgrade", "jq", "--formula"], [["A", "1"], ["B", "2"]]))).toBe(false);
    // Two variables swapped, and a name and value swapped within one.
    expect(samePlan(shown, command(["upgrade", "--formula", "jq"], [["B", "2"], ["A", "1"]]))).toBe(false);
    expect(samePlan(shown, command(["upgrade", "--formula", "jq"], [["1", "A"], ["B", "2"]]))).toBe(false);
    // A variable twice.
    expect(samePlan(shown, command(["upgrade", "--formula", "jq"], [["A", "1"], ["A", "1"], ["B", "2"]]))).toBe(false);
  });
});
