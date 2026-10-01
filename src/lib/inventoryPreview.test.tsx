import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, renderHook, waitFor, within } from "@testing-library/react";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke, Channel } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { watchDock } from "../test/dock";
import App from "../App";
import { isRefreshInFlight, useOperationEvents, writeInventoryPreview } from "./events";
import { useInventoryPreview } from "./inventoryPreview";
import { queryKeys } from "./queryKeys";
import type { InstalledArtifact, InventoryPreview, ManagerInstance, Settings, Snapshot } from "./types";
import { NO_FACTS } from "./types";

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  status: { unavailable: null, notes: [] },
  unverified_version: null,
  read_only_reason: null,
};

function formula(name: string, version: string): InstalledArtifact {
  return {
    key: { instance_id: brew.id, kind: "Formula", name },
    display_name: name,
    version,
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

const jq = formula("jq", "1.8.1");
const ripgrep = formula("ripgrep", "14.1.1");

/** `Snapshot::empty()`, what `get_snapshot` answers before a round commits. */
const STARTUP: Snapshot = {
  generation: 0,
  round: 0,
  detect: "Missing",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
};

const preview: InventoryPreview = { round: 1, instances: [brew], artifacts: [jq, ripgrep] };

/** The round the preview was of, committed: jq has an update. */
const answered: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [brew],
  artifacts: [jq, ripgrep],
  updates: [
    {
      key: jq.key,
      current: "1.8.1",
      target: "1.8.2",
      channel: "Native",
      checkable: true,
      warnings: [],
      blocked: null,
    },
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("writeInventoryPreview", () => {
  it("keeps the first check's list while the cache has only the startup placeholder, or nothing yet", () => {
    const empty = new QueryClient();
    writeInventoryPreview(empty, preview);
    expect(empty.getQueryData(queryKeys.inventoryPreview)).toEqual(preview);

    const starting = new QueryClient();
    starting.setQueryData(queryKeys.snapshot, STARTUP);
    writeInventoryPreview(starting, preview);
    expect(starting.getQueryData(queryKeys.inventoryPreview)).toEqual(preview);
    expect(starting.getQueryData(queryKeys.snapshot)).toBe(STARTUP);
  });

  it("drops a list that arrives after a snapshot: the refresh's reply can overtake the event", () => {
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.snapshot, answered);
    writeInventoryPreview(queryClient, preview);
    expect(queryClient.getQueryData(queryKeys.inventoryPreview)).toBeUndefined();
  });

  it("lets a later round's list replace an earlier one, and never the other way", () => {
    // A round dropped before it committed, and the next one's list.
    const queryClient = new QueryClient();
    const later: InventoryPreview = { ...preview, round: 2, artifacts: [jq] };
    writeInventoryPreview(queryClient, preview);
    writeInventoryPreview(queryClient, later);
    expect(queryClient.getQueryData(queryKeys.inventoryPreview)).toEqual(later);
    writeInventoryPreview(queryClient, preview);
    expect(queryClient.getQueryData(queryKeys.inventoryPreview)).toEqual(later);
  });
});

describe("useInventoryPreview", () => {
  function wrapper(queryClient: QueryClient) {
    return function Wrapper({ children }: { children: React.ReactNode }) {
      return React.createElement(QueryClientProvider, { client: queryClient }, children);
    };
  }

  it("hands out the list only while no snapshot has answered, and drops it for good once one has", async () => {
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "get_snapshot" ? Promise.resolve(STARTUP) : Promise.resolve(undefined),
    );
    const queryClient = new QueryClient();
    const { result } = renderHook(() => useInventoryPreview(), { wrapper: wrapper(queryClient) });
    expect(result.current).toBeNull();

    // React Query tells its observers a tick later: waited for.
    act(() => writeInventoryPreview(queryClient, preview));
    await waitFor(() => expect(result.current).toEqual(preview));

    act(() => queryClient.setQueryData(queryKeys.snapshot, answered));
    await waitFor(() => expect(result.current).toBeNull());
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.inventoryPreview)).toBeNull());
  });

  it("shows nothing early for a list with nothing in it", async () => {
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.snapshot, STARTUP);
    const { result } = renderHook(() => useInventoryPreview(), { wrapper: wrapper(queryClient) });
    act(() => writeInventoryPreview(queryClient, { ...preview, artifacts: [] }));
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.inventoryPreview)).toBeDefined());
    // A tick for the observers, which a list would have reached by now.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(result.current).toBeNull();
  });
});

describe("useOperationEvents", () => {
  it("keeps an InventoryPreview apart from the snapshot, which it neither writes nor refetches", async () => {
    let channel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") channel = (args as { channel: InstanceType<typeof Channel> }).channel;
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.snapshot, STARTUP);
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    renderHook(() => useOperationEvents(), {
      wrapper: ({ children }) => React.createElement(QueryClientProvider, { client: queryClient }, children),
    });
    await waitFor(() => expect(channel).not.toBeNull());

    channel!.onmessage({ InventoryPreview: preview });

    expect(queryClient.getQueryData(queryKeys.inventoryPreview)).toEqual(preview);
    expect(queryClient.getQueryData(queryKeys.snapshot)).toBe(STARTUP);
    expect(invalidate).not.toHaveBeenCalled();
  });
});

