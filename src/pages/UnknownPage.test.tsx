import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UnknownPage } from "./UnknownPage";
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
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "scan_unknown") {
      return scanFailure === null ? Promise.resolve(scan) : Promise.reject(scanFailure);
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

describe("UnknownPage", () => {
  it("scans once when it opens and lists each program under its kind badge", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(await findByText("standalone-tool")).toBeInTheDocument();
    expect(getByText("old-script")).toBeInTheDocument();
    expect(getByText("helper-cli")).toBeInTheDocument();
    expect(getByText("Program")).toBeInTheDocument();
    expect(getByText("Broken link")).toBeInTheDocument();
    expect(getByText("Program (link)")).toBeInTheDocument();
    // The path is the row's first line, home abbreviated as Rust sent it.
    expect(getByText("~/.opencode/bin/standalone-tool")).toBeInTheDocument();
    expect(scanCalls()).toBe(1);
  });

  it("explains a broken link and names the app a program runs inside", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(
        "Points at /Applications/Removed.app/Contents/Resources/scripts/index.js, which no longer exists",
      ),
    ).toBeInTheDocument();
    expect(getByText("Part of Removed")).toBeInTheDocument();
    expect(getByText("Part of Helper")).toBeInTheDocument();
  });

  it("shows size and date, and says when an installer with administrator rights put it there", async () => {
    const { findByText, getByText, queryByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(`${formatBytes(144_300_000)} · ${dateOf(1_758_000_000)}`),
    ).toBeInTheDocument();
    expect(getByText(`${formatBytes(2_100_000)} · ${dateOf(1_700_000_000)}`)).toBeInTheDocument();
    // Exactly one row is not the user's own.
    expect(getByText("Put here by an installer with administrator rights")).toBeInTheDocument();
    // A broken link has no size and no date, and is not blank either: its
    // sentence is the broken-link one, tested above.
    expect(queryByText(/^ · /)).not.toBeInTheDocument();
  });

  it("shows where a link resolves only with technical details on", async () => {
    const hidden = renderWithProviders(<UnknownPage />);
    await hidden.findByText("helper-cli");
    expect(
      hidden.queryByText("Links to /Applications/Helper.app/Contents/Helpers/helper-cli"),
    ).not.toBeInTheDocument();
    hidden.unmount();

    settings = { ...settings, show_technical_details: true };
    const shown = renderWithProviders(<UnknownPage />);
    expect(
      await shown.findByText("Links to /Applications/Helper.app/Contents/Helpers/helper-cli"),
    ).toBeInTheDocument();
    // A plain file resolves to itself; there is nothing to add.
    expect(
      shown.queryByText("Links to /Users/someone/.opencode/bin/standalone-tool"),
    ).not.toBeInTheDocument();
  });

  it("says how many programs known sources accounted for, and where it looked", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(
        "4 more programs came from sources Canager knows and are listed under them.",
      ),
    ).toBeInTheDocument();
    expect(getByText("Looked in:")).toBeInTheDocument();
    expect(getByText("~/.local/bin (5 items)")).toBeInTheDocument();
    expect(getByText("/usr/local/bin (1 item)")).toBeInTheDocument();
  });

  it("warns with the scan's own numbers when it stopped early", async () => {
    scan = { ...baseScan, stopped: { FileLimit: { max_entries: 2000 } } };
    const byFiles = renderWithProviders(<UnknownPage />);
    expect(
      await byFiles.findByText(
        "Canager stopped after looking at 2000 items, so this list may be incomplete.",
      ),
    ).toBeInTheDocument();
    byFiles.unmount();

    scan = { ...baseScan, stopped: { TimeLimit: { max_secs: 10 } } };
    const byTime = renderWithProviders(<UnknownPage />);
    expect(
      await byTime.findByText("Canager stopped after 10 seconds, so this list may be incomplete."),
    ).toBeInTheDocument();
  });

  it("says so when nothing is unexplained, and still says where it looked", async () => {
    scan = { ...baseScan, entries: [], attributed: 7 };
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(
        "Nothing unexplained: every command-line program Canager found came from a source it knows.",
      ),
    ).toBeInTheDocument();
    expect(getByText("~/.local/bin (5 items)")).toBeInTheDocument();
  });

  it("scans again when the button is pressed", async () => {
    const { findByText, getByRole } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");
    expect(scanCalls()).toBe(1);

    fireEvent.click(getByRole("button", { name: "Scan again" }));

    await waitFor(() => expect(scanCalls()).toBe(2));
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
