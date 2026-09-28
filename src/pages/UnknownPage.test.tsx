import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { ScanAgain, UnknownPage } from "./UnknownPage";
import i18n from "../i18n";
import { formatBytes } from "../lib/format";
import { queryKeys } from "../lib/queryKeys";
import type { Settings, Snapshot, UnknownScan } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

// Every name here is invented; the research machine's real ones are
// deliberately not in the repository.
const baseScan: UnknownScan = {
  scanned: [
    { path: "~/.local/bin", entries: 5 },
    { path: "/usr/local/bin", entries: 1 },
  ],
  entries: [
    {
      path: "~/.opencode/bin/standalone-tool",
      kind: "File",
      resolved: "/Users/someone/.opencode/bin/standalone-tool",
      link_target: null,
      size_bytes: 144_300_000,
      modified_at: 1_758_000_000,
      owned_by_me: true,
      app_bundle: null,
    },
    {
      path: "~/.local/bin/old-script",
      kind: "BrokenSymlink",
      resolved: null,
      link_target: "/Applications/Removed.app/Contents/Resources/scripts/index.js",
      size_bytes: null,
      modified_at: null,
      owned_by_me: true,
      app_bundle: "Removed",
    },
    {
      path: "/usr/local/bin/helper-cli",
      kind: "Symlink",
      resolved: "/Applications/Helper.app/Contents/Helpers/helper-cli",
      link_target: "/Applications/Helper.app/Contents/Helpers/helper-cli",
      size_bytes: 2_100_000,
      modified_at: 1_700_000_000,
      owned_by_me: false,
      app_bundle: "Helper",
    },
  ],
  attributed: 4,
  stopped: null,
};

// A refreshed snapshot with nothing in it: the page scans once per
// snapshot `generation` (ruling 9), so `get_snapshot` has to answer for
// the first scan to run at all; what it holds is irrelevant here, the
// scan's judgement is Rust's.
const snapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: 1_789_700_000,
  stale: false,
  errors: [],
};

let settings: Settings;
let scan: UnknownScan;
let scanFailure: string | null;
// With `holdScan`, a scan answers only when the test calls `releaseScan`.
let holdScan: boolean;
let releaseScan: () => void;

beforeEach(() => {
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
  };
  scan = baseScan;
  scanFailure = null;
  holdScan = false;
  releaseScan = () => {};
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "scan_unknown") {
      if (scanFailure !== null) return Promise.reject(scanFailure);
      if (holdScan) {
        return new Promise<UnknownScan>((resolve) => {
          releaseScan = () => resolve(scan);
        });
      }
      return Promise.resolve(scan);
    }
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    return Promise.resolve(undefined);
  });
});

function scanCalls(): number {
  return mockInvoke.mock.calls.filter(([cmd]) => cmd === "scan_unknown").length;
}

function dateOf(seconds: number): string {
  return new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }).format(
    new Date(seconds * 1000),
  );
}

/** The row a program's name is on. */
function rowOf(name: HTMLElement): HTMLElement {
  return name.closest("[data-tool-row]") as HTMLElement;
}