/**
 * The window at launch: `get_snapshot` answers the startup placeholder,
 * the first `refresh` is still checking until the test answers it, and
 * the events channel is the test's to send on.
 */
function launch() {
  // The refresh coordinator is the module's (src/lib/events.ts): a test
  // that left one running would hand this window its reply.
  expect(isRefreshInFlight()).toBe(false);
  let send: ((event: unknown) => void) | null = null;
  let answer: (snapshot: Snapshot) => void = () => {};
  const reports: unknown[] = [];
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "subscribe_events") {
      const channel = (args as { channel: InstanceType<typeof Channel> }).channel;
      send = (event) => channel.onmessage(event);
    }
    if (cmd === "get_snapshot") return Promise.resolve(STARTUP);
    if (cmd === "refresh") return new Promise<Snapshot>((resolve) => (answer = resolve));
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    if (cmd === "report_update_set") reports.push(args);
    return Promise.resolve(undefined);
  });
  // Rows need a height to be drawn in jsdom.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  const rendered = renderWithProviders(<App />);
  return {
    ...rendered,
    reports,
    subtitle: () => rendered.container.querySelector("[data-subtitle]")?.textContent ?? null,
    async preview(sent: InventoryPreview = preview) {
      await waitFor(() => expect(send).not.toBeNull());
      act(() => send!({ InventoryPreview: sent }));
    },
    async answer(snapshot: Snapshot = answered) {
      await act(async () => {
        answer(snapshot);
      });
      await waitFor(() => expect(isRefreshInFlight()).toBe(false));
    },
  };
}

