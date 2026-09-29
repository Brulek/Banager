import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { dragsWindow } from "../test/dragRegion";
import { CheckAgain, HeaderAction, PageHeader } from "./PageHeader";
import { ICON_BUTTON } from "./ui/controls";
import i18n from "../i18n";
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

/** The ⟳'s tooltip, where the time of the last check now is. */
function tooltipOf(button: HTMLElement): string {
  return button.getAttribute("title") ?? "";
}

describe("PageHeader", () => {
  it("shows the page's title as its heading", () => {
    const { getByRole } = renderWithProviders(<PageHeader title="Updates" />);
    expect(getByRole("heading", { level: 1, name: "Updates" })).toBeInTheDocument();
  });

  it("has Check again, the page's own action in its place, or nothing at all on its right", async () => {
    const checks = renderWithProviders(<PageHeader title="Updates" actions={<CheckAgain />} />);
    const checkAgain = checks.getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(tooltipOf(checkAgain)).toMatch(/^Check Again \(⌘R\) · Checked /));
    // When, in the tooltip only: no words beside the button.
    expect(checks.queryByText(/^Checked /)).toBeNull();
    checks.unmount();

    const own = renderWithProviders(<PageHeader title="Unknown" actions={<button type="button">Scan Again</button>} />);
    expect(own.getAllByRole("button").map((button) => button.textContent)).toEqual(["Scan Again"]);
    own.unmount();

    const none = renderWithProviders(<PageHeader title="Settings" actions={null} />);
    expect(none.queryByRole("button")).toBeNull();
    // As tall with nothing on the right as with a button: a toolbar's 52.
    expect(none.getByRole("heading", { level: 1 }).closest("header")?.className).toContain("h-13");
  });

  it("is a 52pt toolbar: the title 20 in, 13 bold, with its subtitle under it in the secondary colour", () => {
    const { getByRole, getByText } = renderWithProviders(
      <PageHeader title="Updates" subtitle={{ text: "10 Updates Available", failed: false }} actions={null} />,
    );
    const title = getByRole("heading", { level: 1, name: "Updates" });
    const header = title.closest("header") as HTMLElement;
    expect(header.className.split(" ")).toEqual(expect.arrayContaining(["h-13", "px-5", "items-center"]));
    // The window's own background: nothing of its own.
    expect(header.className).not.toMatch(/\bbg-/);
    expect(title.className).toContain("text-title");

    const subtitle = getByText("10 Updates Available");
    // Under the title, the two centred together.
    expect(subtitle.previousElementSibling).toBe(title);
    expect(subtitle.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted"]));
    expect(subtitle).not.toHaveAttribute("role");
  });

  it("says a failed check in its subtitle as an alert, in the danger colour", () => {
    const { getByRole } = renderWithProviders(
      <PageHeader title="Installed" subtitle={{ text: "Couldn't check", failed: true }} actions={null} />,
    );
    const alert = getByRole("alert");
    expect(alert).toHaveTextContent("Couldn't check");
    expect(alert.className).toContain("text-danger-text");
    expect(alert.className).not.toContain("text-muted");
  });

  it("has no subtitle where the page has none", () => {
    const { getByRole } = renderWithProviders(<PageHeader title="Overview" subtitle={null} actions={null} />);
    const title = getByRole("heading", { level: 1, name: "Overview" });
    expect(title.nextElementSibling).toBeNull();
  });

  it("draws a hairline along its foot only once the page under it has scrolled", () => {
    const { container, rerender } = renderWithProviders(<PageHeader title="Updates" actions={null} />);
    expect(container.querySelector("[data-scroll-edge]")).toBeNull();

    rerender(<PageHeader title="Updates" actions={null} scrolled />);
    const edge = container.querySelector("[data-scroll-edge]") as HTMLElement;
    expect(edge.className.split(" ")).toEqual(expect.arrayContaining(["h-px", "bg-separator", "bottom-0"]));

    rerender(<PageHeader title="Updates" actions={null} scrolled={false} />);
    expect(container.querySelector("[data-scroll-edge]")).toBeNull();
  });

  it("moves the window from anywhere but its controls, on the traffic lights' line", async () => {
    const { getByRole, getByText } = renderWithProviders(
      <PageHeader title="Updates" subtitle={{ text: "10 Updates Available", failed: false }} />,
    );
    const title = getByRole("heading", { level: 1, name: "Updates" });
    const header = title.closest("header") as HTMLElement;
    const checkAgain = getByRole("button", { name: "Check Again" });

    // The title, the subtitle and the space around them drag the window...
    expect(dragsWindow(header)).toBe(true);
    expect(dragsWindow(title)).toBe(true);
    expect(dragsWindow(getByText("10 Updates Available"))).toBe(true);
    // ...and Check again, its icon too, stays a button.
    expect(dragsWindow(checkAgain)).toBe(false);
    expect(dragsWindow(checkAgain.querySelector("svg") as Element)).toBe(false);

    // A double-click zooms the window and selects no word of the title.
    expect(header.className).toContain("select-none");
    // 52px, its row centred 26px down, as the traffic lights are
    // (src/test/windowChrome.test.ts).
    expect(header.className).toContain("h-13");
    expect(header.className).toContain("items-center");
  });

  it("draws any page's look again the one way: a ⟳ icon button, its name and tooltip, a spinner while it runs", () => {
    const onPress = vi.fn();
    const { getByRole, rerender } = renderWithProviders(
      <HeaderAction label="Scan Again" tooltip="Scan Again · Scanned just now" onPress={onPress} busy={false} />,
    );
    const button = getByRole("button", { name: "Scan Again" });
    expect(button).toHaveAttribute("title", "Scan Again · Scanned just now");
    // No words on it: its name is its label, its glyph 16 in a 28 box.
    expect(button.textContent).toBe("");
    expect(button.className).toBe(ICON_BUTTON);
    expect(button.querySelector("svg")).toHaveAttribute("width", "16");
    expect(button.querySelector(".motion-safe\\:animate-spin")).toBeNull();
    fireEvent.click(button);
    expect(onPress).toHaveBeenCalledTimes(1);

    rerender(<HeaderAction label="Scan Again" tooltip="Scan Again" onPress={onPress} busy />);
    const busy = getByRole("button", { name: "Scan Again" });
    expect(busy).toBeDisabled();
    // A 16px spinner in the same box, in the muted grey rather than a
    // switched-off button's.
    const spinner = busy.querySelector("svg") as SVGElement;
    expect(spinner).toHaveAttribute("width", "16");
    expect(spinner.getAttribute("class")).toContain("motion-safe:animate-spin");
    expect(spinner.getAttribute("class")).toContain("text-muted");
  });

  it("says how long ago the last check finished in Check again's tooltip, with its shortcut, and moves on every minute", async () => {
    // Only the clock and the timer the header ticks on are fake: the
    // snapshot still arrives the way it does in the app.
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT + 3 * 60 + 10) * 1000);
    const { getByRole } = renderWithProviders(<PageHeader title="Updates" />);
    const button = getByRole("button", { name: "Check Again" });

    await waitFor(() => expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked 3 min ago"));

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked 4 min ago");

    act(() => {
      vi.advanceTimersByTime(57 * 60_000);
    });
    expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked 1 hour ago");
  });

  it("says in Chinese as the toolbar's tooltip does: 「重新检查（⌘R）· 上次检查：…」", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT + 3 * 60 + 10) * 1000);
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    try {
      const { getByRole } = renderWithProviders(<PageHeader title="更新" />);
      const button = getByRole("button", { name: "重新检查" });
      const checked = i18n.t("header.checkedMinutesAgo", { count: 3 });
      await waitFor(() => expect(tooltipOf(button)).toBe(`重新检查（⌘R）· ${checked}`));
    } finally {
      await act(async () => {
        await i18n.changeLanguage("en");
      });
    }
  });

  it("says just now for a check that finished under a minute ago", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT + 20) * 1000);
    const { getByRole } = renderWithProviders(<PageHeader title="Updates" />);

    const button = getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked just now"));
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
    const { getByRole, queryClient } = renderWithProviders(<PageHeader title="Updates" />);
    const button = getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked just now"));

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
    expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked 3 min ago");
  });

  it("runs the app's refresh on Check again, off and turning until it is done", async () => {
    const reply = deferredRefresh();
    refreshReply = () => reply.promise;
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime((CHECKED_AT + 10 * 60) * 1000);
    const { getByRole, queryClient } = renderWithProviders(<PageHeader title="Updates" />);
    const button = getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked 10 min ago"));
    expect(button).toBeEnabled();

    fireEvent.click(button);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    await waitFor(() => expect(button).toBeDisabled());
    expect(button.querySelector("svg")?.getAttribute("class")).toContain("animate-spin");
    expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checking…");
    // A second click has nothing to press.
    fireEvent.click(button);
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(1);

    const newer = snapshotCheckedAt(CHECKED_AT + 10 * 60, 5);
    await act(async () => {
      reply.resolve(newer);
    });

    await waitFor(() => expect(button).toBeEnabled());
    expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(newer);
    expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked just now");
    expect(button.querySelector("svg")?.getAttribute("class") ?? "").not.toContain("animate-spin");
  });

  it("is off while a refresh it did not start is running", async () => {
    // The startup refresh, or the one after an operation: the same
    // coordinator, started from outside the header.
    const reply = deferredRefresh();
    refreshReply = () => reply.promise;
    const { getByRole, queryClient } = renderWithProviders(<PageHeader title="Installed" />);
    const button = getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(tooltipOf(button)).toMatch(/Checked /));

    let run: Promise<void> = Promise.resolve();
    act(() => {
      run = refreshIntoCache(queryClient, "test");
    });

    await waitFor(() => expect(button).toBeDisabled());
    expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checking…");

    await act(async () => {
      reply.resolve(snapshotCheckedAt(CHECKED_AT + 60, 5));
      await run;
    });
    await waitFor(() => expect(button).toBeEnabled());
  });

  it("says the check failed in Check again's tooltip when the refresh does, until one works", async () => {
    refreshReply = () => Promise.reject("the session is gone");
    const { getByRole } = renderWithProviders(<PageHeader title="Updates" />);
    const button = getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(tooltipOf(button)).toMatch(/Checked /));

    fireEvent.click(button);

    await waitFor(() => expect(tooltipOf(button)).toBe("Check Again (⌘R) · Couldn't check"));
    expect(button).toBeEnabled();

    refreshReply = () => Promise.resolve(snapshotCheckedAt(Math.floor(Date.now() / 1000), 5));
    fireEvent.click(button);

    await waitFor(() => expect(tooltipOf(button)).toBe("Check Again (⌘R) · Checked just now"));
  });
});
