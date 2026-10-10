import type { ReactNode } from "react";
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { I18nextProvider } from "react-i18next";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import i18n from "../i18n";
import { PLAN_LIFETIME_MS, PLANS_HELD } from "../lib/heldPlans";
import { queryKeys } from "../lib/queryKeys";
import type { OpSummary, IssuedPlan, OpRequest, Plan, Settings, UpdateCandidate } from "../lib/types";
import { useUpdateConfirm } from "./UpdateConfirm";

// Update all of more tools than the backend holds plans for
// (src/lib/heldPlans.ts): the whole flow of the confirmation -- every plan
// asked for at once, then Update -- against a backend that holds at most
// `PLANS_HELD` and lets the oldest go, as `Session::issue` does, and
// refuses one held past `PLAN_LIFETIME_MS` as expired, as `Session::submit`
// does -- by `performance.now()`, the page's clock, set by the test.

const mockInvoke = vi.mocked(invoke);

const SETTINGS: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

const candidate = (name: string): UpdateCandidate => ({
  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
  current: "1.0.0",
  target: "1.1.0",
  channel: "Native",
  checkable: true,
  warnings: [],
  blocked: null,
});

function brewPlan(request: OpRequest, program = "/opt/homebrew/bin/brew"): Plan {
  return {
    request,
    action: { Command: { program, args: ["upgrade", "--formula", request.name], env: [] } },
    needs_password: false,
    locks: ["brew:/opt/homebrew"],
    cancel_policy: "KillThenReconcile",
    warnings: [],
    affected: [],
    timeout_secs: 1800,
  };
}

let held: Map<string, IssuedPlan & { at: number }>;
let submitted: IssuedPlan[];
let planned: string[];
/** A name whose plan comes out different the second time it is worked out. */
let changesWhenPlannedAgain: string | null;
/** The page's clock (`performance.now()`), and the backend's. */
let clock: number;
/** Where the clock is once the plans are worked out: how long preparing took. */
let preparedAt: number;
/** How far the clock moves while a plan is worked out (`plan_operation`). */
let planCost: number;
let clockSpy: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  held = new Map();
  submitted = [];
  planned = [];
  changesWhenPlannedAgain = null;
  clock = 0;
  preparedAt = 0;
  planCost = 0;
  clockSpy = vi.spyOn(performance, "now").mockImplementation(() => clock);
  let next = 0;
  mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
    if (cmd === "get_settings") return Promise.resolve(SETTINGS);
    if (cmd === "plan_operation") {
      const { request } = args as { request: OpRequest };
      clock = Math.max(clock, preparedAt) + planCost;
      const again = planned.includes(request.name);
      planned.push(request.name);
      while (held.size >= PLANS_HELD) held.delete(held.keys().next().value!);
      next += 1;
      const plan =
        again && request.name === changesWhenPlannedAgain
          ? brewPlan(request, "/usr/local/bin/brew")
          : brewPlan(request);
      const issued: IssuedPlan = { id: next.toString(16).padStart(32, "0"), plan, issued_at: 1_790_000_000 };
      held.set(issued.id, { ...issued, at: clock });
      return Promise.resolve(issued);
    }
    if (cmd === "submit_operation") {
      const { planId } = args as { planId: string };
      const issued = held.get(planId);
      // A bare string, as a `Result<_, String>` command rejects.
      if (issued === undefined) return Promise.reject(JSON.stringify({ kind: "unknown" }));
      if (clock - issued.at > PLAN_LIFETIME_MS) return Promise.reject(JSON.stringify({ kind: "expired" }));
      held.delete(planId);
      submitted.push(issued);
      return Promise.resolve(submitted.length);
    }
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  mockInvoke.mockReset();
  clockSpy.mockRestore();
});

function renderConfirm() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(I18nextProvider, { i18n }, children),
    );
  const rendered = renderHook(
    () =>
      useUpdateConfirm({
        nameOf: (c) => c.key.name,
        compare: (a, b) => a.key.name.localeCompare(b.key.name),
        sourceLabelFor: () => "Homebrew",
      }),
    { wrapper },
  );
  return { ...rendered, client };
}

const tools = (count: number) => Array.from({ length: count }, (_, i) => `tool-${String(i).padStart(4, "0")}`);