describe("the window while the first check still checks for updates", () => {
  it("lists what it found on the Installed page, every Uninstall off and nothing called up to date", async () => {
    const app = launch();
    const { getByRole, findByRole, queryByRole, container } = app;
    try {
      fireEvent.click(getByRole("button", { name: "Installed" }));
      // Nothing yet: the first check's spinner, and why it takes a while.
      await waitFor(() => expect(container.querySelector("[data-first-check]")).not.toBeNull());

      await app.preview();
      const uninstallJq = await findByRole("button", { name: "Uninstall jq…" });
      expect(uninstallJq).toBeDisabled();
      expect(getByRole("button", { name: "Uninstall ripgrep…" })).toBeDisabled();
      expect(container.querySelector("[data-first-check]")).toBeNull();
      expect(app.subtitle()).toBe("Found 2 tools · Checking for updates…");
      // The list says once why every Uninstall is off, in its first line,
      // not as the same word on each of its rows; each button keeps it as
      // its tooltip.
      expect(queryByRole("button", { name: "Can't uninstall jq now" })).toBeNull();
      const line = container.querySelector("[data-preview-hold]") as HTMLElement;
      expect(line).toHaveTextContent("You can uninstall once the check is done");
      // One run of text, as a footnote is: not a flex row, whose gap would
      // set the ⓘ's last word apart from the words before it.
      expect(line.className).not.toMatch(/\b(flex|gap-\S+)\b/);
      const tail = line.querySelector("[data-info-tail]") as HTMLElement;
      expect(tail.parentElement).toBe(line);
      expect(tail).toHaveTextContent(/^done/);
      expect(uninstallJq.closest("[data-row-action-why]")).toHaveAttribute(
        "title",
        "Checking for updates. You can uninstall once it's done.",
      );

      // Its details: what it is, and nothing to do yet -- no Update, an
      // Uninstall held off, which its 「状态」 says, and no word on updates.
      fireEvent.click(getByRole("button", { name: "Details: jq" }));
      const inspector = await findByRole("complementary", { name: "jq" });
      expect(within(inspector).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
      expect(within(inspector).getByText("Can't uninstall now")).toBeInTheDocument();
      expect(within(inspector).queryByRole("button", { name: "Update" })).toBeNull();
      expect(within(inspector).queryByText("Up to date")).toBeNull();
      expect(queryByRole("button", { name: "Update" })).toBeNull();
    } finally {
      await app.answer();
    }

    // The round's own snapshot takes its place: the same rows, now with
    // what can be done on them, and the list held is dropped.
    await waitFor(() => expect(getByRole("button", { name: "Uninstall ripgrep…" })).toBeEnabled());
    expect(app.subtitle()).toBe("2 tools");
    expect(queryByRole("button", { name: "Can't uninstall jq now" })).toBeNull();
    expect(container.querySelector("[data-preview-hold]")).toBeNull();
    const inspector = getByRole("complementary", { name: "jq" });
    expect(within(inspector).getByRole("button", { name: "Update" })).toBeEnabled();
    await waitFor(() => expect(app.queryClient.getQueryData(queryKeys.inventoryPreview)).toBeNull());
  });

  it("says a row's own Homebrew mark before the wait every row shares, and both in its details", async () => {
    // The list knows Homebrew's mark already: a disabled cask's row says
    // 「已停用」, as it will once the check is done, not the first check's
    // 「暂时不能卸载」 that every row has. Its Uninstall is still held.
    const oldapp: InstalledArtifact = {
      ...formula("oldapp", "2.3.1"),
      key: { instance_id: brew.id, kind: "Cask", name: "oldapp" },
      facts: {
        ...NO_FACTS,
        homebrew: {
          deprecated: null,
          disabled: { date: "2026-09-01", reason: "fails_gatekeeper_check", replacement: null },
          caveats: null,
          other_versions: [],
        },
      },
    };
    const app = launch();
    const { getByRole, findByRole, queryByRole } = app;
    try {
      fireEvent.click(getByRole("button", { name: "Installed" }));
      await app.preview({ round: 1, instances: [brew], artifacts: [jq, oldapp] });
      expect(await findByRole("button", { name: "Uninstall oldapp…" })).toBeDisabled();
      expect(getByRole("button", { name: "Disabled: oldapp" })).toHaveTextContent("Disabled");
      expect(queryByRole("button", { name: "Can't uninstall oldapp now" })).toBeNull();
      // A row with no word of its own says nothing: the list's line says it.
      expect(queryByRole("button", { name: "Can't uninstall jq now" })).toBeNull();

      fireEvent.click(getByRole("button", { name: "Details: oldapp" }));
      const inspector = await findByRole("complementary", { name: "oldapp" });
      // The mark is said in the callout; 「状态」 keeps the wait.
      expect(inspector.querySelector("[data-inspector-callout] [data-homebrew-mark]")).not.toBeNull();
      const status = within(inspector).getByText("Status").nextElementSibling as HTMLElement;
      expect([...status.querySelectorAll("[data-status-word]")].map((word) => word.textContent)).toEqual([
        "Can't uninstall now",
      ]);
      expect(within(inspector).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    } finally {
      await app.answer({ ...answered, artifacts: [jq, oldapp] });
    }
  });

  it("leaves the Updates page, the Dock's badge and the update notification on the check itself", async () => {
    const dock = watchDock();
    const app = launch();
    const { getByRole, container } = app;
    try {
      fireEvent.click(getByRole("button", { name: "Updates" }));
      await app.preview();
      await waitFor(() => expect(getByRole("button", { name: "Installed" })).toHaveAccessibleDescription("2 installed"));
      // Still the spinner: a list with no updates in it is no answer
      // that there are none.
      await waitFor(() => expect(container.querySelector("[data-first-check]")).not.toBeNull());
      expect(within(container.querySelector("main") as HTMLElement).queryByRole("button", { name: /jq/ })).toBeNull();
      expect(app.subtitle()).toBe("Checking…");
      expect(getByRole("button", { name: "Updates" })).not.toHaveAccessibleDescription();
      expect(dock.counts().every((count) => count === undefined || count === 0)).toBe(true);
      expect(app.reports).toEqual([]);
    } finally {
      await app.answer();
    }
    await waitFor(() => expect(dock.badge()).toBe(1));
    await waitFor(() => expect(app.reports).toEqual([{ round: 1, updates: [{ key_id: "brew:/opt/homebrew|Formula|jq", target: "1.8.2" }] }]));
  });

  it("says on the Overview how many tools it found, with the way to them, and nothing about updates", async () => {
    const app = launch();
    const { getByRole, findByRole, queryByRole, container } = app;
    try {
      const status = await findByRole("heading", { level: 2, name: "Checking…" });
      expect(status.nextElementSibling?.textContent).toBe(
        "The first check looks up every tool's newest version online, and sometimes takes a minute or two.",
      );
      expect(queryByRole("button", { name: "See Tools" })).toBeNull();

      await app.preview();
      await waitFor(() =>
        expect(status.nextElementSibling?.textContent).toBe(
          "Found 2 tools. The first check looks up every tool's newest version online, and sometimes takes a minute or two.",
        ),
      );
      expect(status).toHaveTextContent("Checking…");
      expect(container.querySelector("[data-status]")?.getAttribute("data-status")).toBe("busy");

      fireEvent.click(getByRole("button", { name: "See Tools" }));
      expect(await findByRole("button", { name: "Uninstall jq…" })).toBeDisabled();
      expect(getByRole("heading", { level: 1, name: "Installed" })).toBeInTheDocument();
    } finally {
      await app.answer();
    }
  });

  it("drops a list that arrives after the check has answered", async () => {
    const app = launch();
    const { getByRole, findByRole } = app;
    fireEvent.click(getByRole("button", { name: "Installed" }));
    // The page's own look at the snapshot as it opens, done first: this
    // test sends no SnapshotChanged to set a later one going.
    await waitFor(() => expect(app.queryClient.isFetching()).toBe(0));
    await app.answer();
    expect(await findByRole("button", { name: "Uninstall jq…" })).toBeEnabled();
    await app.preview();
    expect(getByRole("button", { name: "Uninstall jq…" })).toBeEnabled();
    expect(app.subtitle()).toBe("2 tools");
    expect(app.queryClient.getQueryData(queryKeys.inventoryPreview)).toBeUndefined();
  });
});