describe("UnknownPage", () => {
  it("scans once when it opens and lists each program as a row, under the chip for its kind", async () => {
    const { findByText, getByText, container } = renderWithProviders(<UnknownPage />);

    const tool = rowOf(await findByText("standalone-tool"));
    const script = rowOf(getByText("old-script"));
    const helper = rowOf(getByText("helper-cli"));
    expect(container.querySelectorAll("[data-tool-row]")).toHaveLength(3);
    expect(within(tool).getByText("Program")).toBeInTheDocument();
    expect(within(script).getByText("Broken link")).toBeInTheDocument();
    expect(within(helper).getByText("Link")).toBeInTheDocument();
    // The path is the row's line under its name, home abbreviated as Rust
    // sent it.
    expect(within(tool).getByText("~/.opencode/bin/standalone-tool")).toBeInTheDocument();
    expect(scanCalls()).toBe(1);
  });

  it("gives each row a neutral avatar of its own, not a source's", async () => {
    const { findByText } = renderWithProviders(<UnknownPage />);

    const row = rowOf(await findByText("standalone-tool"));
    const avatar = row.querySelector('[aria-hidden="true"]') as HTMLElement;
    expect(avatar.className).toContain("bg-muted");
    expect(avatar.className).toContain("h-8");
    expect(avatar.querySelector("svg")).not.toBeNull();
    expect(avatar.textContent).toBe("");
  });

  it("keeps what a broken link pointed at and the app a program belongs to behind its chip's ⓘ", async () => {
    const { findByText, getByText, queryByText } = renderWithProviders(<UnknownPage />);

    const script = rowOf(await findByText("old-script"));
    // One line a row, by default: the explanations are behind the chip.
    expect(
      queryByText("Points to /Applications/Removed.app/Contents/Resources/scripts/index.js, which is gone"),
    ).toBeNull();
    fireEvent.click(within(script).getByRole("button", { name: "Broken link" }));
    expect(
      within(script).getByText(
        "Points to /Applications/Removed.app/Contents/Resources/scripts/index.js, which is gone",
      ),
    ).toBeInTheDocument();
    expect(within(script).getByText("Part of Removed")).toBeInTheDocument();

    const helper = rowOf(getByText("helper-cli"));
    fireEvent.click(within(helper).getByRole("button", { name: "Link" }));
    expect(within(helper).getByText("Part of Helper")).toBeInTheDocument();
    // Only what can be confirmed: the file is not the user's.
    expect(within(helper).getByText("Owned by the system or another account")).toBeInTheDocument();
  });

  it("puts each row in a slot of its own, marked open while its ⓘ is, so the rows after it cannot cover it", async () => {
    // index.css lifts a `data-list-slot` that holds an open panel over the
    // slots after it; each is `relative z-0`, a stacking context of its own.
    const { findByText } = renderWithProviders(<UnknownPage />);

    const row = rowOf(await findByText("old-script"));
    const slot = row.parentElement as HTMLElement;
    expect(slot).toHaveAttribute("data-list-slot");
    expect(slot.className).toContain("relative");
    fireEvent.click(within(row).getByRole("button", { name: "Broken link" }));
    expect(slot.querySelector("[data-popup-open]")).not.toBeNull();
  });

  it("leaves the chip of a plain program of the user's own a plain label, with nothing behind it", async () => {
    const { findByText } = renderWithProviders(<UnknownPage />);

    const tool = rowOf(await findByText("standalone-tool"));
    expect(within(tool).queryByRole("button")).toBeNull();
    expect(within(tool).getByText("Program").tagName).toBe("SPAN");
  });

  it("shows size and date where a tool's version would be, and nothing there for a broken link", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(`${formatBytes(144_300_000)} · ${dateOf(1_758_000_000)}`),
    ).toBeInTheDocument();
    expect(getByText(`${formatBytes(2_100_000)} · ${dateOf(1_700_000_000)}`)).toBeInTheDocument();
    // A broken link has no size and no date, and no stray separator.
    expect(rowOf(getByText("old-script")).textContent).not.toContain("·");
  });

  it("shows where a link leads only with technical details on", async () => {
    const hidden = renderWithProviders(<UnknownPage />);
    const hiddenRow = rowOf(await hidden.findByText("helper-cli"));
    fireEvent.click(within(hiddenRow).getByRole("button", { name: "Link" }));
    expect(
      within(hiddenRow).queryByText("Links to /Applications/Helper.app/Contents/Helpers/helper-cli"),
    ).not.toBeInTheDocument();
    hidden.unmount();

    settings = { ...settings, show_technical_details: true };
    const shown = renderWithProviders(<UnknownPage />);
    const shownRow = rowOf(await shown.findByText("helper-cli"));
    fireEvent.click(within(shownRow).getByRole("button", { name: "Link" }));
    expect(
      within(shownRow).getByText("Links to /Applications/Helper.app/Contents/Helpers/helper-cli"),
    ).toBeInTheDocument();
    // A plain file resolves to itself; there is nothing to add, so its
    // chip stays a plain label.
    const tool = rowOf(shown.getByText("standalone-tool"));
    expect(within(tool).queryByRole("button")).toBeNull();
  });

  it("says where it looked in one quiet line at the top, and how many programs it recognized", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(await findByText("Looked in: ~/.local/bin, /usr/local/bin")).toBeInTheDocument();
    expect(getByText("Canager recognized 4 more programs and doesn't list them here.")).toBeInTheDocument();
  });

  it("says so in Chinese, the folders run together with 、", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByText } = renderWithProviders(<UnknownPage />);
      expect(await findByText("查找位置：~/.local/bin、/usr/local/bin")).toBeInTheDocument();
      expect(getByText("程序")).toBeInTheDocument();
      expect(getByText("链接")).toBeInTheDocument();
      expect(getByText("失效的链接")).toBeInTheDocument();
      expect(getByText("另有 4 个程序认得出来历，不在这里列出。")).toBeInTheDocument();
      expect(getByText("Canager 认不出这些程序的来历。这里只列出，不运行也不删除。")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("warns with the scan's own numbers when it stopped early", async () => {
    scan = { ...baseScan, stopped: { FileLimit: { max_entries: 2000 } } };
    const byFiles = renderWithProviders(<UnknownPage />);
    expect(
      await byFiles.findByText("Canager stopped after 2000 items and didn't check the rest."),
    ).toBeInTheDocument();
    byFiles.unmount();

    scan = { ...baseScan, stopped: { TimeLimit: { max_secs: 10 } } };
    const byTime = renderWithProviders(<UnknownPage />);
    expect(
      await byTime.findByText("Canager stopped after 10 seconds and didn't check the rest."),
    ).toBeInTheDocument();
    // The rows are still there under it.
    expect(byTime.getByText("standalone-tool")).toBeInTheDocument();
  });

  it("says so when nothing is unexplained, and still says where it looked", async () => {
    scan = { ...baseScan, entries: [], attributed: 7 };
    const { findByText, getByText, container } = renderWithProviders(<UnknownPage />);

    expect(await findByText("No programs of unknown origin")).toBeInTheDocument();
    expect(getByText("Looked in: ~/.local/bin, /usr/local/bin")).toBeInTheDocument();
    expect(container.querySelector("[data-tool-row]")).toBeNull();
  });

  it("vouches only for what it checked when a scan that stopped early found nothing, with no check mark", async () => {
    scan = { ...baseScan, entries: [], stopped: { TimeLimit: { max_secs: 10 } } };
    const stoppedEarly = renderWithProviders(<UnknownPage />);
    expect(await stoppedEarly.findByText("No programs of unknown origin in what Canager checked")).toBeInTheDocument();
    expect(stoppedEarly.getByText("Canager stopped after 10 seconds and didn't check the rest.")).toBeInTheDocument();
    expect(stoppedEarly.queryByText("No programs of unknown origin")).toBeNull();
    expect(stoppedEarly.container.querySelector("svg.text-success")).toBeNull();
    stoppedEarly.unmount();

    scan = { ...baseScan, entries: [] };
    const whole = renderWithProviders(<UnknownPage />);
    expect(await whole.findByText("No programs of unknown origin")).toBeInTheDocument();
    expect(whole.container.querySelector("svg.text-success")).not.toBeNull();
  });

  it("says a scan that stopped early and found nothing vouches only for what it checked, in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      scan = { ...baseScan, entries: [], stopped: { FileLimit: { max_entries: 2000 } } };
      const { findByText } = renderWithProviders(<UnknownPage />);
      expect(await findByText("查过的部分没有来源不明的程序")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("scans again when the button is pressed", async () => {
    // The button is the page header's now (`ScanAgain`), as App puts it.
    const { findByText, getByRole } = renderWithProviders(
      <>
        <ScanAgain />
        <UnknownPage />
      </>,
    );
    await findByText("standalone-tool");
    expect(scanCalls()).toBe(1);

    fireEvent.click(getByRole("button", { name: "Scan again" }));

    await waitFor(() => expect(scanCalls()).toBe(2));
  });

  it("draws no Scan again of its own: the page header has it", async () => {
    const { findByText, getByText, queryByRole } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");

    expect(queryByRole("button", { name: "Scan again" })).toBeNull();
    // What it says at its top stays: what the page is, and where it looked.
    expect(
      getByText(
        "Canager can't tell how these command-line programs got here. It only lists them and never runs or deletes them.",
      ),
    ).toBeInTheDocument();
    expect(getByText("Looked in: ~/.local/bin, /usr/local/bin")).toBeInTheDocument();
  });

  describe("Scan again, in the page header", () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    it("says Scanning… with the button off while a scan runs, then when it answered", async () => {
      // Only the clock and the header's minute tick are fake.
      vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
      vi.setSystemTime(new Date(2026, 8, 28, 9, 0));
      holdScan = true;
      const { findByText, getByRole, getByText, queryByText } = renderWithProviders(
        <>
          <ScanAgain />
          <UnknownPage />
        </>,
      );

      expect(await findByText("Scanning…")).toBeInTheDocument();
      expect(getByRole("button", { name: "Scan again" })).toBeDisabled();

      await act(async () => {
        releaseScan();
      });
      expect(await findByText("Scanned just now")).toBeInTheDocument();
      expect(queryByText("Scanning…")).toBeNull();
      expect(getByRole("button", { name: "Scan again" })).toBeEnabled();

      act(() => {
        vi.advanceTimersByTime(2 * 60_000);
      });
      expect(getByText("Scanned 2 min ago")).toBeInTheDocument();
    });

    it("says nothing before a scan has answered, and nothing about one that failed, whose reason the page says", async () => {
      const alone = renderWithProviders(<ScanAgain />);
      // Nothing asked for a scan: no time, and the button ready.
      expect(alone.getByRole("button", { name: "Scan again" })).toBeEnabled();
      expect(alone.queryByText(/^Scanned|Scanning…/)).toBeNull();
      expect(scanCalls()).toBe(0);
      alone.unmount();

      scanFailure = "boom";
      const failed = renderWithProviders(
        <>
          <ScanAgain />
          <UnknownPage />
        </>,
      );
      expect(await failed.findByRole("alert")).toHaveTextContent("Couldn't scan: boom");
      expect(failed.queryByText(/^Scanned|Scanning…/)).toBeNull();
      expect(failed.getByRole("button", { name: "Scan again" })).toBeEnabled();
    });

    it("says when it scanned in Chinese as the header says when it checked", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        const { findByText, getByRole } = renderWithProviders(
          <>
            <ScanAgain />
            <UnknownPage />
          </>,
        );
        expect(await findByText("上次扫描：刚刚")).toBeInTheDocument();
        expect(getByRole("button", { name: "重新扫描" })).toBeInTheDocument();
      } finally {
        await i18n.changeLanguage("en");
      }
    });
  });

  it("scans again when the sources' snapshot moves underneath it", async () => {
    // Opened before the startup refresh commits, the page judged against
    // an empty snapshot and listed every managed launcher. The refresh
    // landing -- a higher `generation` in the snapshot cache, which is
    // how both `refreshIntoCache` and a `SnapshotChanged` invalidation
    // arrive -- re-runs the scan, so the list corrects itself instead of
    // waiting for a press (ruling 9).
    const { findByText, queryClient } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");
    expect(scanCalls()).toBe(1);

    act(() => {
      queryClient.setQueryData<Snapshot>(queryKeys.snapshot, { ...snapshot, generation: 2 });
    });

    await waitFor(() => expect(scanCalls()).toBe(2));
  });

  it("shows the backend's reason when the scan fails", async () => {
    scanFailure = "boom";
    const { findByRole } = renderWithProviders(<UnknownPage />);

    const alert = await findByRole("alert");
    expect(alert).toHaveTextContent("Couldn't scan: boom");
  });
});
