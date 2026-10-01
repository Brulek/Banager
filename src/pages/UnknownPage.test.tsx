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

/** A row's status (`data-status`): a broken link's word, where its size and date would be; null elsewhere. */
function statusOf(row: HTMLElement): HTMLElement | null {
  return row.querySelector("[data-status]");
}

/** What a row's tooltip says: the slot's `title`, a line a fact. */
function tooltipOf(row: HTMLElement): string[] {
  return (row.parentElement?.getAttribute("title") ?? "").split("\n").filter((line) => line !== "");
}

/** A row's size and date: the column where a tool's version goes. */
function sizeAndDateOf(row: HTMLElement): HTMLElement {
  return row.querySelector("[data-version]") as HTMLElement;
}

describe("UnknownPage", () => {
  it("scans once when it opens and lists each program as a row, a status word only on a broken link", async () => {
    const { findByText, getByText, queryByText, container } = renderWithProviders(<UnknownPage />);

    const tool = rowOf(await findByText("standalone-tool"));
    const script = rowOf(getByText("old-script"));
    const helper = rowOf(getByText("helper-cli"));
    expect(container.querySelectorAll("[data-tool-row]")).toHaveLength(3);
    // No 「程序」「链接」 pills (spec §3.3): a program and a link that works
    // are what these rows are, and say nothing about it.
    expect(queryByText("Program")).toBeNull();
    expect(queryByText("Link")).toBeNull();
    expect(statusOf(tool)).toBeNull();
    // A broken link: the word, muted, after a filled orange ⚠︎.
    const word = within(script).getByRole("button", { name: "Original missing" });
    expect(word.className).toContain("text-muted");
    expect(word.querySelector("svg")?.getAttribute("class")).toContain("text-warning");
    expect(within(helper).queryByText("Original missing")).toBeNull();
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
    // systemGray in the light; in the dark the token's own #48484A
    // (src/test/darkTheme.test.ts), so the tiles are not the brightest
    // thing on the page -- no dark colour of the page's own.
    expect(avatar.className).not.toMatch(/dark:bg-/);
    expect(avatar.className).toContain("h-8");
    expect(avatar.querySelector("svg")).not.toBeNull();
    expect(avatar.textContent).toBe("");
  });

  it("keeps what a broken link pointed at behind its word's ⓘ, and what else there is to say of a row in its tooltip", async () => {
    const { findByText, getByText, queryByText } = renderWithProviders(<UnknownPage />);

    const script = rowOf(await findByText("old-script"));
    // One line a row, by default: the explanations are behind the word.
    expect(
      queryByText("Points to /Applications/Removed.app/Contents/Resources/scripts/index.js, which can't be found"),
    ).toBeNull();
    fireEvent.click(within(script).getByRole("button", { name: "Original missing" }));
    expect(
      within(script).getByText(
        "Points to /Applications/Removed.app/Contents/Resources/scripts/index.js, which can't be found",
      ),
    ).toBeInTheDocument();
    expect(within(script).getByText("Part of Removed")).toBeInTheDocument();
    // The word has them; the row's tooltip does not say them again.
    expect(tooltipOf(script)).toEqual([]);

    // A link that works has no word, and no ⓘ standing alone in a word's
    // place: its facts are the row's tooltip, and said to a screen reader.
    const helper = rowOf(getByText("helper-cli"));
    expect(within(helper).queryByRole("button", { name: "Details: helper-cli" })).toBeNull();
    expect(helper.querySelector("[data-status]")).toBeNull();
    // Only what can be confirmed: the file is not the user's.
    expect(tooltipOf(helper)).toEqual(["Part of Helper", "Owned by the system or another account"]);
    expect(within(helper).getByText("Part of Helper, Owned by the system or another account")).toHaveClass("sr-only");
  });

  it("starts every row's name and path at one x: no status column on any row, the ⓘ only after a broken link's word", async () => {
    const { findByText, container } = renderWithProviders(<UnknownPage />);
    await findByText("old-script");

    for (const row of container.querySelectorAll<HTMLElement>("[data-tool-row]")) {
      // The status, where there is one, is in the size-and-date column at
      // the row's right -- never before the name's column, nor on the
      // path's line.
      const status = statusOf(row);
      if (status !== null) expect(status.closest("[data-version]")).not.toBeNull();
      // Nor an empty status column's 120 on the row: the path takes its room.
      expect(row.querySelector("[data-status-column]")).toBeNull();
      // Every ⓘ on a row is part of a word's button.
      for (const button of within(row).queryAllByRole("button")) {
        if (button.getAttribute("aria-label")?.startsWith("More actions")) continue;
        expect(button.textContent).not.toBe("");
      }
    }
  });

  it("puts each row in a slot of its own, marked open while its ⓘ is, so the rows after it cannot cover it", async () => {
    // index.css lifts a `data-list-slot` that holds an open panel over the
    // slots after it; each is `relative z-0`, a stacking context of its own.
    const { findByText } = renderWithProviders(<UnknownPage />);

    const row = rowOf(await findByText("old-script"));
    const slot = row.parentElement as HTMLElement;
    expect(slot).toHaveAttribute("data-list-slot");
    expect(slot.className).toContain("relative");
    fireEvent.click(within(row).getByRole("button", { name: "Original missing" }));
    expect(slot.querySelector("[data-popup-open]")).not.toBeNull();
  });

  it("gives a plain program of the user's own nothing in its status column: no word, no ⓘ", async () => {
    const { findByText } = renderWithProviders(<UnknownPage />);

    const tool = rowOf(await findByText("standalone-tool"));
    expect(statusOf(tool)).toBeNull();
    expect(within(tool).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
      "More actions for standalone-tool",
    ]);
  });

  it("shows the size and the date in two columns of one line, 72 and 104 wide, and nothing in them for a broken link", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    for (const [name, bytes, seconds] of [
      ["standalone-tool", 144_300_000, 1_758_000_000],
      ["helper-cli", 2_100_000, 1_700_000_000],
    ] as const) {
      const column = sizeAndDateOf(rowOf(await findByText(name)));
      // Where a tool's version goes: 13, muted, one line, figures lined up.
      expect(column.className.split(" ")).toEqual(
        expect.arrayContaining(["text-body", "text-muted", "tabular-nums", "whitespace-nowrap"]),
      );
      const size = column.querySelector("[data-size]") as HTMLElement;
      const date = column.querySelector("[data-date]") as HTMLElement;
      expect(size.textContent).toBe(formatBytes(bytes));
      expect(date.textContent).toBe(dateOf(seconds));
      // Side by side, not one over the other: two columns.
      expect(size.parentElement).toBe(date.parentElement);
      expect(size.parentElement?.className).toContain("flex");
      expect(size.className.split(" ")).toEqual(expect.arrayContaining(["w-18", "truncate"]));
      expect(date.className.split(" ")).toEqual(expect.arrayContaining(["w-26", "ml-4", "truncate"]));
    }
    // A broken link has no size and no date: its word stands in their
    // place, as wide as the two columns, at their right.
    const broken = sizeAndDateOf(rowOf(getByText("old-script")));
    expect(broken.textContent).toBe("Original missing");
    expect(broken.querySelector("[data-size]")).toBeNull();
    const place = broken.querySelector("[data-status]") as HTMLElement;
    expect(place.className.split(" ")).toEqual(expect.arrayContaining(["flex", "w-48", "justify-end"]));
  });

  it("heads the size and the date as Finder does, over their columns, at 11 in the secondary colour, in either language", async () => {
    const english = renderWithProviders(<UnknownPage />);
    const tool = rowOf(await english.findByText("standalone-tool"));
    const heads = english.container.querySelector("[data-column-heads]") as HTMLElement;
    expect(heads.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted", "px-5"]));
    const size = heads.querySelector("[data-size-head]") as HTMLElement;
    const date = heads.querySelector("[data-date-head]") as HTMLElement;
    expect(size.textContent).toBe("Size");
    // `modified_at`: when the program last changed.
    expect(date.textContent).toBe("Date Modified");
    // As wide and as far apart as the columns under them, and at their
    // right as the values are, with the ⋯'s room after them.
    const column = sizeAndDateOf(tool);
    expect(column.className.split(" ")).toContain("text-right");
    const sizeValue = column.querySelector("[data-size]") as HTMLElement;
    const dateValue = column.querySelector("[data-date]") as HTMLElement;
    for (const [head, value] of [
      [size, sizeValue],
      [date, dateValue],
    ] as const) {
      const widths = (element: HTMLElement) => element.className.split(" ").filter((c) => /^(w|ml)-/.test(c));
      expect(widths(head)).toEqual(widths(value));
      expect(head.className.split(" ")).toContain("text-right");
    }
    expect((heads.lastElementChild as HTMLElement).className.split(" ")).toEqual(
      expect.arrayContaining(["ml-4", "w-6"]),
    );
    // Over the list, and no row of it.
    expect(heads.compareDocumentPosition(tool) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(heads.closest("[data-tool-row], [data-list-slot]")).toBeNull();
    english.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      const chinese = renderWithProviders(<UnknownPage />);
      await chinese.findByText("standalone-tool");
      const zhHeads = chinese.container.querySelector("[data-column-heads]") as HTMLElement;
      expect(zhHeads.querySelector("[data-size-head]")?.textContent).toBe("大小");
      expect(zhHeads.querySelector("[data-date-head]")?.textContent).toBe("修改日期");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("gives the headings no focus: Tab and the arrow keys never stop on them", async () => {
    const user = userEvent.setup();
    const { findByText, container } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");
    const heads = container.querySelector("[data-column-heads]") as HTMLElement;
    expect(heads.querySelector("button, a, input, [tabindex]")).toBeNull();
    expect(heads).not.toHaveAttribute("tabindex");
    // The page's first stop is still the first row's ⋯.
    await user.tab();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "More actions for standalone-tool" }),
    );
    expect(heads.contains(document.activeElement)).toBe(false);
  });

  it("heads nothing over broken links alone, which have no size and no date", async () => {
    scan = { ...baseScan, entries: baseScan.entries.filter((entry) => entry.kind === "BrokenSymlink") };
    const { findByText, container } = renderWithProviders(<UnknownPage />);
    await findByText("old-script");
    expect(container.querySelector("[data-column-heads]")).toBeNull();
  });

  it("shows where a link leads only with technical details on", async () => {
    const hidden = renderWithProviders(<UnknownPage />);
    const hiddenRow = rowOf(await hidden.findByText("helper-cli"));
    expect(tooltipOf(hiddenRow)).not.toContain("Links to /Applications/Helper.app/Contents/Helpers/helper-cli");
    hidden.unmount();

    settings = { ...settings, show_technical_details: true };
    const shown = renderWithProviders(<UnknownPage />);
    const shownRow = rowOf(await shown.findByText("helper-cli"));
    expect(tooltipOf(shownRow)).toContain("Links to /Applications/Helper.app/Contents/Helpers/helper-cli");
    // A plain file resolves to itself; there is nothing to add, so it
    // has no tooltip.
    const tool = rowOf(shown.getByText("standalone-tool"));
    expect(statusOf(tool)).toBeNull();
    expect(tool.parentElement).not.toHaveAttribute("title");
  });

  it("lets its paths be selected, to be copied, and nothing else on a row", async () => {
    settings = { ...settings, show_technical_details: true };
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    const helper = rowOf(await findByText("helper-cli"));
    const path = within(helper).getByText("/usr/local/bin/helper-cli");
    expect(path).toHaveClass("select-text");
    // Not its name, nor its size and date.
    expect([...helper.querySelectorAll(".select-text")]).toEqual([path]);
    expect(getByText("Looked in: ~/.local/bin, /usr/local/bin")).toHaveClass("select-text");

    // Behind a broken link's ⓘ, what it pointed at and whose part it was.
    const script = rowOf(getByText("old-script"));
    fireEvent.click(within(script).getByRole("button", { name: "Original missing" }));
    for (const line of [
      "Points to /Applications/Removed.app/Contents/Resources/scripts/index.js, which can't be found",
      "Part of Removed",
    ]) {
      expect(within(script).getByText(line)).toHaveClass("select-text");
    }
  });

  it("says what these are in one line over the list, and where it looked and how many it recognized under it, quieter", async () => {
    const { findByText, getByText, container } = renderWithProviders(<UnknownPage />);

    const lookedIn = await findByText("Looked in: ~/.local/bin, /usr/local/bin");
    const recognized = getByText("4 more programs have a known source and aren't listed here.");
    const intro = getByText("Couldn't determine how these programs were installed.");
    const rows = [...container.querySelectorAll("[data-tool-row]")];
    // Over the list: the one line, 13 in the secondary colour.
    expect(intro.className.split(" ")).toEqual(expect.arrayContaining(["text-body", "text-muted"]));
    expect(intro.compareDocumentPosition(rows[0]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Under it, 11: where it looked, then how many it knew.
    for (const line of [lookedIn, recognized]) {
      expect(rows[rows.length - 1].compareDocumentPosition(line) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(line.parentElement?.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted"]));
    }
    expect(lookedIn.compareDocumentPosition(recognized) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("calls a broken link by what it has lost, the file it pointed to, in six characters or fewer", () => {
    // The scan calls a link broken when it cannot be followed to a file
    // (`canonicalize` fails in scan/mod.rs's `examine`): what the user
    // needs is that the file behind it is not there, which 「链接已失效」
    // left to be worked out. 「找不到」, not 「已不存在」: a link whose
    // target sits where it cannot be read, or that loops, is one too.
    expect(zhCN.unknown.kind.BrokenSymlink).toBe("找不到原文件");
    expect([...zhCN.unknown.kind.BrokenSymlink].length).toBeLessThanOrEqual(6);
    expect(i18n.getFixedT("en")("unknown.kind.BrokenSymlink")).toBe("Original missing");
  });

  it("says so in Chinese, the folders run together with 、", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByText, queryByText } = renderWithProviders(<UnknownPage />);
      expect(await findByText("查找位置：~/.local/bin、/usr/local/bin")).toBeInTheDocument();
      expect(queryByText("程序")).toBeNull();
      expect(queryByText("链接")).toBeNull();
      expect(getByText("找不到原文件")).toBeInTheDocument();
      expect(getByText("另有4个程序已确定来源，未在这里列出。")).toBeInTheDocument();
      expect(getByText("无法确定以下程序的安装来源。")).toBeInTheDocument();
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

  it("says so when nothing is unexplained as a Mac's empty list does, and still says where it looked", async () => {
    scan = { ...baseScan, entries: [], attributed: 7 };
    const { findByText, getByText, queryByText, queryByRole, container } = renderWithProviders(<UnknownPage />);

    const title = await findByText("No other programs");
    // The empty state: a ✓ in a circle, in the tertiary grey, the title
    // 15 semibold under it, and no button.
    const empty = title.closest("[data-empty-state]") as HTMLElement;
    expect(empty).not.toBeNull();
    expect(empty.querySelector("svg")?.getAttribute("class")).toContain("text-tertiary");
    expect(title).toHaveClass("text-section");
    expect(within(empty).queryByRole("button")).toBeNull();
    expect(queryByRole("button", { name: /Scan/ })).toBeNull();
    expect(getByText("Looked in: ~/.local/bin, /usr/local/bin")).toBeInTheDocument();
    expect(container.querySelector("[data-tool-row]")).toBeNull();
    // 「以下程序」 over nothing would point at nothing.
    expect(queryByText("Couldn't determine how these programs were installed.")).toBeNull();
  });

  it("vouches only for what it checked when a scan that stopped early found nothing, with no check mark", async () => {
    scan = { ...baseScan, entries: [], stopped: { TimeLimit: { max_secs: 10 } } };
    const stoppedEarly = renderWithProviders(<UnknownPage />);
    expect(await stoppedEarly.findByText("No other programs in the places checked")).toBeInTheDocument();
    expect(stoppedEarly.getByText("Stopped after 10 seconds; the rest weren't checked.")).toBeInTheDocument();
    expect(stoppedEarly.queryByText("No other programs")).toBeNull();
    // An ⓘ in a circle, not the ✓.
    const partial = stoppedEarly.container.querySelector("[data-empty-state]") as HTMLElement;
    const partialSymbol = partial.querySelector("svg")?.outerHTML;
    stoppedEarly.unmount();

    scan = { ...baseScan, entries: [] };
    const whole = renderWithProviders(<UnknownPage />);
    expect(await whole.findByText("No other programs")).toBeInTheDocument();
    const wholeSymbol = whole.container.querySelector("[data-empty-state] svg")?.outerHTML;
    expect(wholeSymbol).toBeDefined();
    expect(partialSymbol).toBeDefined();
    expect(partialSymbol).not.toBe(wholeSymbol);
    // Never green: an empty list is not a success to celebrate.
    expect(whole.container.querySelector("svg.text-success")).toBeNull();
  });

  it("says a scan that stopped early and found nothing vouches only for what it checked, in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      scan = { ...baseScan, entries: [], stopped: { FileLimit: { max_entries: 2000 } } };
      const stoppedEarly = renderWithProviders(<UnknownPage />);
      expect(await stoppedEarly.findByText("已检查的位置中没有其他程序")).toBeInTheDocument();
      stoppedEarly.unmount();

      // A whole scan that found nothing: the page's name, 其他程序, as a
      // Mac's empty list names what it would list.
      scan = { ...baseScan, entries: [] };
      const whole = renderWithProviders(<UnknownPage />);
      expect(await whole.findByText("没有其他程序")).toBeInTheDocument();
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
      await waitFor(() => expect(button).toHaveAttribute("aria-disabled", "true"));
      expect(button.querySelector("svg")?.getAttribute("class")).toContain("motion-safe:animate-spinner");
      expect(button).toHaveAttribute("title", "Scan Again · Scanning…");

      await act(async () => {
        releaseScan();
      });
      await waitFor(() => expect(button).toHaveAttribute("title", "Scan Again · Scanned just now"));
      expect(button).not.toHaveAttribute("aria-disabled");
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
      expect(alone.getByRole("button", { name: "Scan Again" })).not.toHaveAttribute("aria-disabled");
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
      expect(await failed.findByRole("alert")).toHaveTextContent("Couldn't scan. Try scanning again later.");
      const button = failed.getByRole("button", { name: "Scan Again" });
      await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
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

  it("says a scan failed and what to do, and the scan's own words only with technical details on", async () => {
    scanFailure = "boom";
    const plain = renderWithProviders(<UnknownPage />);
    expect(await plain.findByRole("alert")).toHaveTextContent("Couldn't scan. Try scanning again later.");
    expect(plain.queryByText(/boom/)).toBeNull();
    plain.unmount();

    settings = { ...settings, show_technical_details: true };
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
    expect(item).toHaveAccessibleDescription("The file this link points to can't be found.");
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
    expect(getByRole("button", { name: "Original missing" })).toHaveFocus();
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

describe("in a narrow window (R9)", () => {
  afterEach(() => {
    vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockRestore();
  });

  it("keeps a broken link's word where its size and date would be, every path at one x, and leaves the app to the tooltip", async () => {
    // The list as a window at its narrowest lays it out: 592 wide.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return { width: 592, height: 400, top: 0, left: 0, right: 592, bottom: 400, x: 0, y: 0, toJSON: () => ({}) };
    });
    const { findByText, queryByText, container } = renderWithProviders(<UnknownPage />);

    const script = rowOf(await findByText("old-script"));
    const word = within(script).getByRole("button", { name: "Original missing" });
    const path = within(script).getByText("~/.local/bin/old-script");
    // Not on the path's line: in the column at the row's right.
    expect(word.closest("[data-version]")).not.toBeNull();
    expect(path.parentElement?.querySelector("[data-status]")).toBeNull();
    // Every path's line holds its path first, and nothing before it.
    for (const row of container.querySelectorAll<HTMLElement>("[data-tool-row]")) {
      const line = row.querySelector("[title^='~'], [title^='/']")?.parentElement as HTMLElement;
      expect(line.firstElementChild?.getAttribute("title")).toMatch(/^[~/]/);
    }
    expect(queryByText("Points into Removed.app")).toBeNull();
    expect(queryByText("Points into Helper.app")).toBeNull();
    expect(tooltipOf(rowOf(await findByText("helper-cli")))).toContain("Part of Helper");
    // Said at more length behind the ⓘ, as before.
    fireEvent.click(word);
    expect(within(script).getByText("Part of Removed")).toBeInTheDocument();
    // Its size and date keep their columns.
    const tool = rowOf(await findByText("standalone-tool"));
    expect(sizeAndDateOf(tool).textContent).toBe(`${formatBytes(144_300_000)}${dateOf(1_758_000_000)}`);
  });
});

describe("the size and date headings in a narrow window (R9)", () => {
  afterEach(() => {
    vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockRestore();
  });

  function listWidth(width: number) {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return { width, height: 400, top: 0, left: 0, right: width, bottom: 400, x: 0, y: 0, toJSON: () => ({}) };
    });
  }

  it("stay over the columns while the rows draw them, at a window's narrowest", async () => {
    // 592: the list in a window at its narrowest, the `narrow` fit.
    listWidth(592);
    const { findByText, queryByText, container } = renderWithProviders(<UnknownPage />);
    const tool = rowOf(await findByText("standalone-tool"));
    // Laid out at that width: the app a link points into has left the line.
    await waitFor(() => expect(queryByText("Points into Helper.app")).toBeNull());
    expect(sizeAndDateOf(tool)).not.toBeNull();
    expect(container.querySelector("[data-column-heads]")).not.toBeNull();
  });

  it("go with the columns where a list too narrow for them drops them", async () => {
    // 400: the `minimal` fit, which has no version column.
    listWidth(400);
    const { findByText, container } = renderWithProviders(<UnknownPage />);
    const tool = rowOf(await findByText("standalone-tool"));
    await waitFor(() => expect(sizeAndDateOf(tool)).toBeNull());
    expect(container.querySelector("[data-column-heads]")).toBeNull();
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
    // The tooltip says it at more length.
    expect(tooltipOf(helper)).toContain("Part of Helper");

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
    // Its tooltip still says whose part it is.
    expect(tooltipOf(kit)).toEqual(["Part of Kit"]);
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

describe("protected places", () => {
  const protectedLink: UnknownEntry = {
    path: "~/.local/bin/notes-cli",
    kind: "ProtectedSymlink",
    resolved: null,
    link_target: "/Users/someone/Documents/notes-cli/bin/notes-cli",
    size_bytes: null,
    modified_at: null,
    owned_by_me: true,
    app_bundle: null,
  };

  it("says a link into a protected place points there, plainly, and never where it leads", async () => {
    settings = { ...settings, show_technical_details: true };
    scan = { ...baseScan, entries: [...baseScan.entries, protectedLink] };
    const { findByText, queryByText } = renderWithProviders(<UnknownPage />);

    const row = rowOf(await findByText("notes-cli"));
    // Its word where a size and a date would be, with no ⚠︎: nothing is
    // wrong with it, Banager just did not look.
    const word = within(row).getByRole("button", { name: "Points into a protected place" });
    expect(statusOf(row)?.closest("[data-version]")).not.toBeNull();
    expect(word.querySelector(".text-warning")).toBeNull();
    fireEvent.click(word);
    expect(
      within(row).getByText(
        "Banager doesn't look in Documents, Desktop, Downloads, iCloud Drive, other disks or other protected places, so where this link leads wasn't checked.",
      ),
    ).toBeInTheDocument();
    // No path it leads to anywhere, technical details or not.
    expect(queryByText(/notes-cli\/bin\/notes-cli/)).toBeNull();
    expect(queryByText(/^Links to/)).toBeNull();
    // Show in Finder is off, and says why.
    const menu = openMenu(row);
    const item = within(menu).getByRole("menuitem", { name: "Show in Finder" });
    expect(item).toHaveAttribute("aria-disabled", "true");
    expect(item).toHaveAccessibleDescription("This link points into a protected place, which Banager doesn't read.");
    fireEvent.click(item);
    expect(mockReveal).not.toHaveBeenCalled();
  });

  it("says how many folders it left unread in one quiet line, naming them behind an ⓘ only with technical details on", async () => {
    scan = { ...baseScan, protected_dirs: ["~/Documents/scripts", "~/Desktop/tools"] };
    const plain = renderWithProviders(<UnknownPage />);
    const line = await plain.findByText("2 folders are in protected places and weren't read.");
    expect(line.parentElement?.className.split(" ")).toEqual(expect.arrayContaining(["text-small", "text-muted"]));
    expect(plain.queryByRole("button", { name: /^Details: 2 folders/ })).toBeNull();
    expect(plain.queryByText("~/Documents/scripts")).toBeNull();
    plain.unmount();

    settings = { ...settings, show_technical_details: true };
    const technical = renderWithProviders(<UnknownPage />);
    const info = await technical.findByRole("button", {
      name: "Details: 2 folders are in protected places and weren't read.",
    });
    fireEvent.click(info);
    expect(technical.getByText("~/Documents/scripts")).toBeInTheDocument();
    expect(technical.getByText("~/Desktop/tools")).toBeInTheDocument();
  });

  it("says nothing of protected folders when there were none, or Rust sent none", async () => {
    scan = { ...baseScan, protected_dirs: [] };
    const none = renderWithProviders(<UnknownPage />);
    await none.findByText("standalone-tool");
    expect(none.container.querySelector("[data-protected-dirs]")).toBeNull();
    none.unmount();

    scan = baseScan;
    const absent = renderWithProviders(<UnknownPage />);
    await absent.findByText("standalone-tool");
    expect(absent.container.querySelector("[data-protected-dirs]")).toBeNull();
  });

  it("puts no check mark over a list with nothing in it when folders were left unread", async () => {
    scan = { ...baseScan, entries: [], protected_dirs: ["~/Documents/scripts"] };
    const { findByText, queryByText } = renderWithProviders(<UnknownPage />);

    expect(await findByText("No other programs in the places checked")).toBeInTheDocument();
    expect(queryByText("No other programs")).toBeNull();
    expect(queryByText("1 folder is in a protected place and wasn't read.")).toBeInTheDocument();
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      scan = {
        ...baseScan,
        entries: [...baseScan.entries, protectedLink],
        protected_dirs: ["~/Documents/scripts", "~/Desktop/tools"],
      };
      const { findByText, getByText } = renderWithProviders(<UnknownPage />);
      expect(await findByText("指向受保护的位置")).toBeInTheDocument();
      expect(getByText("有2个文件夹在受保护的位置，没有读取。")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});
