import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { renderWithProviders } from "../test/setup";
import { ScanAgain, UnknownPage } from "./UnknownPage";
import i18n from "../i18n";
import zhCN from "../i18n/zh-CN.json";
import { formatBytes } from "../lib/format";
import { queryKeys } from "../lib/queryKeys";
import type { Settings, Snapshot, UnknownEntry, UnknownScan } from "../lib/types";

const mockInvoke = vi.mocked(invoke);
// Show in Finder's one call (src/test/setup.ts mocks the plugin).
const mockReveal = vi.mocked(revealItemInDir);

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
  round: 1,
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
    auto_check: false,
    notify_updates: false,
  };
  scan = baseScan;
  scanFailure = null;
  holdScan = false;
  releaseScan = () => {};
  mockReveal.mockClear();
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

/** A row's chips (`ToolRow`'s `data-status`). */
function chipsOf(row: HTMLElement): HTMLElement {
  return row.querySelector("[data-status]") as HTMLElement;
}

/** A row's size and date: the column after its chips, where a tool's version goes. */
function sizeAndDateOf(row: HTMLElement): HTMLElement {
  return chipsOf(row).nextElementSibling as HTMLElement;
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
    expect(avatar.className).toContain("bg-neutral-avatar");
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
    expect(within(chipsOf(tool)).queryByRole("button")).toBeNull();
    expect(within(tool).getByText("Program").tagName).toBe("SPAN");
  });

  it("shows the size over the date where a tool's version would be, each its own item, and nothing there for a broken link", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    for (const [name, bytes, seconds] of [
      ["standalone-tool", 144_300_000, 1_758_000_000],
      ["helper-cli", 2_100_000, 1_700_000_000],
    ] as const) {
      const column = sizeAndDateOf(rowOf(await findByText(name)));
      const size = within(column).getByText(formatBytes(bytes));
      const date = within(column).getByText(dateOf(seconds));
      // The size first, then the date, with nothing between them: no dot.
      expect(size.compareDocumentPosition(date) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(column.textContent).toBe(`${formatBytes(bytes)}${dateOf(seconds)}`);
    }
    // A broken link has no size and no date.
    expect(sizeAndDateOf(rowOf(getByText("old-script"))).textContent).toBe("");
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
    expect(within(chipsOf(tool)).queryByRole("button")).toBeNull();
  });

  it("lets its paths be selected, to be copied, and nothing else on a row", async () => {
    settings = { ...settings, show_technical_details: true };
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    const helper = rowOf(await findByText("helper-cli"));
    const path = within(helper).getByText("/usr/local/bin/helper-cli");
    expect(path).toHaveClass("select-text");
    // Not its name, its chip, nor its size and date.
    expect([...helper.querySelectorAll(".select-text")]).toEqual([path]);
    expect(getByText("Looked in: ~/.local/bin, /usr/local/bin")).toHaveClass("select-text");

    // Behind its chip, where it leads and whose it is.
    fireEvent.click(within(helper).getByRole("button", { name: "Link" }));
    for (const line of [
      "Part of Helper",
      "Owned by the system or another account",
      "Links to /Applications/Helper.app/Contents/Helpers/helper-cli",
    ]) {
      expect(within(helper).getByText(line)).toHaveClass("select-text");
    }
  });

  it("says where it looked in one quiet line at the top, and how many programs it recognized", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(await findByText("Looked in: ~/.local/bin, /usr/local/bin")).toBeInTheDocument();
    expect(getByText("4 more programs have a known source and aren't listed here.")).toBeInTheDocument();
  });

  it("says so in Chinese, the folders run together with 、", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByText } = renderWithProviders(<UnknownPage />);
      expect(await findByText("查找位置：~/.local/bin、/usr/local/bin")).toBeInTheDocument();
      expect(getByText("程序")).toBeInTheDocument();
      expect(getByText("链接")).toBeInTheDocument();
      expect(getByText("失效的链接")).toBeInTheDocument();
      expect(getByText("另有4个程序已确定来源，未在这里列出。")).toBeInTheDocument();
      expect(getByText("无法确定以下程序是用什么安装的。")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("warns with the scan's own numbers when it stopped early", async () => {
    scan = { ...baseScan, stopped: { FileLimit: { max_entries: 2000 } } };
    const byFiles = renderWithProviders(<UnknownPage />);
    expect(
      await byFiles.findByText("Stopped after 2000 items; the rest weren't checked."),
    ).toBeInTheDocument();
    byFiles.unmount();

    scan = { ...baseScan, stopped: { TimeLimit: { max_secs: 10 } } };
    const byTime = renderWithProviders(<UnknownPage />);
    expect(
      await byTime.findByText("Stopped after 10 seconds; the rest weren't checked."),
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
    expect(await stoppedEarly.findByText("No programs of unknown origin in the places checked")).toBeInTheDocument();
    expect(stoppedEarly.getByText("Stopped after 10 seconds; the rest weren't checked.")).toBeInTheDocument();
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
      expect(await findByText("已检查的位置中没有来源不明的程序")).toBeInTheDocument();
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

    fireEvent.click(getByRole("button", { name: "Scan Again" }));

    await waitFor(() => expect(scanCalls()).toBe(2));
  });

  it("draws no Scan again of its own: the page header has it", async () => {
    const { findByText, getByText, queryByRole } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");

    expect(queryByRole("button", { name: "Scan Again" })).toBeNull();
    // What it says at its top stays: what the page is, and where it looked.
    expect(
      getByText(
        "Couldn't determine how these programs were installed.",
      ),
    ).toBeInTheDocument();
    expect(getByText("Looked in: ~/.local/bin, /usr/local/bin")).toBeInTheDocument();
  });

  describe("Scan Again, in the toolbar", () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    it("turns, off, while a scan runs, then says in its tooltip when it answered", async () => {
      // Only the clock and the header's minute tick are fake.
      vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
      vi.setSystemTime(new Date(2026, 8, 28, 9, 0));
      holdScan = true;
      const { getByRole } = renderWithProviders(
        <>
          <ScanAgain />
          <UnknownPage />
        </>,
      );

      const button = getByRole("button", { name: "Scan Again" });
      await waitFor(() => expect(button).toBeDisabled());
      expect(button.querySelector("svg")?.getAttribute("class")).toContain("animate-spin");
      expect(button).toHaveAttribute("title", "Scan Again · Scanning…");

      await act(async () => {
        releaseScan();
      });
      await waitFor(() => expect(button).toHaveAttribute("title", "Scan Again · Scanned just now"));
      expect(button).toBeEnabled();
      // Its own shortcut is none: ⌘R is Check again's.
      expect(button.getAttribute("title")).not.toContain("⌘");

      act(() => {
        vi.advanceTimersByTime(2 * 60_000);
      });
      expect(button).toHaveAttribute("title", "Scan Again · Scanned 2 min ago");
    });

    it("says nothing before a scan has answered, and nothing about one that failed, whose reason the page says", async () => {
      const alone = renderWithProviders(<ScanAgain />);
      // Nothing asked for a scan: no time, and the button ready.
      expect(alone.getByRole("button", { name: "Scan Again" })).toBeEnabled();
      expect(alone.getByRole("button", { name: "Scan Again" })).toHaveAttribute("title", "Scan Again");
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
      const button = failed.getByRole("button", { name: "Scan Again" });
      await waitFor(() => expect(button).toBeEnabled());
      expect(button).toHaveAttribute("title", "Scan Again");
    });

    it("says when it scanned in Chinese as the toolbar says when it checked", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        const { getByRole } = renderWithProviders(
          <>
            <ScanAgain />
            <UnknownPage />
          </>,
        );
        const button = getByRole("button", { name: "重新扫描" });
        await waitFor(() =>
          expect(button).toHaveAttribute("title", `重新扫描 · ${i18n.t("unknown.scannedJustNow")}`),
        );
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

/** Opens `row`'s ⋯ menu. */
function openMenu(row: HTMLElement): HTMLElement {
  fireEvent.click(within(row).getByRole("button", { name: /^More actions for / }));
  return screen.getByRole("menu");
}

function chooseFromMenu(row: HTMLElement, item: string) {
  fireEvent.click(within(openMenu(row)).getByRole("menuitem", { name: item }));
}

/** What the page last said about a row's Copy path or Show in Finder. */
function notice(): HTMLElement {
  return screen.getByRole("status");
}

describe("a row's ⋯ menu", () => {
  it("is on every row, named after it, with Show in Finder and Copy path", async () => {
    const { findByText, container } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");

    const rows = [...container.querySelectorAll<HTMLElement>("[data-tool-row]")];
    expect(rows).toHaveLength(3);
    for (const [row, name] of rows.map((row, i) => [row, ["standalone-tool", "old-script", "helper-cli"][i]] as const)) {
      const button = within(row).getByRole("button", { name: `More actions for ${name}` });
      expect(button).toHaveAttribute("aria-haspopup", "menu");
      const menu = openMenu(row);
      expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
        "Show in Finder",
        "Copy Path",
      ]);
      fireEvent.click(button);
      expect(screen.queryByRole("menu")).toBeNull();
    }
  });

  it("asks Finder to show a program where it is, and a link's file where the link points", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    // A program that is no link: its own path, the home folder written
    // out, as the scan resolved it -- with no word needed on the item.
    const tool = rowOf(await findByText("standalone-tool"));
    const own = within(openMenu(tool)).getByRole("menuitem", { name: "Show in Finder" });
    expect(own).not.toHaveAttribute("title");
    fireEvent.click(own);
    await waitFor(() => expect(mockReveal).toHaveBeenCalledTimes(1));
    expect(mockReveal).toHaveBeenLastCalledWith("/Users/someone/.opencode/bin/standalone-tool");

    // A link: the file it points to, which the item says is what Finder
    // shows.
    const helper = rowOf(getByText("helper-cli"));
    const linked = within(openMenu(helper)).getByRole("menuitem", { name: "Show in Finder" });
    expect(linked).toHaveAccessibleDescription("Shows the file this link points to.");
    fireEvent.click(linked);
    await waitFor(() => expect(mockReveal).toHaveBeenCalledTimes(2));
    expect(mockReveal).toHaveBeenLastCalledWith("/Applications/Helper.app/Contents/Helpers/helper-cli");
    // Nothing to say when Finder comes forward.
    expect(notice()).toHaveTextContent(/^$/);
  });

  it("turns Show in Finder off for a broken link, says why, and still copies its path", async () => {
    const { findByText } = renderWithProviders(<UnknownPage />);

    const menu = openMenu(rowOf(await findByText("old-script")));
    const item = within(menu).getByRole("menuitem", { name: "Show in Finder" });
    expect(item).toHaveAttribute("aria-disabled", "true");
    expect(item).toHaveAccessibleDescription("The file this link points to is gone.");
    fireEvent.click(item);
    expect(mockReveal).not.toHaveBeenCalled();
    expect(within(menu).getByRole("menuitem", { name: "Copy Path" })).not.toHaveAttribute("aria-disabled");
  });

  it("is reached with Tab and worked with the keys, as every row's ⋯ is", async () => {
    const user = userEvent.setup();
    const { findByText, getByRole, queryByRole } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");

    // The first row's chip is a plain label, so its ⋯ is the page's first stop.
    await user.tab();
    const button = getByRole("button", { name: "More actions for standalone-tool" });
    expect(button).toHaveFocus();

    // Enter opens it on its first item; the arrow keys move; Enter chooses,
    // and the focus goes back to the ⋯.
    await user.keyboard("{Enter}");
    expect(getByRole("menuitem", { name: "Show in Finder" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(getByRole("menuitem", { name: "Copy Path" })).toHaveFocus();
    await user.keyboard("{ArrowUp}{Enter}");
    await waitFor(() => expect(mockReveal).toHaveBeenCalledWith("/Users/someone/.opencode/bin/standalone-tool"));
    expect(queryByRole("menu")).toBeNull();
    expect(button).toHaveFocus();

    // Escape closes it, the focus back on the ⋯, and Tab goes on to the
    // next row's controls: its chip's ⓘ, then its ⋯.
    await user.keyboard("{ArrowDown}");
    expect(getByRole("menuitem", { name: "Show in Finder" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(queryByRole("menu")).toBeNull();
    expect(button).toHaveFocus();
    await user.tab();
    expect(getByRole("button", { name: "Broken link" })).toHaveFocus();
    await user.tab();
    expect(getByRole("button", { name: "More actions for old-script" })).toHaveFocus();
  });

  it("calls them 在访达中显示 and 拷贝路径 in Chinese, in six characters or fewer", async () => {
    expect(zhCN.unknown.showInFinder).toBe("在访达中显示");
    expect(zhCN.unknown.copyPath).toBe("拷贝路径");
    for (const label of [zhCN.unknown.showInFinder, zhCN.unknown.copyPath]) {
      expect([...label].length).toBeLessThanOrEqual(6);
    }

    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText } = renderWithProviders(<UnknownPage />);
      const helper = rowOf(await findByText("helper-cli"));
      fireEvent.click(within(helper).getByRole("button", { name: "“helper-cli”的更多操作" }));
      const menu = screen.getByRole("menu");
      expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
        "在访达中显示",
        "拷贝路径",
      ]);
      expect(within(menu).getByRole("menuitem", { name: "在访达中显示" })).toHaveAccessibleDescription(
        "显示此链接指向的文件。",
      );
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  describe("Show in Finder that could not show it", () => {
    afterEach(() => {
      vi.useRealTimers();
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    });

    it("says so at the top of the page until a copy has a word of its own", async () => {
      mockReveal.mockRejectedValueOnce("No such file or directory (os error 2)");
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
      const { findByText } = renderWithProviders(<UnknownPage />);

      const tool = rowOf(await findByText("standalone-tool"));
      chooseFromMenu(tool, "Show in Finder");
      await waitFor(() => expect(notice()).toHaveTextContent(/^Couldn't show it in Finder$/));

      chooseFromMenu(tool, "Copy Path");
      await waitFor(() => expect(notice()).toHaveTextContent(/^Copied$/));
    });

    it("takes the word back as a copy's is, after the same while", async () => {
      mockReveal.mockRejectedValueOnce("No such file or directory (os error 2)");
      const { findByText } = renderWithProviders(<UnknownPage />);
      const tool = rowOf(await findByText("standalone-tool"));

      // Only the page's timeouts are fake, from here on: the rows are up.
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      chooseFromMenu(tool, "Show in Finder");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(notice()).toHaveTextContent(/^Couldn't show it in Finder$/);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(2_400);
      });
      expect(notice()).toHaveTextContent(/^Couldn't show it in Finder$/);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(200);
      });
      expect(notice()).toHaveTextContent(/^$/);
    });
  });
});

describe("Copy Path", () => {
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  });

  afterEach(() => {
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
  });

  it("copies the path the row shows, a link's own, and says it did", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    chooseFromMenu(rowOf(await findByText("standalone-tool")), "Copy Path");
    expect(writeText).toHaveBeenLastCalledWith("~/.opencode/bin/standalone-tool");
    await waitFor(() => expect(notice()).toHaveTextContent(/^Copied$/));

    // Where the link is, not where it points.
    chooseFromMenu(rowOf(getByText("helper-cli")), "Copy Path");
    expect(writeText).toHaveBeenLastCalledWith("/usr/local/bin/helper-cli");
    expect(mockReveal).not.toHaveBeenCalled();
  });

  it("says so when the clipboard refuses", async () => {
    writeText.mockRejectedValue(new Error("denied"));
    const { findByText } = renderWithProviders(<UnknownPage />);

    chooseFromMenu(rowOf(await findByText("standalone-tool")), "Copy Path");

    await waitFor(() => expect(notice()).toHaveTextContent(/^Couldn't copy$/));
  });

  it("says it in the words the other pages' Copy command says it in, in Chinese too", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText } = renderWithProviders(<UnknownPage />);
      const tool = rowOf(await findByText("standalone-tool"));
      fireEvent.click(within(tool).getByRole("button", { name: "“standalone-tool”的更多操作" }));
      fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "拷贝路径" }));
      await waitFor(() => expect(notice()).toHaveTextContent(new RegExp(`^${zhCN.common.copied}$`)));
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

describe("the app a link points into", () => {
  it("is on the row's line under its name, after the path, and still behind the ⓘ at more length", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    const helper = rowOf(await findByText("helper-cli"));
    const note = within(helper).getByText("Points into Helper.app");
    const path = within(helper).getByText("/usr/local/bin/helper-cli");
    // One line, the path first; only the path selects.
    expect(note.closest("p")).toBe(path.closest("p"));
    expect(path.compareDocumentPosition(note) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect([...helper.querySelectorAll(".select-text")]).toEqual([path]);
    // The ⓘ says it at more length, as it did.
    fireEvent.click(within(helper).getByRole("button", { name: "Link" }));
    expect(within(helper).getByText("Part of Helper")).toBeInTheDocument();

    // A broken link's: the app it pointed into.
    expect(within(rowOf(getByText("old-script"))).getByText("Points into Removed.app")).toBeInTheDocument();
    // A program of the user's own, in no app: nothing more.
    expect(within(rowOf(getByText("standalone-tool"))).queryByText(/^Points into/)).toBeNull();
  });

  it("is said only of a link that leads into the app, not of a program that is part of one", async () => {
    const entry = (over: Partial<UnknownEntry>): UnknownEntry => ({
      path: "",
      kind: "File",
      resolved: null,
      link_target: null,
      size_bytes: 1_000,
      modified_at: 1_700_000_000,
      owned_by_me: true,
      app_bundle: "Kit",
      ...over,
    });
    scan = {
      ...baseScan,
      entries: [
        // Inside the app, not pointing into it.
        entry({
          path: "~/Applications/Kit.app/Contents/bin/kit",
          resolved: "/Users/someone/Applications/Kit.app/Contents/bin/kit",
        }),
        // A link in the app's folder that leads out of it.
        entry({
          path: "~/Applications/Kit.app/Contents/bin/kit-helper",
          kind: "Symlink",
          resolved: "/opt/kit/bin/kit-helper",
          link_target: "/opt/kit/bin/kit-helper",
        }),
      ],
    };
    const { findByText, getByText, queryByText } = renderWithProviders(<UnknownPage />);

    const kit = rowOf(await findByText("kit"));
    expect(queryByText(/^Points into/)).toBeNull();
    // Its chip's ⓘ still says whose part it is.
    fireEvent.click(within(kit).getByRole("button", { name: "Program" }));
    expect(within(kit).getByText("Part of Kit")).toBeInTheDocument();
    expect(getByText("kit-helper")).toBeInTheDocument();
  });

  it("says 指向 in Chinese, as its ⓘ does of where a link leads", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByText } = renderWithProviders(<UnknownPage />);
      expect(within(rowOf(await findByText("helper-cli"))).getByText("指向Helper.app")).toBeInTheDocument();
      expect(within(rowOf(getByText("old-script"))).getByText("指向Removed.app")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});
