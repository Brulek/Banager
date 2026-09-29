import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { dragsWindow } from "../test/dragRegion";
import { CheckAgain, HeaderAction, PageHeader } from "./PageHeader";
import { refreshIntoCache } from "../lib/events";
import { queryKeys } from "../lib/queries";
import type { Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

// 2026-09-28 09:00:00 UTC, as unix seconds.
const CHECKED_AT = 1790586000;

function snapshotCheckedAt(refreshedAt: number | null, generation = 4): Snapshot {
  return {
    generation,
    round: generation,
    detect: "Found",
    instances: [],
    artifacts: [],
    updates: [],
    refreshed_at: refreshedAt,
    stale: false,
    errors: [],
  };
}

/** A `refresh` reply the test hands back when it chooses. */
function deferredRefresh() {
  let resolve: (snapshot: Snapshot) => void = () => {};
  let reject: (reason: string) => void = () => {};
  const promise = new Promise<Snapshot>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

let refreshReply: () => Promise<Snapshot>;

beforeEach(() => {
  mockInvoke.mockReset();
  refreshReply = () => Promise.resolve(snapshotCheckedAt(CHECKED_AT));
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshotCheckedAt(CHECKED_AT));
    if (cmd === "refresh") return refreshReply();
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("PageHeader", () => {
  it("shows the page's title as its heading", () => {
    const { getByRole } = renderWithProviders(<PageHeader title="Updates" />);
    expect(getByRole("heading", { level: 1, name: "Updates" })).toBeInTheDocument();
  });

  it("has Check again, the page's own action in its place, or nothing at all on its right", async () => {
    const checks = renderWithProviders(<PageHeader title="Updates" actions={<CheckAgain />} />);
    expect(checks.getByRole("button", { name: "Check again" })).toBeInTheDocument();
    expect(await checks.findByText(/^Checked /)).toBeInTheDocument();
    checks.unmount();

    const own = renderWithProviders(<PageHeader title="Unknown" actions={<button type="button">Scan again</button>} />);
    expect(own.getAllByRole("button").map((button) => button.textContent)).toEqual(["Scan again"]);
    expect(own.queryByText(/^Checked /)).toBeNull();
    own.unmount();

    const none = renderWithProviders(<PageHeader title="Settings" actions={null} />);
    expect(none.queryByRole("button")).toBeNull();
    expect(none.queryByText(/^Checked /)).toBeNull();
    // As tall with nothing on the right as with a button.
    expect(none.getByRole("heading", { level: 1 }).nextElementSibling?.className).toContain("min-h-8");
  });

  it("moves the window from anywhere but its controls, on the traffic lights' line", async () => {
    const { getByRole, findByText } = renderWithProviders(<PageHeader title="Updates" />);
    const title = getByRole("heading", { level: 1, name: "Updates" });
    const header = title.closest("header") as HTMLElement;
    const time = await findByText(/^Checked /);
    const checkAgain = getByRole("button", { name: "Check again" });

    // The title, the time and the space around them drag the window...
    expect(dragsWindow(header)).toBe(true);
    expect(dragsWindow(title)).toBe(true);
    expect(dragsWindow(time)).toBe(true);
    // ...and Check again, its icon too, stays a button.
    expect(dragsWindow(checkAgain)).toBe(false);
    expect(dragsWindow(checkAgain.querySelector("svg") as Element)).toBe(false);

    // A double-click zooms the window and selects no word of the title.
    expect(header.className).toContain("select-none");
    // Its 32px row 10px down: centred 26px down, as the traffic lights are
    // (src/test/windowChrome.test.ts).
    expect(header.className).toContain("pt-2.5");
  });

  it("draws any page's look again the one way: when, then the button", () => {
    const onPress = vi.fn();
    const { getByRole, getByText, rerender } = renderWithProviders(
      <HeaderAction status={{ text: "Scanned just now", failed: false }} label="Scan again" onPress={onPress} busy={false} />,
    );
    const button = getByRole("button", { name: "Scan again" });
    expect(getByText("Scanned just now").compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.click(button);
    expect(onPress).toHaveBeenCalledTimes(1);

    rerender(<HeaderAction status={{ text: "Couldn't scan", failed: true }} label="Scan again" onPress={onPress} busy />);
    expect(getByRole("alert")).toHaveTextContent("Couldn't scan");
    expect(getByRole("button", { name: "Scan again" })).toBeDisabled();
  });

  it("says how long ago the last check finished, and moves on every minute", async () => {
    // Only the clock and the timer the header ticks on are fake: the
    // snapshot still arrives the way it does in the app.
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT + 3 * 60 + 10) * 1000);
    const { findByText, getByText } = renderWithProviders(<PageHeader title="Updates" />);

    expect(await findByText("Checked 3 min ago")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(getByText("Checked 4 min ago")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(57 * 60_000);
    });
    expect(getByText("Checked 1 hour ago")).toBeInTheDocument();
  });

  it("says just now for a check that finished under a minute ago", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT + 20) * 1000);
    const { findByText } = renderWithProviders(<PageHeader title="Updates" />);

    expect(await findByText("Checked just now")).toBeInTheDocument();
  });

  it("moves on to a check made after the clock was set back, though it found nothing new", async () => {
    // Round 4 was stamped at 09:00, and then the clock was put back an
    // hour. The daily check's round 5 finds the same things at 08:00 --
    // the same generation -- and is announced and fetched. Judged by the
    // clock it was older than round 4 and never taken: the header went on
    // timing round 4, an hour ahead of the clock, and said "just now" of
    // it for the hour.
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT - 3600) * 1000);
    const { findByText, getByText, queryClient } = renderWithProviders(<PageHeader title="Updates" />);
    await findByText("Checked just now");

    const setBack: Snapshot = { ...snapshotCheckedAt(CHECKED_AT - 3600), round: 5 };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(setBack);
      return Promise.resolve(undefined);
    });
    await act(async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
    });
    expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(setBack);

    act(() => {
      vi.advanceTimersByTime(3 * 60_000);
    });
    expect(getByText("Checked 3 min ago")).toBeInTheDocument();
  });

  it("runs the app's refresh on Check again, off and saying Checking… until it is done", async () => {
    const reply = deferredRefresh();
    refreshReply = () => reply.promise;
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT + 10 * 60) * 1000);
    const { findByText, getByRole, getByText, queryClient } = renderWithProviders(
      <PageHeader title="Updates" />,
    );
    await findByText("Checked 10 min ago");
    const button = getByRole("button", { name: "Check again" });
    expect(button).toBeEnabled();

    fireEvent.click(button);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    await waitFor(() => expect(button).toBeDisabled());
    expect(getByText("Checking…")).toBeInTheDocument();
    // A second click has nothing to press.
    fireEvent.click(button);
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(1);

    const newer = snapshotCheckedAt(CHECKED_AT + 10 * 60, 5);
    await act(async () => {
      reply.resolve(newer);
    });

    await waitFor(() => expect(button).toBeEnabled());
    expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(newer);
    expect(getByText("Checked just now")).toBeInTheDocument();
  });

  it("is off while a refresh it did not start is running", async () => {
    // The startup refresh, or the one after an operation: the same
    // coordinator, started from outside the header.
    const reply = deferredRefresh();
    refreshReply = () => reply.promise;
    const { findByText, getByRole, queryClient } = renderWithProviders(
      <PageHeader title="Installed" />,
    );
    await findByText(/^Checked /);

    let run: Promise<void> = Promise.resolve();
    act(() => {
      run = refreshIntoCache(queryClient, "test");
    });

    const button = getByRole("button", { name: "Check again" });
    await waitFor(() => expect(button).toBeDisabled());
    expect(await findByText("Checking…")).toBeInTheDocument();

    await act(async () => {
      reply.resolve(snapshotCheckedAt(CHECKED_AT + 60, 5));
      await run;
    });
    await waitFor(() => expect(button).toBeEnabled());
  });

  it("says the check failed when the refresh does, until one works", async () => {
    refreshReply = () => Promise.reject("the session is gone");
    const { findByRole, findByText, getByRole, queryByRole } = renderWithProviders(
      <PageHeader title="Updates" />,
    );
    await findByText(/^Checked /);

    fireEvent.click(getByRole("button", { name: "Check again" }));

    expect(await findByRole("alert")).toHaveTextContent("Couldn't check");
    expect(getByRole("button", { name: "Check again" })).toBeEnabled();

    refreshReply = () => Promise.resolve(snapshotCheckedAt(Math.floor(Date.now() / 1000), 5));
    fireEvent.click(getByRole("button", { name: "Check again" }));

    expect(await findByText("Checked just now")).toBeInTheDocument();
    expect(queryByRole("alert")).not.toBeInTheDocument();
  });
});