describe("Update all of more tools than the backend holds plans for", () => {
  it("starts every one of 1,100, each with the plan shown on the sheet", async () => {
    const names = tools(1100);
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(names.map(candidate)));
    const batch = result.current.batch!;
    expect(batch.phase).toBe("ready");
    expect(batch.items.every((item) => item.issued !== null)).toBe(true);
    // The first plans are no longer held as Update is pressed.
    expect(held.size).toBe(PLANS_HELD);
    const shown = new Map(batch.items.map((item) => [item.name, item.issued!.plan]));

    await act(() => result.current.confirmAndSubmit());
    // Every one started: the sheet closes, as on a batch that all started.
    expect(result.current.batch).toBeNull();
    expect(submitted.map((issued) => issued.plan.request.name)).toEqual(names);
    for (const issued of submitted) expect(issued.plan).toEqual(shown.get(issued.plan.request.name));
    expect(new Set(submitted.map((issued) => issued.id)).size).toBe(1100);
  });

  it("does not start one whose plan came out different, and says why under it, in either language", async () => {
    changesWhenPlannedAgain = "tool-0005";
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(tools(1100).map(candidate)));
    await act(() => result.current.confirmAndSubmit());

    const batch = result.current.batch!;
    expect(batch.phase).toBe("done");
    const changed = batch.items.find((item) => item.name === "tool-0005")!;
    expect(changed.submittedOpId).toBeNull();
    expect(submitted.some((issued) => issued.plan.request.name === "tool-0005")).toBe(false);
    expect(batch.items.filter((item) => item.submittedOpId !== null)).toHaveLength(1099);
    expect(result.current.refusalOf(changed)).toEqual({
      text: "Couldn't start the update: This update changed after it was shown, so it didn't run. Open it again and confirm.",
      detail:
        "With this many updates at once, some are prepared again just before they start. This one came out different from what was shown here.",
    });
    await act(() => i18n.changeLanguage("zh-CN"));
    try {
      expect(result.current.refusalOf(changed)?.text).toBe(
        "无法开始更新：这项更新在显示之后有了变化，因此未执行。请重新打开并确认。",
      );
    } finally {
      await act(() => i18n.changeLanguage("en"));
    }
  });

  it("plans nothing again in a batch the backend holds all of", async () => {
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(tools(PLANS_HELD).map(candidate)));
    await act(() => result.current.confirmAndSubmit());
    expect(result.current.batch).toBeNull();
    expect(planned).toHaveLength(PLANS_HELD);
    expect(submitted).toHaveLength(PLANS_HELD);
  });
  it("goes by how long ago the plans were asked for, not since they came back", async () => {
    // Preparing took five minutes; Update is pressed five and a half after
    // that -- ten and a half after the plans were asked for. The ones let
    // go are older than a plan may be, and are refused, not worked out
    // again; those still held are five and a half minutes old, and start.
    preparedAt = 5 * 60_000;
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(tools(1100).map(candidate)));
    clock = preparedAt + 5.5 * 60_000;
    await act(() => result.current.confirmAndSubmit());
    const batch = result.current.batch!;
    expect(batch.phase).toBe("done");
    expect(planned).toHaveLength(1100);
    const refused = batch.items.filter((item) => item.submitError !== null);
    expect(refused.map((item) => item.name)).toEqual(tools(1100 - PLANS_HELD));
    expect(submitted).toHaveLength(PLANS_HELD);
  });

  it("starts them all when Update is pressed within the plans' lifetime of asking for them", async () => {
    preparedAt = 5 * 60_000;
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(tools(1100).map(candidate)));
    clock = 9 * 60_000;
    await act(() => result.current.confirmAndSubmit());
    expect(result.current.batch).toBeNull();
    expect(submitted).toHaveLength(1100);
  });

  it("refuses a let-go update whose planning again ends past the ten minutes, though it began within them", async () => {
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(tools(1100).map(candidate)));
    // Update is pressed half a second before the ten minutes since the
    // plans were asked for are up, and working the first let-go plan out
    // again takes a second: the new plan is fresh, the batch is not.
    clock = PLAN_LIFETIME_MS - 500;
    planCost = 1000;
    await act(() => result.current.confirmAndSubmit());
    const batch = result.current.batch!;
    const first = batch.items.find((item) => item.name === "tool-0000")!;
    expect(planned.filter((name) => name === "tool-0000")).toHaveLength(2);
    expect(first.submittedOpId).toBeNull();
    expect(submitted).toEqual([]);
    expect(result.current.refusalOf(first)?.text).toBe(
      "Couldn't start the update: This confirmation is more than 10 minutes old, so nothing ran. Open it again and confirm.",
    );
  });

  it("says the same sentence of every update once the sheet is older than a plan's lifetime, let go or held", async () => {
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(tools(1100).map(candidate)));
    clock = PLAN_LIFETIME_MS + 1;
    await act(() => result.current.confirmAndSubmit());
    const batch = result.current.batch!;
    expect(submitted).toEqual([]);
    expect(planned).toHaveLength(1100);
    const said = new Set(batch.items.map((item) => result.current.refusalOf(item)?.text));
    expect([...said]).toEqual([
      "Couldn't start the update: This confirmation is more than 10 minutes old, so nothing ran. Open it again and confirm.",
    ]);
  });

  it("starts each update once when Update is pressed twice before the page is drawn again", async () => {
    const { result } = renderConfirm();
    await act(() => result.current.openConfirm(tools(1100).map(candidate)));
    // Both presses reach the same ready batch.
    const { confirmAndSubmit } = result.current;
    await act(() => Promise.all([confirmAndSubmit(), confirmAndSubmit()]));
    expect(submitted.map((issued) => issued.plan.request.name)).toEqual(tools(1100));
    expect(result.current.batch).toBeNull();
  });
});

it.each(["Queued", "Running"] as const)("rechecks a held batch against a %s uninstall at confirmation", async (status) => {
  const { result, client } = renderConfirm();
  await act(() => result.current.openConfirm([candidate("jq"), candidate("wget")]));
  const pending: OpSummary = { id: 90, kind: "Uninstall", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "jq", status, outcome: null, argv_preview: [], cancel_policy: "KillThenReconcile" };
  // No render between the cache update and confirming the held callback.
  const confirm = result.current.confirmAndSubmit;
  client.setQueryData(queryKeys.operations, [pending]);
  await act(() => confirm());
  expect(submitted.map((issued) => issued.plan.request.name)).toEqual(["wget"]);
  const refused = result.current.batch!.items.find((item) => item.name === "jq")!;
  expect(refused.submittedOpId).toBeNull();
  expect(result.current.refusalOf(refused)?.text).toContain("Another operation for this tool hasn't finished");
});
